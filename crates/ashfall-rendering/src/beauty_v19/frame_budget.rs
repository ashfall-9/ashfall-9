//! Frame budget and detail orchestration contracts.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameBudgetConfigV19 {
    pub target_frame_ms: f32,
    pub target_cpu_ms: f32,
    pub target_gpu_ms: f32,
    pub max_upload_mb_per_frame: f32,
    pub max_material_pages_generated_per_frame: u32,
    pub max_geometry_pages_uploaded_per_frame: u32,
    pub max_scatter_instances_visible: u32,
    pub max_shadow_pages_updated_per_frame: u32,
}

impl FrameBudgetConfigV19 {
    pub fn smooth_60hz() -> Self {
        Self {
            target_frame_ms: 16.6,
            target_cpu_ms: 6.0,
            target_gpu_ms: 8.5,
            max_upload_mb_per_frame: 12.0,
            max_material_pages_generated_per_frame: 4,
            max_geometry_pages_uploaded_per_frame: 8,
            max_scatter_instances_visible: 12_000,
            max_shadow_pages_updated_per_frame: 24,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DetailTierV19 {
    Dormant,
    Far,
    Mid,
    Near,
    Hero,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailRequestV19 {
    pub distance_meters: f32,
    pub screen_coverage_0_to_1: f32,
    pub visibility_0_to_1: f32,
    pub story_importance_0_to_1: f32,
    pub player_interaction_0_to_1: f32,
    pub cpu_pressure_0_to_1: f32,
    pub gpu_pressure_0_to_1: f32,
    pub upload_pressure_0_to_1: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailDecisionV19 {
    pub geometry_tier: DetailTierV19,
    pub material_tier: DetailTierV19,
    pub scatter_scale_0_to_1: f32,
    pub animation_update_scale_0_to_1: f32,
    pub shadow_update_scale_0_to_1: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DetailOrchestratorV19;

impl DetailOrchestratorV19 {
    pub fn decide(&self, request: DetailRequestV19) -> DetailDecisionV19 {
        let pressure = request
            .cpu_pressure_0_to_1
            .max(request.gpu_pressure_0_to_1)
            .max(request.upload_pressure_0_to_1)
            .clamp(0.0, 1.0);
        let importance = request
            .screen_coverage_0_to_1
            .max(request.visibility_0_to_1 * 0.7)
            .max(request.story_importance_0_to_1)
            .max(request.player_interaction_0_to_1)
            .clamp(0.0, 1.0);
        let distance_penalty = (request.distance_meters / 80.0).clamp(0.0, 1.0);
        let score = (importance * (1.0 - pressure * 0.55) * (1.0 - distance_penalty * 0.55))
            .clamp(0.0, 1.0);
        let tier = if score > 0.72 {
            DetailTierV19::Hero
        } else if score > 0.45 {
            DetailTierV19::Near
        } else if score > 0.22 {
            DetailTierV19::Mid
        } else if score > 0.05 {
            DetailTierV19::Far
        } else {
            DetailTierV19::Dormant
        };
        DetailDecisionV19 {
            geometry_tier: tier,
            material_tier: tier,
            scatter_scale_0_to_1: (score * (1.0 - pressure * 0.35)).clamp(0.05, 1.0),
            animation_update_scale_0_to_1: (score * (1.0 - pressure * 0.50)).clamp(0.10, 1.0),
            shadow_update_scale_0_to_1: (score * (1.0 - pressure * 0.60)).clamp(0.05, 1.0),
        }
    }
}
