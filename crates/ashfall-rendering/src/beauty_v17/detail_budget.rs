//! Frame budgeting for V17 Beauty Mode.
//!
//! The goal is not to add unlimited detail. The goal is to make every visible
//! object believable, then scale optional detail by distance, screen size,
//! visibility, and frame pressure.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BeautyDetailTierV17 {
    Hero,
    Near,
    Mid,
    Far,
    Impostor,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameBudgetConfigV17 {
    pub target_frame_ms: f32,
    pub target_cpu_beauty_build_ms: f32,
    pub target_gpu_ms: f32,
    pub max_upload_mb_per_frame: f32,
    pub max_material_pages_generated: u32,
    pub max_geometry_pages_uploaded: u32,
    pub max_shadow_pages_updated: u32,
    pub max_visible_beauty_cells: usize,
    pub max_foreground_scatter_items: usize,
    pub max_human_proxies: usize,
    pub max_vehicle_proxies: usize,
}

impl Default for FrameBudgetConfigV17 {
    fn default() -> Self {
        Self {
            target_frame_ms: 16.6,
            target_cpu_beauty_build_ms: 2.4,
            target_gpu_ms: 10.5,
            max_upload_mb_per_frame: 12.0,
            max_material_pages_generated: 6,
            max_geometry_pages_uploaded: 4,
            max_shadow_pages_updated: 8,
            max_visible_beauty_cells: 6,
            max_foreground_scatter_items: 420,
            max_human_proxies: 8,
            max_vehicle_proxies: 5,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameCostSampleV17 {
    pub cpu_beauty_build_ms: f32,
    pub gpu_frame_ms: f32,
    pub upload_mb: f32,
    pub generated_material_pages: u32,
    pub uploaded_geometry_pages: u32,
    pub updated_shadow_pages: u32,
    pub visible_beauty_cells: usize,
    pub foreground_scatter_items: usize,
    pub human_proxies: usize,
    pub vehicle_proxies: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailDecisionV17 {
    pub tier: BeautyDetailTierV17,
    pub geometry_density_0_to_1: f32,
    pub material_page_scale_0_to_1: f32,
    pub scatter_density_0_to_1: f32,
    pub decal_density_0_to_1: f32,
    pub shadow_priority_0_to_1: f32,
    pub reflection_priority_0_to_1: f32,
    pub animation_rate_scale_0_to_1: f32,
    pub weather_detail_scale_0_to_1: f32,
    pub human_detail_scale_0_to_1: f32,
    pub vehicle_detail_scale_0_to_1: f32,
    pub silhouette_identity_locked: bool,
}

impl DetailDecisionV17 {
    pub fn hero() -> Self {
        Self {
            tier: BeautyDetailTierV17::Hero,
            geometry_density_0_to_1: 1.0,
            material_page_scale_0_to_1: 1.0,
            scatter_density_0_to_1: 1.0,
            decal_density_0_to_1: 1.0,
            shadow_priority_0_to_1: 1.0,
            reflection_priority_0_to_1: 1.0,
            animation_rate_scale_0_to_1: 1.0,
            weather_detail_scale_0_to_1: 1.0,
            human_detail_scale_0_to_1: 1.0,
            vehicle_detail_scale_0_to_1: 1.0,
            silhouette_identity_locked: true,
        }
    }

    pub fn far(identity_locked: bool) -> Self {
        Self {
            tier: BeautyDetailTierV17::Far,
            geometry_density_0_to_1: 0.36,
            material_page_scale_0_to_1: 0.34,
            scatter_density_0_to_1: 0.12,
            decal_density_0_to_1: 0.16,
            shadow_priority_0_to_1: 0.18,
            reflection_priority_0_to_1: 0.12,
            animation_rate_scale_0_to_1: 0.25,
            weather_detail_scale_0_to_1: 0.35,
            human_detail_scale_0_to_1: if identity_locked { 0.42 } else { 0.0 },
            vehicle_detail_scale_0_to_1: if identity_locked { 0.42 } else { 0.0 },
            silhouette_identity_locked: identity_locked,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailOrchestratorV17 {
    pub budget: FrameBudgetConfigV17,
    pub last_cost: FrameCostSampleV17,
}

impl Default for DetailOrchestratorV17 {
    fn default() -> Self {
        Self::new(FrameBudgetConfigV17::default())
    }
}

impl DetailOrchestratorV17 {
    pub fn new(budget: FrameBudgetConfigV17) -> Self {
        Self {
            budget,
            last_cost: FrameCostSampleV17::default(),
        }
    }

    pub fn update_cost_sample(&mut self, cost: FrameCostSampleV17) {
        self.last_cost = cost;
    }

    pub fn pressure_0_to_2(&self) -> f32 {
        let cpu = safe_ratio(
            self.last_cost.cpu_beauty_build_ms,
            self.budget.target_cpu_beauty_build_ms,
        );
        let gpu = safe_ratio(self.last_cost.gpu_frame_ms, self.budget.target_gpu_ms);
        let upload = safe_ratio(
            self.last_cost.upload_mb,
            self.budget.max_upload_mb_per_frame,
        );
        let material_pages = safe_ratio(
            self.last_cost.generated_material_pages as f32,
            self.budget.max_material_pages_generated.max(1) as f32,
        );
        let geometry_pages = safe_ratio(
            self.last_cost.uploaded_geometry_pages as f32,
            self.budget.max_geometry_pages_uploaded.max(1) as f32,
        );
        cpu.max(gpu)
            .max(upload)
            .max(material_pages)
            .max(geometry_pages)
            .clamp(0.0, 2.0)
    }

    pub fn decide(
        &self,
        distance_meters: f32,
        screen_fraction_0_to_1: f32,
        story_importance_0_to_1: f32,
        material_importance_0_to_1: f32,
        is_human_or_vehicle: bool,
        is_foreground: bool,
    ) -> DetailDecisionV17 {
        let importance = screen_fraction_0_to_1
            .max(story_importance_0_to_1)
            .max(material_importance_0_to_1)
            .clamp(0.0, 1.0);
        let identity_locked = is_human_or_vehicle || is_foreground || importance > 0.35;

        let mut decision = if is_foreground || importance > 0.55 || distance_meters < 8.0 {
            DetailDecisionV17::hero()
        } else if distance_meters < 22.0 {
            DetailDecisionV17 {
                tier: BeautyDetailTierV17::Near,
                geometry_density_0_to_1: 0.84,
                material_page_scale_0_to_1: 0.86,
                scatter_density_0_to_1: 0.70,
                decal_density_0_to_1: 0.78,
                shadow_priority_0_to_1: 0.76,
                reflection_priority_0_to_1: 0.62,
                animation_rate_scale_0_to_1: 0.76,
                weather_detail_scale_0_to_1: 0.78,
                human_detail_scale_0_to_1: 0.82,
                vehicle_detail_scale_0_to_1: 0.82,
                silhouette_identity_locked: identity_locked,
            }
        } else if distance_meters < 55.0 {
            DetailDecisionV17 {
                tier: BeautyDetailTierV17::Mid,
                geometry_density_0_to_1: 0.58,
                material_page_scale_0_to_1: 0.55,
                scatter_density_0_to_1: 0.32,
                decal_density_0_to_1: 0.38,
                shadow_priority_0_to_1: 0.42,
                reflection_priority_0_to_1: 0.30,
                animation_rate_scale_0_to_1: 0.46,
                weather_detail_scale_0_to_1: 0.52,
                human_detail_scale_0_to_1: 0.55,
                vehicle_detail_scale_0_to_1: 0.55,
                silhouette_identity_locked: identity_locked,
            }
        } else {
            DetailDecisionV17::far(identity_locked)
        };

        let pressure = self.pressure_0_to_2();
        if pressure > 1.0 {
            let cut = ((pressure - 1.0) * 0.45).clamp(0.0, 0.45);
            // Degrade optional detail first.
            decision.scatter_density_0_to_1 *= 1.0 - cut;
            decision.decal_density_0_to_1 *= 1.0 - cut;
            decision.reflection_priority_0_to_1 *= 1.0 - cut;
            decision.weather_detail_scale_0_to_1 *= 1.0 - cut * 0.75;
            decision.material_page_scale_0_to_1 *= 1.0 - cut * 0.50;
            decision.shadow_priority_0_to_1 *= 1.0 - cut * 0.40;
            decision.animation_rate_scale_0_to_1 *= 1.0 - cut * 0.35;

            // Never destroy foreground silhouettes or human/vehicle identity.
            if !decision.silhouette_identity_locked && distance_meters > 35.0 {
                decision.geometry_density_0_to_1 *= 1.0 - cut * 0.65;
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
