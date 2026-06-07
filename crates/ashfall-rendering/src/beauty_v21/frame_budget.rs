//! V21 frame budget and detail orchestration.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameBudgetConfigV21 {
    pub target_frame_ms: f32,
    pub target_cpu_scene_build_ms: f32,
    pub target_gpu_scene_ms: f32,
    pub max_visible_cells: u32,
    pub max_material_pages_generated_per_frame: u32,
    pub max_geometry_upload_mb_per_frame: f32,
    pub max_texture_upload_mb_per_frame: f32,
    pub max_shadow_updates_per_frame: u32,
    pub max_humans_high_detail: u32,
    pub max_plants_visible: u32,
    pub max_stones_visible: u32,
    pub max_landfill_props_visible: u32,
}

impl FrameBudgetConfigV21 {
    pub fn smooth_60hz() -> Self {
        Self {
            target_frame_ms: 16.67,
            target_cpu_scene_build_ms: 2.0,
            target_gpu_scene_ms: 10.5,
            max_visible_cells: 9,
            max_material_pages_generated_per_frame: 8,
            max_geometry_upload_mb_per_frame: 8.0,
            max_texture_upload_mb_per_frame: 12.0,
            max_shadow_updates_per_frame: 4,
            max_humans_high_detail: 6,
            max_plants_visible: 900,
            max_stones_visible: 240,
            max_landfill_props_visible: 180,
        }
    }

    pub fn emergency_smooth() -> Self {
        Self {
            max_visible_cells: 5,
            max_material_pages_generated_per_frame: 3,
            max_plants_visible: 350,
            max_stones_visible: 80,
            max_landfill_props_visible: 60,
            ..Self::smooth_60hz()
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FramePressureV21 {
    pub measured_frame_ms: f32,
    pub measured_cpu_scene_build_ms: f32,
    pub measured_gpu_scene_ms: f32,
    pub upload_mb_this_frame: f32,
    pub material_pages_pending: u32,
    pub visible_cell_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DetailTierV21 {
    Hero,
    Near,
    Mid,
    Far,
    Culled,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailDecisionV21 {
    pub tier: DetailTierV21,
    pub material_page_resolution_scale: f32,
    pub scatter_density_scale: f32,
    pub animation_update_scale: f32,
    pub shadow_update_scale: f32,
}

pub struct DetailOrchestratorV21;

impl DetailOrchestratorV21 {
    pub fn choose_detail(
        budget: FrameBudgetConfigV21,
        pressure: FramePressureV21,
        distance_meters: f32,
        screen_fraction_0_to_1: f32,
        story_importance_0_to_1: f32,
    ) -> DetailDecisionV21 {
        let overloaded = pressure.measured_frame_ms > budget.target_frame_ms
            || pressure.measured_cpu_scene_build_ms > budget.target_cpu_scene_build_ms
            || pressure.measured_gpu_scene_ms > budget.target_gpu_scene_ms
            || pressure.upload_mb_this_frame
                > budget.max_geometry_upload_mb_per_frame + budget.max_texture_upload_mb_per_frame;

        let mut tier = if screen_fraction_0_to_1 > 0.18 || story_importance_0_to_1 > 0.85 {
            DetailTierV21::Hero
        } else if distance_meters < 12.0 || screen_fraction_0_to_1 > 0.06 {
            DetailTierV21::Near
        } else if distance_meters < 38.0 || screen_fraction_0_to_1 > 0.018 {
            DetailTierV21::Mid
        } else if distance_meters < 90.0 {
            DetailTierV21::Far
        } else {
            DetailTierV21::Culled
        };

        if overloaded {
            tier = match tier {
                DetailTierV21::Hero => DetailTierV21::Near,
                DetailTierV21::Near => DetailTierV21::Mid,
                DetailTierV21::Mid => DetailTierV21::Far,
                other => other,
            };
        }

        match tier {
            DetailTierV21::Hero => DetailDecisionV21 {
                tier,
                material_page_resolution_scale: 1.0,
                scatter_density_scale: 1.0,
                animation_update_scale: 1.0,
                shadow_update_scale: 1.0,
            },
            DetailTierV21::Near => DetailDecisionV21 {
                tier,
                material_page_resolution_scale: 0.75,
                scatter_density_scale: 0.75,
                animation_update_scale: 0.8,
                shadow_update_scale: 0.8,
            },
            DetailTierV21::Mid => DetailDecisionV21 {
                tier,
                material_page_resolution_scale: 0.5,
                scatter_density_scale: 0.45,
                animation_update_scale: 0.5,
                shadow_update_scale: 0.45,
            },
            DetailTierV21::Far => DetailDecisionV21 {
                tier,
                material_page_resolution_scale: 0.25,
                scatter_density_scale: 0.18,
                animation_update_scale: 0.20,
                shadow_update_scale: 0.15,
            },
            DetailTierV21::Culled => DetailDecisionV21 {
                tier,
                material_page_resolution_scale: 0.0,
                scatter_density_scale: 0.0,
                animation_update_scale: 0.0,
                shadow_update_scale: 0.0,
            },
        }
    }
}
