//! V22 frame budget and detail orchestration.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameBudgetConfigV22 {
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
    pub max_legacy_beauty_paths_active: u32,
}

impl FrameBudgetConfigV22 {
    pub fn smooth_60hz() -> Self {
        Self {
            target_frame_ms: 16.67,
            target_cpu_scene_build_ms: 1.6,
            target_gpu_scene_ms: 10.0,
            max_visible_cells: 7,
            max_material_pages_generated_per_frame: 4,
            max_geometry_upload_mb_per_frame: 6.0,
            max_texture_upload_mb_per_frame: 8.0,
            max_shadow_updates_per_frame: 3,
            max_humans_high_detail: 4,
            max_plants_visible: 650,
            max_stones_visible: 180,
            max_landfill_props_visible: 130,
            max_legacy_beauty_paths_active: 0,
        }
    }

    pub fn emergency_smooth() -> Self {
        Self {
            max_visible_cells: 4,
            max_material_pages_generated_per_frame: 2,
            max_plants_visible: 220,
            max_stones_visible: 60,
            max_landfill_props_visible: 40,
            max_humans_high_detail: 2,
            ..Self::smooth_60hz()
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FramePressureV22 {
    pub measured_frame_ms: f32,
    pub measured_cpu_scene_build_ms: f32,
    pub measured_gpu_scene_ms: f32,
    pub upload_mb_this_frame: f32,
    pub material_pages_pending: u32,
    pub visible_cell_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DetailTierV22 {
    Hero,
    Near,
    Mid,
    Far,
    Culled,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailDecisionV22 {
    pub tier: DetailTierV22,
    pub material_page_resolution_scale: f32,
    pub scatter_density_scale: f32,
    pub animation_update_scale: f32,
    pub shadow_update_scale: f32,
    pub preserve_silhouette: bool,
    pub preserve_human_coherence: bool,
}

pub struct DetailOrchestratorV22;

impl DetailOrchestratorV22 {
    pub fn choose_detail(
        budget: FrameBudgetConfigV22,
        pressure: FramePressureV22,
        distance_meters: f32,
        screen_fraction_0_to_1: f32,
        story_importance_0_to_1: f32,
    ) -> DetailDecisionV22 {
        let overloaded = pressure.measured_frame_ms > budget.target_frame_ms
            || pressure.measured_cpu_scene_build_ms > budget.target_cpu_scene_build_ms
            || pressure.measured_gpu_scene_ms > budget.target_gpu_scene_ms
            || pressure.upload_mb_this_frame
                > budget.max_geometry_upload_mb_per_frame + budget.max_texture_upload_mb_per_frame;

        let mut tier = if screen_fraction_0_to_1 > 0.18 || story_importance_0_to_1 > 0.85 {
            DetailTierV22::Hero
        } else if distance_meters < 12.0 || screen_fraction_0_to_1 > 0.06 {
            DetailTierV22::Near
        } else if distance_meters < 38.0 || screen_fraction_0_to_1 > 0.018 {
            DetailTierV22::Mid
        } else if distance_meters < 90.0 {
            DetailTierV22::Far
        } else {
            DetailTierV22::Culled
        };

        if overloaded {
            tier = match tier {
                DetailTierV22::Hero => DetailTierV22::Near,
                DetailTierV22::Near => DetailTierV22::Mid,
                DetailTierV22::Mid => DetailTierV22::Far,
                other => other,
            };
        }

        match tier {
            DetailTierV22::Hero => DetailDecisionV22 {
                tier,
                material_page_resolution_scale: 1.0,
                scatter_density_scale: 1.0,
                animation_update_scale: 1.0,
                shadow_update_scale: 1.0,
                preserve_silhouette: true,
                preserve_human_coherence: true,
            },
            DetailTierV22::Near => DetailDecisionV22 {
                tier,
                material_page_resolution_scale: 0.75,
                scatter_density_scale: 0.70,
                animation_update_scale: 0.80,
                shadow_update_scale: 0.75,
                preserve_silhouette: true,
                preserve_human_coherence: true,
            },
            DetailTierV22::Mid => DetailDecisionV22 {
                tier,
                material_page_resolution_scale: 0.50,
                scatter_density_scale: 0.38,
                animation_update_scale: 0.50,
                shadow_update_scale: 0.40,
                preserve_silhouette: true,
                preserve_human_coherence: true,
            },
            DetailTierV22::Far => DetailDecisionV22 {
                tier,
                material_page_resolution_scale: 0.25,
                scatter_density_scale: 0.14,
                animation_update_scale: 0.18,
                shadow_update_scale: 0.12,
                preserve_silhouette: true,
                preserve_human_coherence: true,
            },
            DetailTierV22::Culled => DetailDecisionV22 {
                tier,
                material_page_resolution_scale: 0.0,
                scatter_density_scale: 0.0,
                animation_update_scale: 0.0,
                shadow_update_scale: 0.0,
                preserve_silhouette: false,
                preserve_human_coherence: true,
            },
        }
    }
}
