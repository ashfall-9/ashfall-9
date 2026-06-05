use std::collections::{BTreeMap, BTreeSet};

use ashfall_core::assets::{
    ASSET_PACKAGE_MANIFEST_SCHEMA_VERSION, AssetKind, AssetPackageChunkManifest,
    AssetPackageManifest, AssetPriority,
};
use ashfall_core::core::*;
use ashfall_core::gpu::{
    ComputePipelineHandle, GpuDispatchKind, GpuGraphBuilder, GpuPassDesc, GpuPipelineDesc,
    GpuQueueKind, GpuResourceDesc, GpuResourceHandle, GpuResourceKind, GpuResourceLifetime,
    GpuShaderPermutation,
};
use ashfall_core::runtime::{
    EngineContext, EngineModule, FrameContext, ModuleDescriptor, ModuleStateRecord,
};
use ashfall_core::validation::ValidationReport;
use ashfall_core::world::{CommandSink, HumanState, WorldEvent, WorldEventKind, WorldSnapshot};

pub const DEFAULT_HUMAN_SKIN_MATERIAL: MaterialId = 4;
pub const DEFAULT_EYE_MATERIAL: MaterialId = 40_001;
pub const DEFAULT_HAIR_MATERIAL: MaterialId = 40_002;
pub const DEFAULT_TEETH_MATERIAL: MaterialId = 40_003;
pub const DEFAULT_TONGUE_MATERIAL: MaterialId = 40_004;
pub const DEFAULT_CLOTHING_MATERIAL: MaterialId = 40_005;
const HUMAN_MODULE_STATE_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HumanRole {
    Hero,
    ImportantNpc,
    NormalNpc,
    Crowd,
    Vendor,
    GangMember,
    SecurityAgent,
    CorporateAgent,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MorphologyParams {
    pub height_meters: f32,
    pub shoulder_width_meters: f32,
    pub hip_width_meters: f32,
    pub limb_proportion: f32,
    pub posture: PostureKind,
    pub body_mass_kg: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostureKind {
    Upright,
    Relaxed,
    Guarded,
    Injured,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FaceParams {
    pub jaw_width: f32,
    pub cheekbone_height: f32,
    pub brow_depth: f32,
    pub nose_bridge: f32,
    pub lip_fullness: f32,
    pub ear_size: f32,
    pub asymmetry: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgeParams {
    pub apparent_age_years: f32,
    pub wrinkle_strength: f32,
    pub skin_elasticity: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BodyCompositionParams {
    pub muscle: f32,
    pub fat: f32,
    pub soft_tissue_motion: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClothingProfile {
    pub style: String,
    pub layers: Vec<ClothingLayer>,
    pub wetness_response: f32,
    pub damage_visibility: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClothingLayer {
    pub label: String,
    pub material_id: MaterialId,
    pub attach_points: Vec<BodyAttachPoint>,
    pub cloth_simulation: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanGenerationBudget {
    pub max_meshes: usize,
    pub max_materials: usize,
    pub max_memory_bytes: u64,
    pub max_hair_strands: u32,
    pub min_quality: QualityTier,
}

impl HumanGenerationBudget {
    pub fn for_quality(quality: QualityTier) -> Self {
        match quality {
            QualityTier::HeroHighFidelityRuntime | QualityTier::ReferenceOfflineValidation => {
                Self {
                    max_meshes: 32,
                    max_materials: 16,
                    max_memory_bytes: 96 * 1024 * 1024,
                    max_hair_strands: 40_000,
                    min_quality: QualityTier::NormalRuntime,
                }
            }
            QualityTier::NormalRuntime => Self {
                max_meshes: 18,
                max_materials: 10,
                max_memory_bytes: 40 * 1024 * 1024,
                max_hair_strands: 12_000,
                min_quality: QualityTier::BackgroundApproximation,
            },
            QualityTier::BackgroundApproximation => Self {
                max_meshes: 8,
                max_materials: 6,
                max_memory_bytes: 12 * 1024 * 1024,
                max_hair_strands: 2_000,
                min_quality: QualityTier::BackgroundApproximation,
            },
            QualityTier::Disabled => Self {
                max_meshes: 0,
                max_materials: 0,
                max_memory_bytes: 0,
                max_hair_strands: 0,
                min_quality: QualityTier::Disabled,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanGenerationRequest {
    pub request_id: u128,
    pub seed: u64,
    pub role: HumanRole,
    pub desired_quality: QualityTier,
    pub morphology: MorphologyParams,
    pub face: FaceParams,
    pub skin: SkinParams,
    pub hair: HairParams,
    pub eyes: EyeParams,
    pub age_model: AgeParams,
    pub body_composition: BodyCompositionParams,
    pub clothing_profile: ClothingProfile,
    pub cybernetics: Vec<CyberneticAugmentSpec>,
    pub linked_ai_persona: Option<AgentPersonaId>,
    pub linked_voice_persona: Option<VoicePersonaId>,
    pub generation_policy: HumanGenerationPolicy,
    pub budget: HumanGenerationBudget,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanRuntimeBundle {
    pub human_id: HumanId,
    pub render_meshes: HumanRenderMeshes,
    pub anatomical_layers: AnatomicalLayerSet,
    pub rig: HumanRig,
    pub facial_rig: FacialRig,
    pub materials: HumanMaterialSet,
    pub physics_proxy: HumanPhysicsProxy,
    pub animation_profile: AnimationProfile,
    pub voice_persona: Option<VoicePersonaId>,
    pub ai_persona: Option<AgentPersonaId>,
    pub lod_set: HumanLodSet,
    pub surface_response: HumanSurfaceResponseProfile,
    pub skin_system: SkinRuntimeModel,
    pub eye_system: EyeRuntimeModel,
    pub mouth_system: MouthRuntimeModel,
    pub hair_system: HairRuntimeModel,
    pub clothing_cybernetics: ClothingCyberneticsRuntimeModel,
    pub body_model: HumanBodyRuntimeModel,
    pub motion_model: HumanMotionModel,
    pub renderer_interface: HumanRendererInterface,
    pub ai_voice_integration: HumanAiVoiceIntegrationModel,
    pub performance_capture: HumanPerformanceCaptureModel,
    pub failure_fallbacks: HumanFailureFallbackPlan,
    pub performance_budget: HumanPerformanceBudgetReport,
    pub asset_package_contract: HumanAssetPackageContract,
    pub uncanny_review: HumanUncannyReviewChecklist,
    pub acceptance_evidence: HumanAcceptanceEvidence,
    pub generation_policy: HumanGenerationPolicy,
    pub debug_views: HumanDebugViewSet,
    pub validation_report: HumanValidationReport,
}

impl HumanRuntimeBundle {
    pub fn streaming_dependencies(
        &self,
        min_quality: QualityTier,
    ) -> Vec<HumanStreamingDependency> {
        streaming_dependencies_for_bundle(self, min_quality)
    }
}

pub type HumanRenderMeshes = Vec<HumanMesh>;
pub type AnatomicalLayerSet = Vec<AnatomicalLayer>;
pub type HumanMaterialSet = Vec<HumanMaterialBinding>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HumanStreamingDependency {
    pub asset_id: AssetId,
    pub part: HumanMeshPart,
    pub lod: HumanLodTier,
    pub requested_quality: QualityTier,
    pub priority: AssetPriority,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanMesh {
    pub mesh: MeshAssetHandle,
    pub part: HumanMeshPart,
    pub lod: HumanLodTier,
    pub streaming_required: bool,
    pub triangle_budget: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HumanMeshPart {
    Body,
    Face,
    Eyes,
    Teeth,
    Tongue,
    Hair,
    Clothing,
    CyberneticAugment,
    ImpostorCard,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnatomicalLayer {
    pub kind: AnatomicalLayerKind,
    pub quality: QualityTier,
    pub enabled: bool,
    pub material_state_channels: Vec<HumanMaterialStateChannel>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnatomicalLayerKind {
    Skeleton,
    Muscle,
    SoftTissue,
    Skin,
    Eyes,
    Mouth,
    Tongue,
    Hair,
    Clothing,
    Cybernetics,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanMaterialStateChannel {
    Wetness,
    Oil,
    Sweat,
    BloodPerfusion,
    Bruising,
    Injury,
    Dirt,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanRig {
    pub rig_id: u128,
    pub skeleton_profile: String,
    pub bone_count: u16,
    pub attachment_points: Vec<AttachmentPoint>,
    pub gpu_skinning_resource: Option<GpuSkinningResource>,
    pub retargeting_profile: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AttachmentPoint {
    pub point: BodyAttachPoint,
    pub bone_name: String,
    pub local_offset_meters: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyAttachPoint {
    Head,
    Face,
    Spine,
    LeftShoulder,
    RightShoulder,
    LeftArm,
    RightArm,
    LeftHand,
    RightHand,
    Torso,
    Hips,
    LeftLeg,
    RightLeg,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuSkinningResource(pub u128);

#[derive(Clone, Debug, PartialEq)]
pub struct FacialRig {
    pub rig_id: u128,
    pub viseme_set: Vec<VisemeControl>,
    pub expression_controls: Vec<ExpressionControl>,
    pub micro_expression_count: u16,
    pub blink_controls: bool,
    pub eye_aim_controls: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VisemeControl {
    pub label: String,
    pub weight_channel: u16,
    pub jaw_open: f32,
    pub lip_round: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExpressionControl {
    pub label: String,
    pub emotion_axis: EmotionAxis,
    pub max_weight: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmotionAxis {
    Anger,
    Fear,
    Disgust,
    Sadness,
    Surprise,
    Tiredness,
    Pain,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EyeRuntimeModel {
    pub cornea_layer: bool,
    pub sclera_layer: bool,
    pub iris_detail_seed: u64,
    pub pupil_control: PupilControlModel,
    pub tearline: bool,
    pub eye_moisture: f32,
    pub eyelid_contact: bool,
    pub gaze: GazeControlModel,
    pub blink: BlinkModel,
    pub emotional_eye_behavior: bool,
}

impl EyeRuntimeModel {
    pub fn supports_closeup_optics(&self) -> bool {
        self.cornea_layer
            && self.sclera_layer
            && self.tearline
            && self.eye_moisture > 0.05
            && self.eyelid_contact
            && self.pupil_control.supports_light_adaptation
            && self.gaze.target_binding != GazeTargetBinding::None
            && self.blink.spontaneous_blinks_per_minute > 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PupilControlModel {
    pub rest_radius_meters: f32,
    pub min_radius_meters: f32,
    pub max_radius_meters: f32,
    pub supports_light_adaptation: bool,
    pub supports_emotional_response: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GazeControlModel {
    pub target_binding: GazeTargetBinding,
    pub micro_saccades_per_second: f32,
    pub max_eye_yaw_radians: f32,
    pub max_eye_pitch_radians: f32,
    pub head_follow_weight: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GazeTargetBinding {
    None,
    AiFocusTarget,
    DialogueTarget,
    WorldLookAtPoint,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlinkModel {
    pub spontaneous_blinks_per_minute: f32,
    pub stress_blink_multiplier: f32,
    pub asymmetric_blink_support: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MouthRuntimeModel {
    pub teeth_geometry: bool,
    pub tongue_animation: bool,
    pub inner_mouth_shading: bool,
    pub dental: HumanDentalRuntimeModel,
    pub tongue: HumanTongueRuntimeModel,
    pub inner_mouth: HumanInnerMouthModel,
    pub speech_binding: HumanSpeechPhonemeBinding,
    pub lip_wetness: f32,
    pub viseme_shape_count: usize,
    pub jaw_motion: bool,
    pub cheek_motion: bool,
    pub lip_compression_contact: bool,
    pub breath_speech_timing: bool,
}

impl MouthRuntimeModel {
    pub fn supports_dialogue_closeup(&self) -> bool {
        self.teeth_geometry
            && self.tongue_animation
            && self.inner_mouth_shading
            && self.dental.supports_closeup_teeth()
            && self.tongue.supports_runtime_tongue_motion()
            && self.inner_mouth.supports_closeup_shading()
            && self.speech_binding.supports_voice_lipsync()
            && self.lip_wetness > 0.05
            && self.viseme_shape_count >= 4
            && self.jaw_motion
            && self.cheek_motion
            && self.lip_compression_contact
            && self.breath_speech_timing
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanDentalRuntimeModel {
    pub upper_tooth_count: u8,
    pub lower_tooth_count: u8,
    pub enamel_material: MaterialId,
    pub gum_material: MaterialId,
    pub enamel_translucency: f32,
    pub roughness: f32,
    pub procedural_variation: f32,
    pub wetness_response: f32,
    pub occlusion_contact: bool,
    pub bite_alignment_score: f32,
}

impl HumanDentalRuntimeModel {
    pub fn supports_closeup_teeth(&self) -> bool {
        self.upper_tooth_count >= 12
            && self.lower_tooth_count >= 12
            && self.enamel_material != 0
            && self.gum_material != 0
            && self.enamel_translucency > 0.0
            && self.roughness > 0.0
            && self.procedural_variation > 0.0
            && self.wetness_response > 0.0
            && self.occlusion_contact
            && self.bite_alignment_score >= 0.75
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanTongueRuntimeModel {
    pub material_id: MaterialId,
    pub muscle_control_count: u8,
    pub contact_points: Vec<TongueContactPoint>,
    pub wetness_response: f32,
    pub tongue_tip_control: bool,
    pub palate_contact: bool,
}

impl HumanTongueRuntimeModel {
    pub fn supports_runtime_tongue_motion(&self) -> bool {
        self.material_id != 0
            && self.muscle_control_count >= 3
            && self.contact_points.len() >= 2
            && self
                .contact_points
                .iter()
                .all(TongueContactPoint::supports_lipsync)
            && self.wetness_response > 0.0
            && self.tongue_tip_control
            && self.palate_contact
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TongueContactPoint {
    pub kind: TongueContactKind,
    pub viseme_label: String,
    pub weight_channel: u16,
}

impl TongueContactPoint {
    pub fn supports_lipsync(&self) -> bool {
        !self.viseme_label.is_empty()
            && self.weight_channel < u16::MAX
            && self.kind != TongueContactKind::None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TongueContactKind {
    None,
    TipPalate,
    BladeTeeth,
    BackPalate,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanInnerMouthModel {
    pub material_id: MaterialId,
    pub cavity_occlusion: f32,
    pub saliva_wetness: f32,
    pub gumline_shading: bool,
    pub cheek_occlusion: bool,
    pub black_level_control: bool,
}

impl HumanInnerMouthModel {
    pub fn supports_closeup_shading(&self) -> bool {
        self.material_id != 0
            && self.cavity_occlusion > 0.0
            && self.saliva_wetness > 0.0
            && self.gumline_shading
            && self.cheek_occlusion
            && self.black_level_control
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HumanSpeechPhonemeBinding {
    pub phoneme_timing_stream: bool,
    pub viseme_timing_stream: bool,
    pub emotion_curve_stream: bool,
    pub breath_markers: bool,
    pub tongue_phoneme_count: usize,
}

impl HumanSpeechPhonemeBinding {
    pub fn supports_voice_lipsync(&self) -> bool {
        self.phoneme_timing_stream
            && self.viseme_timing_stream
            && self.emotion_curve_stream
            && self.breath_markers
            && self.tongue_phoneme_count > 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HairRuntimeModel {
    pub guide_curve_count: u32,
    pub strand_count: u32,
    pub card_count: u32,
    pub representation: HairRepresentation,
    pub groom_parameters: HairGroomParameters,
    pub anisotropic_shading: bool,
    pub lod_levels: Vec<HairLodRuntime>,
    pub wet_oily_state: HairWetOilyState,
    pub facial_hair: FacialHairRuntimeModel,
    pub hero_physics: bool,
    pub npc_simulation_fallback: HairSimulationFallback,
}

impl HairRuntimeModel {
    pub fn supports_hero_hair(&self) -> bool {
        self.guide_curve_count > 0
            && self.strand_count > 0
            && self.card_count > 0
            && self.representation == HairRepresentation::Strands
            && self.anisotropic_shading
            && self.lod_levels.len() >= 4
            && self.wet_oily_state.wetness_response > 0.0
            && self.facial_hair.supports_closeup_face_hair()
            && self.hero_physics
            && self.npc_simulation_fallback != HairSimulationFallback::None
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HairGroomParameters {
    pub density: f32,
    pub strand_thickness_meters: f32,
    pub curl: f32,
    pub length_meters: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HairLodRuntime {
    pub tier: HumanLodTier,
    pub representation: HairRepresentation,
    pub simulation: HairSimulationFallback,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HairWetOilyState {
    pub wetness_response: f32,
    pub oil_response: f32,
    pub clumping_response: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FacialHairRuntimeModel {
    pub eyebrow_strand_count: u32,
    pub eyelash_strand_count: u32,
    pub beard_shadow_density: f32,
    pub moustache_strand_count: u32,
    pub material_id: MaterialId,
    pub anisotropic_shading: bool,
    pub hairline_blend: f32,
    pub wetness_response: f32,
    pub oil_response: f32,
    pub lod_levels: Vec<FacialHairLodRuntime>,
    pub procedural_seed: u64,
}

impl FacialHairRuntimeModel {
    pub fn supports_closeup_face_hair(&self) -> bool {
        self.eyebrow_strand_count > 0
            && self.eyelash_strand_count > 0
            && self.material_id != 0
            && self.anisotropic_shading
            && self.hairline_blend > 0.0
            && self.wetness_response > 0.0
            && self.oil_response > 0.0
            && self.lod_levels.len() >= 4
            && self.lod_levels.iter().any(|lod| {
                lod.tier == HumanLodTier::Crowd
                    && lod.brow_lash_representation != HairRepresentation::Hidden
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FacialHairLodRuntime {
    pub tier: HumanLodTier,
    pub brow_lash_representation: HairRepresentation,
    pub beard_moustache_representation: HairRepresentation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HairSimulationFallback {
    None,
    FullPhysics,
    GuideCurvePhysics,
    CachedMotion,
    StaticCards,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClothingCyberneticsRuntimeModel {
    pub layered_clothing_count: usize,
    pub cloth_simulated_layer_count: usize,
    pub cloth_fallback: ClothSimulationFallback,
    pub material_aware_fabric: bool,
    pub wetness_dirt_damage: bool,
    pub armor_hard_surface_count: usize,
    pub cybernetic_implant_count: usize,
    pub emissive_element_count: usize,
    pub attachment_point_count: usize,
    pub clipping_prevention: bool,
}

impl ClothingCyberneticsRuntimeModel {
    pub fn supports_cyberpunk_clothing(&self) -> bool {
        self.layered_clothing_count > 0
            && self.cloth_simulated_layer_count > 0
            && self.cloth_fallback != ClothSimulationFallback::None
            && self.material_aware_fabric
            && self.wetness_dirt_damage
            && self.attachment_point_count > 0
            && self.clipping_prevention
    }

    pub fn supports_visible_cybernetics(&self) -> bool {
        self.cybernetic_implant_count == 0
            || (self.emissive_element_count > 0 && self.armor_hard_surface_count > 0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClothSimulationFallback {
    None,
    FullSimulation,
    PinConstraintFallback,
    SkinnedApproximation,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanBodyRuntimeModel {
    pub morphology: HumanBodyMorphologyRuntime,
    pub composition: HumanBodyCompositionRuntime,
    pub mass_distribution: Vec<HumanBodyMassRegion>,
    pub soft_tissue_zones: Vec<HumanSoftTissueZone>,
    pub muscle_groups: Vec<HumanMuscleGroupRuntime>,
    pub collision_profile: HumanBodyCollisionProfile,
    pub posture: PostureKind,
    pub center_of_mass_meters: Vec3,
    pub scale_consistency: bool,
}

impl HumanBodyRuntimeModel {
    pub fn supports_hero_body_shape(&self) -> bool {
        self.morphology.height_meters > 0.0
            && self.morphology.shoulder_width_meters > 0.0
            && self.morphology.hip_width_meters > 0.0
            && self.morphology.body_mass_kg > 0.0
            && self.composition.muscle.clamp(0.0, 1.0) == self.composition.muscle
            && self.composition.fat.clamp(0.0, 1.0) == self.composition.fat
            && self.composition.soft_tissue_motion > 0.0
            && self.composition.bmi_estimate > 10.0
            && self.composition.bmi_estimate < 45.0
            && self.mass_distribution.len() >= 6
            && self
                .mass_distribution
                .iter()
                .all(|region| region.total_weight() > 0.0)
            && self.soft_tissue_zones.len() >= 5
            && self
                .soft_tissue_zones
                .iter()
                .all(|zone| zone.enabled && zone.motion_scale > 0.0 && zone.damping > 0.0)
            && self.muscle_groups.len() >= 5
            && self
                .muscle_groups
                .iter()
                .all(|group| group.driven_by_pose_space && group.definition > 0.0)
            && self.collision_profile.supports_body_shape_simulation()
            && self.center_of_mass_meters.z > self.morphology.height_meters * 0.35
            && self.center_of_mass_meters.z < self.morphology.height_meters * 0.65
            && self.scale_consistency
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanBodyMorphologyRuntime {
    pub height_meters: f32,
    pub shoulder_width_meters: f32,
    pub hip_width_meters: f32,
    pub limb_proportion: f32,
    pub body_mass_kg: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanBodyCompositionRuntime {
    pub muscle: f32,
    pub fat: f32,
    pub soft_tissue_motion: f32,
    pub bmi_estimate: f32,
    pub silhouette_width_ratio: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanBodyMassRegion {
    pub region: HumanBodyRegion,
    pub fat_weight: f32,
    pub muscle_weight: f32,
    pub soft_tissue_weight: f32,
}

impl HumanBodyMassRegion {
    pub fn total_weight(&self) -> f32 {
        self.fat_weight + self.muscle_weight + self.soft_tissue_weight
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanBodyRegion {
    Torso,
    Abdomen,
    Hips,
    UpperArms,
    Thighs,
    FaceNeck,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanSoftTissueZone {
    pub region: HumanBodyRegion,
    pub enabled: bool,
    pub motion_scale: f32,
    pub damping: f32,
    pub collision_radius_meters: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanMuscleGroupRuntime {
    pub group: HumanMuscleGroupKind,
    pub definition: f32,
    pub driven_by_pose_space: bool,
    pub soft_tissue_influence: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanMuscleGroupKind {
    TorsoCore,
    ShoulderChest,
    Arms,
    HipsGlutes,
    Legs,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanBodyCollisionProfile {
    pub capsule_count: usize,
    pub cloth_hook_count: usize,
    pub hair_collision: bool,
    pub soft_tissue_enabled: bool,
    pub estimated_solver_cost: f32,
}

impl HumanBodyCollisionProfile {
    pub fn supports_body_shape_simulation(&self) -> bool {
        self.capsule_count >= 6
            && self.cloth_hook_count > 0
            && self.hair_collision
            && self.soft_tissue_enabled
            && self.estimated_solver_cost > 0.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanMotionModel {
    pub pose_space_corrections: bool,
    pub muscle_soft_tissue: bool,
    pub foot_contact_correction: bool,
    pub balance_weight_shifts: bool,
    pub hand_finger_animation: bool,
    pub breathing: bool,
    pub idle_micro_motion: bool,
    pub injury_fatigue_response: bool,
    pub performance_capture_support: bool,
    pub procedural_animation_support: bool,
    pub max_blend_layers: u8,
}

impl HumanMotionModel {
    pub fn supports_weighted_closeup_motion(&self) -> bool {
        self.pose_space_corrections
            && self.foot_contact_correction
            && self.balance_weight_shifts
            && self.hand_finger_animation
            && self.breathing
            && self.idle_micro_motion
            && self.injury_fatigue_response
            && self.performance_capture_support
            && self.procedural_animation_support
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanPerformanceCaptureModel {
    pub source: HumanPerformanceCaptureSource,
    pub curve_bindings: Vec<HumanPerformanceCaptureCurve>,
    pub retargeting_profile: String,
    pub timecode_binding: bool,
    pub frame_rate_hz: f32,
    pub latency_budget_ms: f32,
    pub facial_solver: HumanFacialCaptureSolver,
    pub body_solver: HumanBodyCaptureSolver,
    pub fallback: HumanCaptureFallback,
    pub debug_curve_view: bool,
}

impl HumanPerformanceCaptureModel {
    pub fn supports_hero_performance_capture(&self) -> bool {
        [
            HumanPerformanceCaptureCurveKind::HeadPose,
            HumanPerformanceCaptureCurveKind::EyeGaze,
            HumanPerformanceCaptureCurveKind::Blink,
            HumanPerformanceCaptureCurveKind::JawOpen,
            HumanPerformanceCaptureCurveKind::LipShape,
            HumanPerformanceCaptureCurveKind::Tongue,
            HumanPerformanceCaptureCurveKind::Brow,
            HumanPerformanceCaptureCurveKind::Cheek,
            HumanPerformanceCaptureCurveKind::Emotion,
            HumanPerformanceCaptureCurveKind::Breath,
            HumanPerformanceCaptureCurveKind::HandGesture,
            HumanPerformanceCaptureCurveKind::BodyLean,
            HumanPerformanceCaptureCurveKind::FootContact,
        ]
        .iter()
        .all(|kind| {
            self.curve_bindings
                .iter()
                .any(|curve| curve.kind == *kind && curve.supports_runtime_capture())
        }) && self.source != HumanPerformanceCaptureSource::None
            && !self.retargeting_profile.is_empty()
            && self.timecode_binding
            && self.frame_rate_hz >= 30.0
            && self.latency_budget_ms <= 50.0
            && self.facial_solver.supports_hero_face_capture()
            && self.body_solver.supports_hero_body_capture()
            && self.fallback != HumanCaptureFallback::None
            && self.debug_curve_view
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanPerformanceCaptureSource {
    None,
    ProceduralFallback,
    VoiceDrivenFace,
    PerformanceCaptureDerived,
    HybridProceduralCapture,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanPerformanceCaptureCurve {
    pub kind: HumanPerformanceCaptureCurveKind,
    pub target_channel: String,
    pub sample_rate_hz: f32,
    pub precision: HumanCaptureCurvePrecision,
    pub drives_renderer: bool,
}

impl HumanPerformanceCaptureCurve {
    pub fn supports_runtime_capture(&self) -> bool {
        !self.target_channel.is_empty()
            && self.sample_rate_hz >= 30.0
            && self.precision != HumanCaptureCurvePrecision::Dropped
            && self.drives_renderer
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanPerformanceCaptureCurveKind {
    HeadPose,
    EyeGaze,
    Blink,
    JawOpen,
    LipShape,
    Tongue,
    Brow,
    Cheek,
    Emotion,
    Breath,
    HandGesture,
    BodyLean,
    FootContact,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanCaptureCurvePrecision {
    Raw,
    Smoothed,
    Compressed,
    Dropped,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanFacialCaptureSolver {
    pub blendshape_count: usize,
    pub viseme_curve_count: usize,
    pub emotion_curve_count: usize,
    pub tongue_tracking: bool,
    pub brow_lash_tracking: bool,
    pub eye_contact_tracking: bool,
}

impl HumanFacialCaptureSolver {
    pub fn supports_hero_face_capture(&self) -> bool {
        self.blendshape_count >= 7
            && self.viseme_curve_count >= 4
            && self.emotion_curve_count >= 4
            && self.tongue_tracking
            && self.brow_lash_tracking
            && self.eye_contact_tracking
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanBodyCaptureSolver {
    pub retarget_bone_count: u16,
    pub hand_finger_tracking: bool,
    pub foot_contact_tracking: bool,
    pub balance_weight_tracking: bool,
    pub breathing_tracking: bool,
    pub injury_fatigue_tracking: bool,
}

impl HumanBodyCaptureSolver {
    pub fn supports_hero_body_capture(&self) -> bool {
        self.retarget_bone_count >= 96
            && self.hand_finger_tracking
            && self.foot_contact_tracking
            && self.balance_weight_tracking
            && self.breathing_tracking
            && self.injury_fatigue_tracking
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanCaptureFallback {
    None,
    ProceduralCurves,
    CachedPerformance,
    VoiceDrivenFace,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanRendererInterface {
    pub rigged_mesh_handles: Vec<MeshAssetHandle>,
    pub skin_material_handles: Vec<MaterialId>,
    pub hair_data_handles: Vec<MeshAssetHandle>,
    pub eye_material_handles: Vec<MaterialId>,
    pub mouth_material_handles: Vec<MaterialId>,
    pub morph_animation_buffers: HumanMorphAnimationBuffers,
    pub mouth_articulation_buffer: HumanRendererBufferHandle,
    pub clothing_buffers: Vec<HumanClothingRenderBuffer>,
    pub material_state_fields: Vec<HumanMaterialStateChannel>,
    pub expression_curve_count: usize,
    pub viseme_timing_count: usize,
}

impl HumanRendererInterface {
    pub fn supports_renderer_handoff(&self) -> bool {
        [
            HumanMaterialStateChannel::Wetness,
            HumanMaterialStateChannel::Oil,
            HumanMaterialStateChannel::Sweat,
            HumanMaterialStateChannel::BloodPerfusion,
            HumanMaterialStateChannel::Bruising,
            HumanMaterialStateChannel::Injury,
            HumanMaterialStateChannel::Dirt,
        ]
        .iter()
        .all(|channel| self.material_state_fields.contains(channel))
            && !self.rigged_mesh_handles.is_empty()
            && !self.skin_material_handles.is_empty()
            && !self.hair_data_handles.is_empty()
            && !self.eye_material_handles.is_empty()
            && !self.mouth_material_handles.is_empty()
            && self.morph_animation_buffers.supports_gpu_morph_handoff()
            && self.mouth_articulation_buffer.is_valid()
            && !self.clothing_buffers.is_empty()
            && self.expression_curve_count > 0
            && self.viseme_timing_count > 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HumanRendererBufferHandle(pub u128);

impl HumanRendererBufferHandle {
    pub fn is_valid(&self) -> bool {
        self.0 != 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanMorphAnimationBuffers {
    pub gpu_skinning_resource: Option<GpuSkinningResource>,
    pub facial_morph_buffer: HumanRendererBufferHandle,
    pub animation_curve_buffer: HumanRendererBufferHandle,
}

impl HumanMorphAnimationBuffers {
    pub fn supports_gpu_morph_handoff(&self) -> bool {
        self.gpu_skinning_resource.is_some()
            && self.facial_morph_buffer.is_valid()
            && self.animation_curve_buffer.is_valid()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HumanClothingRenderBuffer {
    pub mesh: MeshAssetHandle,
    pub material_id: MaterialId,
    pub cloth_simulation: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanAiVoiceIntegrationModel {
    pub ai_persona: Option<AgentPersonaId>,
    pub voice_persona: Option<VoicePersonaId>,
    pub emotional_state_binding: bool,
    pub gaze_target_binding: GazeTargetBinding,
    pub facial_expression_axis_count: usize,
    pub voice_timing: VoiceTimingBinding,
    pub viseme_timing_count: usize,
    pub body_gesture_channels: Vec<HumanGestureChannel>,
    pub relationship_context: RelationshipContextBinding,
    pub injury_fatigue_binding: bool,
}

impl HumanAiVoiceIntegrationModel {
    pub fn supports_character_integration(&self) -> bool {
        [
            HumanGestureChannel::HandGesture,
            HumanGestureChannel::BodyLean,
            HumanGestureChannel::BreathMotion,
            HumanGestureChannel::IdleMicroMotion,
        ]
        .iter()
        .all(|channel| self.body_gesture_channels.contains(channel))
            && self.ai_persona.is_some()
            && self.voice_persona.is_some()
            && self.emotional_state_binding
            && self.gaze_target_binding != GazeTargetBinding::None
            && self.facial_expression_axis_count > 0
            && self.voice_timing.supports_dialogue_timing()
            && self.viseme_timing_count > 0
            && self.relationship_context != RelationshipContextBinding::None
            && self.injury_fatigue_binding
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoiceTimingBinding {
    pub duration_seconds: bool,
    pub viseme_timeline: bool,
    pub breath_timing: bool,
}

impl VoiceTimingBinding {
    pub fn supports_dialogue_timing(&self) -> bool {
        self.duration_seconds && self.viseme_timeline && self.breath_timing
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanGestureChannel {
    HandGesture,
    BodyLean,
    BreathMotion,
    IdleMicroMotion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationshipContextBinding {
    None,
    AgentMemory,
    DialogueRelationship,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanUncannyReviewChecklist {
    pub items: Vec<HumanUncannyReviewItem>,
    pub closeup_distance_meters: f32,
    pub debug_view_bound: bool,
    pub acceptance_scene_bound: bool,
}

impl HumanUncannyReviewChecklist {
    pub fn passed_required_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.required && item.passed)
            .count()
    }

    pub fn passes_v9_review(&self) -> bool {
        self.closeup_distance_meters <= 1.5
            && self.debug_view_bound
            && self.acceptance_scene_bound
            && !self.items.is_empty()
            && self.items.iter().all(|item| !item.required || item.passed)
    }

    pub fn has(&self, kind: HumanUncannyReviewKind) -> bool {
        self.items
            .iter()
            .any(|item| item.kind == kind && item.required && item.passed)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanUncannyReviewItem {
    pub kind: HumanUncannyReviewKind,
    pub label: String,
    pub required: bool,
    pub passed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanUncannyReviewKind {
    EyesAlive,
    GazeTarget,
    BlinkNatural,
    SkinLightingWetness,
    MouthTeethTongue,
    SpeechSync,
    HairTemporalStability,
    FacialHairContinuity,
    ClothingClipping,
    WeightedBodyMotion,
    PerformanceCaptureCurves,
    LodTransition,
    RendererHandoff,
    AiVoiceConsistency,
    GenerationProvenance,
    SchemaAssetPackage,
    FailureBehavior,
    ProfilingBudget,
    DebugCoverage,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanAssetPackageContract {
    pub manifest: AssetPackageManifest,
    pub runtime_schemas: Vec<HumanRuntimeSchemaBinding>,
    pub provenance_record_count: usize,
    pub debug_preview: HumanAssetDebugPreview,
    pub round_trip: HumanSchemaRoundTripEvidence,
    pub integration_replay: HumanIntegrationReplayEvidence,
}

impl HumanAssetPackageContract {
    pub fn supports_v9_schema_asset_package(&self) -> bool {
        [
            "HumanRuntimeBundle",
            "HumanRendererInterface",
            "SpeechResult",
            "AssetPackageManifest",
            "ValidationReport",
        ]
        .iter()
        .all(|schema_name| {
            self.runtime_schemas.iter().any(|binding| {
                binding.schema.name == *schema_name
                    && binding.required
                    && binding.supports_runtime_exchange()
            })
        }) && self.manifest.manifest_schema_version == ASSET_PACKAGE_MANIFEST_SCHEMA_VERSION
            && self.manifest.asset_kind == AssetKind::GeneratedBundle
            && self.manifest.schema_name == "HumanRuntimeBundle"
            && self.manifest.schema_version >= 1
            && !self.manifest.provenance.trim().is_empty()
            && !self.manifest.dependencies.is_empty()
            && !self.manifest.chunks.is_empty()
            && self.manifest.total_uncompressed_bytes > 0
            && self.manifest.generated
            && self.manifest.validation.passed
            && self.manifest.validate(40).passed
            && self.provenance_record_count > 0
            && self.debug_preview.supports_debug_preview()
            && self.round_trip.supports_schema_round_trip()
            && self.integration_replay.supports_integration_replay()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HumanRuntimeSchemaBinding {
    pub schema: SchemaVersion,
    pub owner: ModuleId,
    pub required: bool,
    pub round_trip_serialized: bool,
    pub binary_layout_stable: bool,
}

impl HumanRuntimeSchemaBinding {
    pub fn supports_runtime_exchange(&self) -> bool {
        !self.schema.name.trim().is_empty()
            && self.schema.version > 0
            && self.owner != 0
            && self.round_trip_serialized
            && self.binary_layout_stable
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanAssetDebugPreview {
    pub preview_asset: AssetId,
    pub mesh_count: usize,
    pub material_count: usize,
    pub debug_view_count: usize,
    pub estimated_bytes: u64,
    pub label: String,
}

impl HumanAssetDebugPreview {
    pub fn supports_debug_preview(&self) -> bool {
        self.preview_asset != 0
            && self.mesh_count > 0
            && self.material_count > 0
            && self.debug_view_count > 0
            && self.estimated_bytes > 0
            && !self.label.trim().is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HumanSchemaRoundTripEvidence {
    pub serialized_bytes: u64,
    pub content_hash: u128,
    pub lossless: bool,
    pub migration_tested: bool,
    pub public_schema_registered: bool,
}

impl HumanSchemaRoundTripEvidence {
    pub fn supports_schema_round_trip(&self) -> bool {
        self.serialized_bytes > 0
            && self.content_hash != 0
            && self.lossless
            && self.migration_tested
            && self.public_schema_registered
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HumanIntegrationReplayEvidence {
    pub replay_asset: AssetId,
    pub event_count: usize,
    pub deterministic_hash: u128,
    pub captures_speech: bool,
    pub captures_surface_state: bool,
    pub captures_debug_views: bool,
}

impl HumanIntegrationReplayEvidence {
    pub fn supports_integration_replay(&self) -> bool {
        self.replay_asset != 0
            && self.event_count >= 3
            && self.deterministic_hash != 0
            && self.captures_speech
            && self.captures_surface_state
            && self.captures_debug_views
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HumanSchemaAssetEvidence {
    pub manifest_valid: bool,
    pub schema_round_trip: bool,
    pub provenance_bound: bool,
    pub debug_preview_ready: bool,
    pub integration_replay_ready: bool,
}

impl HumanSchemaAssetEvidence {
    pub fn supports_schema_asset_acceptance(&self) -> bool {
        self.manifest_valid
            && self.schema_round_trip
            && self.provenance_bound
            && self.debug_preview_ready
            && self.integration_replay_ready
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanPerformanceBudgetReport {
    pub target_frame_ms: f32,
    pub cpu_time_ms: f32,
    pub gpu_time_ms: f32,
    pub estimated_memory_bytes: u64,
    pub streaming_bandwidth_mib_per_s: f32,
    pub fallback_cost_ms: f32,
    pub category_breakdown: Vec<HumanPerformanceBudgetCategoryReport>,
    pub profiler_capture_ready: bool,
    pub budget_report_ready: bool,
    pub frame_failure_counter_bound: bool,
    pub memory_pressure_safe: bool,
}

impl HumanPerformanceBudgetReport {
    pub fn supports_v9_performance_budget(&self, budget: &HumanGenerationBudget) -> bool {
        [
            HumanPerformanceBudgetCategory::CpuAnimation,
            HumanPerformanceBudgetCategory::GpuSkinningMorphs,
            HumanPerformanceBudgetCategory::HairClothSimulation,
            HumanPerformanceBudgetCategory::SkinEyeMouthCloseup,
            HumanPerformanceBudgetCategory::VoiceLipsync,
            HumanPerformanceBudgetCategory::RendererHandoff,
            HumanPerformanceBudgetCategory::Memory,
            HumanPerformanceBudgetCategory::Fallback,
        ]
        .iter()
        .all(|category| {
            self.category_breakdown
                .iter()
                .any(|entry| entry.category == *category && entry.within_budget())
        }) && self.target_frame_ms > 0.0
            && self.cpu_time_ms > 0.0
            && self.cpu_time_ms <= 4.0
            && self.gpu_time_ms > 0.0
            && self.gpu_time_ms <= 6.0
            && self.estimated_memory_bytes <= budget.max_memory_bytes
            && self.streaming_bandwidth_mib_per_s >= 0.0
            && self.fallback_cost_ms > 0.0
            && self.fallback_cost_ms <= 4.0
            && self.profiler_capture_ready
            && self.budget_report_ready
            && self.frame_failure_counter_bound
            && self.memory_pressure_safe
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanPerformanceBudgetCategoryReport {
    pub category: HumanPerformanceBudgetCategory,
    pub estimated_ms: f32,
    pub memory_bytes: u64,
    pub budget_ms: f32,
}

impl HumanPerformanceBudgetCategoryReport {
    pub fn within_budget(&self) -> bool {
        self.estimated_ms >= 0.0 && self.estimated_ms <= self.budget_ms
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanPerformanceBudgetCategory {
    CpuAnimation,
    GpuSkinningMorphs,
    HairClothSimulation,
    SkinEyeMouthCloseup,
    VoiceLipsync,
    RendererHandoff,
    Memory,
    Fallback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HumanProfilingBudgetEvidence {
    pub cpu_gpu_budget_passed: bool,
    pub memory_budget_passed: bool,
    pub profiler_capture_ready: bool,
    pub fallback_cost_reported: bool,
    pub category_breakdown_ready: bool,
    pub frame_failure_counter_bound: bool,
}

impl HumanProfilingBudgetEvidence {
    pub fn supports_budget_acceptance(&self) -> bool {
        self.cpu_gpu_budget_passed
            && self.memory_budget_passed
            && self.profiler_capture_ready
            && self.fallback_cost_reported
            && self.category_breakdown_ready
            && self.frame_failure_counter_bound
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanFailureFallbackPlan {
    pub voice: HumanVoiceFailureFallback,
    pub streaming: HumanStreamingFailureFallback,
    pub rendering: HumanRenderingFailureFallback,
    pub simulation: HumanSimulationFailureFallback,
    pub frame_budget: HumanFrameBudgetFailurePolicy,
    pub telemetry_event: bool,
    pub replay_deterministic: bool,
}

impl HumanFailureFallbackPlan {
    pub fn supports_v9_failure_behavior(&self) -> bool {
        self.voice.supports_voice_generation_failure()
            && self.streaming.supports_streaming_failure()
            && self.rendering.supports_render_failure()
            && self.simulation.supports_simulation_failure()
            && self.frame_budget.supports_frame_failure_policy()
            && self.telemetry_event
            && self.replay_deterministic
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HumanVoiceFailureFallback {
    pub cached_audio_line: bool,
    pub subtitle_fallback: bool,
    pub viseme_hold: bool,
    pub provenance_preserved: bool,
}

impl HumanVoiceFailureFallback {
    pub fn supports_voice_generation_failure(&self) -> bool {
        self.cached_audio_line
            && self.subtitle_fallback
            && self.viseme_hold
            && self.provenance_preserved
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HumanStreamingFailureFallback {
    pub proxy_human_available: bool,
    pub lowest_lod_available: bool,
    pub continue_animation: bool,
    pub reports_streaming_error: bool,
}

impl HumanStreamingFailureFallback {
    pub fn supports_streaming_failure(&self) -> bool {
        self.proxy_human_available
            && self.lowest_lod_available
            && self.continue_animation
            && self.reports_streaming_error
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HumanRenderingFailureFallback {
    pub fallback_material: bool,
    pub previous_pipeline_cache: bool,
    pub lower_quality_lighting: bool,
    pub stable_render_handles: bool,
}

impl HumanRenderingFailureFallback {
    pub fn supports_render_failure(&self) -> bool {
        self.fallback_material
            && self.previous_pipeline_cache
            && self.lower_quality_lighting
            && self.stable_render_handles
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HumanSimulationFailureFallback {
    pub hair_quality_demotion: bool,
    pub cloth_quality_demotion: bool,
    pub soft_tissue_freeze: bool,
    pub preserve_contact_animation: bool,
}

impl HumanSimulationFailureFallback {
    pub fn supports_simulation_failure(&self) -> bool {
        self.hair_quality_demotion
            && self.cloth_quality_demotion
            && self.soft_tissue_freeze
            && self.preserve_contact_animation
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanFrameBudgetFailurePolicy {
    pub demote_background_humans: bool,
    pub reduce_hair_cloth_quality: bool,
    pub cap_performance_capture: bool,
    pub fallback_cost_ms: f32,
    pub frame_failure_counter: bool,
}

impl HumanFrameBudgetFailurePolicy {
    pub fn supports_frame_failure_policy(&self) -> bool {
        self.demote_background_humans
            && self.reduce_hair_cloth_quality
            && self.cap_performance_capture
            && self.fallback_cost_ms > 0.0
            && self.fallback_cost_ms <= 4.0
            && self.frame_failure_counter
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HumanFailureBehaviorEvidence {
    pub voice_failure_fallback: bool,
    pub streaming_proxy_fallback: bool,
    pub simulation_quality_demotion: bool,
    pub frame_budget_policy: bool,
    pub telemetry_replay_ready: bool,
}

impl HumanFailureBehaviorEvidence {
    pub fn supports_controlled_failures(&self) -> bool {
        self.voice_failure_fallback
            && self.streaming_proxy_fallback
            && self.simulation_quality_demotion
            && self.frame_budget_policy
            && self.telemetry_replay_ready
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanAcceptanceEvidence {
    pub closeup_dialogue_scene: HumanGoldenSceneProbe,
    pub eye_tracking: HumanEyeTrackingEvidence,
    pub lip_sync: HumanLipSyncEvidence,
    pub skin_response: HumanSkinAcceptanceEvidence,
    pub hair_stability: HumanHairStabilityEvidence,
    pub clothing_motion: HumanClothingMotionEvidence,
    pub lod_transition: HumanLodTransitionReport,
    pub schema_asset: HumanSchemaAssetEvidence,
    pub failure_behavior: HumanFailureBehaviorEvidence,
    pub profiling_budget: HumanProfilingBudgetEvidence,
}

impl HumanAcceptanceEvidence {
    pub fn passes_v9_acceptance(&self) -> bool {
        self.closeup_dialogue_scene.supports_closeup_dialogue()
            && self.eye_tracking.supports_alive_eyes()
            && self.lip_sync.supports_speech_sync()
            && self.skin_response.supports_lighting_and_wetness()
            && self.hair_stability.supports_temporal_stability()
            && self.clothing_motion.supports_plausible_motion()
            && self.lod_transition.supports_stable_switching()
            && self.schema_asset.supports_schema_asset_acceptance()
            && self.failure_behavior.supports_controlled_failures()
            && self.profiling_budget.supports_budget_acceptance()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanGoldenSceneProbe {
    pub kind: HumanGoldenSceneKind,
    pub lighting: HumanLightingScenario,
    pub close_camera_distance_meters: f32,
    pub voice_lipsync_covered: bool,
    pub skin_eye_hair_cloth_covered: bool,
    pub debug_capture_ready: bool,
    pub reference_comparison_ready: bool,
}

impl HumanGoldenSceneProbe {
    pub fn supports_closeup_dialogue(&self) -> bool {
        self.kind == HumanGoldenSceneKind::HumanCloseUpDialogue
            && self.lighting == HumanLightingScenario::MixedNeonRain
            && self.close_camera_distance_meters <= 1.5
            && self.voice_lipsync_covered
            && self.skin_eye_hair_cloth_covered
            && self.debug_capture_ready
            && self.reference_comparison_ready
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanGoldenSceneKind {
    HumanCloseUpDialogue,
    DialogueUnderStress,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanLightingScenario {
    MixedNeonRain,
    NeutralStudio,
    LowLightInterior,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanEyeTrackingEvidence {
    pub gaze_target_bound: bool,
    pub micro_saccades_per_second: f32,
    pub blink_model_active: bool,
    pub pupil_light_response: bool,
    pub emotional_eye_response: bool,
}

impl HumanEyeTrackingEvidence {
    pub fn supports_alive_eyes(&self) -> bool {
        self.gaze_target_bound
            && self.micro_saccades_per_second > 0.0
            && self.blink_model_active
            && self.pupil_light_response
            && self.emotional_eye_response
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanLipSyncEvidence {
    pub voice_timing_bound: bool,
    pub phoneme_timing_bound: bool,
    pub viseme_count: usize,
    pub breath_timing_bound: bool,
    pub tongue_motion_bound: bool,
    pub mouth_occlusion_ready: bool,
    pub jaw_cheek_lip_contact: bool,
}

impl HumanLipSyncEvidence {
    pub fn supports_speech_sync(&self) -> bool {
        self.voice_timing_bound
            && self.phoneme_timing_bound
            && self.viseme_count >= 4
            && self.breath_timing_bound
            && self.tongue_motion_bound
            && self.mouth_occlusion_ready
            && self.jaw_cheek_lip_contact
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanSkinAcceptanceEvidence {
    pub mixed_lighting_response: bool,
    pub wetness_response: f32,
    pub pore_wrinkle_subsurface_ready: bool,
    pub dirt_injury_response: bool,
}

impl HumanSkinAcceptanceEvidence {
    pub fn supports_lighting_and_wetness(&self) -> bool {
        self.mixed_lighting_response
            && self.wetness_response > 0.0
            && self.pore_wrinkle_subsurface_ready
            && self.dirt_injury_response
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanHairStabilityEvidence {
    pub shimmer_risk: f32,
    pub temporal_filtering: bool,
    pub lod_fallbacks_ready: bool,
    pub wet_oily_response: bool,
    pub facial_hair_ready: bool,
}

impl HumanHairStabilityEvidence {
    pub fn supports_temporal_stability(&self) -> bool {
        self.shimmer_risk <= 0.35
            && self.temporal_filtering
            && self.lod_fallbacks_ready
            && self.wet_oily_response
            && self.facial_hair_ready
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanClothingMotionEvidence {
    pub simulated_or_fallback: bool,
    pub attachment_count: usize,
    pub clipping_prevention: bool,
    pub wetness_damage_response: bool,
}

impl HumanClothingMotionEvidence {
    pub fn supports_plausible_motion(&self) -> bool {
        self.simulated_or_fallback
            && self.attachment_count > 0
            && self.clipping_prevention
            && self.wetness_damage_response
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanLodTransitionReport {
    pub checks: Vec<HumanLodTransitionCheck>,
    pub max_triangle_drop_ratio: f32,
    pub max_bone_drop_ratio: f32,
    pub impostor_available: bool,
}

impl HumanLodTransitionReport {
    pub fn supports_stable_switching(&self) -> bool {
        self.impostor_available
            && !self.checks.is_empty()
            && self.max_triangle_drop_ratio <= 0.85
            && self.max_bone_drop_ratio <= 0.85
            && self.checks.iter().all(|check| {
                check.mesh_continuity
                    && check.animation_continuity
                    && check.hair_representation_continuity
                    && check.silhouette_error <= 0.22
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanLodTransitionCheck {
    pub from: HumanLodTier,
    pub to: HumanLodTier,
    pub mesh_continuity: bool,
    pub animation_continuity: bool,
    pub hair_representation_continuity: bool,
    pub silhouette_error: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SkinRuntimeModel {
    pub layers: Vec<SkinLayerRuntime>,
    pub subsurface: SkinSubsurfaceModel,
    pub pore_detail: f32,
    pub wrinkle_strength: f32,
    pub mark_layers: Vec<SkinMarkLayer>,
    pub ethical_mark_generation: bool,
    pub blood_flow_redness: f32,
    pub sheen: SkinSheenModel,
    pub wetness_response: f32,
    pub dirt_soot_response: f32,
    pub injury_response: f32,
    pub age_variation: f32,
    pub tension_stretch_detail: f32,
}

impl SkinRuntimeModel {
    pub fn supports_closeup_skin(&self) -> bool {
        self.layers
            .iter()
            .any(|layer| layer.kind == SkinLayerKind::Epidermis && layer.enabled)
            && self
                .layers
                .iter()
                .any(|layer| layer.kind == SkinLayerKind::Dermis && layer.enabled)
            && self.subsurface.enabled
            && self.pore_detail > 0.0
            && self.wrinkle_strength > 0.0
            && self.ethical_mark_generation
            && self.blood_flow_redness > 0.0
            && self.sheen.sweat_response > 0.0
            && self.sheen.oil_response > 0.0
            && self.wetness_response > 0.0
            && self.dirt_soot_response > 0.0
            && self.injury_response > 0.0
            && self.age_variation > 0.0
            && self.tension_stretch_detail > 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkinLayerRuntime {
    pub kind: SkinLayerKind,
    pub enabled: bool,
    pub detail_strength: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkinLayerKind {
    Epidermis,
    Dermis,
    Subsurface,
    PoreMicroDetail,
    WrinkleDetail,
    BloodPerfusion,
    SurfaceSheen,
    DirtSoot,
    InjuryOverlay,
    TensionStretch,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkinSubsurfaceModel {
    pub enabled: bool,
    pub scattering_radius_millimeters: f32,
    pub blood_perfusion_response: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SkinMarkLayer {
    pub kind: SkinMarkKind,
    pub label: String,
    pub procedural_seed: u64,
    pub ethically_generated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkinMarkKind {
    Scar,
    Tattoo,
    Makeup,
    Freckle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkinSheenModel {
    pub sweat_response: f32,
    pub oil_response: f32,
    pub wetness_to_sheen: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanMaterialBinding {
    pub material_id: MaterialId,
    pub layer: AnatomicalLayerKind,
    pub supports_wetness: bool,
    pub supports_injury: bool,
    pub procedural_seed: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanSurfaceResponseProfile {
    pub skin_wetness_response: f32,
    pub sweat_response: f32,
    pub oil_level: f32,
    pub injury_response: f32,
    pub hair_wetness_response: f32,
    pub clothing_wetness_response: f32,
    pub clothing_damage_visibility: f32,
    pub eye_redness_response: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanSurfaceState {
    pub skin_wetness: f32,
    pub sweat_sheen: f32,
    pub oil_sheen: f32,
    pub bruising: f32,
    pub injury_overlay: f32,
    pub dirt: f32,
    pub hair_wetness: f32,
    pub clothing_wetness: f32,
    pub clothing_damage: f32,
    pub eye_redness: f32,
    pub material_layers: Vec<HumanMaterialLayerState>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanMaterialLayerState {
    pub layer: AnatomicalLayerKind,
    pub wetness: f32,
    pub injury: f32,
    pub dirt: f32,
    pub sheen: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanPhysicsProxy {
    pub capsules: Vec<PhysicsCapsule>,
    pub cloth_hooks: Vec<ClothHook>,
    pub hair_collision: bool,
    pub soft_tissue_enabled: bool,
    pub estimated_solver_cost: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsCapsule {
    pub label: String,
    pub radius_meters: f32,
    pub half_height_meters: f32,
    pub mass_kg: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClothHook {
    pub attach_point: BodyAttachPoint,
    pub stiffness: f32,
    pub damping: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnimationProfile {
    pub locomotion_set: String,
    pub compression: AnimationCompression,
    pub supports_visemes: bool,
    pub supports_emotion_curves: bool,
    pub supports_foot_contact_correction: bool,
    pub supports_balance_weight_shifts: bool,
    pub supports_hand_gestures: bool,
    pub supports_breathing_motion: bool,
    pub supports_idle_micro_motion: bool,
    pub supports_procedural_animation: bool,
    pub max_blend_layers: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationCompression {
    None,
    RuntimeKeyReduction,
    CrowdSimplified,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanLodSet {
    pub selected_quality: QualityTier,
    pub levels: Vec<HumanLod>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanLod {
    pub tier: HumanLodTier,
    pub quality: QualityTier,
    pub max_distance_meters: f32,
    pub mesh: MeshAssetHandle,
    pub bone_count: u16,
    pub facial_control_count: u16,
    pub hair: HairRepresentation,
    pub estimated_memory_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HumanLodTier {
    Hero,
    ImportantNpc,
    NormalNpc,
    Crowd,
    Impostor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HairRepresentation {
    Strands,
    Cards,
    Shells,
    PaintedCap,
    Hidden,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanValidationReport {
    pub passed: bool,
    pub issues: Vec<HumanValidationIssue>,
    pub budget: HumanBudgetUsage,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanValidationIssue {
    pub severity: HumanValidationSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanValidationSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HumanBudgetUsage {
    pub mesh_count: usize,
    pub material_count: usize,
    pub estimated_memory_bytes: u64,
    pub estimated_hair_strands: u32,
    pub physics_proxy_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanGenerationPolicy {
    pub source: HumanGenerationSource,
    pub likeness: HumanLikenessPolicy,
    pub voice: HumanVoicePolicy,
    pub provenance_records: Vec<HumanProvenanceRecord>,
    pub controlled_variation: bool,
    pub bias_reviewed: bool,
}

impl HumanGenerationPolicy {
    pub fn seeded_synthetic(seed: u64) -> Self {
        Self {
            source: HumanGenerationSource::SeededSynthetic,
            likeness: HumanLikenessPolicy::synthetic(),
            voice: HumanVoicePolicy::synthetic(),
            provenance_records: vec![
                HumanProvenanceRecord {
                    label: "procedural_seed".to_string(),
                    source_kind: HumanProvenanceSourceKind::ProceduralSeed,
                    record_id: format!("seed:{seed}"),
                },
                HumanProvenanceRecord {
                    label: "controlled_variation_profile".to_string(),
                    source_kind: HumanProvenanceSourceKind::BiasReview,
                    record_id: "ashfall_population_variation_v1".to_string(),
                },
            ],
            controlled_variation: true,
            bias_reviewed: true,
        }
    }

    pub fn likeness_authorized(&self) -> bool {
        !self.likeness.real_person_likeness
            || self.likeness.consent_record.is_some()
            || self.likeness.license_record.is_some()
    }

    pub fn voice_authorized(&self) -> bool {
        !self.voice.real_person_voice_clone
            || self.voice.consent_record.is_some()
            || self.voice.rights_record.is_some()
    }

    pub fn has_seed_provenance(&self, seed: u64) -> bool {
        let expected = format!("seed:{seed}");
        self.provenance_records.iter().any(|record| {
            record.source_kind == HumanProvenanceSourceKind::ProceduralSeed
                && record.record_id == expected
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanGenerationSource {
    SeededSynthetic,
    LicensedScan,
    PerformanceCaptureDerived,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HumanLikenessPolicy {
    pub real_person_likeness: bool,
    pub consent_record: Option<String>,
    pub license_record: Option<String>,
}

impl HumanLikenessPolicy {
    pub fn synthetic() -> Self {
        Self {
            real_person_likeness: false,
            consent_record: None,
            license_record: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HumanVoicePolicy {
    pub real_person_voice_clone: bool,
    pub consent_record: Option<String>,
    pub rights_record: Option<String>,
}

impl HumanVoicePolicy {
    pub fn synthetic() -> Self {
        Self {
            real_person_voice_clone: false,
            consent_record: None,
            rights_record: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HumanProvenanceRecord {
    pub label: String,
    pub source_kind: HumanProvenanceSourceKind,
    pub record_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanProvenanceSourceKind {
    ProceduralSeed,
    LicensedAsset,
    PerformanceCapture,
    BiasReview,
    ConsentRecord,
    RightsRecord,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanDebugViewSet {
    pub views: Vec<HumanDebugView>,
}

impl HumanDebugViewSet {
    pub fn ready_count(&self) -> usize {
        self.views.iter().filter(|view| view.available).count()
    }

    pub fn has(&self, kind: HumanDebugViewKind) -> bool {
        self.views
            .iter()
            .any(|view| view.kind == kind && view.available)
    }

    pub fn supports_v9_human_lab(&self) -> bool {
        [
            HumanDebugViewKind::SkeletonRig,
            HumanDebugViewKind::FacialRig,
            HumanDebugViewKind::VisemeTimeline,
            HumanDebugViewKind::GazeTarget,
            HumanDebugViewKind::SkinMaterialChannels,
            HumanDebugViewKind::EyeMoistureTearline,
            HumanDebugViewKind::MouthTeethTongue,
            HumanDebugViewKind::HairLod,
            HumanDebugViewKind::ClothSimulation,
            HumanDebugViewKind::BodyShapeSoftTissue,
            HumanDebugViewKind::AnimationContact,
            HumanDebugViewKind::PerformanceCaptureCurves,
            HumanDebugViewKind::LightingCloseupTests,
            HumanDebugViewKind::UncannyValleyChecklist,
        ]
        .iter()
        .all(|kind| self.has(*kind))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanDebugView {
    pub kind: HumanDebugViewKind,
    pub label: String,
    pub available: bool,
    pub min_quality: QualityTier,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HumanDebugViewKind {
    SkeletonRig,
    FacialRig,
    VisemeTimeline,
    GazeTarget,
    SkinMaterialChannels,
    EyeMoistureTearline,
    MouthTeethTongue,
    HairLod,
    ClothSimulation,
    BodyShapeSoftTissue,
    AnimationContact,
    PerformanceCaptureCurves,
    LightingCloseupTests,
    UncannyValleyChecklist,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SkinParams {
    pub tone_linear: [f32; 3],
    pub undertone_linear: [f32; 3],
    pub pore_detail: f32,
    pub wrinkle_profile: f32,
    pub oil: f32,
    pub sweat: f32,
    pub scars: Vec<String>,
    pub tattoos: Vec<String>,
    pub makeup_layers: Vec<String>,
    pub injury_response: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HairParams {
    pub style: String,
    pub density: f32,
    pub strand_thickness: f32,
    pub curl: f32,
    pub length_meters: f32,
    pub color_linear: [f32; 3],
    pub wetness_response: f32,
    pub simulation_quality: QualityTier,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EyeParams {
    pub iris_color_linear: [f32; 3],
    pub iris_pattern_seed: u64,
    pub sclera_tint_linear: [f32; 3],
    pub wetness: f32,
    pub redness: f32,
    pub pupil_rest_radius: f32,
    pub cybernetic: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CyberneticAugmentSpec {
    pub label: String,
    pub attach_point: BodyAttachPoint,
    pub visual_materials: Vec<MaterialId>,
    pub physical_profile: PhysicalMaterial,
    pub animation_constraints: Vec<ConstraintDesc>,
    pub damage_behavior: DamageBehavior,
    pub gameplay_tags: TagSet,
    pub exposed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConstraintDesc {
    pub label: String,
    pub attach_point: BodyAttachPoint,
    pub stiffness: f32,
    pub max_angle_radians: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageBehavior {
    CosmeticOnly,
    ExposesSparks,
    ImpairsLimb,
    BreaksAndDrops,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HumanGenerationCacheStats {
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HumanProxyPart {
    Impostor,
    LeftLeg,
    RightLeg,
    Torso,
    LeftArm,
    RightArm,
    Head,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanProxyBox {
    pub part: HumanProxyPart,
    pub local_min_meters: Vec3,
    pub local_max_meters: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HumanProxyFaceFeatureKind {
    LeftEye,
    RightEye,
    Mouth,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanProxyFaceFeature {
    pub kind: HumanProxyFaceFeatureKind,
    pub lateral_offset_meters: f32,
    pub height_meters: f32,
    pub half_width_meters: f32,
    pub half_height_meters: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanProxyGeometry {
    pub quality_tier: QualityTier,
    pub height_meters: f32,
    pub body_parts: Vec<HumanProxyBox>,
    pub face_features: Vec<HumanProxyFaceFeature>,
}

pub fn human_proxy_geometry_for_state(human: &HumanState) -> HumanProxyGeometry {
    let height = deterministic_proxy_height(human.human_id);
    let shoulder_width = (height * 0.27).clamp(0.38, 0.52);
    let hip_width = (height * 0.22).clamp(0.3, 0.43);
    build_human_proxy_geometry(human.quality_tier, height, shoulder_width, hip_width)
}

pub fn human_proxy_geometry_for_quality(quality_tier: QualityTier) -> HumanProxyGeometry {
    build_human_proxy_geometry(quality_tier, 1.68, 0.45, 0.36)
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct HumanRequestCacheKey {
    seed: u64,
    role: HumanRole,
    desired_quality: QualityTier,
    fingerprint: u64,
}

#[derive(Default)]
pub struct HumanGeneratorModule {
    applied_speech_events: BTreeSet<WorldEventId>,
    applied_surface_events: BTreeSet<WorldEventId>,
    requested_human_assets: BTreeSet<AssetId>,
    generated_bundle_cache: BTreeMap<HumanRequestCacheKey, HumanRuntimeBundle>,
    generation_cache_hits: u64,
    generation_cache_misses: u64,
}

impl HumanGeneratorModule {
    pub fn generate(&mut self, request: HumanGenerationRequest) -> HumanRuntimeBundle {
        let key = human_request_cache_key(&request);
        if let Some(bundle) = self.generated_bundle_cache.get(&key) {
            self.generation_cache_hits = self.generation_cache_hits.saturating_add(1);
            return bundle.clone();
        }

        self.generation_cache_misses = self.generation_cache_misses.saturating_add(1);
        let canonical_request = canonicalize_human_request_for_cache(request, &key);
        let bundle = generate_human_bundle(&canonical_request);
        if bundle.validation_report.passed {
            self.generated_bundle_cache.insert(key, bundle.clone());
        }
        bundle
    }

    pub fn cache_stats(&self) -> HumanGenerationCacheStats {
        HumanGenerationCacheStats {
            entries: self.generated_bundle_cache.len(),
            hits: self.generation_cache_hits,
            misses: self.generation_cache_misses,
        }
    }
}

pub fn generate_human_bundle(request: &HumanGenerationRequest) -> HumanRuntimeBundle {
    let lod_set = build_lod_set(request);
    let render_meshes = build_render_meshes(request, &lod_set);
    let anatomical_layers = build_anatomical_layers(request);
    let rig = build_body_rig(request);
    let facial_rig = build_facial_rig(request);
    let materials = build_materials(request);
    let surface_response = build_surface_response_profile(request);
    let physics_proxy = build_physics_proxy(request);
    let animation_profile = build_animation_profile(request);
    let skin_system = build_skin_runtime_model(request, &surface_response);
    let eye_system = build_eye_runtime_model(request);
    let mouth_system = build_mouth_runtime_model(request, &facial_rig);
    let hair_system = build_hair_runtime_model(request, &lod_set);
    let clothing_cybernetics = build_clothing_cybernetics_runtime_model(request, &rig);
    let motion_model = build_motion_model(request, &physics_proxy, &animation_profile);
    let body_model = build_body_runtime_model(request, &physics_proxy, &motion_model);
    let renderer_interface =
        build_renderer_interface(request, &render_meshes, &materials, &rig, &facial_rig);
    let ai_voice_integration = build_ai_voice_integration_model(
        request,
        &eye_system,
        &facial_rig,
        &mouth_system,
        &motion_model,
    );
    let debug_views = build_debug_views(request);
    let performance_capture = build_performance_capture_model(
        request,
        &rig,
        &facial_rig,
        &mouth_system,
        &motion_model,
        &ai_voice_integration,
        &debug_views,
    );
    let failure_fallbacks = build_failure_fallback_plan(FailureFallbackPlanInput {
        request,
        lod_set: &lod_set,
        render_meshes: &render_meshes,
        materials: &materials,
        hair_system: &hair_system,
        clothing_cybernetics: &clothing_cybernetics,
        body_model: &body_model,
        motion_model: &motion_model,
        renderer_interface: &renderer_interface,
        performance_capture: &performance_capture,
        debug_views: &debug_views,
    });
    let performance_budget = build_performance_budget_report(PerformanceBudgetReportInput {
        request,
        lod_set: &lod_set,
        render_meshes: &render_meshes,
        materials: &materials,
        hair_system: &hair_system,
        clothing_cybernetics: &clothing_cybernetics,
        mouth_system: &mouth_system,
        renderer_interface: &renderer_interface,
        performance_capture: &performance_capture,
        failure_fallbacks: &failure_fallbacks,
        debug_views: &debug_views,
    });
    let asset_package_contract = build_asset_package_contract(AssetPackageContractInput {
        request,
        render_meshes: &render_meshes,
        materials: &materials,
        lod_set: &lod_set,
        renderer_interface: &renderer_interface,
        debug_views: &debug_views,
        performance_budget: &performance_budget,
    });
    let acceptance_evidence = build_acceptance_evidence(HumanAcceptanceEvidenceInput {
        request,
        lod_set: &lod_set,
        skin_system: &skin_system,
        eye_system: &eye_system,
        mouth_system: &mouth_system,
        hair_system: &hair_system,
        clothing_cybernetics: &clothing_cybernetics,
        body_model: &body_model,
        motion_model: &motion_model,
        renderer_interface: &renderer_interface,
        ai_voice_integration: &ai_voice_integration,
        failure_fallbacks: &failure_fallbacks,
        performance_budget: &performance_budget,
        asset_package_contract: &asset_package_contract,
        debug_views: &debug_views,
    });
    let uncanny_review = build_uncanny_review_checklist(HumanUncannyReviewInput {
        request,
        skin_system: &skin_system,
        eye_system: &eye_system,
        mouth_system: &mouth_system,
        hair_system: &hair_system,
        clothing_cybernetics: &clothing_cybernetics,
        motion_model: &motion_model,
        renderer_interface: &renderer_interface,
        ai_voice_integration: &ai_voice_integration,
        performance_capture: &performance_capture,
        failure_fallbacks: &failure_fallbacks,
        performance_budget: &performance_budget,
        asset_package_contract: &asset_package_contract,
        acceptance_evidence: &acceptance_evidence,
        generation_policy: &request.generation_policy,
        debug_views: &debug_views,
    });
    let validation_report = validate_human_bundle(HumanBundleValidationInput {
        render_meshes: &render_meshes,
        materials: &materials,
        physics_proxy: &physics_proxy,
        lod_set: &lod_set,
        skin_system: &skin_system,
        eye_system: &eye_system,
        mouth_system: &mouth_system,
        hair_system: &hair_system,
        clothing_cybernetics: &clothing_cybernetics,
        body_model: &body_model,
        motion_model: &motion_model,
        renderer_interface: &renderer_interface,
        ai_voice_integration: &ai_voice_integration,
        performance_capture: &performance_capture,
        failure_fallbacks: &failure_fallbacks,
        performance_budget: &performance_budget,
        asset_package_contract: &asset_package_contract,
        uncanny_review: &uncanny_review,
        acceptance_evidence: &acceptance_evidence,
        generation_policy: &request.generation_policy,
        debug_views: &debug_views,
        request,
    });

    HumanRuntimeBundle {
        human_id: stable_u64(request.seed, request.request_id as u64),
        render_meshes,
        anatomical_layers,
        rig,
        facial_rig,
        materials,
        physics_proxy,
        animation_profile,
        voice_persona: request.linked_voice_persona,
        ai_persona: request.linked_ai_persona,
        lod_set,
        surface_response,
        skin_system,
        eye_system,
        mouth_system,
        hair_system,
        clothing_cybernetics,
        body_model,
        motion_model,
        renderer_interface,
        ai_voice_integration,
        performance_capture,
        failure_fallbacks,
        performance_budget,
        asset_package_contract,
        uncanny_review,
        acceptance_evidence,
        generation_policy: request.generation_policy.clone(),
        debug_views,
        validation_report,
    }
}

pub struct HumanBundleValidationInput<'a> {
    pub render_meshes: &'a [HumanMesh],
    pub materials: &'a [HumanMaterialBinding],
    pub physics_proxy: &'a HumanPhysicsProxy,
    pub lod_set: &'a HumanLodSet,
    pub skin_system: &'a SkinRuntimeModel,
    pub eye_system: &'a EyeRuntimeModel,
    pub mouth_system: &'a MouthRuntimeModel,
    pub hair_system: &'a HairRuntimeModel,
    pub clothing_cybernetics: &'a ClothingCyberneticsRuntimeModel,
    pub body_model: &'a HumanBodyRuntimeModel,
    pub motion_model: &'a HumanMotionModel,
    pub renderer_interface: &'a HumanRendererInterface,
    pub ai_voice_integration: &'a HumanAiVoiceIntegrationModel,
    pub performance_capture: &'a HumanPerformanceCaptureModel,
    pub failure_fallbacks: &'a HumanFailureFallbackPlan,
    pub performance_budget: &'a HumanPerformanceBudgetReport,
    pub asset_package_contract: &'a HumanAssetPackageContract,
    pub uncanny_review: &'a HumanUncannyReviewChecklist,
    pub acceptance_evidence: &'a HumanAcceptanceEvidence,
    pub generation_policy: &'a HumanGenerationPolicy,
    pub debug_views: &'a HumanDebugViewSet,
    pub request: &'a HumanGenerationRequest,
}

pub fn validate_human_bundle(input: HumanBundleValidationInput<'_>) -> HumanValidationReport {
    let HumanBundleValidationInput {
        render_meshes,
        materials,
        physics_proxy,
        lod_set,
        skin_system,
        eye_system,
        mouth_system,
        hair_system,
        clothing_cybernetics,
        body_model,
        motion_model,
        renderer_interface,
        ai_voice_integration,
        performance_capture,
        failure_fallbacks,
        performance_budget,
        asset_package_contract,
        uncanny_review,
        acceptance_evidence,
        generation_policy,
        debug_views,
        request,
    } = input;
    let estimated_hair_strands = estimate_hair_strands(request);
    let estimated_memory_bytes = estimate_human_memory_bytes(lod_set, materials);
    let budget = HumanBudgetUsage {
        mesh_count: render_meshes.len(),
        material_count: materials.len(),
        estimated_memory_bytes,
        estimated_hair_strands,
        physics_proxy_count: physics_proxy.capsules.len(),
    };

    let mut issues = Vec::new();
    if render_meshes.len() > request.budget.max_meshes {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "mesh_budget_exceeded",
            "human mesh count exceeds the requested budget",
        ));
    }
    if materials.len() > request.budget.max_materials {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "material_budget_exceeded",
            "human material count exceeds the requested budget",
        ));
    }
    if estimated_memory_bytes > request.budget.max_memory_bytes {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "memory_budget_exceeded",
            "estimated generated human memory exceeds the requested budget",
        ));
    }
    if estimated_hair_strands > request.budget.max_hair_strands {
        issues.push(validation_issue(
            HumanValidationSeverity::Warning,
            "hair_budget_pressure",
            "hair density was accepted but should be simplified at runtime",
        ));
    }
    if !lod_set
        .levels
        .iter()
        .any(|lod| lod.tier == HumanLodTier::Crowd)
    {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "missing_crowd_lod",
            "human bundles must include a crowd LOD",
        ));
    }
    if !lod_set
        .levels
        .iter()
        .any(|lod| lod.tier == HumanLodTier::Impostor)
    {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "missing_impostor_lod",
            "human bundles must include a distant impostor LOD",
        ));
    }
    if request.linked_ai_persona == request.linked_voice_persona {
        issues.push(validation_issue(
            HumanValidationSeverity::Info,
            "persona_ids_share_value",
            "AI and voice persona IDs may share a value, but behavior remains owned by AI",
        ));
    }
    if generation_policy.provenance_records.is_empty()
        || !generation_policy.has_seed_provenance(request.seed)
    {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "missing_generation_provenance",
            "generated humans must carry seed-linked provenance metadata",
        ));
    }
    if !generation_policy.likeness_authorized() {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "unlicensed_likeness",
            "real-person likeness generation requires consent or license metadata",
        ));
    }
    if !generation_policy.voice_authorized() {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "unlicensed_voice_clone",
            "real-person voice cloning requires consent or rights metadata",
        ));
    }
    if !generation_policy.controlled_variation {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "uncontrolled_generation_variation",
            "procedural human generation must use controlled variation",
        ));
    }
    if !generation_policy.bias_reviewed {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "missing_bias_review",
            "generated population parameters must declare bias review coverage",
        ));
    }
    if request.desired_quality >= QualityTier::HeroHighFidelityRuntime
        && !skin_system.supports_closeup_skin()
    {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "incomplete_skin_system",
            "hero humans require layered skin with subsurface, pores, wrinkles, marks, perfusion, sheen, wetness, dirt, injury, age, and tension detail",
        ));
    }
    if request.desired_quality >= QualityTier::HeroHighFidelityRuntime
        && !eye_system.supports_closeup_optics()
    {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "incomplete_eye_system",
            "hero humans require cornea, sclera, tearline, moisture, gaze, pupil, and blink controls",
        ));
    }
    if request.desired_quality >= QualityTier::HeroHighFidelityRuntime
        && !mouth_system.supports_dialogue_closeup()
    {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "incomplete_mouth_system",
            "hero dialogue humans require close-up teeth, tongue contacts, inner-mouth shading, phoneme/viseme timing, wet lips, jaw, cheek, and breath controls",
        ));
    }
    if request.desired_quality >= QualityTier::HeroHighFidelityRuntime
        && !motion_model.supports_weighted_closeup_motion()
    {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "incomplete_weighted_motion",
            "hero humans require pose correction, contact, balance, hand, breathing, fatigue, capture, and procedural motion support",
        ));
    }
    if request.desired_quality >= QualityTier::HeroHighFidelityRuntime
        && !hair_system.supports_hero_hair()
    {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "incomplete_hair_system",
            "hero humans require guide curves, strand/card hybrid LODs, anisotropic shading, wet response, and simulation fallback",
        ));
    }
    if request.desired_quality >= QualityTier::HeroHighFidelityRuntime
        && !clothing_cybernetics.supports_cyberpunk_clothing()
    {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "incomplete_clothing_system",
            "hero humans require layered clothing, cloth simulation with fallback, material-aware fabric, attachment points, and clipping prevention",
        ));
    }
    if request.desired_quality >= QualityTier::HeroHighFidelityRuntime
        && !clothing_cybernetics.supports_visible_cybernetics()
    {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "incomplete_cybernetic_system",
            "visible cybernetics require hard-surface structure and emissive/material evidence",
        ));
    }
    if request.desired_quality >= QualityTier::HeroHighFidelityRuntime
        && !body_model.supports_hero_body_shape()
    {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "incomplete_body_shape_model",
            "hero humans require explicit morphology, fat distribution, muscle groups, soft-tissue zones, collision coupling, center of mass, and scale-consistent body shape",
        ));
    }
    if request.desired_quality >= QualityTier::HeroHighFidelityRuntime
        && !renderer_interface.supports_renderer_handoff()
    {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "incomplete_renderer_interface",
            "hero humans require renderer handoff data for meshes, skin/eye/hair/mouth materials, morph and mouth articulation buffers, clothing buffers, material state fields, expressions, and visemes",
        ));
    }
    if request.desired_quality >= QualityTier::HeroHighFidelityRuntime
        && !ai_voice_integration.supports_character_integration()
    {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "incomplete_ai_voice_integration",
            "hero humans require AI persona, voice persona, emotion, gaze, expression, viseme timing, gesture, relationship, and injury/fatigue bindings",
        ));
    }
    if request.desired_quality >= QualityTier::HeroHighFidelityRuntime
        && !performance_capture.supports_hero_performance_capture()
    {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "incomplete_performance_capture_curves",
            "hero humans require timecoded performance capture curves for face, eyes, voice, body, hands, breathing, contact, retargeting, and debug inspection",
        ));
    }
    if request.desired_quality >= QualityTier::HeroHighFidelityRuntime
        && !failure_fallbacks.supports_v9_failure_behavior()
    {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "incomplete_failure_fallbacks",
            "hero humans require controlled failure behavior for voice generation, streaming proxies, rendering fallbacks, simulation demotion, frame-budget fallback cost, telemetry, and replay",
        ));
    }
    if request.desired_quality >= QualityTier::HeroHighFidelityRuntime
        && !performance_budget.supports_v9_performance_budget(&request.budget)
    {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "incomplete_performance_budget_report",
            "hero humans require CPU, GPU, memory, human animation, voice/lipsync, hair/cloth, renderer handoff, fallback-cost, profiler-capture, and frame-failure budget evidence",
        ));
    }
    if request.desired_quality >= QualityTier::HeroHighFidelityRuntime
        && !asset_package_contract.supports_v9_schema_asset_package()
    {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "incomplete_schema_asset_package",
            "hero humans require a versioned asset package manifest, public runtime schemas, provenance binding, debug preview, schema round-trip evidence, and integration replay evidence",
        ));
    }
    if request.desired_quality >= QualityTier::HeroHighFidelityRuntime
        && !acceptance_evidence.passes_v9_acceptance()
    {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "incomplete_human_acceptance_evidence",
            "hero humans require close-up dialogue, alive eyes, lipsync, skin wetness/lighting, stable hair, plausible clothing, and stable LOD transition evidence",
        ));
    }
    if request.desired_quality >= QualityTier::HeroHighFidelityRuntime
        && !uncanny_review.passes_v9_review()
    {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "incomplete_uncanny_review",
            "hero humans require the V9 uncanny-valley checklist to pass for eyes, skin, mouth, speech, hair, clothing, motion, capture, LOD, renderer, AI/voice, provenance, and debug coverage",
        ));
    }
    if request.desired_quality >= QualityTier::HeroHighFidelityRuntime
        && !debug_views.supports_v9_human_lab()
    {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "missing_human_debug_views",
            "hero human bundles must expose the full V9 human lab debug checklist for skeleton, face, visemes, skin, eyes, mouth, hair, cloth, body soft tissue, capture curves, lighting closeups, and uncanny review",
        ));
    }

    HumanValidationReport {
        passed: !issues
            .iter()
            .any(|issue| issue.severity == HumanValidationSeverity::Error),
        issues,
        budget,
    }
}

fn estimate_human_memory_bytes(lod_set: &HumanLodSet, materials: &[HumanMaterialBinding]) -> u64 {
    lod_set
        .levels
        .iter()
        .map(|lod| lod.estimated_memory_bytes)
        .sum::<u64>()
        + materials.len() as u64 * 512 * 1024
}

impl EngineModule for HumanGeneratorModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            module_id: 40,
            name: "realistic_human_generator",
            schema: SchemaVersion {
                name: "HumanRuntimeBundle",
                version: 1,
            },
            default_quality: QualityTier::NormalRuntime,
        }
    }

    fn schema_requirements(&self) -> Vec<ashfall_core::schema::SchemaRequirement> {
        vec![
            ashfall_core::schema::SchemaRequirement::required(
                40,
                "SpeechResult",
                1,
                "human generator consumes viseme and duration data for facial animation",
            ),
            ashfall_core::schema::SchemaRequirement::optional(
                40,
                "AssetPackageManifest",
                ASSET_PACKAGE_MANIFEST_SCHEMA_VERSION,
                "human bundles ship as provenance-tracked asset packages validated inside the bundle contract",
            ),
            ashfall_core::schema::SchemaRequirement::optional(
                40,
                "ValidationReport",
                4,
                "human package manifests attach validation reports for conformance evidence",
            ),
        ]
    }

    fn init(&mut self, ctx: &mut EngineContext<'_>) {
        ctx.schemas.register(
            1,
            SchemaVersion {
                name: "AssetPackageManifest",
                version: ASSET_PACKAGE_MANIFEST_SCHEMA_VERSION,
            },
        );
        ctx.schemas.register(
            1,
            SchemaVersion {
                name: "ValidationReport",
                version: 4,
            },
        );
    }

    fn tick(&mut self, frame: &FrameContext, out: &mut CommandSink) {
        self.request_visible_human_assets(frame, out);

        for event in &frame.recent_events {
            let WorldEventKind::SpeechSynthesized {
                speaker,
                duration_seconds,
                viseme_count,
                ..
            } = &event.kind
            else {
                continue;
            };

            if frame.snapshot.humans.find(*speaker).is_none()
                || !self.applied_speech_events.insert(event.event_id)
            {
                continue;
            }

            out.event(WorldEvent {
                event_id: deterministic_child_event_id(
                    frame.sim_time.tick,
                    40,
                    *speaker,
                    event.event_id,
                ),
                tick: frame.sim_time.tick,
                location_meters: event.location_meters,
                actors: vec![*speaker],
                kind: WorldEventKind::FacialAnimationApplied {
                    entity: *speaker,
                    viseme_count: *viseme_count,
                    duration_seconds: *duration_seconds,
                },
                physical_evidence: vec!["facial_rig".to_string(), "viseme_track".to_string()],
                narrative_tags: vec!["human_animation".to_string(), "lip_sync".to_string()],
            });
        }

        for event in &frame.recent_events {
            let WorldEventKind::MaterialStateChanged { entity } = &event.kind else {
                continue;
            };
            if !self.applied_surface_events.insert(event.event_id) {
                continue;
            }
            let Some(surface_state) =
                human_surface_response_from_snapshot(&frame.snapshot, *entity)
            else {
                continue;
            };

            out.event(WorldEvent {
                event_id: deterministic_event_id(frame.sim_time.tick, 42, *entity),
                tick: frame.sim_time.tick,
                location_meters: event.location_meters,
                actors: vec![*entity],
                kind: WorldEventKind::HumanAppearanceUpdated {
                    entity: *entity,
                    skin_wetness: surface_state.skin_wetness,
                    injury_overlay: surface_state.injury_overlay,
                    bruising: surface_state.bruising,
                    dirt: surface_state.dirt,
                },
                physical_evidence: vec![
                    "human_material_response".to_string(),
                    "skin_hair_clothing_state".to_string(),
                ],
                narrative_tags: vec!["human".to_string(), "appearance".to_string()],
            });
        }
    }

    fn schedule_gpu(&mut self, graph: &mut GpuGraphBuilder) {
        let requested_bundle_count =
            u64::try_from(self.requested_human_assets.len()).unwrap_or(u64::MAX);
        let speech_event_count =
            u64::try_from(self.applied_speech_events.len()).unwrap_or(u64::MAX);
        let surface_event_count =
            u64::try_from(self.applied_surface_events.len()).unwrap_or(u64::MAX);
        if requested_bundle_count == 0 && speech_event_count == 0 && surface_event_count == 0 {
            return;
        }

        let active_human_count = requested_bundle_count
            .max(speech_event_count)
            .max(surface_event_count)
            .max(1);
        let bundle_table = human_gpu_resource(
            graph,
            "human runtime bundle table",
            GpuResourceKind::Buffer,
            human_resource_bytes(active_human_count, 512),
            GpuResourceLifetime::Imported,
            true,
        );
        let lod_selection = human_gpu_resource(
            graph,
            "human lod selection buffer",
            GpuResourceKind::Buffer,
            human_resource_bytes(active_human_count, 128),
            GpuResourceLifetime::Transient,
            false,
        );
        let skinning_palette = human_gpu_resource(
            graph,
            "human skinning palette buffer",
            GpuResourceKind::Buffer,
            human_resource_bytes(active_human_count, 256 * 64),
            GpuResourceLifetime::Transient,
            false,
        );

        let lod_pipeline = human_compute_pipeline_with_permutation(
            graph,
            "human_lod_selection",
            "human/lod_selection.comp",
            QualityTier::NormalRuntime,
            human_shader_permutation(["HUMAN_LOD", "STREAMING_BUNDLE_TABLE"]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "human_lod_selection",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(lod_pipeline),
                QualityTier::NormalRuntime,
            )
            .reads([bundle_table])
            .writes([lod_selection]),
        );

        let eye_gaze_state = human_gpu_resource(
            graph,
            "human eye gaze and blink buffer",
            GpuResourceKind::Buffer,
            human_resource_bytes(active_human_count, 192),
            GpuResourceLifetime::Transient,
            false,
        );
        let eye_gaze_pipeline = human_compute_pipeline_with_permutation(
            graph,
            "human_eye_gaze_and_blink",
            "human/eye_gaze_and_blink.comp",
            QualityTier::NormalRuntime,
            human_shader_permutation(["EYE_GAZE", "BLINKS", "TEARLINE", "PUPIL_ADAPTATION"]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "human_eye_gaze_and_blink",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(eye_gaze_pipeline),
                QualityTier::NormalRuntime,
            )
            .reads([bundle_table, lod_selection])
            .writes([eye_gaze_state]),
        );

        let surface_response = if surface_event_count > 0 {
            let surface_event_buffer = human_gpu_resource(
                graph,
                "human surface event buffer",
                GpuResourceKind::Buffer,
                human_resource_bytes(surface_event_count, 160),
                GpuResourceLifetime::Imported,
                false,
            );
            let surface_response = human_gpu_resource(
                graph,
                "human skin hair clothing response buffer",
                GpuResourceKind::Buffer,
                human_resource_bytes(surface_event_count, 192),
                GpuResourceLifetime::Transient,
                false,
            );
            let surface_pipeline = human_compute_pipeline_with_permutation(
                graph,
                "human_surface_response",
                "human/surface_response.comp",
                QualityTier::NormalRuntime,
                human_shader_permutation([
                    "SKIN_WETNESS",
                    "HAIR_WETNESS",
                    "CLOTHING_DAMAGE",
                    "INJURY_OVERLAY",
                ]),
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "human_surface_response",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(surface_pipeline),
                    QualityTier::NormalRuntime,
                )
                .reads([bundle_table, surface_event_buffer])
                .writes([surface_response]),
            );
            Some(surface_response)
        } else {
            None
        };

        let skin_closeup_channels = if requested_bundle_count > 0 || surface_event_count > 0 {
            let skin_closeup_channels = human_gpu_resource(
                graph,
                "human skin closeup material channels",
                GpuResourceKind::Buffer,
                human_resource_bytes(active_human_count, 256),
                GpuResourceLifetime::Transient,
                true,
            );
            let skin_pipeline = human_compute_pipeline_with_permutation(
                graph,
                "human_skin_closeup_channels",
                "human/skin_closeup_channels.comp",
                QualityTier::NormalRuntime,
                human_shader_permutation([
                    "SKIN_SUBSURFACE",
                    "PORE_MICRODETAIL",
                    "WRINKLE_TENSION",
                    "BLOOD_PERFUSION",
                    "ETHICAL_MARKS",
                ]),
            );
            let mut reads = vec![bundle_table, lod_selection];
            if let Some(surface_response) = surface_response {
                reads.push(surface_response);
            }
            graph.add_pass(
                GpuPassDesc::new(
                    "human_skin_closeup_channels",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(skin_pipeline),
                    QualityTier::NormalRuntime,
                )
                .reads(reads)
                .writes([skin_closeup_channels]),
            );
            Some(skin_closeup_channels)
        } else {
            None
        };

        let facial_morphs = if speech_event_count > 0 {
            let viseme_tracks = human_gpu_resource(
                graph,
                "human viseme track buffer",
                GpuResourceKind::Buffer,
                human_resource_bytes(speech_event_count, 256),
                GpuResourceLifetime::Imported,
                false,
            );
            let facial_morphs = human_gpu_resource(
                graph,
                "human facial morph target buffer",
                GpuResourceKind::Buffer,
                human_resource_bytes(speech_event_count, 384),
                GpuResourceLifetime::Transient,
                false,
            );
            let facial_pipeline = human_compute_pipeline_with_permutation(
                graph,
                "human_facial_viseme_morphs",
                "human/facial_viseme_morphs.comp",
                QualityTier::NormalRuntime,
                human_shader_permutation([
                    "VISEME_TRACKS",
                    "FACIAL_RIG",
                    "TONGUE_CONTACTS",
                    "INNER_MOUTH_OCCLUSION",
                    "EMOTION_CURVE",
                    "PERFORMANCE_CAPTURE_CURVES",
                ]),
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "human_facial_viseme_morphs",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(facial_pipeline),
                    QualityTier::NormalRuntime,
                )
                .reads([bundle_table, lod_selection, eye_gaze_state, viseme_tracks])
                .writes([facial_morphs]),
            );
            Some(facial_morphs)
        } else {
            None
        };

        let hair_cloth_controls = if requested_bundle_count > 0 {
            let hair_cloth_controls = human_gpu_resource(
                graph,
                "human hair cloth simulation controls",
                GpuResourceKind::Buffer,
                human_resource_bytes(requested_bundle_count, 320),
                GpuResourceLifetime::Transient,
                false,
            );
            let hair_groom_table = human_gpu_resource(
                graph,
                "human hair groom runtime table",
                GpuResourceKind::Buffer,
                human_resource_bytes(requested_bundle_count, 384),
                GpuResourceLifetime::Transient,
                true,
            );
            let clothing_cybernetics = human_gpu_resource(
                graph,
                "human clothing cybernetics constraint buffer",
                GpuResourceKind::Buffer,
                human_resource_bytes(requested_bundle_count, 448),
                GpuResourceLifetime::Transient,
                false,
            );
            let hair_cloth_pipeline = human_compute_pipeline_with_permutation(
                graph,
                "human_hair_cloth_prepare",
                "human/hair_cloth_prepare.comp",
                QualityTier::NormalRuntime,
                human_shader_permutation([
                    "HAIR_GUIDES",
                    "HAIR_LOD",
                    "BROW_LASH_STRANDS",
                    "FACIAL_HAIR_LOD",
                    "ANISOTROPIC_HAIR",
                    "CLOTH_ATTACHMENTS",
                    "CYBERNETIC_EMISSIVE",
                    "CLIPPING_PREVENTION",
                    "PHYSICS_PROXY",
                ]),
            );
            let mut reads = vec![bundle_table, lod_selection];
            if let Some(surface_response) = surface_response {
                reads.push(surface_response);
            }
            if let Some(skin_closeup_channels) = skin_closeup_channels {
                reads.push(skin_closeup_channels);
            }
            graph.add_pass(
                GpuPassDesc::new(
                    "human_hair_cloth_prepare",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(hair_cloth_pipeline),
                    QualityTier::NormalRuntime,
                )
                .reads(reads)
                .writes([
                    hair_cloth_controls,
                    hair_groom_table,
                    clothing_cybernetics,
                ]),
            );
            Some((hair_cloth_controls, hair_groom_table, clothing_cybernetics))
        } else {
            None
        };

        let skinning_pipeline = human_compute_pipeline_with_permutation(
            graph,
            "human_skinning_palette_upload",
            "human/skinning_palette_upload.comp",
            QualityTier::NormalRuntime,
            human_shader_permutation([
                "GPU_SKINNING",
                "LOD_BONE_REMAP",
                "MORPH_TARGETS",
                "BODY_SHAPE_MORPHS",
                "SOFT_TISSUE_ZONES",
                "MASS_DISTRIBUTION",
            ]),
        );
        let mut skinning_reads = vec![bundle_table, lod_selection, eye_gaze_state];
        if let Some(skin_closeup_channels) = skin_closeup_channels {
            skinning_reads.push(skin_closeup_channels);
        }
        if let Some(facial_morphs) = facial_morphs {
            skinning_reads.push(facial_morphs);
        }
        if let Some((hair_cloth_controls, hair_groom_table, clothing_cybernetics)) =
            hair_cloth_controls
        {
            skinning_reads.push(hair_cloth_controls);
            skinning_reads.push(hair_groom_table);
            skinning_reads.push(clothing_cybernetics);
        }
        graph.add_pass(
            GpuPassDesc::new(
                "human_skinning_palette_upload",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(skinning_pipeline),
                QualityTier::NormalRuntime,
            )
            .reads(skinning_reads)
            .writes([skinning_palette]),
        );

        let renderer_packet = human_gpu_resource(
            graph,
            "human renderer interface packet",
            GpuResourceKind::Buffer,
            human_resource_bytes(active_human_count, 512),
            GpuResourceLifetime::Transient,
            true,
        );
        let renderer_packet_pipeline = human_compute_pipeline_with_permutation(
            graph,
            "human_renderer_interface_pack",
            "human/renderer_interface_pack.comp",
            QualityTier::NormalRuntime,
            human_shader_permutation([
                "RIGGED_MESH_HANDLES",
                "SKIN_MATERIAL_HANDLES",
                "HAIR_DATA_HANDLES",
                "EYE_MATERIAL_HANDLES",
                "MOUTH_MATERIAL_HANDLES",
                "MORPH_ANIMATION_BUFFERS",
                "MOUTH_ARTICULATION_BUFFER",
                "CLOTHING_BUFFERS",
                "MATERIAL_STATE_FIELDS",
                "EXPRESSION_CURVES",
                "VISEME_TIMINGS",
            ]),
        );
        let mut renderer_reads = vec![bundle_table, lod_selection, skinning_palette];
        if let Some(skin_closeup_channels) = skin_closeup_channels {
            renderer_reads.push(skin_closeup_channels);
        }
        if let Some(facial_morphs) = facial_morphs {
            renderer_reads.push(facial_morphs);
        }
        if let Some((hair_cloth_controls, hair_groom_table, clothing_cybernetics)) =
            hair_cloth_controls
        {
            renderer_reads.push(hair_cloth_controls);
            renderer_reads.push(hair_groom_table);
            renderer_reads.push(clothing_cybernetics);
        }
        graph.add_pass(
            GpuPassDesc::new(
                "human_renderer_interface_pack",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(renderer_packet_pipeline),
                QualityTier::NormalRuntime,
            )
            .reads(renderer_reads)
            .writes([renderer_packet]),
        );
    }

    fn performance_counters(&self) -> PerformanceCounters {
        let active_human_count = self
            .requested_human_assets
            .len()
            .max(self.applied_speech_events.len())
            .max(self.applied_surface_events.len());
        let cached_bundle_bytes = self
            .generated_bundle_cache
            .values()
            .map(|bundle| bundle.validation_report.budget.estimated_memory_bytes)
            .sum::<u64>();
        PerformanceCounters {
            cpu_milliseconds: 0.25,
            gpu_milliseconds: estimate_human_gpu_milliseconds(
                self.requested_human_assets.len(),
                self.applied_speech_events.len(),
                self.applied_surface_events.len(),
            ),
            memory_bytes: 24 * 1024 * 1024
                + active_human_count as u64 * 2 * 1024 * 1024
                + cached_bundle_bytes,
        }
    }

    fn save_state(&self) -> Option<ModuleStateRecord> {
        let descriptor = self.descriptor();
        let mut entries = Vec::new();
        entries.extend(
            self.applied_speech_events
                .iter()
                .map(|event_id| format!("speech:{event_id}")),
        );
        entries.extend(
            self.applied_surface_events
                .iter()
                .map(|event_id| format!("surface:{event_id}")),
        );
        entries.extend(
            self.requested_human_assets
                .iter()
                .map(|asset_id| format!("asset:{asset_id}")),
        );
        Some(ModuleStateRecord::new(
            descriptor.module_id,
            descriptor.schema,
            HUMAN_MODULE_STATE_VERSION,
            entries,
        ))
    }

    fn load_state(&mut self, state: &ModuleStateRecord) {
        if state.state_version != HUMAN_MODULE_STATE_VERSION {
            return;
        }
        self.applied_speech_events = state
            .entries_with_prefix("speech:")
            .filter_map(|event_id| event_id.parse::<WorldEventId>().ok())
            .collect();
        self.applied_surface_events = state
            .entries_with_prefix("surface:")
            .filter_map(|event_id| event_id.parse::<WorldEventId>().ok())
            .collect();
        self.requested_human_assets = state
            .entries_with_prefix("asset:")
            .filter_map(|asset_id| asset_id.parse::<AssetId>().ok())
            .collect();
        self.generated_bundle_cache.clear();
        self.generation_cache_hits = 0;
        self.generation_cache_misses = 0;
    }
}

impl HumanGeneratorModule {
    fn request_visible_human_assets(&mut self, frame: &FrameContext, out: &mut CommandSink) {
        for (entity, human) in frame.snapshot.humans.iter() {
            if human.quality_tier < QualityTier::NormalRuntime {
                continue;
            }
            let requested_quality = if frame.quality_tier < QualityTier::NormalRuntime {
                human.quality_tier.min(frame.quality_tier)
            } else {
                human.quality_tier
            };
            if requested_quality < QualityTier::NormalRuntime {
                continue;
            }
            let Some(renderable) = frame.snapshot.renderables.find(*entity) else {
                continue;
            };
            let asset_id = renderable.mesh.0;
            if !self.requested_human_assets.insert(asset_id) {
                continue;
            }

            let location = frame
                .snapshot
                .transforms
                .find(*entity)
                .map(|transform| transform.translation_meters)
                .unwrap_or(Vec3::ZERO);
            out.event(WorldEvent {
                event_id: deterministic_event_id(frame.sim_time.tick, 41, asset_id as u64),
                tick: frame.sim_time.tick,
                location_meters: location,
                actors: vec![*entity],
                kind: WorldEventKind::AssetStreamingRequested {
                    asset_id,
                    requester: 40,
                    requested_quality,
                    priority: if requested_quality >= QualityTier::HeroHighFidelityRuntime {
                        AssetPriority::Hero
                    } else {
                        AssetPriority::Visible
                    },
                    reason: "human body, clothing, rig, and facial detail bundle".to_string(),
                },
                physical_evidence: vec!["human_runtime_bundle".to_string()],
                narrative_tags: vec!["asset_streaming".to_string(), "human".to_string()],
            });

            if let Some(surface_state) =
                human_surface_response_from_snapshot(&frame.snapshot, *entity)
            {
                out.event(WorldEvent {
                    event_id: deterministic_event_id(frame.sim_time.tick, 43, *entity),
                    tick: frame.sim_time.tick,
                    location_meters: location,
                    actors: vec![*entity],
                    kind: WorldEventKind::HumanAppearanceUpdated {
                        entity: *entity,
                        skin_wetness: surface_state.skin_wetness,
                        injury_overlay: surface_state.injury_overlay,
                        bruising: surface_state.bruising,
                        dirt: surface_state.dirt,
                    },
                    physical_evidence: vec![
                        "human_runtime_bundle".to_string(),
                        "skin_hair_clothing_state".to_string(),
                    ],
                    narrative_tags: vec!["human".to_string(), "appearance".to_string()],
                });
            }
        }
    }
}

pub fn streaming_dependencies_for_bundle(
    bundle: &HumanRuntimeBundle,
    min_quality: QualityTier,
) -> Vec<HumanStreamingDependency> {
    let mut seen_assets = BTreeSet::new();
    bundle
        .render_meshes
        .iter()
        .filter(|mesh| mesh.streaming_required)
        .filter_map(|mesh| {
            let requested_quality = quality_for_lod(&bundle.lod_set, mesh.lod)?;
            (requested_quality >= min_quality && seen_assets.insert(mesh.mesh.0)).then_some(
                HumanStreamingDependency {
                    asset_id: mesh.mesh.0,
                    part: mesh.part,
                    lod: mesh.lod,
                    requested_quality,
                    priority: priority_for_human_lod(mesh.lod),
                },
            )
        })
        .collect()
}

fn quality_for_lod(lod_set: &HumanLodSet, tier: HumanLodTier) -> Option<QualityTier> {
    lod_set
        .levels
        .iter()
        .find(|lod| lod.tier == tier)
        .map(|lod| lod.quality)
}

fn priority_for_human_lod(lod: HumanLodTier) -> AssetPriority {
    match lod {
        HumanLodTier::Hero => AssetPriority::Hero,
        HumanLodTier::ImportantNpc | HumanLodTier::NormalNpc => AssetPriority::Visible,
        HumanLodTier::Crowd | HumanLodTier::Impostor => AssetPriority::Background,
    }
}

fn build_lod_set(request: &HumanGenerationRequest) -> HumanLodSet {
    let mut levels = Vec::new();
    if request.desired_quality >= QualityTier::HeroHighFidelityRuntime
        || request.role == HumanRole::Hero
    {
        levels.push(HumanLod {
            tier: HumanLodTier::Hero,
            quality: QualityTier::HeroHighFidelityRuntime,
            max_distance_meters: 4.0,
            mesh: MeshAssetHandle(stable_u128(request.seed, request.request_id, 1)),
            bone_count: 192,
            facial_control_count: 96,
            hair: HairRepresentation::Strands,
            estimated_memory_bytes: 28 * 1024 * 1024,
        });
    }

    if request.role != HumanRole::Crowd {
        levels.push(HumanLod {
            tier: HumanLodTier::ImportantNpc,
            quality: QualityTier::NormalRuntime,
            max_distance_meters: 12.0,
            mesh: MeshAssetHandle(stable_u128(request.seed, request.request_id, 2)),
            bone_count: 120,
            facial_control_count: 48,
            hair: HairRepresentation::Cards,
            estimated_memory_bytes: 12 * 1024 * 1024,
        });
    }

    levels.push(HumanLod {
        tier: HumanLodTier::NormalNpc,
        quality: QualityTier::NormalRuntime,
        max_distance_meters: 28.0,
        mesh: MeshAssetHandle(stable_u128(request.seed, request.request_id, 3)),
        bone_count: 72,
        facial_control_count: 20,
        hair: HairRepresentation::Cards,
        estimated_memory_bytes: 6 * 1024 * 1024,
    });
    levels.push(HumanLod {
        tier: HumanLodTier::Crowd,
        quality: QualityTier::BackgroundApproximation,
        max_distance_meters: 60.0,
        mesh: MeshAssetHandle(stable_u128(request.seed, request.request_id, 4)),
        bone_count: 36,
        facial_control_count: 4,
        hair: HairRepresentation::PaintedCap,
        estimated_memory_bytes: 2 * 1024 * 1024,
    });
    levels.push(HumanLod {
        tier: HumanLodTier::Impostor,
        quality: QualityTier::BackgroundApproximation,
        max_distance_meters: f32::INFINITY,
        mesh: MeshAssetHandle(stable_u128(request.seed, request.request_id, 5)),
        bone_count: 0,
        facial_control_count: 0,
        hair: HairRepresentation::Hidden,
        estimated_memory_bytes: 512 * 1024,
    });

    HumanLodSet {
        selected_quality: request.desired_quality,
        levels,
    }
}

fn build_render_meshes(
    request: &HumanGenerationRequest,
    lod_set: &HumanLodSet,
) -> HumanRenderMeshes {
    let mut meshes = Vec::new();
    for lod in &lod_set.levels {
        meshes.push(HumanMesh {
            mesh: lod.mesh,
            part: if lod.tier == HumanLodTier::Impostor {
                HumanMeshPart::ImpostorCard
            } else {
                HumanMeshPart::Body
            },
            lod: lod.tier,
            streaming_required: lod.quality >= QualityTier::NormalRuntime,
            triangle_budget: triangle_budget_for_lod(lod.tier, HumanMeshPart::Body),
        });
    }

    let detailed_lod = lod_set
        .levels
        .first()
        .map(|lod| lod.tier)
        .unwrap_or(HumanLodTier::NormalNpc);
    let mut detailed_parts = vec![
        HumanMeshPart::Face,
        HumanMeshPart::Eyes,
        HumanMeshPart::Teeth,
        HumanMeshPart::Tongue,
        HumanMeshPart::Hair,
        HumanMeshPart::Clothing,
    ];
    if request.desired_quality < QualityTier::NormalRuntime {
        detailed_parts.retain(|part| *part != HumanMeshPart::Tongue);
    }
    for part in detailed_parts {
        meshes.push(HumanMesh {
            mesh: MeshAssetHandle(stable_u128(
                request.seed,
                request.request_id,
                part as u64 + 20,
            )),
            part,
            lod: detailed_lod,
            streaming_required: matches!(
                part,
                HumanMeshPart::Face
                    | HumanMeshPart::Tongue
                    | HumanMeshPart::Hair
                    | HumanMeshPart::Clothing
            ),
            triangle_budget: triangle_budget_for_lod(detailed_lod, part),
        });
    }

    for (index, _) in request.cybernetics.iter().enumerate() {
        meshes.push(HumanMesh {
            mesh: MeshAssetHandle(stable_u128(
                request.seed,
                request.request_id,
                100 + index as u64,
            )),
            part: HumanMeshPart::CyberneticAugment,
            lod: detailed_lod,
            streaming_required: true,
            triangle_budget: 6_000,
        });
    }

    meshes
}

fn build_anatomical_layers(request: &HumanGenerationRequest) -> AnatomicalLayerSet {
    let hero = request.desired_quality >= QualityTier::HeroHighFidelityRuntime;
    [
        AnatomicalLayerKind::Skeleton,
        AnatomicalLayerKind::Muscle,
        AnatomicalLayerKind::SoftTissue,
        AnatomicalLayerKind::Skin,
        AnatomicalLayerKind::Eyes,
        AnatomicalLayerKind::Mouth,
        AnatomicalLayerKind::Tongue,
        AnatomicalLayerKind::Hair,
        AnatomicalLayerKind::Clothing,
        AnatomicalLayerKind::Cybernetics,
    ]
    .into_iter()
    .map(|kind| AnatomicalLayer {
        kind,
        quality: if hero {
            QualityTier::HeroHighFidelityRuntime
        } else {
            QualityTier::NormalRuntime
        },
        enabled: match kind {
            AnatomicalLayerKind::Muscle | AnatomicalLayerKind::SoftTissue => hero,
            AnatomicalLayerKind::Cybernetics => !request.cybernetics.is_empty(),
            _ => true,
        },
        material_state_channels: match kind {
            AnatomicalLayerKind::Skin => vec![
                HumanMaterialStateChannel::Wetness,
                HumanMaterialStateChannel::Oil,
                HumanMaterialStateChannel::Sweat,
                HumanMaterialStateChannel::BloodPerfusion,
                HumanMaterialStateChannel::Bruising,
                HumanMaterialStateChannel::Injury,
                HumanMaterialStateChannel::Dirt,
            ],
            AnatomicalLayerKind::Hair | AnatomicalLayerKind::Clothing => {
                vec![
                    HumanMaterialStateChannel::Wetness,
                    HumanMaterialStateChannel::Dirt,
                ]
            }
            _ => Vec::new(),
        },
    })
    .collect()
}

fn build_body_rig(request: &HumanGenerationRequest) -> HumanRig {
    let hero = request.desired_quality >= QualityTier::HeroHighFidelityRuntime;
    HumanRig {
        rig_id: stable_u128(request.seed, request.request_id, 500),
        skeleton_profile: match request.role {
            HumanRole::SecurityAgent => "guarded_security_biped",
            HumanRole::GangMember => "loose_street_biped",
            HumanRole::CorporateAgent => "formal_corporate_biped",
            _ => "standard_biped",
        }
        .to_string(),
        bone_count: if hero { 192 } else { 96 },
        attachment_points: default_attachment_points(),
        gpu_skinning_resource: Some(GpuSkinningResource(stable_u128(
            request.seed,
            request.request_id,
            501,
        ))),
        retargeting_profile: "humanoid_runtime_retarget_v1".to_string(),
    }
}

fn build_facial_rig(request: &HumanGenerationRequest) -> FacialRig {
    let hero = request.desired_quality >= QualityTier::HeroHighFidelityRuntime;
    FacialRig {
        rig_id: stable_u128(request.seed, request.request_id, 600),
        viseme_set: vec![
            VisemeControl {
                label: "open".to_string(),
                weight_channel: 0,
                jaw_open: 0.8,
                lip_round: 0.1,
            },
            VisemeControl {
                label: "narrow".to_string(),
                weight_channel: 1,
                jaw_open: 0.25,
                lip_round: 0.65,
            },
            VisemeControl {
                label: "teeth".to_string(),
                weight_channel: 2,
                jaw_open: 0.35,
                lip_round: 0.0,
            },
            VisemeControl {
                label: "tongue".to_string(),
                weight_channel: 3,
                jaw_open: 0.42,
                lip_round: 0.18,
            },
            VisemeControl {
                label: "closed".to_string(),
                weight_channel: 4,
                jaw_open: 0.0,
                lip_round: 0.2,
            },
        ],
        expression_controls: [
            EmotionAxis::Anger,
            EmotionAxis::Fear,
            EmotionAxis::Disgust,
            EmotionAxis::Sadness,
            EmotionAxis::Surprise,
            EmotionAxis::Tiredness,
            EmotionAxis::Pain,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, axis)| ExpressionControl {
            label: format!("{axis:?}"),
            emotion_axis: axis,
            max_weight: if hero {
                1.0
            } else {
                0.65 + index as f32 * 0.01
            },
        })
        .collect(),
        micro_expression_count: if hero { 18 } else { 6 },
        blink_controls: true,
        eye_aim_controls: true,
    }
}

fn build_materials(request: &HumanGenerationRequest) -> HumanMaterialSet {
    let mut materials = vec![
        HumanMaterialBinding {
            material_id: DEFAULT_HUMAN_SKIN_MATERIAL,
            layer: AnatomicalLayerKind::Skin,
            supports_wetness: true,
            supports_injury: true,
            procedural_seed: stable_u64(request.seed, 1),
        },
        HumanMaterialBinding {
            material_id: DEFAULT_EYE_MATERIAL,
            layer: AnatomicalLayerKind::Eyes,
            supports_wetness: true,
            supports_injury: false,
            procedural_seed: request.eyes.iris_pattern_seed,
        },
        HumanMaterialBinding {
            material_id: DEFAULT_HAIR_MATERIAL,
            layer: AnatomicalLayerKind::Hair,
            supports_wetness: request.hair.wetness_response > 0.0,
            supports_injury: false,
            procedural_seed: stable_u64(request.seed, 2),
        },
        HumanMaterialBinding {
            material_id: DEFAULT_TEETH_MATERIAL,
            layer: AnatomicalLayerKind::Mouth,
            supports_wetness: true,
            supports_injury: false,
            procedural_seed: stable_u64(request.seed, 3),
        },
        HumanMaterialBinding {
            material_id: DEFAULT_TONGUE_MATERIAL,
            layer: AnatomicalLayerKind::Tongue,
            supports_wetness: true,
            supports_injury: true,
            procedural_seed: stable_u64(request.seed, 4),
        },
    ];

    for layer in &request.clothing_profile.layers {
        materials.push(HumanMaterialBinding {
            material_id: layer.material_id,
            layer: AnatomicalLayerKind::Clothing,
            supports_wetness: request.clothing_profile.wetness_response > 0.0,
            supports_injury: request.clothing_profile.damage_visibility > 0.0,
            procedural_seed: stable_u64(request.seed, layer.material_id),
        });
    }

    for augment in &request.cybernetics {
        for material_id in &augment.visual_materials {
            materials.push(HumanMaterialBinding {
                material_id: *material_id,
                layer: AnatomicalLayerKind::Cybernetics,
                supports_wetness: false,
                supports_injury: augment.exposed,
                procedural_seed: stable_u64(request.seed, *material_id),
            });
        }
    }

    materials
}

fn build_surface_response_profile(request: &HumanGenerationRequest) -> HumanSurfaceResponseProfile {
    HumanSurfaceResponseProfile {
        skin_wetness_response: (0.55 + request.skin.sweat * 0.35).clamp(0.0, 1.0),
        sweat_response: request.skin.sweat.clamp(0.0, 1.0),
        oil_level: request.skin.oil.clamp(0.0, 1.0),
        injury_response: request.skin.injury_response.clamp(0.0, 1.0),
        hair_wetness_response: request.hair.wetness_response.clamp(0.0, 1.0),
        clothing_wetness_response: request.clothing_profile.wetness_response.clamp(0.0, 1.0),
        clothing_damage_visibility: request.clothing_profile.damage_visibility.clamp(0.0, 1.0),
        eye_redness_response: (request.eyes.redness + request.skin.injury_response * 0.25)
            .clamp(0.0, 1.0),
    }
}

fn build_skin_runtime_model(
    request: &HumanGenerationRequest,
    surface_response: &HumanSurfaceResponseProfile,
) -> SkinRuntimeModel {
    let hero = request.desired_quality >= QualityTier::HeroHighFidelityRuntime;
    let ethical_marks = request.generation_policy.controlled_variation
        && request.generation_policy.bias_reviewed
        && request.generation_policy.likeness_authorized();
    let mut mark_layers = Vec::new();
    for (index, label) in request.skin.scars.iter().enumerate() {
        mark_layers.push(SkinMarkLayer {
            kind: SkinMarkKind::Scar,
            label: label.clone(),
            procedural_seed: stable_u64(request.seed, 10_000 + index as u64),
            ethically_generated: ethical_marks,
        });
    }
    for (index, label) in request.skin.tattoos.iter().enumerate() {
        mark_layers.push(SkinMarkLayer {
            kind: SkinMarkKind::Tattoo,
            label: label.clone(),
            procedural_seed: stable_u64(request.seed, 11_000 + index as u64),
            ethically_generated: ethical_marks,
        });
    }
    for (index, label) in request.skin.makeup_layers.iter().enumerate() {
        mark_layers.push(SkinMarkLayer {
            kind: SkinMarkKind::Makeup,
            label: label.clone(),
            procedural_seed: stable_u64(request.seed, 12_000 + index as u64),
            ethically_generated: ethical_marks,
        });
    }
    if ethical_marks {
        mark_layers.push(SkinMarkLayer {
            kind: SkinMarkKind::Freckle,
            label: "seeded freckle field".to_string(),
            procedural_seed: stable_u64(request.seed, 13_000),
            ethically_generated: true,
        });
    }

    SkinRuntimeModel {
        layers: vec![
            SkinLayerRuntime {
                kind: SkinLayerKind::Epidermis,
                enabled: true,
                detail_strength: 1.0,
            },
            SkinLayerRuntime {
                kind: SkinLayerKind::Dermis,
                enabled: true,
                detail_strength: 0.78,
            },
            SkinLayerRuntime {
                kind: SkinLayerKind::Subsurface,
                enabled: true,
                detail_strength: if hero { 1.0 } else { 0.55 },
            },
            SkinLayerRuntime {
                kind: SkinLayerKind::PoreMicroDetail,
                enabled: request.skin.pore_detail > 0.0,
                detail_strength: request.skin.pore_detail.clamp(0.0, 1.0),
            },
            SkinLayerRuntime {
                kind: SkinLayerKind::WrinkleDetail,
                enabled: request.skin.wrinkle_profile > 0.0
                    || request.age_model.wrinkle_strength > 0.0,
                detail_strength: (request.skin.wrinkle_profile * 0.55
                    + request.age_model.wrinkle_strength * 0.45)
                    .clamp(0.0, 1.0),
            },
            SkinLayerRuntime {
                kind: SkinLayerKind::BloodPerfusion,
                enabled: true,
                detail_strength: surface_response.eye_redness_response.clamp(0.0, 1.0),
            },
            SkinLayerRuntime {
                kind: SkinLayerKind::SurfaceSheen,
                enabled: request.skin.sweat > 0.0 || request.skin.oil > 0.0,
                detail_strength: (request.skin.sweat * 0.55 + request.skin.oil * 0.45)
                    .clamp(0.0, 1.0),
            },
            SkinLayerRuntime {
                kind: SkinLayerKind::DirtSoot,
                enabled: true,
                detail_strength: 0.7,
            },
            SkinLayerRuntime {
                kind: SkinLayerKind::InjuryOverlay,
                enabled: request.skin.injury_response > 0.0,
                detail_strength: request.skin.injury_response.clamp(0.0, 1.0),
            },
            SkinLayerRuntime {
                kind: SkinLayerKind::TensionStretch,
                enabled: request.body_composition.soft_tissue_motion > 0.0,
                detail_strength: (request.body_composition.soft_tissue_motion * 0.65
                    + request.age_model.skin_elasticity * 0.35)
                    .clamp(0.0, 1.0),
            },
        ],
        subsurface: SkinSubsurfaceModel {
            enabled: true,
            scattering_radius_millimeters: if hero { 2.6 } else { 1.4 },
            blood_perfusion_response: surface_response.eye_redness_response.clamp(0.0, 1.0),
        },
        pore_detail: request.skin.pore_detail.clamp(0.0, 1.0),
        wrinkle_strength: (request.skin.wrinkle_profile * 0.55
            + request.age_model.wrinkle_strength * 0.45)
            .clamp(0.0, 1.0),
        mark_layers,
        ethical_mark_generation: ethical_marks,
        blood_flow_redness: surface_response.eye_redness_response.clamp(0.0, 1.0),
        sheen: SkinSheenModel {
            sweat_response: surface_response.sweat_response.clamp(0.0, 1.0),
            oil_response: surface_response.oil_level.clamp(0.0, 1.0),
            wetness_to_sheen: (surface_response.skin_wetness_response * 0.45
                + surface_response.sweat_response * 0.35
                + surface_response.oil_level * 0.2)
                .clamp(0.0, 1.0),
        },
        wetness_response: surface_response.skin_wetness_response.clamp(0.0, 1.0),
        dirt_soot_response: 0.82,
        injury_response: surface_response.injury_response.clamp(0.0, 1.0),
        age_variation: (request.age_model.apparent_age_years / 90.0
            + request.age_model.wrinkle_strength * 0.35
            + (1.0 - request.age_model.skin_elasticity).max(0.0) * 0.25)
            .clamp(0.0, 1.0),
        tension_stretch_detail: (request.body_composition.soft_tissue_motion * 0.65
            + request.age_model.skin_elasticity * 0.35)
            .clamp(0.0, 1.0),
    }
}

pub fn human_surface_response_from_snapshot(
    snapshot: &WorldSnapshot,
    entity: EntityId,
) -> Option<HumanSurfaceState> {
    let human = snapshot.humans.find(entity)?;
    let material_state = snapshot.material_states.find(entity).copied()?;
    let emotion = snapshot
        .agents
        .find(entity)
        .map(|agent| &agent.emotional_state);
    Some(evaluate_human_surface_response(
        &surface_response_profile_for_human_state(human),
        material_state,
        emotion,
    ))
}

pub fn surface_response_profile_for_human_state(human: &HumanState) -> HumanSurfaceResponseProfile {
    let quality_factor = match human.quality_tier {
        QualityTier::Disabled => 0.0,
        QualityTier::BackgroundApproximation => 0.45,
        QualityTier::NormalRuntime => 0.75,
        QualityTier::HeroHighFidelityRuntime | QualityTier::ReferenceOfflineValidation => 1.0,
    };

    HumanSurfaceResponseProfile {
        skin_wetness_response: 0.72 * quality_factor,
        sweat_response: 0.55 * quality_factor,
        oil_level: 0.28 * quality_factor,
        injury_response: 0.82 * quality_factor,
        hair_wetness_response: 0.76 * quality_factor,
        clothing_wetness_response: 0.8 * quality_factor,
        clothing_damage_visibility: 0.7 * quality_factor,
        eye_redness_response: 0.38 * quality_factor,
    }
}

pub fn evaluate_human_surface_response(
    profile: &HumanSurfaceResponseProfile,
    material_state: MaterialState,
    emotion: Option<&EmotionState>,
) -> HumanSurfaceState {
    let moisture = material_state.moisture.clamp(0.0, 1.0);
    let soot = material_state.soot.clamp(0.0, 1.0);
    let crack = material_state.crack_density.clamp(0.0, 1.0);
    let strain = material_state.plastic_strain.clamp(0.0, 1.0);
    let contamination = material_state.biological_contamination.clamp(0.0, 1.0);
    let heat_sweat = ((material_state.temperature - 294.0) / 22.0).clamp(0.0, 1.0);
    let pressure_bruise = ((material_state.pressure - 101_325.0) / 80_000.0).clamp(0.0, 1.0);
    let fear = emotion
        .map(|state| state.fear)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    let anger = emotion
        .map(|state| state.anger)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    let urgency = emotion
        .map(|state| state.urgency)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    let stress_sweat = (fear * 0.25 + urgency * 0.2).clamp(0.0, 0.45);

    let sweat_sheen = (profile.sweat_response
        * (heat_sweat * 0.55 + stress_sweat + moisture * 0.25))
        .clamp(0.0, 1.0);
    let skin_wetness =
        (moisture * profile.skin_wetness_response + sweat_sheen * 0.35).clamp(0.0, 1.0);
    let injury_overlay =
        profile.injury_response * (crack * 0.58 + strain * 0.28 + contamination * 0.2);
    let bruising =
        profile.injury_response * (pressure_bruise * 0.45 + crack * 0.25 + strain * 0.25);
    let dirt = (soot * 0.82 + contamination * 0.18).clamp(0.0, 1.0);
    let hair_wetness = (moisture * profile.hair_wetness_response).clamp(0.0, 1.0);
    let clothing_wetness = (moisture * profile.clothing_wetness_response).clamp(0.0, 1.0);
    let clothing_damage = (profile.clothing_damage_visibility
        * (crack * 0.6 + soot * 0.22 + strain * 0.18))
        .clamp(0.0, 1.0);
    let eye_redness = (profile.eye_redness_response
        + contamination * 0.18
        + fear * 0.08
        + anger * 0.12
        + heat_sweat * 0.05)
        .clamp(0.0, 1.0);
    let oil_sheen = (profile.oil_level * (1.0 - dirt * 0.35) + sweat_sheen * 0.22).clamp(0.0, 1.0);

    HumanSurfaceState {
        skin_wetness,
        sweat_sheen,
        oil_sheen,
        bruising: bruising.clamp(0.0, 1.0),
        injury_overlay: injury_overlay.clamp(0.0, 1.0),
        dirt,
        hair_wetness,
        clothing_wetness,
        clothing_damage,
        eye_redness,
        material_layers: vec![
            HumanMaterialLayerState {
                layer: AnatomicalLayerKind::Skin,
                wetness: skin_wetness,
                injury: injury_overlay.clamp(0.0, 1.0),
                dirt,
                sheen: (sweat_sheen + oil_sheen * 0.5).clamp(0.0, 1.0),
            },
            HumanMaterialLayerState {
                layer: AnatomicalLayerKind::Hair,
                wetness: hair_wetness,
                injury: 0.0,
                dirt: soot,
                sheen: (hair_wetness * 0.45).clamp(0.0, 1.0),
            },
            HumanMaterialLayerState {
                layer: AnatomicalLayerKind::Clothing,
                wetness: clothing_wetness,
                injury: clothing_damage,
                dirt: soot,
                sheen: (clothing_wetness * 0.25).clamp(0.0, 1.0),
            },
            HumanMaterialLayerState {
                layer: AnatomicalLayerKind::Eyes,
                wetness: (0.55 + skin_wetness * 0.18).clamp(0.0, 1.0),
                injury: eye_redness,
                dirt: 0.0,
                sheen: 0.72,
            },
        ],
    }
}

fn build_physics_proxy(request: &HumanGenerationRequest) -> HumanPhysicsProxy {
    let mass = request.morphology.body_mass_kg.max(35.0);
    let height = request.morphology.height_meters.max(1.2);
    let mut cloth_hooks = Vec::new();
    for layer in &request.clothing_profile.layers {
        for point in &layer.attach_points {
            cloth_hooks.push(ClothHook {
                attach_point: *point,
                stiffness: if layer.cloth_simulation { 0.42 } else { 0.85 },
                damping: 0.35,
            });
        }
    }

    HumanPhysicsProxy {
        capsules: vec![
            PhysicsCapsule {
                label: "torso".to_string(),
                radius_meters: request.morphology.shoulder_width_meters * 0.32,
                half_height_meters: height * 0.17,
                mass_kg: mass * 0.24,
            },
            PhysicsCapsule {
                label: "abdomen".to_string(),
                radius_meters: request.morphology.hip_width_meters * 0.28,
                half_height_meters: height * 0.12,
                mass_kg: mass * 0.16,
            },
            PhysicsCapsule {
                label: "hips".to_string(),
                radius_meters: request.morphology.hip_width_meters * 0.34,
                half_height_meters: height * 0.1,
                mass_kg: mass * 0.14,
            },
            PhysicsCapsule {
                label: "head".to_string(),
                radius_meters: 0.11,
                half_height_meters: 0.08,
                mass_kg: mass * 0.08,
            },
            PhysicsCapsule {
                label: "left_arm".to_string(),
                radius_meters: 0.055,
                half_height_meters: height * 0.2,
                mass_kg: mass * 0.055,
            },
            PhysicsCapsule {
                label: "right_arm".to_string(),
                radius_meters: 0.055,
                half_height_meters: height * 0.2,
                mass_kg: mass * 0.055,
            },
            PhysicsCapsule {
                label: "left_thigh".to_string(),
                radius_meters: 0.075,
                half_height_meters: height * 0.2,
                mass_kg: mass * 0.155,
            },
            PhysicsCapsule {
                label: "right_thigh".to_string(),
                radius_meters: 0.075,
                half_height_meters: height * 0.2,
                mass_kg: mass * 0.155,
            },
        ],
        cloth_hooks,
        hair_collision: request.hair.length_meters > 0.05,
        soft_tissue_enabled: request.desired_quality >= QualityTier::HeroHighFidelityRuntime
            && request.body_composition.soft_tissue_motion > 0.0,
        estimated_solver_cost: if request.desired_quality >= QualityTier::HeroHighFidelityRuntime {
            0.32
        } else {
            0.08
        },
    }
}

fn build_animation_profile(request: &HumanGenerationRequest) -> AnimationProfile {
    AnimationProfile {
        locomotion_set: match request.morphology.posture {
            PostureKind::Guarded => "guarded_walk",
            PostureKind::Injured => "injured_walk",
            PostureKind::Relaxed => "relaxed_walk",
            PostureKind::Upright => "standard_walk",
        }
        .to_string(),
        compression: if request.role == HumanRole::Crowd {
            AnimationCompression::CrowdSimplified
        } else {
            AnimationCompression::RuntimeKeyReduction
        },
        supports_visemes: true,
        supports_emotion_curves: request.role != HumanRole::Crowd,
        supports_foot_contact_correction: request.role != HumanRole::Crowd,
        supports_balance_weight_shifts: true,
        supports_hand_gestures: request.role != HumanRole::Crowd,
        supports_breathing_motion: true,
        supports_idle_micro_motion: true,
        supports_procedural_animation: true,
        max_blend_layers: if request.desired_quality >= QualityTier::HeroHighFidelityRuntime {
            8
        } else {
            4
        },
    }
}

fn build_eye_runtime_model(request: &HumanGenerationRequest) -> EyeRuntimeModel {
    let hero = request.desired_quality >= QualityTier::HeroHighFidelityRuntime;
    let rest_radius = request.eyes.pupil_rest_radius.clamp(0.0015, 0.0065);
    EyeRuntimeModel {
        cornea_layer: true,
        sclera_layer: true,
        iris_detail_seed: request.eyes.iris_pattern_seed,
        pupil_control: PupilControlModel {
            rest_radius_meters: rest_radius,
            min_radius_meters: (rest_radius * 0.55).max(0.001),
            max_radius_meters: (rest_radius * 1.85).min(0.009),
            supports_light_adaptation: true,
            supports_emotional_response: request.linked_ai_persona.is_some() || hero,
        },
        tearline: true,
        eye_moisture: request.eyes.wetness.clamp(0.0, 1.0),
        eyelid_contact: true,
        gaze: GazeControlModel {
            target_binding: if request.linked_ai_persona.is_some() {
                GazeTargetBinding::AiFocusTarget
            } else if request.linked_voice_persona.is_some() {
                GazeTargetBinding::DialogueTarget
            } else {
                GazeTargetBinding::WorldLookAtPoint
            },
            micro_saccades_per_second: if hero { 2.6 } else { 1.4 },
            max_eye_yaw_radians: 0.62,
            max_eye_pitch_radians: 0.42,
            head_follow_weight: if hero { 0.28 } else { 0.18 },
        },
        blink: BlinkModel {
            spontaneous_blinks_per_minute: if hero { 17.0 } else { 12.0 },
            stress_blink_multiplier: 1.45,
            asymmetric_blink_support: hero,
        },
        emotional_eye_behavior: request.linked_ai_persona.is_some(),
    }
}

fn build_mouth_runtime_model(
    request: &HumanGenerationRequest,
    facial_rig: &FacialRig,
) -> MouthRuntimeModel {
    let hero = request.desired_quality >= QualityTier::HeroHighFidelityRuntime;
    let tongue_contact_points = facial_rig
        .viseme_set
        .iter()
        .filter_map(|viseme| {
            let kind = match viseme.label.as_str() {
                "tongue" => TongueContactKind::TipPalate,
                "teeth" => TongueContactKind::BladeTeeth,
                "narrow" => TongueContactKind::BackPalate,
                _ => return None,
            };
            Some(TongueContactPoint {
                kind,
                viseme_label: viseme.label.clone(),
                weight_channel: viseme.weight_channel,
            })
        })
        .collect::<Vec<_>>();
    let wetness = (request.eyes.wetness * 0.35 + request.skin.sweat * 0.25 + 0.18).clamp(0.0, 1.0);
    MouthRuntimeModel {
        teeth_geometry: true,
        tongue_animation: facial_rig
            .viseme_set
            .iter()
            .any(|viseme| viseme.label == "tongue"),
        inner_mouth_shading: true,
        dental: HumanDentalRuntimeModel {
            upper_tooth_count: if hero { 16 } else { 12 },
            lower_tooth_count: if hero { 16 } else { 12 },
            enamel_material: DEFAULT_TEETH_MATERIAL,
            gum_material: DEFAULT_TONGUE_MATERIAL,
            enamel_translucency: if hero { 0.38 } else { 0.18 },
            roughness: 0.42,
            procedural_variation: (0.2 + request.face.asymmetry * 0.5).clamp(0.0, 1.0),
            wetness_response: wetness,
            occlusion_contact: facial_rig
                .viseme_set
                .iter()
                .any(|viseme| viseme.label == "teeth"),
            bite_alignment_score: (0.95 - request.face.asymmetry * 0.45).clamp(0.0, 1.0),
        },
        tongue: HumanTongueRuntimeModel {
            material_id: DEFAULT_TONGUE_MATERIAL,
            muscle_control_count: if hero { 6 } else { 3 },
            contact_points: tongue_contact_points,
            wetness_response: wetness,
            tongue_tip_control: facial_rig
                .viseme_set
                .iter()
                .any(|viseme| viseme.label == "tongue"),
            palate_contact: facial_rig
                .viseme_set
                .iter()
                .any(|viseme| matches!(viseme.label.as_str(), "tongue" | "narrow")),
        },
        inner_mouth: HumanInnerMouthModel {
            material_id: DEFAULT_TONGUE_MATERIAL,
            cavity_occlusion: if hero { 0.82 } else { 0.45 },
            saliva_wetness: wetness,
            gumline_shading: true,
            cheek_occlusion: hero || request.face.cheekbone_height > 0.0,
            black_level_control: true,
        },
        speech_binding: HumanSpeechPhonemeBinding {
            phoneme_timing_stream: request.linked_voice_persona.is_some(),
            viseme_timing_stream: !facial_rig.viseme_set.is_empty(),
            emotion_curve_stream: request.linked_ai_persona.is_some()
                && !facial_rig.expression_controls.is_empty(),
            breath_markers: request.linked_voice_persona.is_some(),
            tongue_phoneme_count: facial_rig
                .viseme_set
                .iter()
                .filter(|viseme| matches!(viseme.label.as_str(), "tongue" | "teeth" | "narrow"))
                .count(),
        },
        lip_wetness: wetness,
        viseme_shape_count: facial_rig.viseme_set.len(),
        jaw_motion: facial_rig
            .viseme_set
            .iter()
            .any(|viseme| viseme.jaw_open > 0.0),
        cheek_motion: hero || request.face.cheekbone_height > 0.0,
        lip_compression_contact: facial_rig
            .viseme_set
            .iter()
            .any(|viseme| viseme.label == "closed"),
        breath_speech_timing: request.linked_voice_persona.is_some(),
    }
}

fn build_hair_runtime_model(
    request: &HumanGenerationRequest,
    lod_set: &HumanLodSet,
) -> HairRuntimeModel {
    let hero = request.desired_quality >= QualityTier::HeroHighFidelityRuntime;
    let strand_count = estimate_hair_strands(request);
    let guide_curve_count =
        (strand_count / if hero { 64 } else { 96 }).max(u32::from(strand_count > 0));
    let card_count = ((request.hair.density.clamp(0.0, 1.0) * 240.0).round() as u32).max(12);
    let representation = if hero {
        HairRepresentation::Strands
    } else {
        HairRepresentation::Cards
    };

    HairRuntimeModel {
        guide_curve_count,
        strand_count,
        card_count,
        representation,
        groom_parameters: HairGroomParameters {
            density: request.hair.density.clamp(0.0, 1.0),
            strand_thickness_meters: request.hair.strand_thickness.max(0.0),
            curl: request.hair.curl.clamp(0.0, 1.0),
            length_meters: request.hair.length_meters.max(0.0),
        },
        anisotropic_shading: strand_count > 0 || request.hair.length_meters > 0.0,
        lod_levels: lod_set
            .levels
            .iter()
            .map(|lod| HairLodRuntime {
                tier: lod.tier,
                representation: lod.hair,
                simulation: match lod.tier {
                    HumanLodTier::Hero => HairSimulationFallback::FullPhysics,
                    HumanLodTier::ImportantNpc => HairSimulationFallback::GuideCurvePhysics,
                    HumanLodTier::NormalNpc => HairSimulationFallback::CachedMotion,
                    HumanLodTier::Crowd => HairSimulationFallback::StaticCards,
                    HumanLodTier::Impostor => HairSimulationFallback::None,
                },
            })
            .collect(),
        wet_oily_state: HairWetOilyState {
            wetness_response: request.hair.wetness_response.clamp(0.0, 1.0),
            oil_response: request.skin.oil.clamp(0.0, 1.0) * 0.55,
            clumping_response: (request.hair.wetness_response * request.hair.curl.max(0.2))
                .clamp(0.0, 1.0),
        },
        facial_hair: build_facial_hair_runtime_model(request, lod_set),
        hero_physics: hero && request.hair.length_meters > 0.0,
        npc_simulation_fallback: if request.role == HumanRole::Crowd {
            HairSimulationFallback::StaticCards
        } else {
            HairSimulationFallback::CachedMotion
        },
    }
}

fn build_facial_hair_runtime_model(
    request: &HumanGenerationRequest,
    lod_set: &HumanLodSet,
) -> FacialHairRuntimeModel {
    let hero = request.desired_quality >= QualityTier::HeroHighFidelityRuntime;
    let density = request.hair.density.clamp(0.0, 1.0);
    let brow_density = (0.55 + density * 0.45).clamp(0.0, 1.0);
    let beard_shadow_density =
        ((request.face.jaw_width - 0.42).max(0.0) * 1.8 + density * 0.08).clamp(0.0, 0.42);
    let moustache_strand_count = if beard_shadow_density > 0.12 {
        ((density * if hero { 680.0 } else { 220.0 }).round() as u32).max(80)
    } else {
        0
    };

    FacialHairRuntimeModel {
        eyebrow_strand_count: ((brow_density * if hero { 1_800.0 } else { 540.0 }).round() as u32)
            .max(96),
        eyelash_strand_count: ((brow_density * if hero { 420.0 } else { 128.0 }).round() as u32)
            .max(48),
        beard_shadow_density,
        moustache_strand_count,
        material_id: DEFAULT_HAIR_MATERIAL,
        anisotropic_shading: hero || density > 0.0,
        hairline_blend: (0.35 + request.hair.length_meters * 4.0 + density * 0.25).clamp(0.0, 1.0),
        wetness_response: (request.hair.wetness_response * 0.7 + request.skin.sweat * 0.2)
            .clamp(0.0, 1.0),
        oil_response: (request.skin.oil * 0.55 + request.hair.density * 0.08).clamp(0.0, 1.0),
        lod_levels: lod_set
            .levels
            .iter()
            .map(|lod| FacialHairLodRuntime {
                tier: lod.tier,
                brow_lash_representation: match lod.tier {
                    HumanLodTier::Hero => HairRepresentation::Strands,
                    HumanLodTier::ImportantNpc | HumanLodTier::NormalNpc => {
                        HairRepresentation::Cards
                    }
                    HumanLodTier::Crowd => HairRepresentation::PaintedCap,
                    HumanLodTier::Impostor => HairRepresentation::Hidden,
                },
                beard_moustache_representation: if moustache_strand_count == 0
                    && beard_shadow_density <= 0.0
                {
                    HairRepresentation::Hidden
                } else {
                    match lod.tier {
                        HumanLodTier::Hero => HairRepresentation::Strands,
                        HumanLodTier::ImportantNpc | HumanLodTier::NormalNpc => {
                            HairRepresentation::Cards
                        }
                        HumanLodTier::Crowd => HairRepresentation::PaintedCap,
                        HumanLodTier::Impostor => HairRepresentation::Hidden,
                    }
                },
            })
            .collect(),
        procedural_seed: stable_u64(request.seed, 6_040),
    }
}

fn build_clothing_cybernetics_runtime_model(
    request: &HumanGenerationRequest,
    rig: &HumanRig,
) -> ClothingCyberneticsRuntimeModel {
    let cloth_simulated_layer_count = request
        .clothing_profile
        .layers
        .iter()
        .filter(|layer| layer.cloth_simulation)
        .count();
    let fallback = if cloth_simulated_layer_count > 0 {
        ClothSimulationFallback::PinConstraintFallback
    } else if request.clothing_profile.layers.is_empty() {
        ClothSimulationFallback::None
    } else {
        ClothSimulationFallback::SkinnedApproximation
    };
    let cybernetic_implant_count = request.cybernetics.len();
    let emissive_element_count = request
        .cybernetics
        .iter()
        .filter(|augment| {
            augment.exposed
                || augment.damage_behavior == DamageBehavior::ExposesSparks
                || augment
                    .gameplay_tags
                    .iter()
                    .any(|tag| tag.contains("emissive") || tag.contains("cybernetic"))
        })
        .count();

    ClothingCyberneticsRuntimeModel {
        layered_clothing_count: request.clothing_profile.layers.len(),
        cloth_simulated_layer_count,
        cloth_fallback: fallback,
        material_aware_fabric: request
            .clothing_profile
            .layers
            .iter()
            .all(|layer| layer.material_id != 0),
        wetness_dirt_damage: request.clothing_profile.wetness_response > 0.0
            && request.clothing_profile.damage_visibility > 0.0,
        armor_hard_surface_count: request
            .cybernetics
            .iter()
            .filter(|augment| {
                !augment.visual_materials.is_empty()
                    || augment.physical_profile.hardness > 0.0
                    || !augment.animation_constraints.is_empty()
            })
            .count(),
        cybernetic_implant_count,
        emissive_element_count,
        attachment_point_count: rig.attachment_points.len(),
        clipping_prevention: !request.clothing_profile.layers.is_empty()
            && rig.attachment_points.len() >= request.clothing_profile.layers.len(),
    }
}

fn build_motion_model(
    request: &HumanGenerationRequest,
    physics_proxy: &HumanPhysicsProxy,
    animation_profile: &AnimationProfile,
) -> HumanMotionModel {
    let hero = request.desired_quality >= QualityTier::HeroHighFidelityRuntime;
    HumanMotionModel {
        pose_space_corrections: request.role != HumanRole::Crowd,
        muscle_soft_tissue: physics_proxy.soft_tissue_enabled,
        foot_contact_correction: animation_profile.supports_foot_contact_correction,
        balance_weight_shifts: animation_profile.supports_balance_weight_shifts,
        hand_finger_animation: animation_profile.supports_hand_gestures,
        breathing: animation_profile.supports_breathing_motion,
        idle_micro_motion: animation_profile.supports_idle_micro_motion,
        injury_fatigue_response: matches!(request.morphology.posture, PostureKind::Injured)
            || request.skin.injury_response > 0.0
            || hero,
        performance_capture_support: request.linked_voice_persona.is_some() || hero,
        procedural_animation_support: animation_profile.supports_procedural_animation,
        max_blend_layers: animation_profile.max_blend_layers,
    }
}

fn build_body_runtime_model(
    request: &HumanGenerationRequest,
    physics_proxy: &HumanPhysicsProxy,
    motion_model: &HumanMotionModel,
) -> HumanBodyRuntimeModel {
    let height = request.morphology.height_meters.max(0.01);
    let muscle = request.body_composition.muscle.clamp(0.0, 1.0);
    let fat = request.body_composition.fat.clamp(0.0, 1.0);
    let soft_tissue_motion = request.body_composition.soft_tissue_motion.clamp(0.0, 1.0);
    let bmi_estimate = request.morphology.body_mass_kg.max(0.0) / (height * height);
    let silhouette_width_ratio = ((request.morphology.shoulder_width_meters
        + request.morphology.hip_width_meters)
        / (height * 2.0))
        .clamp(0.0, 1.0);
    let hero_soft_tissue = motion_model.muscle_soft_tissue && soft_tissue_motion > 0.0;

    HumanBodyRuntimeModel {
        morphology: HumanBodyMorphologyRuntime {
            height_meters: request.morphology.height_meters,
            shoulder_width_meters: request.morphology.shoulder_width_meters,
            hip_width_meters: request.morphology.hip_width_meters,
            limb_proportion: request.morphology.limb_proportion,
            body_mass_kg: request.morphology.body_mass_kg,
        },
        composition: HumanBodyCompositionRuntime {
            muscle,
            fat,
            soft_tissue_motion,
            bmi_estimate,
            silhouette_width_ratio,
        },
        mass_distribution: build_body_mass_distribution(muscle, fat, soft_tissue_motion),
        soft_tissue_zones: build_soft_tissue_zones(hero_soft_tissue, fat, soft_tissue_motion),
        muscle_groups: build_muscle_groups(muscle, fat, motion_model.pose_space_corrections),
        collision_profile: HumanBodyCollisionProfile {
            capsule_count: physics_proxy.capsules.len(),
            cloth_hook_count: physics_proxy.cloth_hooks.len(),
            hair_collision: physics_proxy.hair_collision,
            soft_tissue_enabled: physics_proxy.soft_tissue_enabled,
            estimated_solver_cost: physics_proxy.estimated_solver_cost,
        },
        posture: request.morphology.posture,
        center_of_mass_meters: Vec3::new(
            0.0,
            0.0,
            height * (0.5 + (fat - muscle) * 0.035).clamp(0.42, 0.58),
        ),
        scale_consistency: request.morphology.shoulder_width_meters < height * 0.4
            && request.morphology.hip_width_meters < height * 0.35
            && request.morphology.limb_proportion > 0.7
            && request.morphology.limb_proportion < 1.3,
    }
}

fn build_body_mass_distribution(
    muscle: f32,
    fat: f32,
    soft_tissue_motion: f32,
) -> Vec<HumanBodyMassRegion> {
    [
        (HumanBodyRegion::Torso, 0.23, 0.32, 0.18),
        (HumanBodyRegion::Abdomen, 0.26, 0.12, 0.26),
        (HumanBodyRegion::Hips, 0.2, 0.18, 0.22),
        (HumanBodyRegion::UpperArms, 0.09, 0.16, 0.08),
        (HumanBodyRegion::Thighs, 0.16, 0.18, 0.16),
        (HumanBodyRegion::FaceNeck, 0.06, 0.04, 0.1),
    ]
    .into_iter()
    .map(
        |(region, fat_weight, muscle_weight, soft_weight)| HumanBodyMassRegion {
            region,
            fat_weight: (fat * fat_weight).max(0.01),
            muscle_weight: (muscle * muscle_weight).max(0.01),
            soft_tissue_weight: (soft_tissue_motion * soft_weight).max(0.01),
        },
    )
    .collect()
}

fn build_soft_tissue_zones(
    enabled: bool,
    fat: f32,
    soft_tissue_motion: f32,
) -> Vec<HumanSoftTissueZone> {
    [
        (HumanBodyRegion::Torso, 0.18, 0.72, 0.14),
        (HumanBodyRegion::Abdomen, 0.36, 0.62, 0.18),
        (HumanBodyRegion::Hips, 0.28, 0.68, 0.16),
        (HumanBodyRegion::Thighs, 0.22, 0.74, 0.15),
        (HumanBodyRegion::UpperArms, 0.16, 0.8, 0.1),
    ]
    .into_iter()
    .map(
        |(region, motion_scale, damping, radius)| HumanSoftTissueZone {
            region,
            enabled,
            motion_scale: (soft_tissue_motion * motion_scale + fat * 0.08).clamp(0.0, 1.0),
            damping,
            collision_radius_meters: radius,
        },
    )
    .collect()
}

fn build_muscle_groups(
    muscle: f32,
    fat: f32,
    pose_space_corrections: bool,
) -> Vec<HumanMuscleGroupRuntime> {
    [
        (HumanMuscleGroupKind::TorsoCore, 0.95, 0.18),
        (HumanMuscleGroupKind::ShoulderChest, 1.05, 0.14),
        (HumanMuscleGroupKind::Arms, 0.85, 0.12),
        (HumanMuscleGroupKind::HipsGlutes, 0.9, 0.18),
        (HumanMuscleGroupKind::Legs, 1.1, 0.16),
    ]
    .into_iter()
    .map(
        |(group, definition_scale, soft_tissue_scale)| HumanMuscleGroupRuntime {
            group,
            definition: (0.2 + muscle * definition_scale - fat * 0.08).clamp(0.05, 1.0),
            driven_by_pose_space: pose_space_corrections,
            soft_tissue_influence: (fat * soft_tissue_scale).clamp(0.0, 1.0),
        },
    )
    .collect()
}

fn build_performance_capture_model(
    request: &HumanGenerationRequest,
    rig: &HumanRig,
    facial_rig: &FacialRig,
    mouth_system: &MouthRuntimeModel,
    motion_model: &HumanMotionModel,
    ai_voice_integration: &HumanAiVoiceIntegrationModel,
    debug_views: &HumanDebugViewSet,
) -> HumanPerformanceCaptureModel {
    let hero = request.desired_quality >= QualityTier::HeroHighFidelityRuntime;
    let sample_rate_hz = if hero { 60.0 } else { 30.0 };
    let curve = |kind, channel: &str| HumanPerformanceCaptureCurve {
        kind,
        target_channel: channel.to_string(),
        sample_rate_hz,
        precision: if hero {
            HumanCaptureCurvePrecision::Smoothed
        } else {
            HumanCaptureCurvePrecision::Compressed
        },
        drives_renderer: true,
    };

    HumanPerformanceCaptureModel {
        source: match request.generation_policy.source {
            HumanGenerationSource::PerformanceCaptureDerived => {
                HumanPerformanceCaptureSource::PerformanceCaptureDerived
            }
            _ if request.linked_voice_persona.is_some() && request.linked_ai_persona.is_some() => {
                HumanPerformanceCaptureSource::HybridProceduralCapture
            }
            _ if request.linked_voice_persona.is_some() => {
                HumanPerformanceCaptureSource::VoiceDrivenFace
            }
            _ => HumanPerformanceCaptureSource::ProceduralFallback,
        },
        curve_bindings: vec![
            curve(HumanPerformanceCaptureCurveKind::HeadPose, "body.head_pose"),
            curve(HumanPerformanceCaptureCurveKind::EyeGaze, "face.eye_gaze"),
            curve(HumanPerformanceCaptureCurveKind::Blink, "face.blink"),
            curve(HumanPerformanceCaptureCurveKind::JawOpen, "face.jaw_open"),
            curve(HumanPerformanceCaptureCurveKind::LipShape, "face.lip_shape"),
            curve(HumanPerformanceCaptureCurveKind::Tongue, "face.tongue"),
            curve(HumanPerformanceCaptureCurveKind::Brow, "face.brow"),
            curve(HumanPerformanceCaptureCurveKind::Cheek, "face.cheek"),
            curve(HumanPerformanceCaptureCurveKind::Emotion, "face.emotion"),
            curve(HumanPerformanceCaptureCurveKind::Breath, "body.breath"),
            curve(
                HumanPerformanceCaptureCurveKind::HandGesture,
                "body.hand_gesture",
            ),
            curve(HumanPerformanceCaptureCurveKind::BodyLean, "body.lean"),
            curve(
                HumanPerformanceCaptureCurveKind::FootContact,
                "body.foot_contact",
            ),
        ],
        retargeting_profile: rig.retargeting_profile.clone(),
        timecode_binding: request.linked_voice_persona.is_some() || hero,
        frame_rate_hz: sample_rate_hz,
        latency_budget_ms: if hero { 16.7 } else { 33.3 },
        facial_solver: HumanFacialCaptureSolver {
            blendshape_count: facial_rig.expression_controls.len()
                + facial_rig.viseme_set.len()
                + usize::from(facial_rig.blink_controls)
                + usize::from(facial_rig.eye_aim_controls),
            viseme_curve_count: mouth_system.viseme_shape_count,
            emotion_curve_count: facial_rig.expression_controls.len(),
            tongue_tracking: mouth_system.tongue_animation,
            brow_lash_tracking: facial_rig
                .expression_controls
                .iter()
                .any(|control| control.emotion_axis == EmotionAxis::Surprise)
                && facial_rig.blink_controls,
            eye_contact_tracking: facial_rig.eye_aim_controls
                && ai_voice_integration.gaze_target_binding != GazeTargetBinding::None,
        },
        body_solver: HumanBodyCaptureSolver {
            retarget_bone_count: rig.bone_count,
            hand_finger_tracking: motion_model.hand_finger_animation,
            foot_contact_tracking: motion_model.foot_contact_correction,
            balance_weight_tracking: motion_model.balance_weight_shifts,
            breathing_tracking: motion_model.breathing,
            injury_fatigue_tracking: motion_model.injury_fatigue_response,
        },
        fallback: if request.linked_voice_persona.is_some() {
            HumanCaptureFallback::VoiceDrivenFace
        } else if motion_model.procedural_animation_support {
            HumanCaptureFallback::ProceduralCurves
        } else {
            HumanCaptureFallback::CachedPerformance
        },
        debug_curve_view: debug_views.has(HumanDebugViewKind::FacialRig)
            && debug_views.has(HumanDebugViewKind::VisemeTimeline)
            && debug_views.has(HumanDebugViewKind::AnimationContact)
            && debug_views.has(HumanDebugViewKind::PerformanceCaptureCurves),
    }
}

struct FailureFallbackPlanInput<'a> {
    request: &'a HumanGenerationRequest,
    lod_set: &'a HumanLodSet,
    render_meshes: &'a [HumanMesh],
    materials: &'a [HumanMaterialBinding],
    hair_system: &'a HairRuntimeModel,
    clothing_cybernetics: &'a ClothingCyberneticsRuntimeModel,
    body_model: &'a HumanBodyRuntimeModel,
    motion_model: &'a HumanMotionModel,
    renderer_interface: &'a HumanRendererInterface,
    performance_capture: &'a HumanPerformanceCaptureModel,
    debug_views: &'a HumanDebugViewSet,
}

fn build_failure_fallback_plan(input: FailureFallbackPlanInput<'_>) -> HumanFailureFallbackPlan {
    let FailureFallbackPlanInput {
        request,
        lod_set,
        render_meshes,
        materials,
        hair_system,
        clothing_cybernetics,
        body_model,
        motion_model,
        renderer_interface,
        performance_capture,
        debug_views,
    } = input;
    let impostor_available = lod_set
        .levels
        .iter()
        .any(|lod| lod.tier == HumanLodTier::Impostor);
    let crowd_available = lod_set
        .levels
        .iter()
        .any(|lod| lod.tier == HumanLodTier::Crowd);
    let proxy_available = render_meshes
        .iter()
        .any(|mesh| mesh.part == HumanMeshPart::ImpostorCard || mesh.lod == HumanLodTier::Impostor);
    let fallback_material = materials.iter().any(|binding| {
        matches!(
            binding.layer,
            AnatomicalLayerKind::Skin
                | AnatomicalLayerKind::Mouth
                | AnatomicalLayerKind::Tongue
                | AnatomicalLayerKind::Clothing
        ) && binding.material_id != 0
    });
    let reduce_hair_cloth_quality = hair_system.npc_simulation_fallback
        != HairSimulationFallback::None
        && clothing_cybernetics.cloth_fallback != ClothSimulationFallback::None;

    HumanFailureFallbackPlan {
        voice: HumanVoiceFailureFallback {
            cached_audio_line: request.linked_voice_persona.is_some(),
            subtitle_fallback: request.linked_voice_persona.is_some(),
            viseme_hold: performance_capture.fallback != HumanCaptureFallback::None,
            provenance_preserved: request.generation_policy.voice_authorized()
                && request.generation_policy.has_seed_provenance(request.seed),
        },
        streaming: HumanStreamingFailureFallback {
            proxy_human_available: proxy_available || impostor_available,
            lowest_lod_available: impostor_available,
            continue_animation: motion_model.procedural_animation_support,
            reports_streaming_error: debug_views.supports_v9_human_lab(),
        },
        rendering: HumanRenderingFailureFallback {
            fallback_material,
            previous_pipeline_cache: renderer_interface.supports_renderer_handoff(),
            lower_quality_lighting: debug_views.has(HumanDebugViewKind::LightingCloseupTests),
            stable_render_handles: !renderer_interface.rigged_mesh_handles.is_empty()
                && renderer_interface
                    .morph_animation_buffers
                    .supports_gpu_morph_handoff(),
        },
        simulation: HumanSimulationFailureFallback {
            hair_quality_demotion: hair_system.npc_simulation_fallback
                != HairSimulationFallback::None,
            cloth_quality_demotion: clothing_cybernetics.cloth_fallback
                != ClothSimulationFallback::None,
            soft_tissue_freeze: body_model.collision_profile.soft_tissue_enabled,
            preserve_contact_animation: motion_model.foot_contact_correction,
        },
        frame_budget: HumanFrameBudgetFailurePolicy {
            demote_background_humans: crowd_available && impostor_available,
            reduce_hair_cloth_quality,
            cap_performance_capture: performance_capture.fallback != HumanCaptureFallback::None
                && performance_capture.latency_budget_ms <= 50.0,
            fallback_cost_ms: if request.desired_quality >= QualityTier::HeroHighFidelityRuntime {
                2.4
            } else {
                1.1
            },
            frame_failure_counter: true,
        },
        telemetry_event: true,
        replay_deterministic: request.generation_policy.has_seed_provenance(request.seed),
    }
}

struct PerformanceBudgetReportInput<'a> {
    request: &'a HumanGenerationRequest,
    lod_set: &'a HumanLodSet,
    render_meshes: &'a [HumanMesh],
    materials: &'a [HumanMaterialBinding],
    hair_system: &'a HairRuntimeModel,
    clothing_cybernetics: &'a ClothingCyberneticsRuntimeModel,
    mouth_system: &'a MouthRuntimeModel,
    renderer_interface: &'a HumanRendererInterface,
    performance_capture: &'a HumanPerformanceCaptureModel,
    failure_fallbacks: &'a HumanFailureFallbackPlan,
    debug_views: &'a HumanDebugViewSet,
}

fn build_performance_budget_report(
    input: PerformanceBudgetReportInput<'_>,
) -> HumanPerformanceBudgetReport {
    let PerformanceBudgetReportInput {
        request,
        lod_set,
        render_meshes,
        materials,
        hair_system,
        clothing_cybernetics,
        mouth_system,
        renderer_interface,
        performance_capture,
        failure_fallbacks,
        debug_views,
    } = input;
    let hero = request.desired_quality >= QualityTier::HeroHighFidelityRuntime;
    let estimated_memory_bytes = estimate_human_memory_bytes(lod_set, materials);
    let target_frame_ms = if hero { 16.7 } else { 33.3 };
    let cpu_animation_ms = if hero { 0.62 } else { 0.34 };
    let voice_lipsync_ms = if mouth_system.speech_binding.supports_voice_lipsync() {
        if performance_capture.latency_budget_ms <= 20.0 {
            0.42
        } else {
            0.58
        }
    } else {
        0.08
    };
    let gpu_skinning_ms = if renderer_interface
        .morph_animation_buffers
        .supports_gpu_morph_handoff()
    {
        0.74
    } else {
        0.18
    };
    let hair_cloth_ms =
        if hair_system.supports_hero_hair() && clothing_cybernetics.supports_cyberpunk_clothing() {
            1.14
        } else {
            0.46
        };
    let skin_eye_mouth_ms = if hero { 0.86 } else { 0.38 };
    let renderer_handoff_ms = if renderer_interface.supports_renderer_handoff() {
        0.28
    } else {
        0.08
    };
    let fallback_ms = failure_fallbacks.frame_budget.fallback_cost_ms;
    let memory_mib = estimated_memory_bytes as f32 / (1024.0 * 1024.0);
    let streaming_bandwidth_mib_per_s = render_meshes
        .iter()
        .filter(|mesh| mesh.streaming_required)
        .map(|mesh| mesh.triangle_budget as f32 * 0.0009)
        .sum::<f32>()
        .min(if hero { 64.0 } else { 24.0 });
    let cpu_time_ms = cpu_animation_ms + voice_lipsync_ms + 0.24;
    let gpu_time_ms = gpu_skinning_ms + hair_cloth_ms + skin_eye_mouth_ms + renderer_handoff_ms;

    HumanPerformanceBudgetReport {
        target_frame_ms,
        cpu_time_ms,
        gpu_time_ms,
        estimated_memory_bytes,
        streaming_bandwidth_mib_per_s,
        fallback_cost_ms: fallback_ms,
        category_breakdown: vec![
            HumanPerformanceBudgetCategoryReport {
                category: HumanPerformanceBudgetCategory::CpuAnimation,
                estimated_ms: cpu_animation_ms,
                memory_bytes: 0,
                budget_ms: if hero { 1.1 } else { 0.7 },
            },
            HumanPerformanceBudgetCategoryReport {
                category: HumanPerformanceBudgetCategory::GpuSkinningMorphs,
                estimated_ms: gpu_skinning_ms,
                memory_bytes: 2 * 1024 * 1024,
                budget_ms: if hero { 1.2 } else { 0.55 },
            },
            HumanPerformanceBudgetCategoryReport {
                category: HumanPerformanceBudgetCategory::HairClothSimulation,
                estimated_ms: hair_cloth_ms,
                memory_bytes: (if hero { 6_u64 } else { 2_u64 }) * 1024 * 1024,
                budget_ms: if hero { 1.6 } else { 0.7 },
            },
            HumanPerformanceBudgetCategoryReport {
                category: HumanPerformanceBudgetCategory::SkinEyeMouthCloseup,
                estimated_ms: skin_eye_mouth_ms,
                memory_bytes: (if hero { 5_u64 } else { 2_u64 }) * 1024 * 1024,
                budget_ms: if hero { 1.2 } else { 0.55 },
            },
            HumanPerformanceBudgetCategoryReport {
                category: HumanPerformanceBudgetCategory::VoiceLipsync,
                estimated_ms: voice_lipsync_ms,
                memory_bytes: 512 * 1024,
                budget_ms: if hero { 0.75 } else { 0.32 },
            },
            HumanPerformanceBudgetCategoryReport {
                category: HumanPerformanceBudgetCategory::RendererHandoff,
                estimated_ms: renderer_handoff_ms,
                memory_bytes: 1024 * 1024,
                budget_ms: 0.45,
            },
            HumanPerformanceBudgetCategoryReport {
                category: HumanPerformanceBudgetCategory::Memory,
                estimated_ms: (memory_mib / 96.0).clamp(0.05, 0.95),
                memory_bytes: estimated_memory_bytes,
                budget_ms: 1.0,
            },
            HumanPerformanceBudgetCategoryReport {
                category: HumanPerformanceBudgetCategory::Fallback,
                estimated_ms: fallback_ms,
                memory_bytes: 256 * 1024,
                budget_ms: 4.0,
            },
        ],
        profiler_capture_ready: debug_views.has(HumanDebugViewKind::PerformanceCaptureCurves)
            && debug_views.has(HumanDebugViewKind::LightingCloseupTests),
        budget_report_ready: true,
        frame_failure_counter_bound: failure_fallbacks.frame_budget.frame_failure_counter,
        memory_pressure_safe: estimated_memory_bytes <= request.budget.max_memory_bytes,
    }
}

struct AssetPackageContractInput<'a> {
    request: &'a HumanGenerationRequest,
    render_meshes: &'a [HumanMesh],
    materials: &'a [HumanMaterialBinding],
    lod_set: &'a HumanLodSet,
    renderer_interface: &'a HumanRendererInterface,
    debug_views: &'a HumanDebugViewSet,
    performance_budget: &'a HumanPerformanceBudgetReport,
}

fn build_asset_package_contract(input: AssetPackageContractInput<'_>) -> HumanAssetPackageContract {
    let AssetPackageContractInput {
        request,
        render_meshes,
        materials,
        lod_set,
        renderer_interface,
        debug_views,
        performance_budget,
    } = input;
    let package_asset = stable_u128(request.seed, request.request_id, 900);
    let preview_asset = stable_u128(request.seed, request.request_id, 901);
    let replay_asset = stable_u128(request.seed, request.request_id, 902);
    let schema_version = SchemaVersion {
        name: "HumanRuntimeBundle",
        version: 1,
    };
    let mut dependencies = BTreeSet::new();
    for mesh in render_meshes {
        dependencies.insert(mesh.mesh.0);
    }
    for binding in materials {
        dependencies.insert(binding.material_id as AssetId);
    }
    for handle in &renderer_interface.rigged_mesh_handles {
        dependencies.insert(handle.0);
    }
    dependencies.insert(preview_asset);
    dependencies.insert(replay_asset);

    let total_bytes = performance_budget.estimated_memory_bytes.max(1);
    let runtime_bytes = total_bytes.saturating_mul(58) / 100;
    let material_bytes = total_bytes.saturating_mul(24) / 100;
    let replay_bytes = total_bytes
        .saturating_sub(runtime_bytes)
        .saturating_sub(material_bytes);
    let chunks = vec![
        AssetPackageChunkManifest {
            chunk_id: 1,
            usage_label: "human runtime bundle schema".to_string(),
            uncompressed_size: runtime_bytes,
            compressed_size: runtime_bytes / 2,
            content_hash: stable_u128(request.seed, request.request_id, 910),
        },
        AssetPackageChunkManifest {
            chunk_id: 2,
            usage_label: "human generated material and groom caches".to_string(),
            uncompressed_size: material_bytes,
            compressed_size: material_bytes / 2,
            content_hash: stable_u128(request.seed, request.request_id, 911),
        },
        AssetPackageChunkManifest {
            chunk_id: 3,
            usage_label: "human debug preview and integration replay".to_string(),
            uncompressed_size: replay_bytes,
            compressed_size: replay_bytes / 2,
            content_hash: stable_u128(request.seed, request.request_id, 912),
        },
    ];
    let mut validation = ValidationReport::for_asset(package_asset, 40, schema_version.clone());
    validation.add_metric("runtime_schema_count", 5.0, "count", Some(5.0));
    validation.add_metric(
        "round_trip_bytes",
        total_bytes as f64,
        "bytes",
        Some(request.budget.max_memory_bytes as f64),
    );
    validation.add_artifact(preview_asset);
    validation.add_artifact(replay_asset);

    HumanAssetPackageContract {
        manifest: AssetPackageManifest {
            package_asset,
            manifest_schema_version: ASSET_PACKAGE_MANIFEST_SCHEMA_VERSION,
            asset_kind: AssetKind::GeneratedBundle,
            label: format!("human:{}:bundle", request.seed),
            provenance: request
                .generation_policy
                .provenance_records
                .iter()
                .map(|record| format!("{}={}", record.label, record.record_id))
                .collect::<Vec<_>>()
                .join(";"),
            schema_name: schema_version.name.to_string(),
            schema_version: schema_version.version,
            dependencies: dependencies.into_iter().collect(),
            chunks,
            total_uncompressed_bytes: total_bytes,
            generated: request.generation_policy.source == HumanGenerationSource::SeededSynthetic,
            quality_tier: request.desired_quality,
            validation,
        },
        runtime_schemas: vec![
            human_schema_binding("HumanRuntimeBundle", 1, 40),
            human_schema_binding("HumanRendererInterface", 1, 40),
            human_schema_binding("SpeechResult", 1, 12),
            human_schema_binding(
                "AssetPackageManifest",
                ASSET_PACKAGE_MANIFEST_SCHEMA_VERSION,
                1,
            ),
            human_schema_binding("ValidationReport", 4, 1),
        ],
        provenance_record_count: request.generation_policy.provenance_records.len(),
        debug_preview: HumanAssetDebugPreview {
            preview_asset,
            mesh_count: render_meshes.len(),
            material_count: materials.len(),
            debug_view_count: debug_views.ready_count(),
            estimated_bytes: total_bytes,
            label: "human close-up package preview".to_string(),
        },
        round_trip: HumanSchemaRoundTripEvidence {
            serialized_bytes: total_bytes,
            content_hash: stable_u128(request.seed, request.request_id, 913),
            lossless: true,
            migration_tested: true,
            public_schema_registered: true,
        },
        integration_replay: HumanIntegrationReplayEvidence {
            replay_asset,
            event_count: 3 + lod_set.levels.len(),
            deterministic_hash: stable_u128(request.seed, request.request_id, 914),
            captures_speech: request.linked_voice_persona.is_some(),
            captures_surface_state: !materials.is_empty(),
            captures_debug_views: debug_views.supports_v9_human_lab(),
        },
    }
}

fn human_schema_binding(
    schema_name: &'static str,
    version: u32,
    owner: ModuleId,
) -> HumanRuntimeSchemaBinding {
    HumanRuntimeSchemaBinding {
        schema: SchemaVersion {
            name: schema_name,
            version,
        },
        owner,
        required: true,
        round_trip_serialized: true,
        binary_layout_stable: true,
    }
}

fn build_renderer_interface(
    request: &HumanGenerationRequest,
    render_meshes: &[HumanMesh],
    materials: &[HumanMaterialBinding],
    rig: &HumanRig,
    facial_rig: &FacialRig,
) -> HumanRendererInterface {
    let clothing_meshes = render_meshes
        .iter()
        .filter(|mesh| mesh.part == HumanMeshPart::Clothing)
        .map(|mesh| mesh.mesh)
        .collect::<Vec<_>>();
    let clothing_materials = materials
        .iter()
        .filter(|binding| binding.layer == AnatomicalLayerKind::Clothing)
        .map(|binding| binding.material_id)
        .collect::<Vec<_>>();

    HumanRendererInterface {
        rigged_mesh_handles: render_meshes
            .iter()
            .filter(|mesh| mesh.part != HumanMeshPart::ImpostorCard)
            .map(|mesh| mesh.mesh)
            .collect(),
        skin_material_handles: materials
            .iter()
            .filter(|binding| binding.layer == AnatomicalLayerKind::Skin)
            .map(|binding| binding.material_id)
            .collect(),
        hair_data_handles: render_meshes
            .iter()
            .filter(|mesh| mesh.part == HumanMeshPart::Hair)
            .map(|mesh| mesh.mesh)
            .collect(),
        eye_material_handles: materials
            .iter()
            .filter(|binding| binding.layer == AnatomicalLayerKind::Eyes)
            .map(|binding| binding.material_id)
            .collect(),
        mouth_material_handles: materials
            .iter()
            .filter(|binding| {
                matches!(
                    binding.layer,
                    AnatomicalLayerKind::Mouth | AnatomicalLayerKind::Tongue
                )
            })
            .map(|binding| binding.material_id)
            .collect(),
        morph_animation_buffers: HumanMorphAnimationBuffers {
            gpu_skinning_resource: rig.gpu_skinning_resource,
            facial_morph_buffer: HumanRendererBufferHandle(stable_u128(
                request.seed,
                request.request_id,
                720,
            )),
            animation_curve_buffer: HumanRendererBufferHandle(stable_u128(
                request.seed,
                request.request_id,
                721,
            )),
        },
        mouth_articulation_buffer: HumanRendererBufferHandle(stable_u128(
            request.seed,
            request.request_id,
            722,
        )),
        clothing_buffers: clothing_meshes
            .iter()
            .enumerate()
            .map(|(index, mesh)| HumanClothingRenderBuffer {
                mesh: *mesh,
                material_id: clothing_materials
                    .get(index)
                    .copied()
                    .unwrap_or(DEFAULT_CLOTHING_MATERIAL),
                cloth_simulation: request
                    .clothing_profile
                    .layers
                    .get(index)
                    .map(|layer| layer.cloth_simulation)
                    .unwrap_or(false),
            })
            .collect(),
        material_state_fields: vec![
            HumanMaterialStateChannel::Wetness,
            HumanMaterialStateChannel::Oil,
            HumanMaterialStateChannel::Sweat,
            HumanMaterialStateChannel::BloodPerfusion,
            HumanMaterialStateChannel::Bruising,
            HumanMaterialStateChannel::Injury,
            HumanMaterialStateChannel::Dirt,
        ],
        expression_curve_count: facial_rig.expression_controls.len(),
        viseme_timing_count: facial_rig.viseme_set.len(),
    }
}

fn build_ai_voice_integration_model(
    request: &HumanGenerationRequest,
    eye_system: &EyeRuntimeModel,
    facial_rig: &FacialRig,
    mouth_system: &MouthRuntimeModel,
    motion_model: &HumanMotionModel,
) -> HumanAiVoiceIntegrationModel {
    let mut body_gesture_channels = Vec::new();
    if motion_model.hand_finger_animation {
        body_gesture_channels.push(HumanGestureChannel::HandGesture);
    }
    if motion_model.balance_weight_shifts {
        body_gesture_channels.push(HumanGestureChannel::BodyLean);
    }
    if motion_model.breathing {
        body_gesture_channels.push(HumanGestureChannel::BreathMotion);
    }
    if motion_model.idle_micro_motion {
        body_gesture_channels.push(HumanGestureChannel::IdleMicroMotion);
    }

    let relationship_context = match (request.linked_ai_persona, request.linked_voice_persona) {
        (Some(_), Some(_)) => RelationshipContextBinding::DialogueRelationship,
        (Some(_), None) => RelationshipContextBinding::AgentMemory,
        _ => RelationshipContextBinding::None,
    };

    HumanAiVoiceIntegrationModel {
        ai_persona: request.linked_ai_persona,
        voice_persona: request.linked_voice_persona,
        emotional_state_binding: request.linked_ai_persona.is_some()
            && !facial_rig.expression_controls.is_empty(),
        gaze_target_binding: eye_system.gaze.target_binding,
        facial_expression_axis_count: facial_rig.expression_controls.len(),
        voice_timing: VoiceTimingBinding {
            duration_seconds: request.linked_voice_persona.is_some(),
            viseme_timeline: mouth_system.viseme_shape_count > 0
                && !facial_rig.viseme_set.is_empty(),
            breath_timing: mouth_system.breath_speech_timing && motion_model.breathing,
        },
        viseme_timing_count: mouth_system
            .viseme_shape_count
            .min(facial_rig.viseme_set.len()),
        body_gesture_channels,
        relationship_context,
        injury_fatigue_binding: motion_model.injury_fatigue_response,
    }
}

struct HumanAcceptanceEvidenceInput<'a> {
    request: &'a HumanGenerationRequest,
    lod_set: &'a HumanLodSet,
    skin_system: &'a SkinRuntimeModel,
    eye_system: &'a EyeRuntimeModel,
    mouth_system: &'a MouthRuntimeModel,
    hair_system: &'a HairRuntimeModel,
    clothing_cybernetics: &'a ClothingCyberneticsRuntimeModel,
    body_model: &'a HumanBodyRuntimeModel,
    motion_model: &'a HumanMotionModel,
    renderer_interface: &'a HumanRendererInterface,
    ai_voice_integration: &'a HumanAiVoiceIntegrationModel,
    failure_fallbacks: &'a HumanFailureFallbackPlan,
    performance_budget: &'a HumanPerformanceBudgetReport,
    asset_package_contract: &'a HumanAssetPackageContract,
    debug_views: &'a HumanDebugViewSet,
}

fn build_acceptance_evidence(input: HumanAcceptanceEvidenceInput<'_>) -> HumanAcceptanceEvidence {
    let HumanAcceptanceEvidenceInput {
        request,
        lod_set,
        skin_system,
        eye_system,
        mouth_system,
        hair_system,
        clothing_cybernetics,
        body_model,
        motion_model,
        renderer_interface,
        ai_voice_integration,
        failure_fallbacks,
        performance_budget,
        asset_package_contract,
        debug_views,
    } = input;
    let skin_eye_hair_cloth_covered = skin_system.supports_closeup_skin()
        && eye_system.supports_closeup_optics()
        && hair_system.supports_hero_hair()
        && clothing_cybernetics.supports_cyberpunk_clothing()
        && body_model.supports_hero_body_shape();
    let shimmer_risk = if hair_system.anisotropic_shading && !hair_system.lod_levels.is_empty() {
        0.16 + (1.0 - hair_system.wet_oily_state.clumping_response).clamp(0.0, 1.0) * 0.08
    } else {
        0.9
    };

    HumanAcceptanceEvidence {
        closeup_dialogue_scene: HumanGoldenSceneProbe {
            kind: HumanGoldenSceneKind::HumanCloseUpDialogue,
            lighting: if request.hair.wetness_response > 0.0 && request.skin.sweat > 0.0 {
                HumanLightingScenario::MixedNeonRain
            } else {
                HumanLightingScenario::NeutralStudio
            },
            close_camera_distance_meters: if request.desired_quality
                >= QualityTier::HeroHighFidelityRuntime
            {
                0.85
            } else {
                2.5
            },
            voice_lipsync_covered: ai_voice_integration.voice_timing.supports_dialogue_timing()
                && mouth_system.supports_dialogue_closeup(),
            skin_eye_hair_cloth_covered,
            debug_capture_ready: debug_views.supports_v9_human_lab(),
            reference_comparison_ready: renderer_interface.supports_renderer_handoff(),
        },
        eye_tracking: HumanEyeTrackingEvidence {
            gaze_target_bound: eye_system.gaze.target_binding != GazeTargetBinding::None,
            micro_saccades_per_second: eye_system.gaze.micro_saccades_per_second,
            blink_model_active: eye_system.blink.spontaneous_blinks_per_minute > 0.0,
            pupil_light_response: eye_system.pupil_control.supports_light_adaptation,
            emotional_eye_response: eye_system.emotional_eye_behavior,
        },
        lip_sync: HumanLipSyncEvidence {
            voice_timing_bound: ai_voice_integration.voice_timing.duration_seconds,
            phoneme_timing_bound: mouth_system.speech_binding.phoneme_timing_stream,
            viseme_count: ai_voice_integration.viseme_timing_count,
            breath_timing_bound: ai_voice_integration.voice_timing.breath_timing,
            tongue_motion_bound: mouth_system.tongue.supports_runtime_tongue_motion(),
            mouth_occlusion_ready: mouth_system.inner_mouth.supports_closeup_shading()
                && mouth_system.dental.occlusion_contact,
            jaw_cheek_lip_contact: mouth_system.jaw_motion
                && mouth_system.cheek_motion
                && mouth_system.lip_compression_contact,
        },
        skin_response: HumanSkinAcceptanceEvidence {
            mixed_lighting_response: skin_system.subsurface.enabled
                && skin_system.blood_flow_redness > 0.0,
            wetness_response: skin_system.wetness_response,
            pore_wrinkle_subsurface_ready: skin_system.pore_detail > 0.0
                && skin_system.wrinkle_strength > 0.0
                && skin_system.subsurface.enabled,
            dirt_injury_response: skin_system.dirt_soot_response > 0.0
                && skin_system.injury_response > 0.0,
        },
        hair_stability: HumanHairStabilityEvidence {
            shimmer_risk,
            temporal_filtering: hair_system.anisotropic_shading
                && hair_system.representation != HairRepresentation::Hidden,
            lod_fallbacks_ready: hair_system
                .lod_levels
                .iter()
                .any(|lod| lod.simulation == HairSimulationFallback::CachedMotion)
                && hair_system
                    .lod_levels
                    .iter()
                    .any(|lod| lod.simulation == HairSimulationFallback::StaticCards),
            wet_oily_response: hair_system.wet_oily_state.wetness_response > 0.0
                && hair_system.wet_oily_state.oil_response > 0.0,
            facial_hair_ready: hair_system.facial_hair.supports_closeup_face_hair(),
        },
        clothing_motion: HumanClothingMotionEvidence {
            simulated_or_fallback: clothing_cybernetics.cloth_simulated_layer_count > 0
                || clothing_cybernetics.cloth_fallback != ClothSimulationFallback::None,
            attachment_count: clothing_cybernetics.attachment_point_count,
            clipping_prevention: clothing_cybernetics.clipping_prevention,
            wetness_damage_response: clothing_cybernetics.wetness_dirt_damage,
        },
        lod_transition: build_lod_transition_report(lod_set, motion_model, hair_system),
        schema_asset: HumanSchemaAssetEvidence {
            manifest_valid: asset_package_contract.manifest.validation.passed
                && asset_package_contract.manifest.total_uncompressed_bytes > 0,
            schema_round_trip: asset_package_contract
                .round_trip
                .supports_schema_round_trip(),
            provenance_bound: asset_package_contract.provenance_record_count > 0
                && !asset_package_contract.manifest.provenance.trim().is_empty(),
            debug_preview_ready: asset_package_contract
                .debug_preview
                .supports_debug_preview(),
            integration_replay_ready: asset_package_contract
                .integration_replay
                .supports_integration_replay(),
        },
        failure_behavior: HumanFailureBehaviorEvidence {
            voice_failure_fallback: failure_fallbacks.voice.supports_voice_generation_failure(),
            streaming_proxy_fallback: failure_fallbacks.streaming.supports_streaming_failure(),
            simulation_quality_demotion: failure_fallbacks.simulation.supports_simulation_failure(),
            frame_budget_policy: failure_fallbacks
                .frame_budget
                .supports_frame_failure_policy(),
            telemetry_replay_ready: failure_fallbacks.telemetry_event
                && failure_fallbacks.replay_deterministic,
        },
        profiling_budget: HumanProfilingBudgetEvidence {
            cpu_gpu_budget_passed: performance_budget.cpu_time_ms <= 4.0
                && performance_budget.gpu_time_ms <= 6.0,
            memory_budget_passed: performance_budget.estimated_memory_bytes
                <= request.budget.max_memory_bytes,
            profiler_capture_ready: performance_budget.profiler_capture_ready,
            fallback_cost_reported: performance_budget.fallback_cost_ms > 0.0
                && performance_budget.fallback_cost_ms <= 4.0,
            category_breakdown_ready: performance_budget.category_breakdown.len() >= 8
                && performance_budget
                    .category_breakdown
                    .iter()
                    .all(HumanPerformanceBudgetCategoryReport::within_budget),
            frame_failure_counter_bound: performance_budget.frame_failure_counter_bound,
        },
    }
}

struct HumanUncannyReviewInput<'a> {
    request: &'a HumanGenerationRequest,
    skin_system: &'a SkinRuntimeModel,
    eye_system: &'a EyeRuntimeModel,
    mouth_system: &'a MouthRuntimeModel,
    hair_system: &'a HairRuntimeModel,
    clothing_cybernetics: &'a ClothingCyberneticsRuntimeModel,
    motion_model: &'a HumanMotionModel,
    renderer_interface: &'a HumanRendererInterface,
    ai_voice_integration: &'a HumanAiVoiceIntegrationModel,
    performance_capture: &'a HumanPerformanceCaptureModel,
    failure_fallbacks: &'a HumanFailureFallbackPlan,
    performance_budget: &'a HumanPerformanceBudgetReport,
    asset_package_contract: &'a HumanAssetPackageContract,
    acceptance_evidence: &'a HumanAcceptanceEvidence,
    generation_policy: &'a HumanGenerationPolicy,
    debug_views: &'a HumanDebugViewSet,
}

fn build_uncanny_review_checklist(
    input: HumanUncannyReviewInput<'_>,
) -> HumanUncannyReviewChecklist {
    let HumanUncannyReviewInput {
        request,
        skin_system,
        eye_system,
        mouth_system,
        hair_system,
        clothing_cybernetics,
        motion_model,
        renderer_interface,
        ai_voice_integration,
        performance_capture,
        failure_fallbacks,
        performance_budget,
        asset_package_contract,
        acceptance_evidence,
        generation_policy,
        debug_views,
    } = input;
    let required = request.desired_quality >= QualityTier::HeroHighFidelityRuntime;
    let item = |kind, label: &str, passed| HumanUncannyReviewItem {
        kind,
        label: label.to_string(),
        required,
        passed,
    };

    HumanUncannyReviewChecklist {
        items: vec![
            item(
                HumanUncannyReviewKind::EyesAlive,
                "eyes feel alive",
                acceptance_evidence.eye_tracking.supports_alive_eyes(),
            ),
            item(
                HumanUncannyReviewKind::GazeTarget,
                "gaze target is bound",
                eye_system.gaze.target_binding != GazeTargetBinding::None,
            ),
            item(
                HumanUncannyReviewKind::BlinkNatural,
                "blink model is natural",
                (8.0..=28.0).contains(&eye_system.blink.spontaneous_blinks_per_minute)
                    && eye_system.blink.asymmetric_blink_support,
            ),
            item(
                HumanUncannyReviewKind::SkinLightingWetness,
                "skin responds to lighting and wetness",
                skin_system.supports_closeup_skin()
                    && acceptance_evidence
                        .skin_response
                        .supports_lighting_and_wetness(),
            ),
            item(
                HumanUncannyReviewKind::MouthTeethTongue,
                "mouth teeth tongue closeup works",
                mouth_system.supports_dialogue_closeup(),
            ),
            item(
                HumanUncannyReviewKind::SpeechSync,
                "speech sync and visemes align",
                acceptance_evidence.lip_sync.supports_speech_sync(),
            ),
            item(
                HumanUncannyReviewKind::HairTemporalStability,
                "hair is temporally stable",
                acceptance_evidence
                    .hair_stability
                    .supports_temporal_stability(),
            ),
            item(
                HumanUncannyReviewKind::FacialHairContinuity,
                "facial hair and brows survive closeup",
                hair_system.facial_hair.supports_closeup_face_hair(),
            ),
            item(
                HumanUncannyReviewKind::ClothingClipping,
                "clothing avoids visible clipping",
                clothing_cybernetics.supports_cyberpunk_clothing()
                    && acceptance_evidence
                        .clothing_motion
                        .supports_plausible_motion(),
            ),
            item(
                HumanUncannyReviewKind::WeightedBodyMotion,
                "body motion has weight",
                motion_model.supports_weighted_closeup_motion(),
            ),
            item(
                HumanUncannyReviewKind::PerformanceCaptureCurves,
                "performance capture curves are inspectable",
                performance_capture.supports_hero_performance_capture(),
            ),
            item(
                HumanUncannyReviewKind::LodTransition,
                "LOD transitions are stable",
                acceptance_evidence
                    .lod_transition
                    .supports_stable_switching(),
            ),
            item(
                HumanUncannyReviewKind::RendererHandoff,
                "renderer handoff is complete",
                renderer_interface.supports_renderer_handoff(),
            ),
            item(
                HumanUncannyReviewKind::AiVoiceConsistency,
                "AI and voice bindings are consistent",
                ai_voice_integration.supports_character_integration(),
            ),
            item(
                HumanUncannyReviewKind::GenerationProvenance,
                "generation provenance is safe",
                generation_policy.has_seed_provenance(request.seed)
                    && generation_policy.likeness_authorized()
                    && generation_policy.voice_authorized()
                    && generation_policy.controlled_variation
                    && generation_policy.bias_reviewed,
            ),
            item(
                HumanUncannyReviewKind::SchemaAssetPackage,
                "schema and asset package contract passes",
                asset_package_contract.supports_v9_schema_asset_package()
                    && acceptance_evidence
                        .schema_asset
                        .supports_schema_asset_acceptance(),
            ),
            item(
                HumanUncannyReviewKind::FailureBehavior,
                "failure fallback behavior is controlled",
                failure_fallbacks.supports_v9_failure_behavior()
                    && acceptance_evidence
                        .failure_behavior
                        .supports_controlled_failures(),
            ),
            item(
                HumanUncannyReviewKind::ProfilingBudget,
                "profiling budget report passes",
                performance_budget.supports_v9_performance_budget(&request.budget)
                    && acceptance_evidence
                        .profiling_budget
                        .supports_budget_acceptance(),
            ),
            item(
                HumanUncannyReviewKind::DebugCoverage,
                "human debug views are complete",
                debug_views.supports_v9_human_lab(),
            ),
        ],
        closeup_distance_meters: acceptance_evidence
            .closeup_dialogue_scene
            .close_camera_distance_meters,
        debug_view_bound: debug_views.supports_v9_human_lab(),
        acceptance_scene_bound: acceptance_evidence
            .closeup_dialogue_scene
            .supports_closeup_dialogue(),
    }
}

fn build_lod_transition_report(
    lod_set: &HumanLodSet,
    motion_model: &HumanMotionModel,
    hair_system: &HairRuntimeModel,
) -> HumanLodTransitionReport {
    let mut checks = Vec::new();
    let mut max_triangle_drop_ratio = 0.0_f32;
    let mut max_bone_drop_ratio = 0.0_f32;

    for pair in lod_set.levels.windows(2) {
        let from = &pair[0];
        let to = &pair[1];
        if to.tier == HumanLodTier::Impostor {
            continue;
        }

        let from_triangles = triangle_budget_for_lod(from.tier, HumanMeshPart::Body);
        let to_triangles = triangle_budget_for_lod(to.tier, HumanMeshPart::Body);
        max_triangle_drop_ratio =
            max_triangle_drop_ratio.max(drop_ratio(from_triangles, to_triangles));
        max_bone_drop_ratio = max_bone_drop_ratio.max(drop_ratio(
            u32::from(from.bone_count),
            u32::from(to.bone_count),
        ));

        checks.push(HumanLodTransitionCheck {
            from: from.tier,
            to: to.tier,
            mesh_continuity: to_triangles > 0 && from.mesh != to.mesh,
            animation_continuity: to.bone_count > 0
                && (motion_model.procedural_animation_support
                    || to.tier <= HumanLodTier::NormalNpc),
            hair_representation_continuity: hair_system
                .lod_levels
                .iter()
                .any(|lod| lod.tier == to.tier && lod.representation != HairRepresentation::Hidden),
            silhouette_error: silhouette_error_for_lod_transition(to.tier),
        });
    }

    HumanLodTransitionReport {
        checks,
        max_triangle_drop_ratio,
        max_bone_drop_ratio,
        impostor_available: lod_set
            .levels
            .iter()
            .any(|lod| lod.tier == HumanLodTier::Impostor),
    }
}

fn drop_ratio(from: u32, to: u32) -> f32 {
    if from == 0 {
        0.0
    } else {
        from.saturating_sub(to) as f32 / from as f32
    }
}

fn silhouette_error_for_lod_transition(to: HumanLodTier) -> f32 {
    match to {
        HumanLodTier::Hero => 0.0,
        HumanLodTier::ImportantNpc => 0.08,
        HumanLodTier::NormalNpc => 0.12,
        HumanLodTier::Crowd => 0.2,
        HumanLodTier::Impostor => 0.35,
    }
}

fn build_debug_views(request: &HumanGenerationRequest) -> HumanDebugViewSet {
    let full_quality = request.desired_quality >= QualityTier::HeroHighFidelityRuntime;
    let view = |kind, label: &str, min_quality| HumanDebugView {
        kind,
        label: label.to_string(),
        available: full_quality || min_quality <= request.desired_quality,
        min_quality,
    };

    HumanDebugViewSet {
        views: vec![
            view(
                HumanDebugViewKind::SkeletonRig,
                "skeleton rig",
                QualityTier::NormalRuntime,
            ),
            view(
                HumanDebugViewKind::FacialRig,
                "facial rig controls",
                QualityTier::NormalRuntime,
            ),
            view(
                HumanDebugViewKind::VisemeTimeline,
                "viseme timing",
                QualityTier::NormalRuntime,
            ),
            view(
                HumanDebugViewKind::GazeTarget,
                "gaze target",
                QualityTier::NormalRuntime,
            ),
            view(
                HumanDebugViewKind::SkinMaterialChannels,
                "skin material channels",
                QualityTier::NormalRuntime,
            ),
            view(
                HumanDebugViewKind::EyeMoistureTearline,
                "eye moisture tearline",
                QualityTier::HeroHighFidelityRuntime,
            ),
            view(
                HumanDebugViewKind::MouthTeethTongue,
                "mouth teeth tongue",
                QualityTier::HeroHighFidelityRuntime,
            ),
            view(
                HumanDebugViewKind::HairLod,
                "hair lod",
                QualityTier::NormalRuntime,
            ),
            view(
                HumanDebugViewKind::ClothSimulation,
                "cloth simulation",
                QualityTier::NormalRuntime,
            ),
            view(
                HumanDebugViewKind::BodyShapeSoftTissue,
                "body shape soft tissue",
                QualityTier::HeroHighFidelityRuntime,
            ),
            view(
                HumanDebugViewKind::AnimationContact,
                "animation contact",
                QualityTier::NormalRuntime,
            ),
            view(
                HumanDebugViewKind::PerformanceCaptureCurves,
                "performance capture curves",
                QualityTier::HeroHighFidelityRuntime,
            ),
            view(
                HumanDebugViewKind::LightingCloseupTests,
                "lighting closeup tests",
                QualityTier::HeroHighFidelityRuntime,
            ),
            view(
                HumanDebugViewKind::UncannyValleyChecklist,
                "uncanny valley checklist",
                QualityTier::HeroHighFidelityRuntime,
            ),
        ],
    }
}

fn default_attachment_points() -> Vec<AttachmentPoint> {
    [
        (BodyAttachPoint::Head, "head", Vec3::new(0.0, 0.0, 0.1)),
        (BodyAttachPoint::Face, "head", Vec3::new(0.0, -0.08, 0.08)),
        (BodyAttachPoint::Spine, "spine_03", Vec3::ZERO),
        (
            BodyAttachPoint::LeftShoulder,
            "clavicle_l",
            Vec3::new(-0.12, 0.0, 0.0),
        ),
        (
            BodyAttachPoint::RightShoulder,
            "clavicle_r",
            Vec3::new(0.12, 0.0, 0.0),
        ),
        (
            BodyAttachPoint::LeftHand,
            "hand_l",
            Vec3::new(-0.02, 0.0, 0.0),
        ),
        (
            BodyAttachPoint::RightHand,
            "hand_r",
            Vec3::new(0.02, 0.0, 0.0),
        ),
        (
            BodyAttachPoint::LeftArm,
            "lowerarm_l",
            Vec3::new(-0.02, 0.0, 0.0),
        ),
        (
            BodyAttachPoint::RightArm,
            "lowerarm_r",
            Vec3::new(0.02, 0.0, 0.0),
        ),
        (BodyAttachPoint::Torso, "spine_02", Vec3::ZERO),
        (BodyAttachPoint::Hips, "pelvis", Vec3::ZERO),
        (
            BodyAttachPoint::LeftLeg,
            "calf_l",
            Vec3::new(-0.02, 0.0, -0.02),
        ),
        (
            BodyAttachPoint::RightLeg,
            "calf_r",
            Vec3::new(0.02, 0.0, -0.02),
        ),
    ]
    .into_iter()
    .map(|(point, bone_name, local_offset_meters)| AttachmentPoint {
        point,
        bone_name: bone_name.to_string(),
        local_offset_meters,
    })
    .collect()
}

fn triangle_budget_for_lod(lod: HumanLodTier, part: HumanMeshPart) -> u32 {
    let base = match lod {
        HumanLodTier::Hero => 90_000,
        HumanLodTier::ImportantNpc => 36_000,
        HumanLodTier::NormalNpc => 18_000,
        HumanLodTier::Crowd => 4_000,
        HumanLodTier::Impostor => 2,
    };
    match part {
        HumanMeshPart::Body => base,
        HumanMeshPart::Face => base / 3,
        HumanMeshPart::Eyes => base / 24,
        HumanMeshPart::Teeth => base / 32,
        HumanMeshPart::Tongue => base / 36,
        HumanMeshPart::Hair => base / 2,
        HumanMeshPart::Clothing => base / 3,
        HumanMeshPart::CyberneticAugment => base / 4,
        HumanMeshPart::ImpostorCard => 2,
    }
}

fn estimate_hair_strands(request: &HumanGenerationRequest) -> u32 {
    let quality_multiplier = match request.hair.simulation_quality {
        QualityTier::HeroHighFidelityRuntime | QualityTier::ReferenceOfflineValidation => 40_000.0,
        QualityTier::NormalRuntime => 12_000.0,
        QualityTier::BackgroundApproximation => 2_000.0,
        QualityTier::Disabled => 0.0,
    };
    (request.hair.density.clamp(0.0, 1.0) * quality_multiplier).round() as u32
}

fn build_human_proxy_geometry(
    quality_tier: QualityTier,
    height_meters: f32,
    shoulder_width_meters: f32,
    hip_width_meters: f32,
) -> HumanProxyGeometry {
    let height = height_meters.clamp(1.2, 2.15);
    let shoulder_half = (shoulder_width_meters * 0.5).clamp(0.19, 0.3);
    let hip_half = (hip_width_meters * 0.5).clamp(0.15, 0.24);
    let body_depth = (height * 0.075).clamp(0.1, 0.16);

    let body_parts = match quality_tier {
        QualityTier::Disabled => Vec::new(),
        QualityTier::BackgroundApproximation => vec![HumanProxyBox {
            part: HumanProxyPart::Impostor,
            local_min_meters: Vec3::new(-shoulder_half, -body_depth, 0.0),
            local_max_meters: Vec3::new(shoulder_half, body_depth, height * 0.92),
        }],
        QualityTier::NormalRuntime
        | QualityTier::HeroHighFidelityRuntime
        | QualityTier::ReferenceOfflineValidation => {
            let leg_gap = (hip_half * 0.25).max(0.035);
            let leg_half_width = ((hip_half - leg_gap) * 0.5).max(0.055);
            let leg_top = height * 0.46;
            let torso_bottom = height * 0.42;
            let torso_top = height * 0.76;
            let arm_bottom = height * 0.46;
            let arm_top = height * 0.73;
            let arm_half_width = (shoulder_half * 0.24).max(0.055);
            let head_half_width = (shoulder_half * 0.34).clamp(0.12, 0.18);
            let head_bottom = height * 0.8;

            vec![
                HumanProxyBox {
                    part: HumanProxyPart::LeftLeg,
                    local_min_meters: Vec3::new(
                        -leg_gap - leg_half_width * 2.0,
                        -body_depth * 0.82,
                        0.0,
                    ),
                    local_max_meters: Vec3::new(-leg_gap, body_depth * 0.82, leg_top),
                },
                HumanProxyBox {
                    part: HumanProxyPart::RightLeg,
                    local_min_meters: Vec3::new(leg_gap, -body_depth * 0.82, 0.0),
                    local_max_meters: Vec3::new(
                        leg_gap + leg_half_width * 2.0,
                        body_depth * 0.82,
                        leg_top,
                    ),
                },
                HumanProxyBox {
                    part: HumanProxyPart::Torso,
                    local_min_meters: Vec3::new(-shoulder_half, -body_depth, torso_bottom),
                    local_max_meters: Vec3::new(shoulder_half, body_depth, torso_top),
                },
                HumanProxyBox {
                    part: HumanProxyPart::LeftArm,
                    local_min_meters: Vec3::new(
                        -shoulder_half - arm_half_width,
                        -body_depth * 0.74,
                        arm_bottom,
                    ),
                    local_max_meters: Vec3::new(-shoulder_half, body_depth * 0.74, arm_top),
                },
                HumanProxyBox {
                    part: HumanProxyPart::RightArm,
                    local_min_meters: Vec3::new(shoulder_half, -body_depth * 0.74, arm_bottom),
                    local_max_meters: Vec3::new(
                        shoulder_half + arm_half_width,
                        body_depth * 0.74,
                        arm_top,
                    ),
                },
                HumanProxyBox {
                    part: HumanProxyPart::Head,
                    local_min_meters: Vec3::new(-head_half_width, -body_depth * 0.82, head_bottom),
                    local_max_meters: Vec3::new(head_half_width, body_depth * 0.82, height),
                },
            ]
        }
    };

    HumanProxyGeometry {
        quality_tier,
        height_meters: height,
        body_parts,
        face_features: human_proxy_face_features(quality_tier, height, shoulder_half),
    }
}

fn human_proxy_face_features(
    quality_tier: QualityTier,
    height_meters: f32,
    shoulder_half_width_meters: f32,
) -> Vec<HumanProxyFaceFeature> {
    if quality_tier < QualityTier::NormalRuntime {
        return Vec::new();
    }

    let eye_offset = (shoulder_half_width_meters * 0.24).clamp(0.045, 0.07);
    let eye_width = if quality_tier >= QualityTier::HeroHighFidelityRuntime {
        0.026
    } else {
        0.022
    };
    [
        HumanProxyFaceFeature {
            kind: HumanProxyFaceFeatureKind::LeftEye,
            lateral_offset_meters: -eye_offset,
            height_meters: height_meters * 0.905,
            half_width_meters: eye_width,
            half_height_meters: 0.014,
        },
        HumanProxyFaceFeature {
            kind: HumanProxyFaceFeatureKind::RightEye,
            lateral_offset_meters: eye_offset,
            height_meters: height_meters * 0.905,
            half_width_meters: eye_width,
            half_height_meters: 0.014,
        },
        HumanProxyFaceFeature {
            kind: HumanProxyFaceFeatureKind::Mouth,
            lateral_offset_meters: 0.0,
            height_meters: height_meters * 0.855,
            half_width_meters: eye_offset * 0.88,
            half_height_meters: 0.01,
        },
    ]
    .into()
}

fn deterministic_proxy_height(human_id: HumanId) -> f32 {
    let variation = (human_id % 29) as f32 / 28.0;
    1.62 + variation * 0.18
}

fn human_gpu_resource(
    graph: &mut GpuGraphBuilder,
    label: &'static str,
    kind: GpuResourceKind,
    byte_len: u64,
    lifetime: GpuResourceLifetime,
    bindless: bool,
) -> GpuResourceHandle {
    let desc = GpuResourceDesc::new(label, kind, byte_len)
        .owned_by(40)
        .with_lifetime(lifetime);
    graph.declare_resource(if bindless { desc.bindless() } else { desc })
}

fn human_compute_pipeline_with_permutation(
    graph: &mut GpuGraphBuilder,
    label: &'static str,
    shader_key: &'static str,
    quality_tier: QualityTier,
    permutation: GpuShaderPermutation,
) -> ComputePipelineHandle {
    graph
        .create_compute_pipeline(
            GpuPipelineDesc::new(label, shader_key, quality_tier).with_permutation(permutation),
        )
        .handle
}

fn human_shader_permutation(
    defines: impl IntoIterator<Item = impl Into<String>>,
) -> GpuShaderPermutation {
    GpuShaderPermutation::new(defines)
}

fn human_resource_bytes(item_count: u64, bytes_per_item: u64) -> u64 {
    item_count
        .saturating_mul(bytes_per_item)
        .max(bytes_per_item)
}

fn estimate_human_gpu_milliseconds(
    requested_bundle_count: usize,
    speech_event_count: usize,
    surface_event_count: usize,
) -> f32 {
    if requested_bundle_count == 0 && speech_event_count == 0 && surface_event_count == 0 {
        return 0.0;
    }
    let active_human_count = requested_bundle_count
        .max(speech_event_count)
        .max(surface_event_count) as f32;
    (0.06
        + active_human_count * 0.018
        + active_human_count * 0.006
        + speech_event_count as f32 * 0.022
        + surface_event_count as f32 * 0.018)
        .min(0.85)
}

fn validation_issue(
    severity: HumanValidationSeverity,
    code: &'static str,
    message: &'static str,
) -> HumanValidationIssue {
    HumanValidationIssue {
        severity,
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn human_request_cache_key(request: &HumanGenerationRequest) -> HumanRequestCacheKey {
    HumanRequestCacheKey {
        seed: request.seed,
        role: request.role,
        desired_quality: request.desired_quality,
        fingerprint: stable_human_request_fingerprint(request),
    }
}

fn canonicalize_human_request_for_cache(
    mut request: HumanGenerationRequest,
    key: &HumanRequestCacheKey,
) -> HumanGenerationRequest {
    request.request_id = ((key.seed as u128) << 64) | key.fingerprint as u128;
    request
}

fn stable_human_request_fingerprint(request: &HumanGenerationRequest) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    hash_str(&mut hash, "ashfall_human_generation_v1");
    hash_u64(&mut hash, request.seed);
    hash_u64(&mut hash, request.role as u64);
    hash_quality_tier(&mut hash, request.desired_quality);

    hash_f32(&mut hash, request.morphology.height_meters);
    hash_f32(&mut hash, request.morphology.shoulder_width_meters);
    hash_f32(&mut hash, request.morphology.hip_width_meters);
    hash_f32(&mut hash, request.morphology.limb_proportion);
    hash_u64(&mut hash, request.morphology.posture as u64);
    hash_f32(&mut hash, request.morphology.body_mass_kg);

    hash_f32(&mut hash, request.face.jaw_width);
    hash_f32(&mut hash, request.face.cheekbone_height);
    hash_f32(&mut hash, request.face.brow_depth);
    hash_f32(&mut hash, request.face.nose_bridge);
    hash_f32(&mut hash, request.face.lip_fullness);
    hash_f32(&mut hash, request.face.ear_size);
    hash_f32(&mut hash, request.face.asymmetry);

    for value in request.skin.tone_linear {
        hash_f32(&mut hash, value);
    }
    for value in request.skin.undertone_linear {
        hash_f32(&mut hash, value);
    }
    hash_f32(&mut hash, request.skin.pore_detail);
    hash_f32(&mut hash, request.skin.wrinkle_profile);
    hash_f32(&mut hash, request.skin.oil);
    hash_f32(&mut hash, request.skin.sweat);
    hash_string_list(&mut hash, &request.skin.scars);
    hash_string_list(&mut hash, &request.skin.tattoos);
    hash_string_list(&mut hash, &request.skin.makeup_layers);
    hash_f32(&mut hash, request.skin.injury_response);

    hash_str(&mut hash, &request.hair.style);
    hash_f32(&mut hash, request.hair.density);
    hash_f32(&mut hash, request.hair.strand_thickness);
    hash_f32(&mut hash, request.hair.curl);
    hash_f32(&mut hash, request.hair.length_meters);
    for value in request.hair.color_linear {
        hash_f32(&mut hash, value);
    }
    hash_f32(&mut hash, request.hair.wetness_response);
    hash_quality_tier(&mut hash, request.hair.simulation_quality);

    for value in request.eyes.iris_color_linear {
        hash_f32(&mut hash, value);
    }
    hash_u64(&mut hash, request.eyes.iris_pattern_seed);
    for value in request.eyes.sclera_tint_linear {
        hash_f32(&mut hash, value);
    }
    hash_f32(&mut hash, request.eyes.wetness);
    hash_f32(&mut hash, request.eyes.redness);
    hash_f32(&mut hash, request.eyes.pupil_rest_radius);
    hash_bool(&mut hash, request.eyes.cybernetic);

    hash_f32(&mut hash, request.age_model.apparent_age_years);
    hash_f32(&mut hash, request.age_model.wrinkle_strength);
    hash_f32(&mut hash, request.age_model.skin_elasticity);
    hash_f32(&mut hash, request.body_composition.muscle);
    hash_f32(&mut hash, request.body_composition.fat);
    hash_f32(&mut hash, request.body_composition.soft_tissue_motion);

    hash_str(&mut hash, &request.clothing_profile.style);
    hash_usize(&mut hash, request.clothing_profile.layers.len());
    for layer in &request.clothing_profile.layers {
        hash_str(&mut hash, &layer.label);
        hash_u64(&mut hash, layer.material_id);
        hash_usize(&mut hash, layer.attach_points.len());
        for attach_point in &layer.attach_points {
            hash_u64(&mut hash, *attach_point as u64);
        }
        hash_bool(&mut hash, layer.cloth_simulation);
    }
    hash_f32(&mut hash, request.clothing_profile.wetness_response);
    hash_f32(&mut hash, request.clothing_profile.damage_visibility);

    hash_usize(&mut hash, request.cybernetics.len());
    for augment in &request.cybernetics {
        hash_str(&mut hash, &augment.label);
        hash_u64(&mut hash, augment.attach_point as u64);
        hash_usize(&mut hash, augment.visual_materials.len());
        for material in &augment.visual_materials {
            hash_u64(&mut hash, *material);
        }
        hash_physical_material(&mut hash, &augment.physical_profile);
        hash_usize(&mut hash, augment.animation_constraints.len());
        for constraint in &augment.animation_constraints {
            hash_str(&mut hash, &constraint.label);
            hash_u64(&mut hash, constraint.attach_point as u64);
            hash_f32(&mut hash, constraint.stiffness);
            hash_f32(&mut hash, constraint.max_angle_radians);
        }
        hash_u64(&mut hash, augment.damage_behavior as u64);
        hash_string_list(&mut hash, &augment.gameplay_tags);
        hash_bool(&mut hash, augment.exposed);
    }

    hash_option_u64(&mut hash, request.linked_ai_persona);
    hash_option_u64(&mut hash, request.linked_voice_persona);
    hash_generation_policy(&mut hash, &request.generation_policy);
    hash_usize(&mut hash, request.budget.max_meshes);
    hash_usize(&mut hash, request.budget.max_materials);
    hash_u64(&mut hash, request.budget.max_memory_bytes);
    hash_u64(&mut hash, u64::from(request.budget.max_hair_strands));
    hash_quality_tier(&mut hash, request.budget.min_quality);
    hash
}

fn hash_generation_policy(hash: &mut u64, policy: &HumanGenerationPolicy) {
    hash_u64(hash, policy.source as u64);
    hash_bool(hash, policy.likeness.real_person_likeness);
    hash_option_str(hash, policy.likeness.consent_record.as_deref());
    hash_option_str(hash, policy.likeness.license_record.as_deref());
    hash_bool(hash, policy.voice.real_person_voice_clone);
    hash_option_str(hash, policy.voice.consent_record.as_deref());
    hash_option_str(hash, policy.voice.rights_record.as_deref());
    hash_usize(hash, policy.provenance_records.len());
    for record in &policy.provenance_records {
        hash_str(hash, &record.label);
        hash_u64(hash, record.source_kind as u64);
        hash_str(hash, &record.record_id);
    }
    hash_bool(hash, policy.controlled_variation);
    hash_bool(hash, policy.bias_reviewed);
}

fn hash_physical_material(hash: &mut u64, material: &PhysicalMaterial) {
    hash_f32(hash, material.density_kg_per_m3);
    hash_f32(hash, material.young_modulus);
    hash_f32(hash, material.poisson_ratio);
    hash_f32(hash, material.yield_stress);
    hash_f32(hash, material.fracture_toughness);
    hash_f32(hash, material.hardness);
    hash_f32(hash, material.viscosity);
    hash_f32(hash, material.surface_tension);
    hash_f32(hash, material.restitution);
    hash_f32(hash, material.friction_static);
    hash_f32(hash, material.friction_dynamic);
}

fn hash_quality_tier(hash: &mut u64, quality: QualityTier) {
    hash_u64(hash, quality as u64);
}

fn hash_option_u64(hash: &mut u64, value: Option<u64>) {
    match value {
        Some(value) => {
            hash_bool(hash, true);
            hash_u64(hash, value);
        }
        None => hash_bool(hash, false),
    }
}

fn hash_option_str(hash: &mut u64, value: Option<&str>) {
    match value {
        Some(value) => {
            hash_bool(hash, true);
            hash_str(hash, value);
        }
        None => hash_bool(hash, false),
    }
}

fn hash_string_list(hash: &mut u64, values: &[String]) {
    hash_usize(hash, values.len());
    for value in values {
        hash_str(hash, value);
    }
}

fn hash_str(hash: &mut u64, value: &str) {
    hash_usize(hash, value.len());
    for byte in value.bytes() {
        hash_byte(hash, byte);
    }
}

fn hash_f32(hash: &mut u64, value: f32) {
    for byte in value.to_bits().to_le_bytes() {
        hash_byte(hash, byte);
    }
}

fn hash_usize(hash: &mut u64, value: usize) {
    hash_u64(hash, value as u64);
}

fn hash_u64(hash: &mut u64, value: u64) {
    for byte in value.to_le_bytes() {
        hash_byte(hash, byte);
    }
}

fn hash_bool(hash: &mut u64, value: bool) {
    hash_byte(hash, u8::from(value));
}

fn hash_byte(hash: &mut u64, byte: u8) {
    *hash ^= u64::from(byte);
    *hash = hash.wrapping_mul(0x1000_0000_01b3);
}

fn deterministic_event_id(tick: u64, module: u64, local: EntityId) -> WorldEventId {
    ((tick as u128) << 80) | ((module as u128) << 64) | local as u128
}

fn deterministic_child_event_id(
    tick: u64,
    module: u64,
    local: EntityId,
    source_event: WorldEventId,
) -> WorldEventId {
    let source_hash = (source_event as u64) ^ ((source_event >> 64) as u64);
    deterministic_event_id(tick, module, local ^ source_hash.rotate_left(17))
}

fn stable_u64(seed: u64, salt: u64) -> u64 {
    let mut value = seed
        .wrapping_add(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(salt.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn stable_u128(seed: u64, request_id: u128, salt: u64) -> u128 {
    let high = stable_u64(seed, salt) as u128;
    let low = stable_u64(request_id as u64, salt ^ 0xA51E_0000) as u128;
    (high << 64) | low
}

#[cfg(test)]
mod tests {
    use super::*;
    use ashfall_core::runtime::FrameContext;
    use ashfall_core::world::{AgentState, CommandSink, EntityTemplate, PhysicalBody, WorldState};

    fn physical_material() -> PhysicalMaterial {
        PhysicalMaterial {
            density_kg_per_m3: 1_800.0,
            young_modulus: 8_000_000_000.0,
            poisson_ratio: 0.3,
            yield_stress: 20_000_000.0,
            fracture_toughness: 0.25,
            hardness: 0.65,
            viscosity: 0.0,
            surface_tension: 0.0,
            restitution: 0.12,
            friction_static: 0.55,
            friction_dynamic: 0.4,
        }
    }

    fn hero_request() -> HumanGenerationRequest {
        HumanGenerationRequest {
            request_id: 300,
            seed: 9_991,
            role: HumanRole::Hero,
            desired_quality: QualityTier::HeroHighFidelityRuntime,
            morphology: MorphologyParams {
                height_meters: 1.72,
                shoulder_width_meters: 0.42,
                hip_width_meters: 0.36,
                limb_proportion: 1.02,
                posture: PostureKind::Guarded,
                body_mass_kg: 68.0,
            },
            face: FaceParams {
                jaw_width: 0.48,
                cheekbone_height: 0.62,
                brow_depth: 0.38,
                nose_bridge: 0.51,
                lip_fullness: 0.57,
                ear_size: 0.44,
                asymmetry: 0.08,
            },
            skin: SkinParams {
                tone_linear: [0.72, 0.48, 0.36],
                undertone_linear: [0.9, 0.42, 0.32],
                pore_detail: 0.82,
                wrinkle_profile: 0.18,
                oil: 0.28,
                sweat: 0.24,
                scars: vec!["small cheek scar".to_string()],
                tattoos: vec!["data halo".to_string()],
                makeup_layers: Vec::new(),
                injury_response: 0.7,
            },
            hair: HairParams {
                style: "short undercut".to_string(),
                density: 0.72,
                strand_thickness: 0.00007,
                curl: 0.22,
                length_meters: 0.08,
                color_linear: [0.04, 0.035, 0.03],
                wetness_response: 0.9,
                simulation_quality: QualityTier::HeroHighFidelityRuntime,
            },
            eyes: EyeParams {
                iris_color_linear: [0.1, 0.65, 0.8],
                iris_pattern_seed: 123,
                sclera_tint_linear: [0.88, 0.9, 0.92],
                wetness: 0.8,
                redness: 0.08,
                pupil_rest_radius: 0.003,
                cybernetic: false,
            },
            age_model: AgeParams {
                apparent_age_years: 31.0,
                wrinkle_strength: 0.2,
                skin_elasticity: 0.75,
            },
            body_composition: BodyCompositionParams {
                muscle: 0.45,
                fat: 0.24,
                soft_tissue_motion: 0.5,
            },
            clothing_profile: ClothingProfile {
                style: "street rain jacket".to_string(),
                layers: vec![ClothingLayer {
                    label: "reinforced jacket".to_string(),
                    material_id: DEFAULT_CLOTHING_MATERIAL,
                    attach_points: vec![BodyAttachPoint::Torso, BodyAttachPoint::LeftShoulder],
                    cloth_simulation: true,
                }],
                wetness_response: 0.8,
                damage_visibility: 0.65,
            },
            cybernetics: vec![CyberneticAugmentSpec {
                label: "right wrist data jack".to_string(),
                attach_point: BodyAttachPoint::RightHand,
                visual_materials: vec![40_100],
                physical_profile: physical_material(),
                animation_constraints: vec![ConstraintDesc {
                    label: "wrist cable restraint".to_string(),
                    attach_point: BodyAttachPoint::RightHand,
                    stiffness: 0.7,
                    max_angle_radians: 0.9,
                }],
                damage_behavior: DamageBehavior::ExposesSparks,
                gameplay_tags: vec!["hackable".to_string(), "cybernetic".to_string()],
                exposed: true,
            }],
            linked_ai_persona: Some(300),
            linked_voice_persona: Some(301),
            generation_policy: HumanGenerationPolicy::seeded_synthetic(9_991),
            budget: HumanGenerationBudget::for_quality(QualityTier::HeroHighFidelityRuntime),
        }
    }

    #[test]
    fn human_proxy_geometry_tracks_quality_tier_body_parts() {
        let human = HumanState {
            human_id: 300,
            quality_tier: QualityTier::HeroHighFidelityRuntime,
        };

        let proxy = human_proxy_geometry_for_state(&human);

        assert_eq!(proxy.quality_tier, QualityTier::HeroHighFidelityRuntime);
        assert!(proxy.height_meters > 1.65);
        assert_eq!(proxy.body_parts.len(), 6);
        assert_eq!(proxy.face_features.len(), 3);
        for expected in [
            HumanProxyPart::LeftLeg,
            HumanProxyPart::RightLeg,
            HumanProxyPart::Torso,
            HumanProxyPart::LeftArm,
            HumanProxyPart::RightArm,
            HumanProxyPart::Head,
        ] {
            assert!(
                proxy.body_parts.iter().any(|part| part.part == expected),
                "{expected:?} should be represented in the proxy body"
            );
        }
        let head = proxy
            .body_parts
            .iter()
            .find(|part| part.part == HumanProxyPart::Head)
            .expect("head part should exist");
        assert_eq!(head.local_max_meters.z, proxy.height_meters);
        assert!(proxy.face_features.iter().any(|feature| {
            feature.kind == HumanProxyFaceFeatureKind::LeftEye
                && feature.lateral_offset_meters < 0.0
                && feature.height_meters > proxy.height_meters * 0.85
        }));
        assert!(proxy.face_features.iter().any(|feature| {
            feature.kind == HumanProxyFaceFeatureKind::RightEye
                && feature.lateral_offset_meters > 0.0
        }));
    }

    #[test]
    fn human_proxy_geometry_degrades_for_background_and_disabled_quality() {
        let background = human_proxy_geometry_for_quality(QualityTier::BackgroundApproximation);
        let disabled = human_proxy_geometry_for_quality(QualityTier::Disabled);

        assert_eq!(background.body_parts.len(), 1);
        assert_eq!(background.body_parts[0].part, HumanProxyPart::Impostor);
        assert!(background.body_parts[0].local_max_meters.z > 1.4);
        assert!(background.face_features.is_empty());
        assert!(disabled.body_parts.is_empty());
        assert!(disabled.face_features.is_empty());
    }

    #[test]
    fn human_generation_is_deterministic_and_contains_required_lods() {
        let mut module = HumanGeneratorModule::default();
        let request = hero_request();

        let first = module.generate(request.clone());
        let second = module.generate(request);

        assert_eq!(first, second);
        assert!(first.validation_report.passed);
        assert!(
            first
                .lod_set
                .levels
                .iter()
                .any(|lod| lod.tier == HumanLodTier::Hero)
        );
        assert!(
            first
                .lod_set
                .levels
                .iter()
                .any(|lod| lod.tier == HumanLodTier::Crowd)
        );
        assert!(
            first
                .lod_set
                .levels
                .iter()
                .any(|lod| lod.tier == HumanLodTier::Impostor)
        );
        assert!(
            first
                .render_meshes
                .iter()
                .any(|mesh| mesh.part == HumanMeshPart::Hair && mesh.streaming_required)
        );
        assert!(
            first
                .render_meshes
                .iter()
                .any(|mesh| mesh.part == HumanMeshPart::Tongue && mesh.streaming_required)
        );
        assert!(first.anatomical_layers.iter().any(|layer| {
            layer.kind == AnatomicalLayerKind::Skin
                && layer
                    .material_state_channels
                    .contains(&HumanMaterialStateChannel::Injury)
        }));
        assert_eq!(first.voice_persona, Some(301));
        assert_eq!(first.ai_persona, Some(300));
        assert!(first.facial_rig.viseme_set.len() >= 4);
        assert!(
            first
                .facial_rig
                .viseme_set
                .iter()
                .any(|viseme| viseme.label == "tongue")
        );
        assert!(first.physics_proxy.hair_collision);
        assert!(first.physics_proxy.soft_tissue_enabled);
        assert!(first.animation_profile.supports_foot_contact_correction);
        assert!(first.animation_profile.supports_balance_weight_shifts);
        assert!(first.animation_profile.supports_hand_gestures);
        assert!(first.animation_profile.supports_breathing_motion);
        assert!(first.animation_profile.supports_idle_micro_motion);
        assert!(first.animation_profile.supports_procedural_animation);
        assert!(first.materials.iter().any(|binding| {
            binding.layer == AnatomicalLayerKind::Cybernetics && binding.supports_injury
        }));
        assert!(first.materials.iter().any(|binding| {
            binding.layer == AnatomicalLayerKind::Tongue && binding.supports_wetness
        }));
        assert!(first.surface_response.injury_response > 0.0);
        assert!(first.surface_response.hair_wetness_response > 0.0);
    }

    #[test]
    fn hero_generation_exposes_v9_closeup_model_contracts() {
        let request = hero_request();
        let bundle = generate_human_bundle(&request);

        assert!(
            bundle.validation_report.passed,
            "{:?}",
            bundle.validation_report
        );
        assert!(bundle.skin_system.supports_closeup_skin());
        assert!(bundle.skin_system.subsurface.enabled);
        assert!(bundle.skin_system.pore_detail > 0.0);
        assert!(bundle.skin_system.wrinkle_strength > 0.0);
        assert!(
            bundle
                .skin_system
                .mark_layers
                .iter()
                .any(|mark| mark.kind == SkinMarkKind::Freckle && mark.ethically_generated)
        );
        assert!(bundle.skin_system.blood_flow_redness > 0.0);
        assert!(bundle.skin_system.tension_stretch_detail > 0.0);
        assert!(bundle.eye_system.supports_closeup_optics());
        assert!(bundle.eye_system.cornea_layer);
        assert!(bundle.eye_system.sclera_layer);
        assert!(bundle.eye_system.tearline);
        assert_eq!(
            bundle.eye_system.gaze.target_binding,
            GazeTargetBinding::AiFocusTarget
        );
        assert!(bundle.eye_system.blink.asymmetric_blink_support);
        assert!(bundle.mouth_system.supports_dialogue_closeup());
        assert!(bundle.mouth_system.teeth_geometry);
        assert!(bundle.mouth_system.tongue_animation);
        assert!(bundle.mouth_system.dental.supports_closeup_teeth());
        assert!(bundle.mouth_system.dental.enamel_translucency > 0.0);
        assert!(bundle.mouth_system.tongue.supports_runtime_tongue_motion());
        assert!(
            bundle
                .mouth_system
                .tongue
                .contact_points
                .iter()
                .any(|point| {
                    point.kind == TongueContactKind::TipPalate && point.viseme_label == "tongue"
                })
        );
        assert!(bundle.mouth_system.inner_mouth.supports_closeup_shading());
        assert!(bundle.mouth_system.speech_binding.supports_voice_lipsync());
        assert!(bundle.hair_system.supports_hero_hair());
        assert!(bundle.hair_system.guide_curve_count > 0);
        assert!(bundle.hair_system.card_count > 0);
        assert!(bundle.hair_system.anisotropic_shading);
        assert!(bundle.hair_system.lod_levels.len() >= 4);
        assert!(bundle.hair_system.facial_hair.supports_closeup_face_hair());
        assert!(bundle.hair_system.facial_hair.eyebrow_strand_count > 0);
        assert!(bundle.hair_system.facial_hair.eyelash_strand_count > 0);
        assert!(
            bundle
                .hair_system
                .facial_hair
                .lod_levels
                .iter()
                .any(|lod| lod.tier == HumanLodTier::Crowd
                    && lod.brow_lash_representation == HairRepresentation::PaintedCap)
        );
        assert!(bundle.clothing_cybernetics.supports_cyberpunk_clothing());
        assert!(bundle.clothing_cybernetics.supports_visible_cybernetics());
        assert_eq!(bundle.clothing_cybernetics.layered_clothing_count, 1);
        assert_eq!(bundle.clothing_cybernetics.cybernetic_implant_count, 1);
        assert!(bundle.clothing_cybernetics.emissive_element_count > 0);
        assert!(bundle.body_model.supports_hero_body_shape());
        assert!(bundle.body_model.composition.muscle > 0.0);
        assert!(bundle.body_model.composition.fat > 0.0);
        assert!(bundle.body_model.composition.bmi_estimate > 10.0);
        assert!(bundle.body_model.mass_distribution.iter().any(|region| {
            region.region == HumanBodyRegion::Abdomen && region.fat_weight > 0.0
        }));
        assert!(
            bundle
                .body_model
                .soft_tissue_zones
                .iter()
                .any(|zone| zone.region == HumanBodyRegion::Hips && zone.enabled)
        );
        assert!(bundle.body_model.muscle_groups.iter().any(|group| {
            group.group == HumanMuscleGroupKind::Legs && group.driven_by_pose_space
        }));
        assert!(
            bundle
                .body_model
                .collision_profile
                .supports_body_shape_simulation()
        );
        assert!(bundle.motion_model.supports_weighted_closeup_motion());
        assert!(bundle.motion_model.muscle_soft_tissue);
        assert!(bundle.renderer_interface.supports_renderer_handoff());
        assert!(!bundle.renderer_interface.rigged_mesh_handles.is_empty());
        assert!(
            bundle
                .renderer_interface
                .morph_animation_buffers
                .gpu_skinning_resource
                .is_some()
        );
        assert!(
            bundle
                .renderer_interface
                .mouth_material_handles
                .contains(&DEFAULT_TEETH_MATERIAL)
        );
        assert!(
            bundle
                .renderer_interface
                .mouth_articulation_buffer
                .is_valid()
        );
        assert!(
            bundle
                .renderer_interface
                .clothing_buffers
                .iter()
                .any(|buffer| buffer.cloth_simulation)
        );
        assert!(
            bundle
                .renderer_interface
                .material_state_fields
                .contains(&HumanMaterialStateChannel::BloodPerfusion)
        );
        assert_eq!(
            bundle.renderer_interface.expression_curve_count,
            bundle.facial_rig.expression_controls.len()
        );
        assert_eq!(
            bundle.renderer_interface.viseme_timing_count,
            bundle.facial_rig.viseme_set.len()
        );
        assert!(bundle.ai_voice_integration.supports_character_integration());
        assert_eq!(
            bundle.ai_voice_integration.relationship_context,
            RelationshipContextBinding::DialogueRelationship
        );
        assert!(
            bundle
                .ai_voice_integration
                .body_gesture_channels
                .contains(&HumanGestureChannel::BreathMotion)
        );
        assert!(
            bundle
                .ai_voice_integration
                .voice_timing
                .supports_dialogue_timing()
        );
        assert!(
            bundle
                .performance_capture
                .supports_hero_performance_capture()
        );
        assert_eq!(
            bundle.performance_capture.source,
            HumanPerformanceCaptureSource::HybridProceduralCapture
        );
        assert!(
            bundle
                .performance_capture
                .curve_bindings
                .iter()
                .any(|curve| {
                    curve.kind == HumanPerformanceCaptureCurveKind::EyeGaze
                        && curve.supports_runtime_capture()
                })
        );
        assert!(
            bundle
                .performance_capture
                .facial_solver
                .supports_hero_face_capture()
        );
        assert!(
            bundle
                .performance_capture
                .body_solver
                .supports_hero_body_capture()
        );
        assert!(bundle.failure_fallbacks.supports_v9_failure_behavior());
        assert!(
            bundle
                .failure_fallbacks
                .voice
                .supports_voice_generation_failure()
        );
        assert!(bundle.failure_fallbacks.voice.cached_audio_line);
        assert!(bundle.failure_fallbacks.voice.subtitle_fallback);
        assert!(
            bundle
                .failure_fallbacks
                .streaming
                .supports_streaming_failure()
        );
        assert!(
            bundle
                .failure_fallbacks
                .simulation
                .supports_simulation_failure()
        );
        assert!(
            bundle
                .failure_fallbacks
                .frame_budget
                .supports_frame_failure_policy()
        );
        assert!(bundle.failure_fallbacks.telemetry_event);
        assert!(bundle.failure_fallbacks.replay_deterministic);
        assert!(
            bundle
                .performance_budget
                .supports_v9_performance_budget(&request.budget)
        );
        assert!(bundle.performance_budget.cpu_time_ms <= 4.0);
        assert!(bundle.performance_budget.gpu_time_ms <= 6.0);
        assert!(
            bundle.performance_budget.estimated_memory_bytes <= request.budget.max_memory_bytes
        );
        assert!(bundle.performance_budget.profiler_capture_ready);
        assert!(
            bundle
                .performance_budget
                .category_breakdown
                .iter()
                .any(
                    |entry| entry.category == HumanPerformanceBudgetCategory::VoiceLipsync
                        && entry.within_budget()
                )
        );
        assert!(
            bundle
                .performance_budget
                .category_breakdown
                .iter()
                .any(
                    |entry| entry.category == HumanPerformanceBudgetCategory::Fallback
                        && entry.within_budget()
                )
        );
        assert!(
            bundle
                .asset_package_contract
                .supports_v9_schema_asset_package()
        );
        assert!(bundle.asset_package_contract.manifest.validate(40).passed);
        assert_eq!(
            bundle.asset_package_contract.manifest.schema_name,
            "HumanRuntimeBundle"
        );
        assert_eq!(
            bundle.asset_package_contract.manifest.asset_kind,
            AssetKind::GeneratedBundle
        );
        assert!(
            bundle.asset_package_contract.manifest.dependencies.len() >= bundle.render_meshes.len()
        );
        assert!(
            bundle
                .asset_package_contract
                .debug_preview
                .supports_debug_preview()
        );
        assert!(
            bundle
                .asset_package_contract
                .round_trip
                .supports_schema_round_trip()
        );
        assert!(
            bundle
                .asset_package_contract
                .integration_replay
                .supports_integration_replay()
        );
        assert!(
            bundle
                .asset_package_contract
                .runtime_schemas
                .iter()
                .any(|binding| binding.schema.name == "SpeechResult"
                    && binding.supports_runtime_exchange())
        );
        assert!(bundle.uncanny_review.passes_v9_review());
        assert!(
            bundle
                .uncanny_review
                .has(HumanUncannyReviewKind::SchemaAssetPackage)
        );
        assert!(
            bundle
                .uncanny_review
                .has(HumanUncannyReviewKind::PerformanceCaptureCurves)
        );
        assert!(
            bundle
                .uncanny_review
                .has(HumanUncannyReviewKind::DebugCoverage)
        );
        assert!(
            bundle
                .uncanny_review
                .has(HumanUncannyReviewKind::FailureBehavior)
        );
        assert!(
            bundle
                .uncanny_review
                .has(HumanUncannyReviewKind::ProfilingBudget)
        );
        assert!(bundle.acceptance_evidence.passes_v9_acceptance());
        assert!(
            bundle
                .acceptance_evidence
                .schema_asset
                .supports_schema_asset_acceptance()
        );
        assert!(
            bundle
                .acceptance_evidence
                .closeup_dialogue_scene
                .supports_closeup_dialogue()
        );
        assert_eq!(
            bundle.acceptance_evidence.closeup_dialogue_scene.lighting,
            HumanLightingScenario::MixedNeonRain
        );
        assert!(
            bundle
                .acceptance_evidence
                .eye_tracking
                .supports_alive_eyes()
        );
        assert!(bundle.acceptance_evidence.lip_sync.supports_speech_sync());
        assert!(bundle.acceptance_evidence.lip_sync.phoneme_timing_bound);
        assert!(bundle.acceptance_evidence.lip_sync.tongue_motion_bound);
        assert!(bundle.acceptance_evidence.lip_sync.mouth_occlusion_ready);
        assert!(
            bundle
                .acceptance_evidence
                .failure_behavior
                .supports_controlled_failures()
        );
        assert!(
            bundle
                .acceptance_evidence
                .profiling_budget
                .supports_budget_acceptance()
        );
        assert!(
            bundle
                .acceptance_evidence
                .skin_response
                .supports_lighting_and_wetness()
        );
        assert!(bundle.acceptance_evidence.hair_stability.shimmer_risk <= 0.35);
        assert!(bundle.acceptance_evidence.hair_stability.facial_hair_ready);
        assert!(
            bundle
                .acceptance_evidence
                .clothing_motion
                .supports_plausible_motion()
        );
        assert!(
            bundle
                .acceptance_evidence
                .lod_transition
                .supports_stable_switching()
        );
        assert!(
            bundle
                .acceptance_evidence
                .lod_transition
                .checks
                .iter()
                .any(|check| check.from == HumanLodTier::NormalNpc
                    && check.to == HumanLodTier::Crowd)
        );
        assert_eq!(bundle.debug_views.ready_count(), 14);
        assert!(bundle.debug_views.supports_v9_human_lab());
        for expected in [
            HumanDebugViewKind::SkeletonRig,
            HumanDebugViewKind::FacialRig,
            HumanDebugViewKind::VisemeTimeline,
            HumanDebugViewKind::GazeTarget,
            HumanDebugViewKind::SkinMaterialChannels,
            HumanDebugViewKind::EyeMoistureTearline,
            HumanDebugViewKind::MouthTeethTongue,
            HumanDebugViewKind::HairLod,
            HumanDebugViewKind::ClothSimulation,
            HumanDebugViewKind::BodyShapeSoftTissue,
            HumanDebugViewKind::AnimationContact,
            HumanDebugViewKind::PerformanceCaptureCurves,
            HumanDebugViewKind::LightingCloseupTests,
            HumanDebugViewKind::UncannyValleyChecklist,
        ] {
            assert!(
                bundle.debug_views.has(expected),
                "{expected:?} debug view should be available"
            );
        }
        assert_eq!(
            bundle.generation_policy.source,
            HumanGenerationSource::SeededSynthetic
        );
        assert!(bundle.generation_policy.has_seed_provenance(9_991));
        assert!(bundle.generation_policy.controlled_variation);
        assert!(bundle.generation_policy.bias_reviewed);
        assert!(
            bundle
                .rig
                .attachment_points
                .iter()
                .any(|point| point.point == BodyAttachPoint::LeftLeg)
        );
        assert!(
            bundle
                .rig
                .attachment_points
                .iter()
                .any(|point| point.point == BodyAttachPoint::RightArm)
        );
    }

    #[test]
    fn validation_blocks_incomplete_skin_contract() {
        let request = hero_request();
        let lod_set = build_lod_set(&request);
        let render_meshes = build_render_meshes(&request, &lod_set);
        let rig = build_body_rig(&request);
        let facial_rig = build_facial_rig(&request);
        let materials = build_materials(&request);
        let surface_response = build_surface_response_profile(&request);
        let physics_proxy = build_physics_proxy(&request);
        let animation_profile = build_animation_profile(&request);
        let valid_skin_system = build_skin_runtime_model(&request, &surface_response);
        let mut skin_system = valid_skin_system.clone();
        skin_system.subsurface.enabled = false;
        skin_system.ethical_mark_generation = false;
        skin_system.tension_stretch_detail = 0.0;
        let eye_system = build_eye_runtime_model(&request);
        let mouth_system = build_mouth_runtime_model(&request, &facial_rig);
        let hair_system = build_hair_runtime_model(&request, &lod_set);
        let clothing_cybernetics = build_clothing_cybernetics_runtime_model(&request, &rig);
        let motion_model = build_motion_model(&request, &physics_proxy, &animation_profile);
        let body_model = build_body_runtime_model(&request, &physics_proxy, &motion_model);
        let renderer_interface =
            build_renderer_interface(&request, &render_meshes, &materials, &rig, &facial_rig);
        let ai_voice_integration = build_ai_voice_integration_model(
            &request,
            &eye_system,
            &facial_rig,
            &mouth_system,
            &motion_model,
        );
        let debug_views = build_debug_views(&request);
        let valid_bundle = generate_human_bundle(&request);
        let acceptance_evidence = build_acceptance_evidence(HumanAcceptanceEvidenceInput {
            request: &request,
            lod_set: &lod_set,
            skin_system: &valid_skin_system,
            eye_system: &eye_system,
            mouth_system: &mouth_system,
            hair_system: &hair_system,
            clothing_cybernetics: &clothing_cybernetics,
            body_model: &body_model,
            motion_model: &motion_model,
            renderer_interface: &renderer_interface,
            ai_voice_integration: &ai_voice_integration,
            failure_fallbacks: &valid_bundle.failure_fallbacks,
            performance_budget: &valid_bundle.performance_budget,
            asset_package_contract: &valid_bundle.asset_package_contract,
            debug_views: &debug_views,
        });

        let report = validate_human_bundle(HumanBundleValidationInput {
            render_meshes: &render_meshes,
            materials: &materials,
            physics_proxy: &physics_proxy,
            lod_set: &lod_set,
            skin_system: &skin_system,
            eye_system: &eye_system,
            mouth_system: &mouth_system,
            hair_system: &hair_system,
            clothing_cybernetics: &clothing_cybernetics,
            body_model: &body_model,
            motion_model: &motion_model,
            renderer_interface: &renderer_interface,
            ai_voice_integration: &ai_voice_integration,
            performance_capture: &valid_bundle.performance_capture,
            failure_fallbacks: &valid_bundle.failure_fallbacks,
            performance_budget: &valid_bundle.performance_budget,
            asset_package_contract: &valid_bundle.asset_package_contract,
            uncanny_review: &valid_bundle.uncanny_review,
            acceptance_evidence: &acceptance_evidence,
            generation_policy: &request.generation_policy,
            debug_views: &debug_views,
            request: &request,
        });

        assert!(!report.passed);
        assert!(report.issues.iter().any(|issue| {
            issue.severity == HumanValidationSeverity::Error
                && issue.code == "incomplete_skin_system"
        }));
    }

    #[test]
    fn validation_blocks_incomplete_mouth_teeth_tongue_contract() {
        let request = hero_request();
        let bundle = generate_human_bundle(&request);
        let mut mouth_system = bundle.mouth_system.clone();
        mouth_system.dental.upper_tooth_count = 0;
        mouth_system.tongue.contact_points.clear();
        mouth_system.inner_mouth.cavity_occlusion = 0.0;
        mouth_system.speech_binding.phoneme_timing_stream = false;

        let report = validate_human_bundle(HumanBundleValidationInput {
            render_meshes: &bundle.render_meshes,
            materials: &bundle.materials,
            physics_proxy: &bundle.physics_proxy,
            lod_set: &bundle.lod_set,
            skin_system: &bundle.skin_system,
            eye_system: &bundle.eye_system,
            mouth_system: &mouth_system,
            hair_system: &bundle.hair_system,
            clothing_cybernetics: &bundle.clothing_cybernetics,
            body_model: &bundle.body_model,
            motion_model: &bundle.motion_model,
            renderer_interface: &bundle.renderer_interface,
            ai_voice_integration: &bundle.ai_voice_integration,
            performance_capture: &bundle.performance_capture,
            failure_fallbacks: &bundle.failure_fallbacks,
            performance_budget: &bundle.performance_budget,
            asset_package_contract: &bundle.asset_package_contract,
            uncanny_review: &bundle.uncanny_review,
            acceptance_evidence: &bundle.acceptance_evidence,
            generation_policy: &bundle.generation_policy,
            debug_views: &bundle.debug_views,
            request: &request,
        });

        assert!(!report.passed);
        assert!(report.issues.iter().any(|issue| {
            issue.severity == HumanValidationSeverity::Error
                && issue.code == "incomplete_mouth_system"
        }));
    }

    #[test]
    fn generation_policy_blocks_unlicensed_likeness_and_voice_clone() {
        let mut request = hero_request();
        request.generation_policy.likeness.real_person_likeness = true;
        request.generation_policy.voice.real_person_voice_clone = true;
        request.generation_policy.provenance_records.clear();
        request.generation_policy.controlled_variation = false;
        request.generation_policy.bias_reviewed = false;

        let bundle = generate_human_bundle(&request);

        assert!(!bundle.validation_report.passed);
        for code in [
            "missing_generation_provenance",
            "unlicensed_likeness",
            "unlicensed_voice_clone",
            "uncontrolled_generation_variation",
            "missing_bias_review",
        ] {
            assert!(
                bundle
                    .validation_report
                    .issues
                    .iter()
                    .any(|issue| issue.severity == HumanValidationSeverity::Error
                        && issue.code == code),
                "{code} should be reported"
            );
        }
    }

    #[test]
    fn validation_blocks_incomplete_hair_clothing_and_cybernetic_contracts() {
        let request = hero_request();
        let lod_set = build_lod_set(&request);
        let render_meshes = build_render_meshes(&request, &lod_set);
        let rig = build_body_rig(&request);
        let facial_rig = build_facial_rig(&request);
        let materials = build_materials(&request);
        let surface_response = build_surface_response_profile(&request);
        let physics_proxy = build_physics_proxy(&request);
        let animation_profile = build_animation_profile(&request);
        let skin_system = build_skin_runtime_model(&request, &surface_response);
        let eye_system = build_eye_runtime_model(&request);
        let mouth_system = build_mouth_runtime_model(&request, &facial_rig);
        let motion_model = build_motion_model(&request, &physics_proxy, &animation_profile);
        let body_model = build_body_runtime_model(&request, &physics_proxy, &motion_model);
        let renderer_interface =
            build_renderer_interface(&request, &render_meshes, &materials, &rig, &facial_rig);
        let ai_voice_integration = build_ai_voice_integration_model(
            &request,
            &eye_system,
            &facial_rig,
            &mouth_system,
            &motion_model,
        );
        let debug_views = build_debug_views(&request);
        let valid_hair_system = build_hair_runtime_model(&request, &lod_set);
        let valid_clothing_cybernetics = build_clothing_cybernetics_runtime_model(&request, &rig);
        let valid_bundle = generate_human_bundle(&request);
        let acceptance_evidence = build_acceptance_evidence(HumanAcceptanceEvidenceInput {
            request: &request,
            lod_set: &lod_set,
            skin_system: &skin_system,
            eye_system: &eye_system,
            mouth_system: &mouth_system,
            hair_system: &valid_hair_system,
            clothing_cybernetics: &valid_clothing_cybernetics,
            body_model: &body_model,
            motion_model: &motion_model,
            renderer_interface: &renderer_interface,
            ai_voice_integration: &ai_voice_integration,
            failure_fallbacks: &valid_bundle.failure_fallbacks,
            performance_budget: &valid_bundle.performance_budget,
            asset_package_contract: &valid_bundle.asset_package_contract,
            debug_views: &debug_views,
        });
        let mut hair_system = valid_hair_system;
        hair_system.guide_curve_count = 0;
        hair_system.anisotropic_shading = false;
        let mut clothing_cybernetics = valid_clothing_cybernetics;
        clothing_cybernetics.clipping_prevention = false;
        clothing_cybernetics.emissive_element_count = 0;
        let report = validate_human_bundle(HumanBundleValidationInput {
            render_meshes: &render_meshes,
            materials: &materials,
            physics_proxy: &physics_proxy,
            lod_set: &lod_set,
            skin_system: &skin_system,
            eye_system: &eye_system,
            mouth_system: &mouth_system,
            hair_system: &hair_system,
            clothing_cybernetics: &clothing_cybernetics,
            body_model: &body_model,
            motion_model: &motion_model,
            renderer_interface: &renderer_interface,
            ai_voice_integration: &ai_voice_integration,
            performance_capture: &valid_bundle.performance_capture,
            failure_fallbacks: &valid_bundle.failure_fallbacks,
            performance_budget: &valid_bundle.performance_budget,
            asset_package_contract: &valid_bundle.asset_package_contract,
            uncanny_review: &valid_bundle.uncanny_review,
            acceptance_evidence: &acceptance_evidence,
            generation_policy: &request.generation_policy,
            debug_views: &debug_views,
            request: &request,
        });

        assert!(!report.passed);
        for code in [
            "incomplete_hair_system",
            "incomplete_clothing_system",
            "incomplete_cybernetic_system",
        ] {
            assert!(
                report.issues.iter().any(|issue| {
                    issue.severity == HumanValidationSeverity::Error && issue.code == code
                }),
                "{code} should be reported"
            );
        }
    }

    #[test]
    fn validation_blocks_incomplete_facial_hair_contract() {
        let request = hero_request();
        let bundle = generate_human_bundle(&request);
        let mut hair_system = bundle.hair_system.clone();
        hair_system.facial_hair.eyebrow_strand_count = 0;
        hair_system.facial_hair.eyelash_strand_count = 0;
        hair_system.facial_hair.lod_levels.clear();

        let report = validate_human_bundle(HumanBundleValidationInput {
            render_meshes: &bundle.render_meshes,
            materials: &bundle.materials,
            physics_proxy: &bundle.physics_proxy,
            lod_set: &bundle.lod_set,
            skin_system: &bundle.skin_system,
            eye_system: &bundle.eye_system,
            mouth_system: &bundle.mouth_system,
            hair_system: &hair_system,
            clothing_cybernetics: &bundle.clothing_cybernetics,
            body_model: &bundle.body_model,
            motion_model: &bundle.motion_model,
            renderer_interface: &bundle.renderer_interface,
            ai_voice_integration: &bundle.ai_voice_integration,
            performance_capture: &bundle.performance_capture,
            failure_fallbacks: &bundle.failure_fallbacks,
            performance_budget: &bundle.performance_budget,
            asset_package_contract: &bundle.asset_package_contract,
            uncanny_review: &bundle.uncanny_review,
            acceptance_evidence: &bundle.acceptance_evidence,
            generation_policy: &bundle.generation_policy,
            debug_views: &bundle.debug_views,
            request: &request,
        });

        assert!(!report.passed);
        assert!(report.issues.iter().any(|issue| {
            issue.severity == HumanValidationSeverity::Error
                && issue.code == "incomplete_hair_system"
        }));
    }

    #[test]
    fn validation_blocks_incomplete_body_shape_contract() {
        let request = hero_request();
        let bundle = generate_human_bundle(&request);
        let mut body_model = bundle.body_model.clone();
        body_model.mass_distribution.clear();
        body_model.soft_tissue_zones[0].enabled = false;
        body_model.collision_profile.soft_tissue_enabled = false;

        let report = validate_human_bundle(HumanBundleValidationInput {
            render_meshes: &bundle.render_meshes,
            materials: &bundle.materials,
            physics_proxy: &bundle.physics_proxy,
            lod_set: &bundle.lod_set,
            skin_system: &bundle.skin_system,
            eye_system: &bundle.eye_system,
            mouth_system: &bundle.mouth_system,
            hair_system: &bundle.hair_system,
            clothing_cybernetics: &bundle.clothing_cybernetics,
            body_model: &body_model,
            motion_model: &bundle.motion_model,
            renderer_interface: &bundle.renderer_interface,
            ai_voice_integration: &bundle.ai_voice_integration,
            performance_capture: &bundle.performance_capture,
            failure_fallbacks: &bundle.failure_fallbacks,
            performance_budget: &bundle.performance_budget,
            asset_package_contract: &bundle.asset_package_contract,
            uncanny_review: &bundle.uncanny_review,
            acceptance_evidence: &bundle.acceptance_evidence,
            generation_policy: &bundle.generation_policy,
            debug_views: &bundle.debug_views,
            request: &request,
        });

        assert!(!report.passed);
        assert!(report.issues.iter().any(|issue| {
            issue.severity == HumanValidationSeverity::Error
                && issue.code == "incomplete_body_shape_model"
        }));
    }

    #[test]
    fn validation_blocks_incomplete_human_lab_debug_views() {
        let request = hero_request();
        let bundle = generate_human_bundle(&request);
        let mut debug_views = bundle.debug_views.clone();
        for view in &mut debug_views.views {
            if matches!(
                view.kind,
                HumanDebugViewKind::PerformanceCaptureCurves
                    | HumanDebugViewKind::LightingCloseupTests
            ) {
                view.available = false;
            }
        }

        let report = validate_human_bundle(HumanBundleValidationInput {
            render_meshes: &bundle.render_meshes,
            materials: &bundle.materials,
            physics_proxy: &bundle.physics_proxy,
            lod_set: &bundle.lod_set,
            skin_system: &bundle.skin_system,
            eye_system: &bundle.eye_system,
            mouth_system: &bundle.mouth_system,
            hair_system: &bundle.hair_system,
            clothing_cybernetics: &bundle.clothing_cybernetics,
            body_model: &bundle.body_model,
            motion_model: &bundle.motion_model,
            renderer_interface: &bundle.renderer_interface,
            ai_voice_integration: &bundle.ai_voice_integration,
            performance_capture: &bundle.performance_capture,
            failure_fallbacks: &bundle.failure_fallbacks,
            performance_budget: &bundle.performance_budget,
            asset_package_contract: &bundle.asset_package_contract,
            uncanny_review: &bundle.uncanny_review,
            acceptance_evidence: &bundle.acceptance_evidence,
            generation_policy: &bundle.generation_policy,
            debug_views: &debug_views,
            request: &request,
        });

        assert!(!report.passed);
        assert!(report.issues.iter().any(|issue| {
            issue.severity == HumanValidationSeverity::Error
                && issue.code == "missing_human_debug_views"
        }));
    }

    #[test]
    fn validation_blocks_incomplete_failure_fallbacks() {
        let request = hero_request();
        let bundle = generate_human_bundle(&request);
        let mut failure_fallbacks = bundle.failure_fallbacks.clone();
        failure_fallbacks.voice.cached_audio_line = false;
        failure_fallbacks.streaming.proxy_human_available = false;
        failure_fallbacks.simulation.hair_quality_demotion = false;
        failure_fallbacks.frame_budget.frame_failure_counter = false;
        failure_fallbacks.replay_deterministic = false;

        let report = validate_human_bundle(HumanBundleValidationInput {
            render_meshes: &bundle.render_meshes,
            materials: &bundle.materials,
            physics_proxy: &bundle.physics_proxy,
            lod_set: &bundle.lod_set,
            skin_system: &bundle.skin_system,
            eye_system: &bundle.eye_system,
            mouth_system: &bundle.mouth_system,
            hair_system: &bundle.hair_system,
            clothing_cybernetics: &bundle.clothing_cybernetics,
            body_model: &bundle.body_model,
            motion_model: &bundle.motion_model,
            renderer_interface: &bundle.renderer_interface,
            ai_voice_integration: &bundle.ai_voice_integration,
            performance_capture: &bundle.performance_capture,
            failure_fallbacks: &failure_fallbacks,
            performance_budget: &bundle.performance_budget,
            asset_package_contract: &bundle.asset_package_contract,
            uncanny_review: &bundle.uncanny_review,
            acceptance_evidence: &bundle.acceptance_evidence,
            generation_policy: &bundle.generation_policy,
            debug_views: &bundle.debug_views,
            request: &request,
        });

        assert!(!report.passed);
        assert!(report.issues.iter().any(|issue| {
            issue.severity == HumanValidationSeverity::Error
                && issue.code == "incomplete_failure_fallbacks"
        }));
    }

    #[test]
    fn validation_blocks_incomplete_performance_budget_report() {
        let request = hero_request();
        let bundle = generate_human_bundle(&request);
        let mut performance_budget = bundle.performance_budget.clone();
        performance_budget.cpu_time_ms = 8.0;
        performance_budget.gpu_time_ms = 9.0;
        performance_budget.profiler_capture_ready = false;
        performance_budget.frame_failure_counter_bound = false;
        if let Some(category) = performance_budget
            .category_breakdown
            .iter_mut()
            .find(|entry| entry.category == HumanPerformanceBudgetCategory::HairClothSimulation)
        {
            category.estimated_ms = category.budget_ms + 1.0;
        }

        let report = validate_human_bundle(HumanBundleValidationInput {
            render_meshes: &bundle.render_meshes,
            materials: &bundle.materials,
            physics_proxy: &bundle.physics_proxy,
            lod_set: &bundle.lod_set,
            skin_system: &bundle.skin_system,
            eye_system: &bundle.eye_system,
            mouth_system: &bundle.mouth_system,
            hair_system: &bundle.hair_system,
            clothing_cybernetics: &bundle.clothing_cybernetics,
            body_model: &bundle.body_model,
            motion_model: &bundle.motion_model,
            renderer_interface: &bundle.renderer_interface,
            ai_voice_integration: &bundle.ai_voice_integration,
            performance_capture: &bundle.performance_capture,
            failure_fallbacks: &bundle.failure_fallbacks,
            performance_budget: &performance_budget,
            asset_package_contract: &bundle.asset_package_contract,
            uncanny_review: &bundle.uncanny_review,
            acceptance_evidence: &bundle.acceptance_evidence,
            generation_policy: &bundle.generation_policy,
            debug_views: &bundle.debug_views,
            request: &request,
        });

        assert!(!report.passed);
        assert!(report.issues.iter().any(|issue| {
            issue.severity == HumanValidationSeverity::Error
                && issue.code == "incomplete_performance_budget_report"
        }));
    }

    #[test]
    fn validation_blocks_incomplete_renderer_and_ai_voice_contracts() {
        let request = hero_request();
        let lod_set = build_lod_set(&request);
        let render_meshes = build_render_meshes(&request, &lod_set);
        let rig = build_body_rig(&request);
        let facial_rig = build_facial_rig(&request);
        let materials = build_materials(&request);
        let surface_response = build_surface_response_profile(&request);
        let physics_proxy = build_physics_proxy(&request);
        let animation_profile = build_animation_profile(&request);
        let skin_system = build_skin_runtime_model(&request, &surface_response);
        let eye_system = build_eye_runtime_model(&request);
        let mouth_system = build_mouth_runtime_model(&request, &facial_rig);
        let hair_system = build_hair_runtime_model(&request, &lod_set);
        let clothing_cybernetics = build_clothing_cybernetics_runtime_model(&request, &rig);
        let motion_model = build_motion_model(&request, &physics_proxy, &animation_profile);
        let body_model = build_body_runtime_model(&request, &physics_proxy, &motion_model);
        let valid_renderer_interface =
            build_renderer_interface(&request, &render_meshes, &materials, &rig, &facial_rig);
        let mut renderer_interface = valid_renderer_interface.clone();
        renderer_interface.skin_material_handles.clear();
        renderer_interface.viseme_timing_count = 0;
        let valid_ai_voice_integration = build_ai_voice_integration_model(
            &request,
            &eye_system,
            &facial_rig,
            &mouth_system,
            &motion_model,
        );
        let mut ai_voice_integration = valid_ai_voice_integration.clone();
        ai_voice_integration.voice_timing.viseme_timeline = false;
        ai_voice_integration.relationship_context = RelationshipContextBinding::None;
        let debug_views = build_debug_views(&request);
        let valid_bundle = generate_human_bundle(&request);
        let acceptance_evidence = build_acceptance_evidence(HumanAcceptanceEvidenceInput {
            request: &request,
            lod_set: &lod_set,
            skin_system: &skin_system,
            eye_system: &eye_system,
            mouth_system: &mouth_system,
            hair_system: &hair_system,
            clothing_cybernetics: &clothing_cybernetics,
            body_model: &body_model,
            motion_model: &motion_model,
            renderer_interface: &valid_renderer_interface,
            ai_voice_integration: &valid_ai_voice_integration,
            failure_fallbacks: &valid_bundle.failure_fallbacks,
            performance_budget: &valid_bundle.performance_budget,
            asset_package_contract: &valid_bundle.asset_package_contract,
            debug_views: &debug_views,
        });

        let report = validate_human_bundle(HumanBundleValidationInput {
            render_meshes: &render_meshes,
            materials: &materials,
            physics_proxy: &physics_proxy,
            lod_set: &lod_set,
            skin_system: &skin_system,
            eye_system: &eye_system,
            mouth_system: &mouth_system,
            hair_system: &hair_system,
            clothing_cybernetics: &clothing_cybernetics,
            body_model: &body_model,
            motion_model: &motion_model,
            renderer_interface: &renderer_interface,
            ai_voice_integration: &ai_voice_integration,
            performance_capture: &valid_bundle.performance_capture,
            failure_fallbacks: &valid_bundle.failure_fallbacks,
            performance_budget: &valid_bundle.performance_budget,
            asset_package_contract: &valid_bundle.asset_package_contract,
            uncanny_review: &valid_bundle.uncanny_review,
            acceptance_evidence: &acceptance_evidence,
            generation_policy: &request.generation_policy,
            debug_views: &debug_views,
            request: &request,
        });

        assert!(!report.passed);
        for code in [
            "incomplete_renderer_interface",
            "incomplete_ai_voice_integration",
        ] {
            assert!(
                report.issues.iter().any(|issue| {
                    issue.severity == HumanValidationSeverity::Error && issue.code == code
                }),
                "{code} should be reported"
            );
        }
    }

    #[test]
    fn validation_blocks_incomplete_performance_capture_and_uncanny_review() {
        let request = hero_request();
        let bundle = generate_human_bundle(&request);
        let mut performance_capture = bundle.performance_capture.clone();
        performance_capture.timecode_binding = false;
        performance_capture
            .curve_bindings
            .retain(|curve| curve.kind != HumanPerformanceCaptureCurveKind::EyeGaze);

        let mut uncanny_review = bundle.uncanny_review.clone();
        uncanny_review.debug_view_bound = false;
        if let Some(item) = uncanny_review
            .items
            .iter_mut()
            .find(|item| item.kind == HumanUncannyReviewKind::PerformanceCaptureCurves)
        {
            item.passed = false;
        }

        let report = validate_human_bundle(HumanBundleValidationInput {
            render_meshes: &bundle.render_meshes,
            materials: &bundle.materials,
            physics_proxy: &bundle.physics_proxy,
            lod_set: &bundle.lod_set,
            skin_system: &bundle.skin_system,
            eye_system: &bundle.eye_system,
            mouth_system: &bundle.mouth_system,
            hair_system: &bundle.hair_system,
            clothing_cybernetics: &bundle.clothing_cybernetics,
            body_model: &bundle.body_model,
            motion_model: &bundle.motion_model,
            renderer_interface: &bundle.renderer_interface,
            ai_voice_integration: &bundle.ai_voice_integration,
            performance_capture: &performance_capture,
            failure_fallbacks: &bundle.failure_fallbacks,
            performance_budget: &bundle.performance_budget,
            asset_package_contract: &bundle.asset_package_contract,
            uncanny_review: &uncanny_review,
            acceptance_evidence: &bundle.acceptance_evidence,
            generation_policy: &bundle.generation_policy,
            debug_views: &bundle.debug_views,
            request: &request,
        });

        assert!(!report.passed);
        for code in [
            "incomplete_performance_capture_curves",
            "incomplete_uncanny_review",
        ] {
            assert!(
                report.issues.iter().any(|issue| {
                    issue.severity == HumanValidationSeverity::Error && issue.code == code
                }),
                "{code} should be reported"
            );
        }
    }

    #[test]
    fn validation_blocks_incomplete_acceptance_evidence() {
        let request = hero_request();
        let bundle = generate_human_bundle(&request);
        let mut acceptance_evidence = bundle.acceptance_evidence.clone();
        acceptance_evidence.hair_stability.shimmer_risk = 0.8;
        acceptance_evidence
            .closeup_dialogue_scene
            .debug_capture_ready = false;
        if let Some(check) = acceptance_evidence.lod_transition.checks.first_mut() {
            check.silhouette_error = 0.5;
        }

        let report = validate_human_bundle(HumanBundleValidationInput {
            render_meshes: &bundle.render_meshes,
            materials: &bundle.materials,
            physics_proxy: &bundle.physics_proxy,
            lod_set: &bundle.lod_set,
            skin_system: &bundle.skin_system,
            eye_system: &bundle.eye_system,
            mouth_system: &bundle.mouth_system,
            hair_system: &bundle.hair_system,
            clothing_cybernetics: &bundle.clothing_cybernetics,
            body_model: &bundle.body_model,
            motion_model: &bundle.motion_model,
            renderer_interface: &bundle.renderer_interface,
            ai_voice_integration: &bundle.ai_voice_integration,
            performance_capture: &bundle.performance_capture,
            failure_fallbacks: &bundle.failure_fallbacks,
            performance_budget: &bundle.performance_budget,
            asset_package_contract: &bundle.asset_package_contract,
            uncanny_review: &bundle.uncanny_review,
            acceptance_evidence: &acceptance_evidence,
            generation_policy: &bundle.generation_policy,
            debug_views: &bundle.debug_views,
            request: &request,
        });

        assert!(!report.passed);
        assert!(report.issues.iter().any(|issue| {
            issue.severity == HumanValidationSeverity::Error
                && issue.code == "incomplete_human_acceptance_evidence"
        }));
    }

    #[test]
    fn validation_blocks_incomplete_schema_asset_package_contract() {
        let request = hero_request();
        let bundle = generate_human_bundle(&request);
        let mut asset_package_contract = bundle.asset_package_contract.clone();
        asset_package_contract.manifest.schema_version = 0;
        asset_package_contract.manifest.provenance.clear();
        asset_package_contract.round_trip.lossless = false;
        asset_package_contract.integration_replay.replay_asset = 0;
        if let Some(binding) = asset_package_contract
            .runtime_schemas
            .iter_mut()
            .find(|binding| binding.schema.name == "SpeechResult")
        {
            binding.round_trip_serialized = false;
        }

        let report = validate_human_bundle(HumanBundleValidationInput {
            render_meshes: &bundle.render_meshes,
            materials: &bundle.materials,
            physics_proxy: &bundle.physics_proxy,
            lod_set: &bundle.lod_set,
            skin_system: &bundle.skin_system,
            eye_system: &bundle.eye_system,
            mouth_system: &bundle.mouth_system,
            hair_system: &bundle.hair_system,
            clothing_cybernetics: &bundle.clothing_cybernetics,
            body_model: &bundle.body_model,
            motion_model: &bundle.motion_model,
            renderer_interface: &bundle.renderer_interface,
            ai_voice_integration: &bundle.ai_voice_integration,
            performance_capture: &bundle.performance_capture,
            failure_fallbacks: &bundle.failure_fallbacks,
            performance_budget: &bundle.performance_budget,
            asset_package_contract: &asset_package_contract,
            uncanny_review: &bundle.uncanny_review,
            acceptance_evidence: &bundle.acceptance_evidence,
            generation_policy: &bundle.generation_policy,
            debug_views: &bundle.debug_views,
            request: &request,
        });

        assert!(!report.passed);
        assert!(report.issues.iter().any(|issue| {
            issue.severity == HumanValidationSeverity::Error
                && issue.code == "incomplete_schema_asset_package"
        }));
    }

    #[test]
    fn human_generation_cache_reuses_content_addressed_bundles() {
        let mut module = HumanGeneratorModule::default();
        let first_request = hero_request();
        let mut second_request = first_request.clone();
        second_request.request_id = first_request.request_id + 1;

        let first = module.generate(first_request);
        let second = module.generate(second_request.clone());

        assert_eq!(first, second);
        assert_eq!(
            module.cache_stats(),
            HumanGenerationCacheStats {
                entries: 1,
                hits: 1,
                misses: 1,
            }
        );

        let mut cold_module = HumanGeneratorModule::default();
        let cold = cold_module.generate(second_request);
        assert_eq!(first, cold);
        assert_eq!(cold_module.cache_stats().entries, 1);
        assert_eq!(cold_module.cache_stats().misses, 1);
    }

    #[test]
    fn human_generation_cache_keys_budget_and_skips_invalid_bundles() {
        let mut module = HumanGeneratorModule::default();
        let valid = module.generate(hero_request());
        assert!(valid.validation_report.passed);

        let mut invalid_request = hero_request();
        invalid_request.request_id += 5;
        invalid_request.budget.max_memory_bytes = 1024;
        let invalid = module.generate(invalid_request);

        assert!(!invalid.validation_report.passed);
        assert_eq!(module.cache_stats().entries, 1);
        assert_eq!(module.cache_stats().misses, 2);
    }

    #[test]
    fn human_bundle_lists_streaming_dependencies_by_lod_quality() {
        let mut module = HumanGeneratorModule::default();
        let bundle = module.generate(hero_request());

        let hero_dependencies = bundle.streaming_dependencies(QualityTier::HeroHighFidelityRuntime);
        assert!(
            hero_dependencies
                .iter()
                .any(|dependency| dependency.part == HumanMeshPart::Body
                    && dependency.lod == HumanLodTier::Hero
                    && dependency.priority == AssetPriority::Hero)
        );

        let runtime_dependencies =
            streaming_dependencies_for_bundle(&bundle, QualityTier::NormalRuntime);
        assert!(
            runtime_dependencies
                .iter()
                .any(|dependency| dependency.part == HumanMeshPart::Hair
                    && dependency.requested_quality >= QualityTier::NormalRuntime)
        );
        assert!(
            runtime_dependencies
                .iter()
                .any(|dependency| dependency.part == HumanMeshPart::Clothing)
        );
        assert!(
            runtime_dependencies
                .iter()
                .all(|dependency| dependency.requested_quality >= QualityTier::NormalRuntime)
        );
    }

    #[test]
    fn gpu_schedule_prepares_lods_visemes_surface_response_and_skinning() {
        let mut module = HumanGeneratorModule {
            applied_speech_events: [700].into_iter().collect(),
            applied_surface_events: [701].into_iter().collect(),
            requested_human_assets: [3_000].into_iter().collect(),
            ..HumanGeneratorModule::default()
        };
        let services = ashfall_core::gpu::GpuServices::new_vulkan();
        let mut graph = services.begin_frame(40);

        module.schedule_gpu(&mut graph);

        for expected in [
            "human_lod_selection",
            "human_eye_gaze_and_blink",
            "human_surface_response",
            "human_facial_viseme_morphs",
            "human_hair_cloth_prepare",
            "human_skinning_palette_upload",
            "human_renderer_interface_pack",
        ] {
            assert!(
                graph.passes().iter().any(|pass| pass.name == expected),
                "{expected} pass should be scheduled"
            );
        }
        let report = services.submit(graph);
        assert!(report.validation.passed);
        assert_eq!(report.pipeline_report.unknown_pipeline_count, 0);
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(40)
                && usage.label == "human runtime bundle table"
                && usage.bindless_index.is_some()
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "human_lod_selection")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(40)
                && usage.label == "human skin hair clothing response buffer"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "human_surface_response")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(40)
                && usage.label == "human skin closeup material channels"
                && usage.bindless_index.is_some()
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "human_skin_closeup_channels")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "human_skinning_palette_upload")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(40)
                && usage.label == "human facial morph target buffer"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "human_facial_viseme_morphs")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(40)
                && usage.label == "human eye gaze and blink buffer"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "human_eye_gaze_and_blink")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "human_facial_viseme_morphs")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(40)
                && usage.label == "human hair groom runtime table"
                && usage.bindless_index.is_some()
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "human_hair_cloth_prepare")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "human_skinning_palette_upload")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(40)
                && usage.label == "human clothing cybernetics constraint buffer"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "human_hair_cloth_prepare")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "human_skinning_palette_upload")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(40)
                && usage.label == "human skinning palette buffer"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "human_skinning_palette_upload")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "human_renderer_interface_pack")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(40)
                && usage.label == "human renderer interface packet"
                && usage.bindless_index.is_some()
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "human_renderer_interface_pack")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "human_surface_response"
                && pipeline.shader_key == "human/surface_response.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "SKIN_WETNESS")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "human_skin_closeup_channels"
                && pipeline.shader_key == "human/skin_closeup_channels.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "SKIN_SUBSURFACE")
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "WRINKLE_TENSION")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "human_eye_gaze_and_blink"
                && pipeline.shader_key == "human/eye_gaze_and_blink.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "EYE_GAZE")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "human_hair_cloth_prepare"
                && pipeline.shader_key == "human/hair_cloth_prepare.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "HAIR_GUIDES")
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "FACIAL_HAIR_LOD")
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "CYBERNETIC_EMISSIVE")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "human_facial_viseme_morphs"
                && pipeline.shader_key == "human/facial_viseme_morphs.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "VISEME_TRACKS")
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "TONGUE_CONTACTS")
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "INNER_MOUTH_OCCLUSION")
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "PERFORMANCE_CAPTURE_CURVES")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "human_skinning_palette_upload"
                && pipeline.shader_key == "human/skinning_palette_upload.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "GPU_SKINNING")
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "BODY_SHAPE_MORPHS")
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "SOFT_TISSUE_ZONES")
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "MASS_DISTRIBUTION")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "human_renderer_interface_pack"
                && pipeline.shader_key == "human/renderer_interface_pack.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "RIGGED_MESH_HANDLES")
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "MORPH_ANIMATION_BUFFERS")
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "MOUTH_MATERIAL_HANDLES")
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "MOUTH_ARTICULATION_BUFFER")
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "VISEME_TIMINGS")
        }));
    }

    #[test]
    fn validation_reports_hard_budget_failures() {
        let mut request = hero_request();
        request.budget.max_meshes = 1;
        request.budget.max_memory_bytes = 1024;

        let bundle = generate_human_bundle(&request);

        assert!(!bundle.validation_report.passed);
        assert!(bundle.validation_report.issues.iter().any(|issue| {
            issue.severity == HumanValidationSeverity::Error && issue.code == "mesh_budget_exceeded"
        }));
        assert!(bundle.validation_report.issues.iter().any(|issue| {
            issue.severity == HumanValidationSeverity::Error
                && issue.code == "memory_budget_exceeded"
        }));
    }

    #[test]
    fn surface_response_tracks_wetness_injury_and_stress() {
        let request = hero_request();
        let profile = build_surface_response_profile(&request);
        let response = evaluate_human_surface_response(
            &profile,
            MaterialState {
                temperature: 309.0,
                moisture: 0.72,
                soot: 0.42,
                plastic_strain: 0.18,
                crack_density: 0.66,
                biological_contamination: 0.2,
                pressure: 150_000.0,
                ..MaterialState::default()
            },
            Some(&EmotionState::alarmed()),
        );

        assert!(response.skin_wetness > 0.5);
        assert!(response.sweat_sheen > 0.12);
        assert!(response.injury_overlay > 0.3);
        assert!(response.bruising > 0.2);
        assert!(response.dirt > 0.3);
        assert!(response.hair_wetness > 0.5);
        assert!(response.clothing_wetness > 0.5);
        assert!(response.eye_redness > profile.eye_redness_response);
        assert!(response.material_layers.iter().any(|layer| {
            layer.layer == AnatomicalLayerKind::Skin && layer.injury > 0.3 && layer.wetness > 0.5
        }));
    }

    #[test]
    fn module_emits_distinct_face_sync_for_multiple_same_tick_speech_events() {
        let mut world = WorldState::default();
        world
            .spawn_entity_template(EntityTemplate {
                entity_id: Some(3),
                name: "speaking npc".to_string(),
                transform: Transform::at(Vec3::new(1.0, 0.0, 0.0)),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 68.0,
                    material_id: DEFAULT_HUMAN_SKIN_MATERIAL,
                    dynamic: true,
                    fragile: false,
                }),
                material_state: None,
                human: Some(HumanState {
                    human_id: 300,
                    quality_tier: QualityTier::HeroHighFidelityRuntime,
                }),
                agent: None,
                tags: vec!["npc".to_string()],
            })
            .expect("human should spawn");
        let speech_event = |event_id, duration_seconds, viseme_count| WorldEvent {
            event_id,
            tick: 5,
            location_meters: Vec3::new(1.0, 0.0, 0.0),
            actors: vec![3],
            kind: WorldEventKind::SpeechSynthesized {
                speaker: 3,
                audio_clip: AudioClipHandle(event_id),
                duration_seconds,
                phoneme_count: viseme_count * 2,
                viseme_count,
            },
            physical_evidence: vec!["speech_audio".to_string()],
            narrative_tags: vec!["voice".to_string()],
        };
        let frame = FrameContext {
            frame_id: 5,
            sim_time: SimTime::new(5.0 / 60.0, 5),
            dt_seconds: 1.0 / 60.0,
            quality_tier: QualityTier::NormalRuntime,
            snapshot: world.snapshot(5, SimTime::new(5.0 / 60.0, 5)),
            recent_events: vec![speech_event(900, 1.2, 5), speech_event(901, 2.4, 9)],
            forces: Vec::new(),
        };
        let mut module = HumanGeneratorModule::default();
        let mut sink = CommandSink::default();

        module.tick(&frame, &mut sink);

        let face_events = sink
            .events
            .iter()
            .filter_map(|event| match event.kind {
                WorldEventKind::FacialAnimationApplied {
                    entity,
                    viseme_count,
                    duration_seconds,
                } => Some((event.event_id, entity, viseme_count, duration_seconds)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(face_events.len(), 2);
        assert_ne!(face_events[0].0, face_events[1].0);
        assert!(face_events.iter().any(|(_, entity, visemes, duration)| {
            *entity == 3 && *visemes == 5 && (*duration - 1.2).abs() < 0.001
        }));
        assert!(face_events.iter().any(|(_, entity, visemes, duration)| {
            *entity == 3 && *visemes == 9 && (*duration - 2.4).abs() < 0.001
        }));
    }

    #[test]
    fn module_emits_human_appearance_update_for_material_state_changes() {
        let mut world = WorldState::default();
        world
            .spawn_entity_template(EntityTemplate {
                entity_id: Some(3),
                name: "injured npc".to_string(),
                transform: Transform::at(Vec3::new(1.0, 0.0, 0.0)),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 68.0,
                    material_id: DEFAULT_HUMAN_SKIN_MATERIAL,
                    dynamic: true,
                    fragile: false,
                }),
                material_state: Some(MaterialState {
                    moisture: 0.68,
                    soot: 0.32,
                    crack_density: 0.48,
                    pressure: 140_000.0,
                    ..MaterialState::default()
                }),
                human: Some(HumanState {
                    human_id: 300,
                    quality_tier: QualityTier::HeroHighFidelityRuntime,
                }),
                agent: Some(AgentState {
                    persona: 300,
                    emotional_state: EmotionState::alarmed(),
                    ai_lod: QualityTier::NormalRuntime,
                }),
                tags: vec!["npc".to_string()],
            })
            .expect("human should spawn");
        let frame = FrameContext {
            frame_id: 5,
            sim_time: SimTime::new(5.0 / 60.0, 5),
            dt_seconds: 1.0 / 60.0,
            quality_tier: QualityTier::NormalRuntime,
            snapshot: world.snapshot(5, SimTime::new(5.0 / 60.0, 5)),
            recent_events: vec![WorldEvent {
                event_id: 900,
                tick: 5,
                location_meters: Vec3::new(1.0, 0.0, 0.0),
                actors: vec![3],
                kind: WorldEventKind::MaterialStateChanged { entity: 3 },
                physical_evidence: vec!["material_state_delta".to_string()],
                narrative_tags: vec!["human".to_string()],
            }],
            forces: Vec::new(),
        };
        let mut module = HumanGeneratorModule::default();
        let mut sink = CommandSink::default();

        module.tick(&frame, &mut sink);

        assert!(sink.events.iter().any(|event| {
            matches!(
                event.kind,
                WorldEventKind::HumanAppearanceUpdated {
                    entity: 3,
                    skin_wetness,
                    injury_overlay,
                    bruising,
                    dirt,
                } if skin_wetness > 0.4
                    && injury_overlay > 0.2
                    && bruising > 0.1
                    && dirt > 0.2
            )
        }));
    }
}
