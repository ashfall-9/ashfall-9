//! Frame budget and detail orchestration contracts.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameBudgetConfigV20 {
    pub target_frame_ms: f32,
    pub target_cpu_ms: f32,
    pub target_gpu_ms: f32,
    pub max_upload_mb_per_frame: f32,
    pub max_material_pages_generated_per_frame: u32,
    pub max_geometry_pages_uploaded_per_frame: u32,
    pub max_shadow_pages_updated_per_frame: u32,
    pub max_scatter_instances_near: u32,
    pub max_scatter_instances_far: u32,
}

impl FrameBudgetConfigV20 {
    pub fn smooth_60hz() -> Self {
        Self {
            target_frame_ms: 16.6,
            target_cpu_ms: 6.0,
            target_gpu_ms: 8.5,
            max_upload_mb_per_frame: 24.0,
            max_material_pages_generated_per_frame: 8,
            max_geometry_pages_uploaded_per_frame: 12,
            max_shadow_pages_updated_per_frame: 16,
            max_scatter_instances_near: 1_500,
            max_scatter_instances_far: 4_000,
        }
    }

    pub fn smooth_30hz() -> Self {
        Self {
            target_frame_ms: 33.3,
            target_cpu_ms: 10.0,
            target_gpu_ms: 17.0,
            max_upload_mb_per_frame: 36.0,
            max_material_pages_generated_per_frame: 12,
            max_geometry_pages_uploaded_per_frame: 18,
            max_shadow_pages_updated_per_frame: 24,
            max_scatter_instances_near: 2_500,
            max_scatter_instances_far: 8_000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BeautyDetailTierV20 {
    Disabled,
    Far,
    Mid,
    Near,
    Hero,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailDecisionInputV20 {
    pub distance_meters: f32,
    pub screen_fraction_0_to_1: f32,
    pub visible: bool,
    pub story_importance_0_to_1: f32,
    pub interaction_chance_0_to_1: f32,
    pub cpu_pressure_0_to_1: f32,
    pub gpu_pressure_0_to_1: f32,
    pub memory_pressure_0_to_1: f32,
}

pub struct DetailOrchestratorV20;

impl DetailOrchestratorV20 {
    pub fn choose_tier(input: DetailDecisionInputV20) -> BeautyDetailTierV20 {
        if !input.visible {
            return BeautyDetailTierV20::Disabled;
        }

        let pressure = input
            .cpu_pressure_0_to_1
            .max(input.gpu_pressure_0_to_1)
            .max(input.memory_pressure_0_to_1)
            .clamp(0.0, 1.0);
        let importance = input
            .story_importance_0_to_1
            .max(input.interaction_chance_0_to_1)
            .max(input.screen_fraction_0_to_1)
            .clamp(0.0, 1.0);

        if importance > 0.75 && input.distance_meters < 12.0 && pressure < 0.85 {
            BeautyDetailTierV20::Hero
        } else if input.distance_meters < 20.0 && pressure < 0.90 {
            BeautyDetailTierV20::Near
        } else if input.distance_meters < 55.0 && pressure < 0.95 {
            BeautyDetailTierV20::Mid
        } else {
            BeautyDetailTierV20::Far
        }
    }
}
