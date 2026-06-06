//! Detail budgeting for smooth performance.
//!
//! The engine should store or generate rich detail, but only spend frame time on
//! visible and important detail. Low quality must not mean returning to boxes and rods.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BeautyDetailTierV15 {
    Hero,
    Near,
    Mid,
    Far,
    Impostor,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameBudgetConfigV15 {
    pub target_frame_ms: f32,
    pub target_cpu_scene_ms: f32,
    pub target_gpu_ms: f32,
    pub max_upload_mb_per_frame: f32,
    pub max_material_pages_generated: u32,
    pub max_geometry_pages_uploaded: u32,
    pub max_shadow_pages_updated: u32,
}

impl Default for FrameBudgetConfigV15 {
    fn default() -> Self {
        Self {
            target_frame_ms: 16.6,
            target_cpu_scene_ms: 3.0,
            target_gpu_ms: 10.0,
            max_upload_mb_per_frame: 12.0,
            max_material_pages_generated: 4,
            max_geometry_pages_uploaded: 2,
            max_shadow_pages_updated: 8,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameCostSampleV15 {
    pub cpu_scene_ms: f32,
    pub gpu_frame_ms: f32,
    pub upload_mb: f32,
    pub generated_material_pages: u32,
    pub uploaded_geometry_pages: u32,
    pub updated_shadow_pages: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailDecisionV15 {
    pub tier: BeautyDetailTierV15,
    pub geometry_density: f32,
    pub material_page_scale: f32,
    pub scatter_density: f32,
    pub decal_density: f32,
    pub shadow_priority: f32,
    pub reflection_priority: f32,
    pub animation_rate_scale: f32,
    pub weather_detail_scale: f32,
}

impl DetailDecisionV15 {
    pub fn hero() -> Self {
        Self {
            tier: BeautyDetailTierV15::Hero,
            geometry_density: 1.0,
            material_page_scale: 1.0,
            scatter_density: 1.0,
            decal_density: 1.0,
            shadow_priority: 1.0,
            reflection_priority: 1.0,
            animation_rate_scale: 1.0,
            weather_detail_scale: 1.0,
        }
    }

    pub fn far() -> Self {
        Self {
            tier: BeautyDetailTierV15::Far,
            geometry_density: 0.35,
            material_page_scale: 0.35,
            scatter_density: 0.15,
            decal_density: 0.1,
            shadow_priority: 0.15,
            reflection_priority: 0.1,
            animation_rate_scale: 0.25,
            weather_detail_scale: 0.3,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DetailOrchestratorV15 {
    pub budget: FrameBudgetConfigV15,
    pub last_cost: FrameCostSampleV15,
}

impl DetailOrchestratorV15 {
    pub fn new(budget: FrameBudgetConfigV15) -> Self {
        Self {
            budget,
            last_cost: FrameCostSampleV15::default(),
        }
    }

    pub fn update_cost_sample(&mut self, cost: FrameCostSampleV15) {
        self.last_cost = cost;
    }

    pub fn global_pressure(&self) -> f32 {
        let cpu = safe_ratio(self.last_cost.cpu_scene_ms, self.budget.target_cpu_scene_ms);
        let gpu = safe_ratio(self.last_cost.gpu_frame_ms, self.budget.target_gpu_ms);
        let upload = safe_ratio(
            self.last_cost.upload_mb,
            self.budget.max_upload_mb_per_frame,
        );
        let material_pages = safe_ratio(
            self.last_cost.generated_material_pages as f32,
            self.budget.max_material_pages_generated.max(1) as f32,
        );
        cpu.max(gpu).max(upload).max(material_pages).clamp(0.0, 2.0)
    }

    pub fn decide(
        &self,
        distance_meters: f32,
        screen_fraction: f32,
        story_importance: f32,
        material_importance: f32,
        is_human_or_vehicle: bool,
    ) -> DetailDecisionV15 {
        let importance = screen_fraction
            .max(story_importance)
            .max(material_importance)
            .clamp(0.0, 1.0);
        let pressure = self.global_pressure();

        let mut decision = if importance > 0.55 || distance_meters < 8.0 {
            DetailDecisionV15::hero()
        } else if distance_meters < 22.0 {
            DetailDecisionV15 {
                tier: BeautyDetailTierV15::Near,
                geometry_density: 0.85,
                material_page_scale: 0.85,
                scatter_density: 0.75,
                decal_density: 0.8,
                shadow_priority: 0.8,
                reflection_priority: 0.65,
                animation_rate_scale: 0.8,
                weather_detail_scale: 0.8,
            }
        } else if distance_meters < 55.0 {
            DetailDecisionV15 {
                tier: BeautyDetailTierV15::Mid,
                geometry_density: 0.6,
                material_page_scale: 0.55,
                scatter_density: 0.35,
                decal_density: 0.4,
                shadow_priority: 0.45,
                reflection_priority: 0.3,
                animation_rate_scale: 0.5,
                weather_detail_scale: 0.55,
            }
        } else {
            DetailDecisionV15::far()
        };

        if pressure > 1.0 {
            let cut = ((pressure - 1.0) * 0.45).clamp(0.0, 0.45);
            decision.scatter_density *= 1.0 - cut;
            decision.decal_density *= 1.0 - cut;
            decision.reflection_priority *= 1.0 - cut;
            decision.weather_detail_scale *= 1.0 - cut * 0.7;
            decision.material_page_scale *= 1.0 - cut * 0.5;

            if !is_human_or_vehicle && distance_meters > 35.0 {
                decision.geometry_density *= 1.0 - cut * 0.7;
            }
        }

        decision
    }
}

fn safe_ratio(value: f32, target: f32) -> f32 {
    if target <= f32::EPSILON {
        0.0
    } else {
        value / target
    }
}
