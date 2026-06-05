use std::collections::{BTreeMap, BTreeSet};

use ashfall_core::assets::AssetPriority;
use ashfall_core::core::*;
use ashfall_core::gpu::{
    ComputePipelineHandle, GpuDispatchKind, GpuGraphBuilder, GpuPassDesc, GpuPipelineDesc,
    GpuQueueKind, GpuResourceDesc, GpuResourceHandle, GpuResourceKind, GpuResourceLifetime,
    GpuShaderPermutation,
};
use ashfall_core::runtime::{EngineModule, FrameContext, ModuleDescriptor, ModuleStateRecord};
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
    pub eye_system: EyeRuntimeModel,
    pub mouth_system: MouthRuntimeModel,
    pub motion_model: HumanMotionModel,
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
            && self.lip_wetness > 0.05
            && self.viseme_shape_count >= 4
            && self.jaw_motion
            && self.cheek_motion
            && self.lip_compression_contact
            && self.breath_speech_timing
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
    FacialRig,
    VisemeTimeline,
    GazeTarget,
    SkinMaterialChannels,
    EyeMoistureTearline,
    HairLod,
    ClothSimulation,
    AnimationContact,
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
    let eye_system = build_eye_runtime_model(request);
    let mouth_system = build_mouth_runtime_model(request, &facial_rig);
    let motion_model = build_motion_model(request, &physics_proxy, &animation_profile);
    let debug_views = build_debug_views(request);
    let validation_report = validate_human_bundle(HumanBundleValidationInput {
        render_meshes: &render_meshes,
        materials: &materials,
        physics_proxy: &physics_proxy,
        lod_set: &lod_set,
        eye_system: &eye_system,
        mouth_system: &mouth_system,
        motion_model: &motion_model,
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
        eye_system,
        mouth_system,
        motion_model,
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
    pub eye_system: &'a EyeRuntimeModel,
    pub mouth_system: &'a MouthRuntimeModel,
    pub motion_model: &'a HumanMotionModel,
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
        eye_system,
        mouth_system,
        motion_model,
        generation_policy,
        debug_views,
        request,
    } = input;
    let estimated_hair_strands = estimate_hair_strands(request);
    let estimated_memory_bytes = lod_set
        .levels
        .iter()
        .map(|lod| lod.estimated_memory_bytes)
        .sum::<u64>()
        + materials.len() as u64 * 512 * 1024;
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
            "hero dialogue humans require teeth, tongue, wet lips, visemes, jaw, cheek, and breath timing controls",
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
        && debug_views.ready_count() < 9
    {
        issues.push(validation_issue(
            HumanValidationSeverity::Error,
            "missing_human_debug_views",
            "hero human bundles must expose the full V9 debug-view checklist",
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
        vec![ashfall_core::schema::SchemaRequirement::required(
            40,
            "SpeechResult",
            1,
            "human generator consumes viseme and duration data for facial animation",
        )]
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
                human_shader_permutation(["VISEME_TRACKS", "FACIAL_RIG", "EMOTION_CURVE"]),
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
            let hair_cloth_pipeline = human_compute_pipeline_with_permutation(
                graph,
                "human_hair_cloth_prepare",
                "human/hair_cloth_prepare.comp",
                QualityTier::NormalRuntime,
                human_shader_permutation(["HAIR_LOD", "CLOTH_ATTACHMENTS", "PHYSICS_PROXY"]),
            );
            let mut reads = vec![bundle_table, lod_selection];
            if let Some(surface_response) = surface_response {
                reads.push(surface_response);
            }
            graph.add_pass(
                GpuPassDesc::new(
                    "human_hair_cloth_prepare",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(hair_cloth_pipeline),
                    QualityTier::NormalRuntime,
                )
                .reads(reads)
                .writes([hair_cloth_controls]),
            );
            Some(hair_cloth_controls)
        } else {
            None
        };

        let skinning_pipeline = human_compute_pipeline_with_permutation(
            graph,
            "human_skinning_palette_upload",
            "human/skinning_palette_upload.comp",
            QualityTier::NormalRuntime,
            human_shader_permutation(["GPU_SKINNING", "LOD_BONE_REMAP", "MORPH_TARGETS"]),
        );
        let mut skinning_reads = vec![bundle_table, lod_selection, eye_gaze_state];
        if let Some(facial_morphs) = facial_morphs {
            skinning_reads.push(facial_morphs);
        }
        if let Some(hair_cloth_controls) = hair_cloth_controls {
            skinning_reads.push(hair_cloth_controls);
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
                radius_meters: request.morphology.shoulder_width_meters * 0.35,
                half_height_meters: height * 0.24,
                mass_kg: mass * 0.45,
            },
            PhysicsCapsule {
                label: "head".to_string(),
                radius_meters: 0.11,
                half_height_meters: 0.08,
                mass_kg: mass * 0.08,
            },
            PhysicsCapsule {
                label: "legs".to_string(),
                radius_meters: 0.09,
                half_height_meters: height * 0.28,
                mass_kg: mass * 0.32,
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
    MouthRuntimeModel {
        teeth_geometry: true,
        tongue_animation: facial_rig
            .viseme_set
            .iter()
            .any(|viseme| viseme.label == "tongue"),
        inner_mouth_shading: true,
        lip_wetness: (request.eyes.wetness * 0.35 + request.skin.sweat * 0.25 + 0.18)
            .clamp(0.0, 1.0),
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
                HumanDebugViewKind::AnimationContact,
                "animation contact",
                QualityTier::NormalRuntime,
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
        let bundle = generate_human_bundle(&hero_request());

        assert!(bundle.validation_report.passed);
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
        assert!(bundle.motion_model.supports_weighted_closeup_motion());
        assert!(bundle.motion_model.muscle_soft_tissue);
        assert_eq!(bundle.debug_views.ready_count(), 9);
        for expected in [
            HumanDebugViewKind::FacialRig,
            HumanDebugViewKind::VisemeTimeline,
            HumanDebugViewKind::GazeTarget,
            HumanDebugViewKind::SkinMaterialChannels,
            HumanDebugViewKind::EyeMoistureTearline,
            HumanDebugViewKind::HairLod,
            HumanDebugViewKind::ClothSimulation,
            HumanDebugViewKind::AnimationContact,
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
                && usage.label == "human skinning palette buffer"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "human_skinning_palette_upload")
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
            pipeline.label == "human_eye_gaze_and_blink"
                && pipeline.shader_key == "human/eye_gaze_and_blink.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "EYE_GAZE")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "human_facial_viseme_morphs"
                && pipeline.shader_key == "human/facial_viseme_morphs.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "VISEME_TRACKS")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "human_skinning_palette_upload"
                && pipeline.shader_key == "human/skinning_palette_upload.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "GPU_SKINNING")
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
