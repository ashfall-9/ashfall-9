use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use ashfall_core::core::*;
use ashfall_core::gpu::{
    ComputePipelineHandle, GpuDispatchKind, GpuGraphBuilder, GpuPassDesc, GpuPipelineDesc,
    GpuQueueKind, GpuResourceDesc, GpuResourceHandle, GpuResourceKind, GpuResourceLifetime,
    GpuShaderPermutation,
};
use ashfall_core::runtime::{EngineModule, FrameContext, ModuleDescriptor, ModuleStateRecord};
use ashfall_core::world::{
    AudioEvent, AudioEventKind, CommandSink, WorldCommand, WorldEvent, WorldEventKind,
    WorldSnapshot,
};
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Source};

pub type SpeechIntent = String;
pub type InterruptionPolicy = String;
pub type VoiceTimingReport = PerformanceCounters;
pub type AudioTimingReport = PerformanceCounters;
pub type AudioFrameOutputBuffer = AudioBufferHandle;
pub type EmotionCurve = Vec<(f32, EmotionState)>;
const VOICE_MODULE_STATE_VERSION: u32 = 2;
const AUDIO_MIXER_MODULE_STATE_VERSION: u32 = 1;
const VOICE_GENERATION_JOBS_PER_FRAME: usize = 1;
const DEFERRED_SPEECH_TEXT: &str = "Hold on.";

#[derive(Clone, Debug, PartialEq)]
pub struct RodioPreviewTone {
    pub frequency_hz: f32,
    pub duration_seconds: f32,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub preview_samples: Vec<f32>,
}

pub fn rodio_preview_tone(frequency_hz: f32, duration_seconds: f32, gain: f32) -> RodioPreviewTone {
    let frequency_hz = frequency_hz.clamp(40.0, 4_000.0);
    let duration_seconds = duration_seconds.clamp(0.01, 10.0);
    let gain = gain.clamp(0.0, 1.0);
    let mut source = rodio::source::SineWave::new(frequency_hz)
        .amplify(gain)
        .take_duration(Duration::from_secs_f32(duration_seconds));
    let sample_rate_hz = source.sample_rate().get();
    let channels = source.channels().get();
    let preview_samples = source.by_ref().take(32).collect();

    RodioPreviewTone {
        frequency_hz,
        duration_seconds,
        sample_rate_hz,
        channels,
        preview_samples,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RodioEventCueSpec {
    pub label: String,
    pub frequency_hz: f32,
    pub duration_seconds: f32,
    pub gain: f32,
}

impl RodioEventCueSpec {
    pub fn preview(&self) -> RodioPreviewTone {
        rodio_preview_tone(self.frequency_hz, self.duration_seconds, self.gain)
    }
}

pub struct RodioEventAudioSink {
    sink: MixerDeviceSink,
}

impl RodioEventAudioSink {
    pub fn open_default() -> Result<Self, String> {
        let mut sink = DeviceSinkBuilder::open_default_sink()
            .map_err(|error| format!("failed to open default audio output: {error}"))?;
        sink.log_on_drop(false);
        Ok(Self { sink })
    }

    pub fn play_cue(&self, cue: &RodioEventCueSpec) -> RodioPreviewTone {
        let preview = cue.preview();
        let source = rodio::source::SineWave::new(cue.frequency_hz)
            .amplify(cue.gain.clamp(0.0, 1.0))
            .take_duration(Duration::from_secs_f32(
                cue.duration_seconds.clamp(0.01, 10.0),
            ));
        self.sink.mixer().add(source);
        preview
    }

    pub fn play_world_event(&self, event: &WorldEvent) -> Option<RodioPreviewTone> {
        rodio_event_cue_for_world_event(event).map(|cue| self.play_cue(&cue))
    }

    pub fn play_world_events(&self, events: &[WorldEvent], max_cues: usize) -> usize {
        events
            .iter()
            .filter_map(|event| self.play_world_event(event))
            .take(max_cues)
            .count()
    }
}

pub fn rodio_event_cue_for_world_event(event: &WorldEvent) -> Option<RodioEventCueSpec> {
    match &event.kind {
        WorldEventKind::SoundEmitted {
            kind,
            material_id,
            intensity,
            occlusion_hint,
            ..
        } => Some(rodio_cue_for_sound(
            *kind,
            *material_id,
            *intensity,
            *occlusion_hint,
        )),
        WorldEventKind::GlassWallFractured { .. } => Some(RodioEventCueSpec {
            label: "glass fracture".to_string(),
            frequency_hz: 1_260.0,
            duration_seconds: 0.18,
            gain: 0.28,
        }),
        WorldEventKind::MetalBent { .. } => Some(RodioEventCueSpec {
            label: "metal strain".to_string(),
            frequency_hz: 180.0,
            duration_seconds: 0.24,
            gain: 0.22,
        }),
        WorldEventKind::StreetFlooded => Some(RodioEventCueSpec {
            label: "water surge".to_string(),
            frequency_hz: 260.0,
            duration_seconds: 0.32,
            gain: 0.18,
        }),
        WorldEventKind::ToxicGasReleased => Some(RodioEventCueSpec {
            label: "gas warning".to_string(),
            frequency_hz: 380.0,
            duration_seconds: 0.28,
            gain: 0.2,
        }),
        WorldEventKind::NpcWitnessedCrime { .. } | WorldEventKind::PlayerIdentityExposed => {
            Some(RodioEventCueSpec {
                label: "social alarm".to_string(),
                frequency_hz: 720.0,
                duration_seconds: 0.22,
                gain: 0.18,
            })
        }
        WorldEventKind::NpcHeardSound { .. }
        | WorldEventKind::AgentIntentProposed { .. }
        | WorldEventKind::AgentDecisionExplained { .. } => Some(RodioEventCueSpec {
            label: "agent reaction".to_string(),
            frequency_hz: 540.0,
            duration_seconds: 0.14,
            gain: 0.14,
        }),
        WorldEventKind::NavigationMoveBlocked { .. } => Some(RodioEventCueSpec {
            label: "blocked step".to_string(),
            frequency_hz: 210.0,
            duration_seconds: 0.12,
            gain: 0.1,
        }),
        WorldEventKind::DialogueEmitted { .. }
        | WorldEventKind::VoiceLineSpoken { .. }
        | WorldEventKind::SpeechSynthesized { .. } => Some(RodioEventCueSpec {
            label: "voice cue".to_string(),
            frequency_hz: 460.0,
            duration_seconds: 0.18,
            gain: 0.16,
        }),
        WorldEventKind::SecurityAlertRaised { .. }
        | WorldEventKind::SurveillanceIncreased { .. }
        | WorldEventKind::FactionReputationChanged { .. } => Some(RodioEventCueSpec {
            label: "security alert".to_string(),
            frequency_hz: 920.0,
            duration_seconds: 0.2,
            gain: 0.2,
        }),
        WorldEventKind::PowerTransformerOverheated { .. } => Some(RodioEventCueSpec {
            label: "electrical warning".to_string(),
            frequency_hz: 110.0,
            duration_seconds: 0.26,
            gain: 0.18,
        }),
        _ => None,
    }
}

fn rodio_cue_for_sound(
    kind: AudioEventKind,
    material_id: Option<MaterialId>,
    intensity: f32,
    occlusion_hint: f32,
) -> RodioEventCueSpec {
    let effective_intensity =
        material_aware_intensity(kind, material_id, intensity, occlusion_hint).clamp(0.0, 1.0);
    RodioEventCueSpec {
        label: format!("{kind:?}"),
        frequency_hz: rodio_cue_frequency_hz(kind),
        duration_seconds: rodio_cue_duration_seconds(kind),
        gain: (0.08 + effective_intensity * 0.24).clamp(0.0, 0.34),
    }
}

fn rodio_cue_frequency_hz(kind: AudioEventKind) -> f32 {
    match kind {
        AudioEventKind::GlassImpact => 980.0,
        AudioEventKind::GlassShatter => 1_420.0,
        AudioEventKind::MetalBend => 170.0,
        AudioEventKind::ConcreteCrack => 220.0,
        AudioEventKind::Footstep => 120.0,
        AudioEventKind::WaterSplash => 310.0,
        AudioEventKind::SteamLeak => 640.0,
        AudioEventKind::Gunshot => 1_100.0,
        AudioEventKind::Explosion => 80.0,
        AudioEventKind::ElectricBuzz | AudioEventKind::NeonHum => 90.0,
        AudioEventKind::DoorOpen => 210.0,
        AudioEventKind::ClothRustle => 480.0,
        AudioEventKind::HumanBreath => 260.0,
        AudioEventKind::VoiceSpeech => 430.0,
    }
}

fn rodio_cue_duration_seconds(kind: AudioEventKind) -> f32 {
    match kind {
        AudioEventKind::Explosion => 0.45,
        AudioEventKind::GlassShatter => 0.28,
        AudioEventKind::WaterSplash => 0.24,
        AudioEventKind::VoiceSpeech => 0.22,
        AudioEventKind::NeonHum | AudioEventKind::ElectricBuzz => 0.35,
        _ => 0.16,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpeechRequest {
    pub request_id: u128,
    pub speaker: EntityId,
    pub voice_persona: VoicePersonaId,
    pub text: String,
    pub emotion: EmotionState,
    pub intent: SpeechIntent,
    pub urgency: f32,
    pub loudness: f32,
    pub language: LanguageId,
    pub interruption_policy: InterruptionPolicy,
    pub location: Option<Vec3>,
    pub priority: SpeechPriority,
    pub mode: SpeechGenerationMode,
    pub max_latency_ms: u32,
    pub safety_context: VoiceSafetyContext,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpeechPriority {
    Background,
    #[default]
    Normal,
    Important,
    Critical,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpeechGenerationMode {
    PreAuthored,
    #[default]
    CachedGenerated,
    OnDemandGenerated,
    CrowdChatter,
    RadioComms,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceSafetyContext {
    pub allow_generated_voice: bool,
    pub content_review_required: bool,
    pub licensed_persona_required: bool,
}

impl Default for VoiceSafetyContext {
    fn default() -> Self {
        Self {
            allow_generated_voice: true,
            content_review_required: false,
            licensed_persona_required: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpeechResult {
    pub request_id: u128,
    pub speaker: EntityId,
    pub voice_persona: VoicePersonaId,
    pub audio_clip: AudioClipHandle,
    pub phonemes: Vec<PhonemeTiming>,
    pub visemes: Vec<VisemeTiming>,
    pub emotion_curve: EmotionCurve,
    pub prosody: ProsodyFrameSet,
    pub duration_seconds: f32,
    pub cache_status: SpeechCacheStatus,
    pub validation_report: SpeechValidationReport,
    pub generation_cost: VoiceTimingReport,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeechCacheStatus {
    Generated,
    CacheHit,
    CapturedPerformance,
    Fallback,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProsodyFrameSet {
    pub frames: Vec<ProsodyFrame>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProsodyFrame {
    pub time_seconds: f32,
    pub pitch_hz: f32,
    pub energy: f32,
    pub breathiness: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PhonemeTiming {
    pub phoneme: String,
    pub start_seconds: f32,
    pub end_seconds: f32,
    pub confidence: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VisemeTiming {
    pub viseme: String,
    pub start_seconds: f32,
    pub end_seconds: f32,
    pub weight: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PerformanceCaptureClip {
    pub clip_id: u128,
    pub speaker: EntityId,
    pub voice_persona: VoicePersonaId,
    pub source: PerformanceCaptureSource,
    pub provenance: PerformanceCaptureProvenance,
    pub timecode: PerformanceCaptureTimecode,
    pub audio_track: AudioClipHandle,
    pub duration_seconds: f32,
    pub phonemes: Vec<PhonemeTiming>,
    pub visemes: Vec<VisemeTiming>,
    pub emotion_curve: EmotionCurve,
    pub prosody: ProsodyFrameSet,
    pub face_curves: Vec<PerformanceCurveSample>,
    pub body_curves: Vec<PerformanceCurveSample>,
    pub calibration: PerformanceCaptureCalibration,
    pub target_rig: PerformanceRigMapping,
    pub validation_notes: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerformanceCaptureSource {
    ActorSession,
    ManualAnimation,
    AiGeneratedAnimationHints,
    Placeholder,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PerformanceCaptureProvenance {
    pub actor_or_source_label: String,
    pub session_label: String,
    pub rights_verified: bool,
    pub consent_verified: bool,
    pub retargeting_allowed: bool,
    pub notes: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerformanceCaptureTimecode {
    pub start_seconds: f32,
    pub frame_rate: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PerformanceCurveSample {
    pub channel: PerformanceCurveChannel,
    pub time_seconds: f32,
    pub value: f32,
    pub confidence: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerformanceCurveChannel {
    JawOpen,
    LipRound,
    LipSpread,
    BrowRaise,
    EyeSquint,
    HeadYaw,
    HeadPitch,
    ChestBreath,
    BodyLean,
    HandGesture,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerformanceCaptureCalibration {
    pub audio_sample_rate_hz: u32,
    pub video_frame_rate: f32,
    pub face_solve_confidence: f32,
    pub body_solve_confidence: f32,
    pub audio_video_offset_seconds: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PerformanceRigMapping {
    pub target_rig_label: String,
    pub viseme_set_label: String,
    pub mapped_face_channels: Vec<PerformanceCurveChannel>,
    pub mapped_body_channels: Vec<PerformanceCurveChannel>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PerformanceCaptureValidationReport {
    pub passed: bool,
    pub issues: Vec<PerformanceCaptureValidationIssue>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PerformanceCaptureValidationIssue {
    pub severity: SpeechValidationSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpeechValidationReport {
    pub passed: bool,
    pub issues: Vec<SpeechValidationIssue>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpeechValidationIssue {
    pub severity: SpeechValidationSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeechValidationSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpeechComfortReport {
    pub passed: bool,
    pub score: f32,
    pub duration_seconds: f32,
    pub phonemes_per_second: f32,
    pub visemes_per_second: f32,
    pub average_phoneme_confidence: f32,
    pub max_pitch_jump_hz: f32,
    pub max_energy: f32,
    pub max_breathiness: f32,
    pub issue_count: usize,
    pub issues: Vec<SpeechComfortIssue>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpeechComfortIssue {
    pub severity: SpeechComfortIssueSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpeechComfortIssueSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConversationComfortReport {
    pub passed: bool,
    pub line_count: usize,
    pub unique_voice_persona_count: usize,
    pub total_duration_seconds: f32,
    pub average_line_score: f32,
    pub minimum_line_score: f32,
    pub projected_ten_minute_score: f32,
    pub fallback_count: usize,
    pub cache_hit_count: usize,
    pub repeated_clip_count: usize,
    pub issue_count: usize,
    pub issues: Vec<SpeechComfortIssue>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VoicePersona {
    pub id: VoicePersonaId,
    pub display_name: String,
    pub timbre_profile: TimbreProfile,
    pub pitch_range: PitchRange,
    pub speaking_rate: f32,
    pub accent_profile: AccentProfile,
    pub emotional_range: EmotionalRange,
    pub breath_profile: BreathProfile,
    pub hesitation_style: HesitationStyle,
    pub fatigue_response: FatigueResponse,
    pub injury_response: InjuryVoiceResponse,
    pub provenance: VoiceProvenance,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TimbreProfile {
    pub brightness: f32,
    pub roughness: f32,
    pub warmth: f32,
    pub nasality: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PitchRange {
    pub min_hz: f32,
    pub resting_hz: f32,
    pub max_hz: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccentProfile {
    pub label: String,
    pub rhythm: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmotionalRange {
    pub supports_fear: bool,
    pub supports_anger: bool,
    pub supports_sadness: bool,
    pub supports_urgency: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BreathProfile {
    pub breaths_per_minute: f32,
    pub audible_breathiness: f32,
    pub panic_multiplier: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HesitationStyle {
    None,
    ShortPauses,
    FillerWords,
    BrokenPhrases,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FatigueResponse {
    pub pitch_drop_hz: f32,
    pub speaking_rate_multiplier: f32,
    pub breathiness_gain: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InjuryVoiceResponse {
    pub pain_pitch_shift_hz: f32,
    pub breath_interruptions: f32,
    pub volume_drop: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceProvenance {
    pub source: VoiceSource,
    pub rights_verified: bool,
    pub actor_consent: bool,
    pub generated_model_label: String,
    pub replacement_allowed: bool,
    pub notes: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceSource {
    GeneratedOriginal,
    ActorProvided,
    LicensedLibrary,
    PlaceholderSynthetic,
    Unverified,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActiveSoundInfo {
    pub event_id: WorldEventId,
    pub source_entity: Option<EntityId>,
    pub event_kind: AudioEventKind,
    pub material_id: Option<MaterialId>,
    pub clip: AudioClipHandle,
    pub bus: AudioBusKind,
    pub priority: AudioPriority,
    pub location: Vec3,
    pub distance_to_listener_meters: f32,
    pub effective_intensity: f32,
    pub radius_meters: f32,
    pub remaining_seconds: f32,
    pub spatial: SpatialAudioParams,
    pub material_profile: MaterialSoundProfile,
    pub acoustic_environment: AcousticEnvironment,
    pub acoustic_zone_entity: Option<EntityId>,
    pub occlusion_sources: Vec<AudioOcclusionReport>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SoundSpatializationInput {
    pub event_id: WorldEventId,
    pub kind: AudioEventKind,
    pub source_entity: Option<EntityId>,
    pub material_id: Option<MaterialId>,
    pub location: Vec3,
    pub listener: Vec3,
    pub intensity: f32,
    pub radius_meters: f32,
    pub occlusion_hint: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpatialAudioParams {
    pub gain: f32,
    pub pan: f32,
    pub low_pass_hz: f32,
    pub reverb_send: f32,
    pub early_reflection_gain: f32,
    pub reverb_tail_seconds: f32,
    pub air_absorption: f32,
    pub occlusion: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AudioBusKind {
    Voice,
    Impact,
    Foley,
    Ambience,
    Mechanical,
    Music,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AudioPriority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcousticSpaceKind {
    Outdoor,
    NarrowAlley,
    SmallInterior,
    LargeInterior,
    Tunnel,
    DampUtility,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AcousticEnvironment {
    pub space: AcousticSpaceKind,
    pub reverb_send: f32,
    pub early_reflection_gain: f32,
    pub reverb_tail_seconds: f32,
    pub air_absorption: f32,
    pub obstruction_density: f32,
}

impl Default for AcousticEnvironment {
    fn default() -> Self {
        Self {
            space: AcousticSpaceKind::Outdoor,
            reverb_send: 0.12,
            early_reflection_gain: 0.08,
            reverb_tail_seconds: 0.8,
            air_absorption: 0.04,
            obstruction_density: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioOcclusionReport {
    pub occluder_entity: EntityId,
    pub occluder_label: String,
    pub occlusion: f32,
    pub distance_to_sound_ray_meters: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AcousticSceneEvaluation {
    pub environment: AcousticEnvironment,
    pub zone_entity: Option<EntityId>,
    pub source_inside_zone: bool,
    pub listener_inside_zone: bool,
    pub world_occlusion: f32,
    pub occlusion_sources: Vec<AudioOcclusionReport>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialSoundProfile {
    pub family: SoundFamily,
    pub absorption: f32,
    pub resonance: f32,
    pub wet_modifier: f32,
    pub break_brightness: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundFamily {
    Glass,
    Concrete,
    Metal,
    Water,
    Human,
    Electric,
    Generic,
}

pub type AudioDistrictId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundscapeWeatherState {
    Clear,
    Rain,
    Flooding,
    GasHazard,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DistrictAudioState {
    pub district: AudioDistrictId,
    pub weather: SoundscapeWeatherState,
    pub power_level: f32,
    pub crowd_density: f32,
    pub traffic_density: f32,
    pub crime_pressure: f32,
    pub alertness: f32,
    pub pollution_level: f32,
    pub active_events: Vec<WorldEventId>,
}

impl DistrictAudioState {
    pub fn baseline(district: AudioDistrictId) -> Self {
        Self {
            district,
            weather: SoundscapeWeatherState::Clear,
            power_level: 1.0,
            crowd_density: 0.45,
            traffic_density: 0.55,
            crime_pressure: 0.2,
            alertness: 0.15,
            pollution_level: 0.35,
            active_events: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SoundscapeLayerKind {
    Traffic,
    Rain,
    NeonHum,
    Drones,
    Crowd,
    Advertisements,
    TransformerHum,
    Machinery,
    Ventilation,
    WaterDrips,
    PoliceScanner,
    EmergencyAlarm,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SoundscapeLayer {
    pub kind: SoundscapeLayerKind,
    pub clip: AudioClipHandle,
    pub bus: AudioBusKind,
    pub priority: AudioPriority,
    pub intensity: f32,
    pub ducking: f32,
    pub event_driven: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MusicGameplayState {
    Exploration,
    Stealth,
    Combat,
    Social,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MusicStemKind {
    Pulse,
    Bass,
    Texture,
    Tension,
    Melody,
    Percussion,
    RadioBed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MusicTransitionKind {
    Hold,
    LayerIn,
    LayerOut,
    Crossfade,
    Stinger,
    SilenceDrop,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdaptiveScoreInput {
    pub district: AudioDistrictId,
    pub district_identity: String,
    pub weather: SoundscapeWeatherState,
    pub danger_level: f32,
    pub faction_tension: f32,
    pub story_thread_intensity: f32,
    pub player_reputation: f32,
    pub gameplay_state: MusicGameplayState,
    pub crowd_density: f32,
    pub power_level: f32,
    pub radio_signal_strength: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MusicStem {
    pub kind: MusicStemKind,
    pub clip: AudioClipHandle,
    pub intensity: f32,
    pub ducking: f32,
    pub diegetic: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiegeticMusicSource {
    pub label: String,
    pub clip: AudioClipHandle,
    pub intensity: f32,
    pub radio_filtered: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadioStationState {
    pub station_id: u64,
    pub label: String,
    pub enabled: bool,
    pub signal_strength: f32,
    pub faction_pressure: f32,
    pub comms_filter: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdaptiveScoreOutput {
    pub input: AdaptiveScoreInput,
    pub stems: Vec<MusicStem>,
    pub transition: MusicTransitionKind,
    pub silence_tension: f32,
    pub diegetic_sources: Vec<DiegeticMusicSource>,
    pub radio_station: RadioStationState,
    pub validation_report: AdaptiveScoreValidationReport,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdaptiveScoreValidationReport {
    pub passed: bool,
    pub issues: Vec<AdaptiveScoreValidationIssue>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdaptiveScoreValidationIssue {
    pub severity: SoundscapeValidationSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DistrictSoundscapeOutput {
    pub state: DistrictAudioState,
    pub layers: Vec<SoundscapeLayer>,
    pub adaptive_score: AdaptiveScoreOutput,
    pub tension_score: f32,
    pub music_ducking: f32,
    pub validation_report: SoundscapeValidationReport,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SoundscapeValidationReport {
    pub passed: bool,
    pub issues: Vec<SoundscapeValidationIssue>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoundscapeValidationIssue {
    pub severity: SoundscapeValidationSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundscapeValidationSeverity {
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioFrameOutput {
    pub mixed_audio_buffer: AudioFrameOutputBuffer,
    pub sample_rate_hz: u32,
    pub listener_location: Vec3,
    pub active_sounds: Vec<ActiveSoundInfo>,
    pub dropped_sounds: Vec<DroppedSound>,
    pub bus_outputs: Vec<AudioBusMix>,
    pub district_soundscape: DistrictSoundscapeOutput,
    pub audio_events_emitted: Vec<WorldEvent>,
    pub peak_intensity: f32,
    pub timing: AudioTimingReport,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DroppedSound {
    pub event_id: WorldEventId,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioBusMix {
    pub bus: AudioBusKind,
    pub sound_count: usize,
    pub peak_intensity: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedSpeechLog {
    pub request_id: u128,
    pub speaker: EntityId,
    pub voice_persona: VoicePersonaId,
    pub text_hash: u64,
    pub cache_status: SpeechCacheStatus,
    pub validation_passed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SpeechCacheKey {
    voice_persona: VoicePersonaId,
    normalized_text: String,
    intent: String,
    language: LanguageId,
    mode: SpeechGenerationMode,
    emotion_bucket: u8,
    loudness_bucket: u8,
}

#[derive(Default)]
pub struct VoiceAudioModule {
    spoken_dialogue_events: BTreeSet<WorldEventId>,
    speech_results: BTreeMap<WorldEventId, SpeechResult>,
    speech_cache: BTreeMap<SpeechCacheKey, SpeechResult>,
    cached_requests: BTreeMap<SpeechCacheKey, SpeechRequest>,
    pending_generation_jobs: BTreeMap<SpeechCacheKey, SpeechRequest>,
    generated_log: Vec<GeneratedSpeechLog>,
}

impl VoiceAudioModule {
    pub fn speech_results(&self) -> impl Iterator<Item = &SpeechResult> {
        self.speech_results.values()
    }

    pub fn generated_log(&self) -> &[GeneratedSpeechLog] {
        &self.generated_log
    }

    pub fn cache_len(&self) -> usize {
        self.speech_cache.len()
    }

    pub fn pending_generation_len(&self) -> usize {
        self.pending_generation_jobs.len()
    }

    pub fn speech_comfort_reports(&self) -> Vec<SpeechComfortReport> {
        self.speech_results
            .values()
            .map(|result| {
                let persona = persona_for_voice(result.voice_persona);
                evaluate_speech_comfort(result, &persona)
            })
            .collect()
    }

    pub fn conversation_comfort_report(&self) -> ConversationComfortReport {
        evaluate_conversation_comfort(self.speech_results.values())
    }

    pub fn process_speech_request(&mut self, request: &SpeechRequest) -> SpeechResult {
        self.complete_generation_jobs(VOICE_GENERATION_JOBS_PER_FRAME);
        let persona = persona_for_voice(request.voice_persona);
        let key = cache_key(request);
        let mut result = if let Some(cached) = self.speech_cache.get(&key) {
            let mut cached = cached.clone();
            cached.request_id = request.request_id;
            cached.speaker = request.speaker;
            cached.cache_status = SpeechCacheStatus::CacheHit;
            cached
        } else if should_defer_speech_generation(request) {
            self.pending_generation_jobs
                .entry(key)
                .or_insert_with(|| request.clone());
            fallback_speech_result(request, &persona)
        } else {
            let result = synthesize_speech_with_persona(request, &persona);
            if result.validation_report.passed {
                self.cached_requests.insert(key.clone(), request.clone());
                self.speech_cache.insert(key, result.clone());
            }
            result
        };

        if !result.validation_report.passed {
            result.cache_status = SpeechCacheStatus::Fallback;
        }
        self.generated_log.push(GeneratedSpeechLog {
            request_id: request.request_id,
            speaker: request.speaker,
            voice_persona: request.voice_persona,
            text_hash: stable_u64(&request.text),
            cache_status: result.cache_status,
            validation_passed: result.validation_report.passed,
        });
        result
    }

    fn complete_generation_jobs(&mut self, max_jobs: usize) {
        let keys = self
            .pending_generation_jobs
            .keys()
            .take(max_jobs)
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            let Some(request) = self.pending_generation_jobs.remove(&key) else {
                continue;
            };
            let persona = persona_for_voice(request.voice_persona);
            let result = synthesize_speech_with_persona(&request, &persona);
            if result.validation_report.passed {
                self.cached_requests.insert(key.clone(), request);
                self.speech_cache.insert(key, result);
            }
        }
    }
}

impl EngineModule for VoiceAudioModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            module_id: 60,
            name: "voice_audio",
            schema: SchemaVersion {
                name: "SpeechResult",
                version: 1,
            },
            default_quality: QualityTier::NormalRuntime,
        }
    }

    fn schema_requirements(&self) -> Vec<ashfall_core::schema::SchemaRequirement> {
        vec![ashfall_core::schema::SchemaRequirement::required(
            60,
            "AiFrameOutput",
            1,
            "voice synthesis consumes dialogue intent produced by AI/story modules",
        )]
    }

    fn tick(&mut self, frame: &FrameContext, out: &mut CommandSink) {
        for event in &frame.recent_events {
            let WorldEventKind::DialogueEmitted {
                speaker,
                text,
                emotion,
                voice_persona,
                ..
            } = &event.kind
            else {
                continue;
            };
            if !self.spoken_dialogue_events.insert(event.event_id) {
                continue;
            }

            let request = apply_voice_quality_to_request(
                speech_request_from_dialogue_event(
                    event.event_id,
                    *speaker,
                    text.clone(),
                    emotion.clone(),
                    *voice_persona,
                    event.location_meters,
                ),
                frame.quality_tier,
            );
            let result = self.process_speech_request(&request);
            self.speech_results.insert(event.event_id, result.clone());

            out.event(WorldEvent {
                event_id: deterministic_child_event_id(
                    frame.sim_time.tick,
                    60,
                    *speaker,
                    event.event_id,
                ),
                tick: frame.sim_time.tick,
                location_meters: event.location_meters,
                actors: vec![*speaker],
                kind: WorldEventKind::VoiceLineSpoken { speaker: *speaker },
                physical_evidence: vec!["audio_clip".to_string(), "viseme_track".to_string()],
                narrative_tags: vec!["voice".to_string()],
            });
            out.event(WorldEvent {
                event_id: deterministic_child_event_id(
                    frame.sim_time.tick,
                    62,
                    *speaker,
                    event.event_id,
                ),
                tick: frame.sim_time.tick,
                location_meters: event.location_meters,
                actors: vec![*speaker],
                kind: WorldEventKind::SpeechSynthesized {
                    speaker: *speaker,
                    audio_clip: result.audio_clip,
                    duration_seconds: result.duration_seconds,
                    phoneme_count: result.phonemes.len(),
                    viseme_count: result.visemes.len(),
                },
                physical_evidence: vec![
                    "audio_clip".to_string(),
                    "phoneme_track".to_string(),
                    "viseme_track".to_string(),
                ],
                narrative_tags: vec!["voice".to_string(), "speech_synthesized".to_string()],
            });

            out.command(WorldCommand::EmitSound(AudioEvent {
                event_id: deterministic_child_event_id(
                    frame.sim_time.tick,
                    61,
                    *speaker,
                    event.event_id,
                ),
                source_entity: Some(*speaker),
                location: event.location_meters,
                event_kind: AudioEventKind::VoiceSpeech,
                material_id: None,
                intensity: request.loudness,
                radius_meters: 14.0,
                occlusion_hint: 0.0,
                tags: vec!["speech".to_string()],
            }));
        }
    }

    fn performance_counters(&self) -> PerformanceCounters {
        PerformanceCounters {
            cpu_milliseconds: 0.16
                + self.pending_generation_jobs.len() as f32 * 0.015
                + self.generated_log.len().min(8) as f32 * 0.002,
            gpu_milliseconds: 0.0,
            memory_bytes: 18 * 1024 * 1024
                + self.speech_cache.len() as u64 * 256 * 1024
                + self.pending_generation_jobs.len() as u64 * 32 * 1024,
        }
    }

    fn save_state(&self) -> Option<ModuleStateRecord> {
        let descriptor = self.descriptor();
        let mut entries = self
            .spoken_dialogue_events
            .iter()
            .map(|event_id| format!("spoken:{event_id}"))
            .collect::<Vec<_>>();
        entries.extend(
            self.cached_requests
                .values()
                .map(|request| format!("cache_request:{}", encode_speech_request(request))),
        );
        entries.extend(
            self.pending_generation_jobs
                .values()
                .map(|request| format!("pending_request:{}", encode_speech_request(request))),
        );
        Some(ModuleStateRecord::new(
            descriptor.module_id,
            descriptor.schema,
            VOICE_MODULE_STATE_VERSION,
            entries,
        ))
    }

    fn load_state(&mut self, state: &ModuleStateRecord) {
        if !(1..=VOICE_MODULE_STATE_VERSION).contains(&state.state_version) {
            return;
        }
        self.spoken_dialogue_events = state
            .entries_with_prefix("spoken:")
            .filter_map(|event_id| event_id.parse::<WorldEventId>().ok())
            .collect();
        self.speech_results.clear();
        self.speech_cache.clear();
        self.cached_requests.clear();
        self.pending_generation_jobs.clear();
        self.generated_log.clear();
        if state.state_version >= 2 {
            for request in state
                .entries_with_prefix("cache_request:")
                .filter_map(parse_speech_request)
            {
                let key = cache_key(&request);
                let persona = persona_for_voice(request.voice_persona);
                let result = synthesize_speech_with_persona(&request, &persona);
                if result.validation_report.passed {
                    self.cached_requests.insert(key.clone(), request);
                    self.speech_cache.insert(key, result);
                }
            }
            for request in state
                .entries_with_prefix("pending_request:")
                .filter_map(parse_speech_request)
            {
                self.pending_generation_jobs
                    .insert(cache_key(&request), request);
            }
        }
    }
}

pub fn district_audio_state_from_events<'a>(
    district: AudioDistrictId,
    events: impl IntoIterator<Item = &'a WorldEvent>,
) -> DistrictAudioState {
    let mut state = DistrictAudioState::baseline(district);
    for event in events {
        let mut relevant = true;
        match &event.kind {
            WorldEventKind::StreetFlooded => {
                state.weather = SoundscapeWeatherState::Flooding;
                state.traffic_density *= 0.55;
                state.crowd_density *= 0.72;
                state.pollution_level = (state.pollution_level + 0.12).clamp(0.0, 1.0);
            }
            WorldEventKind::ToxicGasReleased => {
                state.weather = SoundscapeWeatherState::GasHazard;
                state.crowd_density *= 0.38;
                state.traffic_density *= 0.68;
                state.alertness = (state.alertness + 0.42).clamp(0.0, 1.0);
                state.pollution_level = (state.pollution_level + 0.55).clamp(0.0, 1.0);
            }
            WorldEventKind::PowerTransformerOverheated { .. } => {
                state.power_level = state.power_level.min(0.28);
                state.traffic_density *= 0.82;
                state.alertness = (state.alertness + 0.34).clamp(0.0, 1.0);
            }
            WorldEventKind::SecurityAlertRaised { severity, .. } => {
                state.alertness = state.alertness.max(*severity);
                state.crime_pressure = state.crime_pressure.max((*severity * 0.85).clamp(0.0, 1.0));
            }
            WorldEventKind::SurveillanceIncreased { amount, .. } => {
                state.alertness = (state.alertness + *amount * 0.8).clamp(0.0, 1.0);
            }
            WorldEventKind::FactionReputationChanged { delta, .. } if *delta < 0.0 => {
                state.crime_pressure = (state.crime_pressure + delta.abs() * 0.65).clamp(0.0, 1.0);
                state.alertness = (state.alertness + delta.abs() * 0.35).clamp(0.0, 1.0);
            }
            WorldEventKind::PlayerIdentityExposed | WorldEventKind::NpcWitnessedCrime { .. } => {
                state.alertness = state.alertness.max(0.62);
                state.crime_pressure = state.crime_pressure.max(0.58);
            }
            WorldEventKind::SoundEmitted {
                kind,
                intensity,
                occlusion_hint,
                ..
            } => {
                let loudness = soundscape_perceived_loudness(*intensity, *occlusion_hint);
                match kind {
                    AudioEventKind::Gunshot | AudioEventKind::Explosion => {
                        state.alertness = state.alertness.max((0.72 + loudness * 0.25).min(1.0));
                        state.crime_pressure = state.crime_pressure.max(0.78);
                    }
                    AudioEventKind::GlassShatter | AudioEventKind::GlassImpact => {
                        state.alertness = state.alertness.max((0.38 + loudness * 0.35).min(1.0));
                        state.crime_pressure = state.crime_pressure.max(0.38);
                    }
                    AudioEventKind::WaterSplash => {
                        if state.weather == SoundscapeWeatherState::Clear {
                            state.weather = SoundscapeWeatherState::Rain;
                        }
                    }
                    AudioEventKind::SteamLeak => {
                        state.pollution_level = (state.pollution_level + 0.18).clamp(0.0, 1.0);
                    }
                    AudioEventKind::VoiceSpeech
                    | AudioEventKind::HumanBreath
                    | AudioEventKind::Footstep
                    | AudioEventKind::ClothRustle
                    | AudioEventKind::DoorOpen
                    | AudioEventKind::ElectricBuzz
                    | AudioEventKind::NeonHum
                    | AudioEventKind::ConcreteCrack
                    | AudioEventKind::MetalBend => {
                        relevant = false;
                    }
                }
            }
            _ => {
                if event
                    .narrative_tags
                    .iter()
                    .any(|tag| tag.contains("rain") || tag.contains("weather"))
                {
                    state.weather = SoundscapeWeatherState::Rain;
                } else {
                    relevant = false;
                }
            }
        }
        if relevant {
            state.active_events.push(event.event_id);
        }
    }
    state.power_level = state.power_level.clamp(0.0, 1.0);
    state.crowd_density = state.crowd_density.clamp(0.0, 1.0);
    state.traffic_density = state.traffic_density.clamp(0.0, 1.0);
    state.crime_pressure = state.crime_pressure.clamp(0.0, 1.0);
    state.alertness = state.alertness.clamp(0.0, 1.0);
    state.pollution_level = state.pollution_level.clamp(0.0, 1.0);
    state.active_events.sort_unstable();
    state.active_events.dedup();
    state
}

fn soundscape_perceived_loudness(intensity: f32, occlusion_hint: f32) -> f32 {
    (intensity.clamp(0.0, 1.25) * (1.0 - occlusion_hint.clamp(0.0, 1.0) * 0.65)).clamp(0.0, 1.0)
}

fn district_soundscape_event_is_relevant(event: &WorldEvent) -> bool {
    !district_audio_state_from_events(0, std::iter::once(event))
        .active_events
        .is_empty()
}

pub fn adaptive_score_input_from_district_audio_state(
    state: &DistrictAudioState,
) -> AdaptiveScoreInput {
    let danger_level = state.alertness.max(state.crime_pressure).max(
        if state.weather == SoundscapeWeatherState::GasHazard {
            0.78
        } else {
            0.0
        },
    );
    let gameplay_state = if danger_level >= 0.8 {
        MusicGameplayState::Combat
    } else if state.alertness > 0.55 || state.crime_pressure > 0.5 {
        MusicGameplayState::Stealth
    } else if state.crowd_density > 0.7 && state.alertness < 0.35 {
        MusicGameplayState::Social
    } else {
        MusicGameplayState::Exploration
    };
    AdaptiveScoreInput {
        district: state.district,
        district_identity: soundscape_weather_label(state.weather).to_string(),
        weather: state.weather,
        danger_level,
        faction_tension: state.crime_pressure,
        story_thread_intensity: (state.active_events.len() as f32 * 0.18 + danger_level * 0.55)
            .clamp(0.0, 1.0),
        player_reputation: (1.0 - state.crime_pressure * 0.75).clamp(-1.0, 1.0),
        gameplay_state,
        crowd_density: state.crowd_density,
        power_level: state.power_level,
        radio_signal_strength: (state.power_level * (1.0 - state.pollution_level * 0.35))
            .clamp(0.0, 1.0),
    }
}

pub fn generate_adaptive_score(input: &AdaptiveScoreInput) -> AdaptiveScoreOutput {
    let mut stems = Vec::new();
    let tension = input
        .danger_level
        .max(input.faction_tension)
        .max(input.story_thread_intensity * 0.85)
        .clamp(0.0, 1.0);
    let silence_tension = if tension > 0.82 {
        (tension - 0.72).clamp(0.0, 0.22)
    } else {
        0.0
    };

    push_music_stem(
        input,
        &mut stems,
        MusicStemKind::Texture,
        (0.22 + input.crowd_density * 0.18 + input.power_level * 0.12).clamp(0.0, 0.7),
        0.08 + input.danger_level * 0.08,
        false,
    );
    if tension > 0.32 {
        push_music_stem(
            input,
            &mut stems,
            MusicStemKind::Tension,
            (0.18 + tension * 0.55).clamp(0.0, 0.9),
            0.14 + tension * 0.12,
            false,
        );
    }
    if matches!(
        input.gameplay_state,
        MusicGameplayState::Stealth | MusicGameplayState::Combat
    ) {
        push_music_stem(
            input,
            &mut stems,
            MusicStemKind::Pulse,
            (0.24 + input.danger_level * 0.42).clamp(0.0, 0.85),
            0.18 + input.danger_level * 0.16,
            false,
        );
    }
    if input.gameplay_state == MusicGameplayState::Combat {
        push_music_stem(
            input,
            &mut stems,
            MusicStemKind::Percussion,
            (0.3 + input.danger_level * 0.44).clamp(0.0, 0.92),
            0.28,
            false,
        );
        push_music_stem(
            input,
            &mut stems,
            MusicStemKind::Bass,
            (0.22 + input.faction_tension * 0.45).clamp(0.0, 0.82),
            0.22,
            false,
        );
    } else if input.gameplay_state == MusicGameplayState::Social && input.danger_level < 0.45 {
        push_music_stem(
            input,
            &mut stems,
            MusicStemKind::Melody,
            (0.16 + input.crowd_density * 0.32).clamp(0.0, 0.62),
            0.08,
            false,
        );
    }

    let mut diegetic_sources = Vec::new();
    let radio_station = RadioStationState {
        station_id: 9_000 + input.district,
        label: format!("district {} rainband", input.district),
        enabled: input.radio_signal_strength > 0.18,
        signal_strength: input.radio_signal_strength,
        faction_pressure: input.faction_tension,
        comms_filter: (1.0 - input.radio_signal_strength + input.faction_tension * 0.35)
            .clamp(0.0, 1.0),
    };
    if radio_station.enabled {
        let radio_intensity = (0.12 + input.radio_signal_strength * 0.22).min(0.42).max(
            if input.gameplay_state == MusicGameplayState::Combat {
                0.06
            } else {
                0.0
            },
        );
        push_music_stem(
            input,
            &mut stems,
            MusicStemKind::RadioBed,
            radio_intensity,
            0.2 + input.danger_level * 0.18,
            true,
        );
        diegetic_sources.push(DiegeticMusicSource {
            label: radio_station.label.clone(),
            clip: music_clip_handle(input.district, MusicStemKind::RadioBed),
            intensity: radio_intensity,
            radio_filtered: radio_station.comms_filter > 0.2,
        });
    }

    stems.sort_by(|left, right| {
        right
            .intensity
            .partial_cmp(&left.intensity)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.kind.cmp(&right.kind))
    });
    let transition = if silence_tension > 0.0 {
        MusicTransitionKind::SilenceDrop
    } else if input.gameplay_state == MusicGameplayState::Combat {
        MusicTransitionKind::Stinger
    } else if tension > 0.45 {
        MusicTransitionKind::LayerIn
    } else if input.radio_signal_strength < 0.22 {
        MusicTransitionKind::LayerOut
    } else if input.story_thread_intensity > 0.35 {
        MusicTransitionKind::Crossfade
    } else {
        MusicTransitionKind::Hold
    };
    let validation_report = validate_adaptive_score(input, &stems, &radio_station);

    AdaptiveScoreOutput {
        input: input.clone(),
        stems,
        transition,
        silence_tension,
        diegetic_sources,
        radio_station,
        validation_report,
    }
}

pub fn generate_district_soundscape(state: &DistrictAudioState) -> DistrictSoundscapeOutput {
    let mut layers = Vec::new();
    let traffic_after_power = state.traffic_density * (0.45 + state.power_level * 0.55);
    let crowd_after_alert = state.crowd_density * (1.0 - state.alertness * 0.28).clamp(0.2, 1.0);
    let event_driven = !state.active_events.is_empty();

    push_soundscape_layer(
        state,
        &mut layers,
        soundscape_layer_spec(
            SoundscapeLayerKind::Traffic,
            AudioBusKind::Ambience,
            AudioPriority::Low,
            traffic_after_power * 0.45,
            0.08,
            false,
        ),
    );
    push_soundscape_layer(
        state,
        &mut layers,
        soundscape_layer_spec(
            SoundscapeLayerKind::Crowd,
            AudioBusKind::Ambience,
            AudioPriority::Low,
            crowd_after_alert * 0.5,
            0.1,
            false,
        ),
    );
    push_soundscape_layer(
        state,
        &mut layers,
        soundscape_layer_spec(
            SoundscapeLayerKind::NeonHum,
            AudioBusKind::Mechanical,
            AudioPriority::Low,
            if state.power_level < 0.4 {
                0.08 + state.power_level * 0.12
            } else {
                state.power_level * 0.34
            },
            if state.power_level < 0.4 { 0.35 } else { 0.12 },
            state.power_level < 0.4,
        ),
    );
    push_soundscape_layer(
        state,
        &mut layers,
        soundscape_layer_spec(
            SoundscapeLayerKind::Advertisements,
            AudioBusKind::Ambience,
            AudioPriority::Low,
            state.power_level * (1.0 - state.alertness * 0.25) * 0.24,
            0.16,
            false,
        ),
    );
    push_soundscape_layer(
        state,
        &mut layers,
        soundscape_layer_spec(
            SoundscapeLayerKind::Drones,
            AudioBusKind::Mechanical,
            AudioPriority::Normal,
            (state.alertness * 0.36 + state.traffic_density * 0.12).clamp(0.0, 0.55),
            0.18,
            state.alertness > 0.4,
        ),
    );
    push_soundscape_layer(
        state,
        &mut layers,
        soundscape_layer_spec(
            SoundscapeLayerKind::TransformerHum,
            AudioBusKind::Mechanical,
            AudioPriority::Low,
            state.power_level * 0.28,
            0.08,
            false,
        ),
    );
    push_soundscape_layer(
        state,
        &mut layers,
        soundscape_layer_spec(
            SoundscapeLayerKind::Machinery,
            AudioBusKind::Mechanical,
            AudioPriority::Low,
            (0.2 + state.pollution_level * 0.2) * state.power_level.max(0.35),
            0.12,
            false,
        ),
    );

    if matches!(
        state.weather,
        SoundscapeWeatherState::Rain | SoundscapeWeatherState::Flooding
    ) {
        push_soundscape_layer(
            state,
            &mut layers,
            soundscape_layer_spec(
                SoundscapeLayerKind::Rain,
                AudioBusKind::Ambience,
                AudioPriority::Normal,
                if state.weather == SoundscapeWeatherState::Flooding {
                    0.58
                } else {
                    0.34
                },
                0.18,
                event_driven,
            ),
        );
    }
    if state.weather == SoundscapeWeatherState::Flooding {
        push_soundscape_layer(
            state,
            &mut layers,
            soundscape_layer_spec(
                SoundscapeLayerKind::WaterDrips,
                AudioBusKind::Foley,
                AudioPriority::Normal,
                0.38,
                0.12,
                true,
            ),
        );
    }
    if state.weather == SoundscapeWeatherState::GasHazard || state.pollution_level > 0.65 {
        push_soundscape_layer(
            state,
            &mut layers,
            soundscape_layer_spec(
                SoundscapeLayerKind::Ventilation,
                AudioBusKind::Mechanical,
                AudioPriority::High,
                (0.28 + state.pollution_level * 0.38).clamp(0.0, 0.8),
                0.2,
                true,
            ),
        );
    }
    if state.power_level < 0.45 || state.weather == SoundscapeWeatherState::GasHazard {
        push_soundscape_layer(
            state,
            &mut layers,
            soundscape_layer_spec(
                SoundscapeLayerKind::EmergencyAlarm,
                AudioBusKind::Mechanical,
                AudioPriority::Critical,
                (0.42 + state.alertness * 0.36).clamp(0.0, 0.9),
                0.28,
                true,
            ),
        );
    }
    if state.alertness > 0.45 {
        push_soundscape_layer(
            state,
            &mut layers,
            soundscape_layer_spec(
                SoundscapeLayerKind::PoliceScanner,
                AudioBusKind::Ambience,
                AudioPriority::High,
                (state.alertness * 0.42).clamp(0.0, 0.75),
                0.18,
                true,
            ),
        );
    }

    layers.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.kind.cmp(&right.kind))
    });
    let tension_score = state.alertness.max(state.crime_pressure).max(
        if state.weather == SoundscapeWeatherState::GasHazard {
            0.78
        } else {
            0.0
        },
    );
    let adaptive_score_input = adaptive_score_input_from_district_audio_state(state);
    let adaptive_score = generate_adaptive_score(&adaptive_score_input);
    let music_ducking = (tension_score * 0.35
        + adaptive_score.silence_tension * 0.32
        + layers
            .iter()
            .filter(|layer| layer.priority >= AudioPriority::High)
            .map(|layer| layer.intensity * 0.12)
            .sum::<f32>())
    .clamp(0.0, 0.75);
    let validation_report = validate_soundscape(state, &layers);

    DistrictSoundscapeOutput {
        state: state.clone(),
        layers,
        adaptive_score,
        tension_score,
        music_ducking,
        validation_report,
    }
}

#[derive(Clone, Copy)]
struct SoundscapeLayerSpec {
    kind: SoundscapeLayerKind,
    bus: AudioBusKind,
    priority: AudioPriority,
    intensity: f32,
    ducking: f32,
    event_driven: bool,
}

fn soundscape_layer_spec(
    kind: SoundscapeLayerKind,
    bus: AudioBusKind,
    priority: AudioPriority,
    intensity: f32,
    ducking: f32,
    event_driven: bool,
) -> SoundscapeLayerSpec {
    SoundscapeLayerSpec {
        kind,
        bus,
        priority,
        intensity,
        ducking,
        event_driven,
    }
}

fn push_soundscape_layer(
    state: &DistrictAudioState,
    layers: &mut Vec<SoundscapeLayer>,
    spec: SoundscapeLayerSpec,
) {
    let intensity = spec.intensity.clamp(0.0, 1.0);
    if intensity < 0.035 {
        return;
    }
    layers.push(SoundscapeLayer {
        kind: spec.kind,
        clip: soundscape_clip_handle(state.district, spec.kind),
        bus: spec.bus,
        priority: spec.priority,
        intensity,
        ducking: spec.ducking.clamp(0.0, 1.0),
        event_driven: spec.event_driven,
    });
}

fn push_music_stem(
    input: &AdaptiveScoreInput,
    stems: &mut Vec<MusicStem>,
    kind: MusicStemKind,
    intensity: f32,
    ducking: f32,
    diegetic: bool,
) {
    let intensity = intensity.clamp(0.0, 1.0);
    if intensity < 0.035 {
        return;
    }
    stems.push(MusicStem {
        kind,
        clip: music_clip_handle(input.district, kind),
        intensity,
        ducking: ducking.clamp(0.0, 1.0),
        diegetic,
    });
}

fn validate_adaptive_score(
    input: &AdaptiveScoreInput,
    stems: &[MusicStem],
    radio_station: &RadioStationState,
) -> AdaptiveScoreValidationReport {
    let mut issues = Vec::new();
    if stems.is_empty() {
        issues.push(adaptive_score_issue(
            SoundscapeValidationSeverity::Error,
            "missing_music_stems",
            "adaptive score should always produce at least one stem or intentional silence marker",
        ));
    }
    if input.danger_level > 0.6
        && !stems.iter().any(|stem| {
            matches!(
                stem.kind,
                MusicStemKind::Tension | MusicStemKind::Pulse | MusicStemKind::Percussion
            )
        })
    {
        issues.push(adaptive_score_issue(
            SoundscapeValidationSeverity::Error,
            "danger_without_tension_stem",
            "dangerous scenes need tension, pulse, or percussion score support",
        ));
    }
    if radio_station.enabled
        && !stems
            .iter()
            .any(|stem| stem.kind == MusicStemKind::RadioBed && stem.diegetic)
    {
        issues.push(adaptive_score_issue(
            SoundscapeValidationSeverity::Warning,
            "radio_without_diegetic_bed",
            "enabled radio station should expose a diegetic radio bed stem",
        ));
    }
    AdaptiveScoreValidationReport {
        passed: !issues
            .iter()
            .any(|issue| issue.severity == SoundscapeValidationSeverity::Error),
        issues,
    }
}

fn adaptive_score_issue(
    severity: SoundscapeValidationSeverity,
    code: &str,
    message: &str,
) -> AdaptiveScoreValidationIssue {
    AdaptiveScoreValidationIssue {
        severity,
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn validate_soundscape(
    state: &DistrictAudioState,
    layers: &[SoundscapeLayer],
) -> SoundscapeValidationReport {
    let mut issues = Vec::new();
    if layers.is_empty() {
        issues.push(soundscape_issue(
            SoundscapeValidationSeverity::Error,
            "missing_soundscape_layers",
            "district soundscape should generate at least one ambience layer",
        ));
    }
    if state.power_level < 0.45
        && !layers
            .iter()
            .any(|layer| layer.kind == SoundscapeLayerKind::EmergencyAlarm)
    {
        issues.push(soundscape_issue(
            SoundscapeValidationSeverity::Warning,
            "power_loss_without_alarm",
            "power loss should alter alarms or emergency ambience",
        ));
    }
    if state.weather == SoundscapeWeatherState::GasHazard
        && !layers
            .iter()
            .any(|layer| layer.kind == SoundscapeLayerKind::Ventilation)
    {
        issues.push(soundscape_issue(
            SoundscapeValidationSeverity::Warning,
            "gas_hazard_without_ventilation",
            "gas hazards should make ventilation or warning layers audible",
        ));
    }
    if state.alertness > 0.55
        && !layers.iter().any(|layer| {
            matches!(
                layer.kind,
                SoundscapeLayerKind::PoliceScanner | SoundscapeLayerKind::EmergencyAlarm
            )
        })
    {
        issues.push(soundscape_issue(
            SoundscapeValidationSeverity::Warning,
            "alert_state_without_security_audio",
            "high district alertness should be audible through scanner, siren, or alarm layers",
        ));
    }
    SoundscapeValidationReport {
        passed: !issues
            .iter()
            .any(|issue| issue.severity == SoundscapeValidationSeverity::Error),
        issues,
    }
}

fn soundscape_issue(
    severity: SoundscapeValidationSeverity,
    code: &str,
    message: &str,
) -> SoundscapeValidationIssue {
    SoundscapeValidationIssue {
        severity,
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn soundscape_clip_handle(district: AudioDistrictId, kind: SoundscapeLayerKind) -> AudioClipHandle {
    AudioClipHandle(
        70_000
            + stable_u64(&format!(
                "district:{district}:soundscape:{}",
                soundscape_layer_label(kind)
            )) as u128,
    )
}

fn soundscape_layer_label(kind: SoundscapeLayerKind) -> &'static str {
    match kind {
        SoundscapeLayerKind::Traffic => "traffic",
        SoundscapeLayerKind::Rain => "rain",
        SoundscapeLayerKind::NeonHum => "neon_hum",
        SoundscapeLayerKind::Drones => "drones",
        SoundscapeLayerKind::Crowd => "crowd",
        SoundscapeLayerKind::Advertisements => "advertisements",
        SoundscapeLayerKind::TransformerHum => "transformer_hum",
        SoundscapeLayerKind::Machinery => "machinery",
        SoundscapeLayerKind::Ventilation => "ventilation",
        SoundscapeLayerKind::WaterDrips => "water_drips",
        SoundscapeLayerKind::PoliceScanner => "police_scanner",
        SoundscapeLayerKind::EmergencyAlarm => "emergency_alarm",
    }
}

fn music_clip_handle(district: AudioDistrictId, kind: MusicStemKind) -> AudioClipHandle {
    AudioClipHandle(
        80_000
            + stable_u64(&format!(
                "district:{district}:music:{}",
                music_stem_label(kind)
            )) as u128,
    )
}

fn music_stem_label(kind: MusicStemKind) -> &'static str {
    match kind {
        MusicStemKind::Pulse => "pulse",
        MusicStemKind::Bass => "bass",
        MusicStemKind::Texture => "texture",
        MusicStemKind::Tension => "tension",
        MusicStemKind::Melody => "melody",
        MusicStemKind::Percussion => "percussion",
        MusicStemKind::RadioBed => "radio_bed",
    }
}

fn soundscape_weather_label(weather: SoundscapeWeatherState) -> &'static str {
    match weather {
        SoundscapeWeatherState::Clear => "clear",
        SoundscapeWeatherState::Rain => "rain",
        SoundscapeWeatherState::Flooding => "flooding",
        SoundscapeWeatherState::GasHazard => "gas_hazard",
    }
}

#[derive(Default)]
pub struct AudioMixerModule {
    mixed_sound_events: BTreeSet<WorldEventId>,
    mixed_soundscape_events: BTreeSet<WorldEventId>,
    last_output: Option<AudioFrameOutput>,
    max_active_sounds: usize,
}

impl AudioMixerModule {
    pub fn last_output(&self) -> Option<&AudioFrameOutput> {
        self.last_output.as_ref()
    }
}

impl EngineModule for AudioMixerModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            module_id: 65,
            name: "audio_mixer",
            schema: SchemaVersion {
                name: "AudioFrameOutput",
                version: 1,
            },
            default_quality: QualityTier::NormalRuntime,
        }
    }

    fn schema_requirements(&self) -> Vec<ashfall_core::schema::SchemaRequirement> {
        vec![
            ashfall_core::schema::SchemaRequirement::required(
                65,
                "SpeechResult",
                1,
                "audio mixer consumes synthesized speech clips and timing metadata",
            ),
            ashfall_core::schema::SchemaRequirement::required(
                65,
                "PhysicsOutput",
                1,
                "audio mixer consumes physics-origin impact, splash, and steam sounds",
            ),
        ]
    }

    fn tick(&mut self, frame: &FrameContext, out: &mut CommandSink) {
        let listener = listener_location(frame);
        let quality_max_active = match frame.quality_tier {
            QualityTier::Disabled => 0,
            QualityTier::BackgroundApproximation => 8,
            QualityTier::NormalRuntime => 32,
            QualityTier::HeroHighFidelityRuntime => 64,
            QualityTier::ReferenceOfflineValidation => usize::MAX,
        };
        let max_active = match self.max_active_sounds {
            0 => quality_max_active,
            configured => configured.min(quality_max_active),
        };
        let mut active_sounds = Vec::new();
        let mut dropped_sounds = Vec::new();
        let mut audio_events_emitted = Vec::new();
        let soundscape_events = frame
            .recent_events
            .iter()
            .filter(|event| !self.mixed_soundscape_events.contains(&event.event_id))
            .filter(|event| district_soundscape_event_is_relevant(event))
            .collect::<Vec<_>>();
        let soundscape_state =
            district_audio_state_from_events(0, soundscape_events.iter().copied());
        let district_soundscape = generate_district_soundscape(&soundscape_state);

        for event in &frame.recent_events {
            let WorldEventKind::SoundEmitted {
                kind,
                source_entity,
                material_id,
                intensity,
                radius_meters,
                occlusion_hint,
            } = &event.kind
            else {
                continue;
            };
            if !self.mixed_sound_events.insert(event.event_id) {
                continue;
            }

            let acoustic_scene =
                evaluate_acoustic_scene(&frame.snapshot, event.location_meters, listener);
            let material_descriptor = material_id.and_then(|id| frame.snapshot.materials.find(id));
            let material_state = source_entity
                .and_then(|entity| frame.snapshot.material_states.find(entity).copied());
            let combined_occlusion =
                combine_occlusion(*occlusion_hint, acoustic_scene.world_occlusion);
            let sound = spatialize_sound_with_acoustic_context(
                SoundSpatializationInput {
                    event_id: event.event_id,
                    kind: *kind,
                    source_entity: *source_entity,
                    material_id: *material_id,
                    location: event.location_meters,
                    listener,
                    intensity: *intensity,
                    radius_meters: *radius_meters,
                    occlusion_hint: combined_occlusion,
                },
                AcousticSoundContext {
                    environment: acoustic_scene.environment,
                    zone_entity: acoustic_scene.zone_entity,
                    occlusion_sources: acoustic_scene.occlusion_sources,
                    material_descriptor,
                    material_state,
                },
            );
            active_sounds.push(sound);
            audio_events_emitted.push(event.clone());
        }

        if active_sounds.is_empty() && district_soundscape.state.active_events.is_empty() {
            return;
        }

        active_sounds.sort_by(|left, right| {
            right.priority.cmp(&left.priority).then_with(|| {
                right
                    .effective_intensity
                    .partial_cmp(&left.effective_intensity)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        });
        if active_sounds.len() > max_active {
            for dropped in active_sounds.drain(max_active..) {
                dropped_sounds.push(DroppedSound {
                    event_id: dropped.event_id,
                    reason: "active sound budget exceeded".to_string(),
                });
            }
        }

        let material_aware_sound_count = active_sounds
            .iter()
            .filter(|sound| sound.material_id.is_some())
            .count();
        let peak_sound_intensity = active_sounds
            .iter()
            .map(|sound| sound.effective_intensity)
            .fold(0.0, f32::max);
        let peak_soundscape_intensity = district_soundscape
            .layers
            .iter()
            .map(|layer| layer.intensity)
            .fold(0.0, f32::max);
        let peak_score_intensity = district_soundscape
            .adaptive_score
            .stems
            .iter()
            .map(|stem| stem.intensity)
            .fold(0.0, f32::max);
        let peak_intensity = peak_sound_intensity
            .max(peak_soundscape_intensity)
            .max(peak_score_intensity);
        let mixed_audio_buffer =
            AudioBufferHandle(65_000 + frame.frame_id as u128 + active_sounds.len() as u128);
        let score_stem_count = district_soundscape.adaptive_score.stems.len();
        let estimated_gpu_milliseconds = estimate_audio_gpu_milliseconds(
            &active_sounds,
            district_soundscape
                .layers
                .len()
                .saturating_add(score_stem_count),
        );
        let soundscape_memory_bytes = (district_soundscape.layers.len() as u64 * 48 * 1024)
            .saturating_add(score_stem_count as u64 * 64 * 1024)
            .saturating_add(
                district_soundscape.adaptive_score.diegetic_sources.len() as u64 * 16 * 1024,
            );

        let output = AudioFrameOutput {
            mixed_audio_buffer,
            sample_rate_hz: 48_000,
            listener_location: listener,
            bus_outputs: mix_buses_with_soundscape(
                &active_sounds,
                &district_soundscape.layers,
                &district_soundscape.adaptive_score,
            ),
            active_sounds,
            dropped_sounds,
            district_soundscape,
            audio_events_emitted,
            peak_intensity,
            timing: PerformanceCounters {
                cpu_milliseconds: 0.18,
                gpu_milliseconds: estimated_gpu_milliseconds,
                memory_bytes: 512 * 1024 + soundscape_memory_bytes,
            },
        };

        out.event(WorldEvent {
            event_id: deterministic_event_id(frame.sim_time.tick, 65, frame.frame_id),
            tick: frame.sim_time.tick,
            location_meters: listener,
            actors: output
                .active_sounds
                .iter()
                .filter_map(|sound| sound.source_entity)
                .collect(),
            kind: WorldEventKind::AudioFrameMixed {
                active_sound_count: output.active_sounds.len(),
                material_aware_sound_count,
                peak_intensity,
                mixed_audio_buffer,
            },
            physical_evidence: vec!["mixed_audio_buffer".to_string()],
            narrative_tags: vec!["audio".to_string(), "mix".to_string()],
        });
        self.mixed_soundscape_events.extend(
            output
                .district_soundscape
                .state
                .active_events
                .iter()
                .copied(),
        );
        self.last_output = Some(output);
    }

    fn schedule_gpu(&mut self, graph: &mut GpuGraphBuilder) {
        let Some(output) = self.last_output.as_ref() else {
            return;
        };
        if output.active_sounds.is_empty() && output.district_soundscape.layers.is_empty() {
            return;
        }

        let active_sound_count = u64::try_from(output.active_sounds.len()).unwrap_or(u64::MAX);
        let bus_count = u64::try_from(output.bus_outputs.len().max(1)).unwrap_or(u64::MAX);
        let soundscape_layer_count =
            u64::try_from(output.district_soundscape.layers.len()).unwrap_or(u64::MAX);
        let has_soundscape = soundscape_layer_count > 0;
        let occlusion_report_count = output
            .active_sounds
            .iter()
            .map(|sound| sound.occlusion_sources.len())
            .sum::<usize>();
        let occlusion_report_count = u64::try_from(occlusion_report_count).unwrap_or(u64::MAX);
        let has_occlusion = output
            .active_sounds
            .iter()
            .any(|sound| sound.spatial.occlusion > 0.01 || !sound.occlusion_sources.is_empty());
        let has_reverb = output.active_sounds.iter().any(|sound| {
            sound.spatial.reverb_send > 0.01 || sound.spatial.reverb_tail_seconds > 0.25
        });

        let active_sound_list = audio_gpu_resource(
            graph,
            "audio active sound list",
            GpuResourceKind::Buffer,
            audio_resource_bytes(active_sound_count, 160),
            GpuResourceLifetime::Imported,
        );
        let spatial_params = audio_gpu_resource(
            graph,
            "audio spatial parameter buffer",
            GpuResourceKind::Buffer,
            audio_resource_bytes(active_sound_count, 96),
            GpuResourceLifetime::Transient,
        );
        let bus_accumulation = audio_gpu_resource(
            graph,
            "audio bus accumulation buffer",
            GpuResourceKind::Buffer,
            audio_resource_bytes(bus_count, 256),
            GpuResourceLifetime::Transient,
        );
        let final_mix_output = audio_gpu_resource(
            graph,
            "audio final mix output buffer",
            GpuResourceKind::Buffer,
            audio_output_buffer_bytes(output),
            GpuResourceLifetime::Persistent,
        );

        let spatial_pipeline = audio_compute_pipeline_with_permutation(
            graph,
            "audio_spatialization",
            "audio/spatialization.comp",
            QualityTier::NormalRuntime,
            audio_shader_permutation(["SPATIAL_AUDIO", "MATERIAL_AWARE_GAIN"]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "audio_spatialization",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(spatial_pipeline),
                QualityTier::NormalRuntime,
            )
            .reads([active_sound_list])
            .writes([spatial_params]),
        );

        let filtered_spatial_params = if has_occlusion {
            let occlusion_scene = audio_gpu_resource(
                graph,
                "audio occlusion scene buffer",
                GpuResourceKind::Buffer,
                audio_resource_bytes(occlusion_report_count.max(1), 96),
                GpuResourceLifetime::Imported,
            );
            let filtered_spatial_params = audio_gpu_resource(
                graph,
                "audio occlusion filtered spatial buffer",
                GpuResourceKind::Buffer,
                audio_resource_bytes(active_sound_count, 96),
                GpuResourceLifetime::Transient,
            );
            let occlusion_pipeline = audio_compute_pipeline_with_permutation(
                graph,
                "audio_occlusion_filter",
                "audio/occlusion_filter.comp",
                QualityTier::NormalRuntime,
                audio_shader_permutation(["OCCLUSION_RAYS", "MATERIAL_OCCLUDERS"]),
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "audio_occlusion_filter",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(occlusion_pipeline),
                    QualityTier::NormalRuntime,
                )
                .reads([active_sound_list, spatial_params, occlusion_scene])
                .writes([filtered_spatial_params]),
            );
            filtered_spatial_params
        } else {
            spatial_params
        };

        let reverb_tail_buffer = if has_reverb {
            let reverb_tail_buffer = audio_gpu_resource(
                graph,
                "audio reverb tail buffer",
                GpuResourceKind::Buffer,
                audio_output_buffer_bytes(output) / 2,
                GpuResourceLifetime::Transient,
            );
            let reverb_pipeline = audio_compute_pipeline_with_permutation(
                graph,
                "audio_reverb_convolution",
                "audio/reverb_convolution.comp",
                QualityTier::NormalRuntime,
                audio_shader_permutation(["REVERB_ZONES", "CONVOLUTION_REVERB"]),
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "audio_reverb_convolution",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(reverb_pipeline),
                    QualityTier::NormalRuntime,
                )
                .reads([active_sound_list, filtered_spatial_params])
                .writes([reverb_tail_buffer]),
            );
            Some(reverb_tail_buffer)
        } else {
            None
        };

        let soundscape_mix = if has_soundscape {
            let soundscape_layers = audio_gpu_resource(
                graph,
                "audio district soundscape layer buffer",
                GpuResourceKind::Buffer,
                audio_resource_bytes(soundscape_layer_count, 96),
                GpuResourceLifetime::Imported,
            );
            let soundscape_mix = audio_gpu_resource(
                graph,
                "audio soundscape mix buffer",
                GpuResourceKind::Buffer,
                audio_resource_bytes(bus_count, 128),
                GpuResourceLifetime::Transient,
            );
            let soundscape_pipeline = audio_compute_pipeline_with_permutation(
                graph,
                "audio_soundscape_layers",
                "audio/soundscape_layers.comp",
                QualityTier::BackgroundApproximation,
                audio_shader_permutation(["DISTRICT_SOUNDSCAPE", "STATE_DRIVEN_AMBIENCE"]),
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "audio_soundscape_layers",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(soundscape_pipeline),
                    QualityTier::BackgroundApproximation,
                )
                .reads([soundscape_layers])
                .writes([soundscape_mix]),
            );
            Some(soundscape_mix)
        } else {
            None
        };

        let bus_pipeline = audio_compute_pipeline_with_permutation(
            graph,
            "audio_bus_mixdown",
            "audio/bus_mixdown.comp",
            QualityTier::NormalRuntime,
            audio_shader_permutation(["BUS_MIXDOWN", "PRIORITY_LIMITER"]),
        );
        let mut bus_reads = vec![active_sound_list, filtered_spatial_params];
        if let Some(reverb_tail_buffer) = reverb_tail_buffer {
            bus_reads.push(reverb_tail_buffer);
        }
        if let Some(soundscape_mix) = soundscape_mix {
            bus_reads.push(soundscape_mix);
        }
        graph.add_pass(
            GpuPassDesc::new(
                "audio_bus_mixdown",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(bus_pipeline),
                QualityTier::NormalRuntime,
            )
            .reads(bus_reads)
            .writes([bus_accumulation]),
        );

        let final_pipeline = audio_compute_pipeline_with_permutation(
            graph,
            "audio_final_mix",
            "audio/final_mix.comp",
            QualityTier::BackgroundApproximation,
            audio_shader_permutation(["FINAL_MIX", "PEAK_LIMITER"]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "audio_final_mix",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(final_pipeline),
                QualityTier::BackgroundApproximation,
            )
            .reads([bus_accumulation])
            .writes([final_mix_output]),
        );
    }

    fn performance_counters(&self) -> PerformanceCounters {
        self.last_output
            .as_ref()
            .map(|output| PerformanceCounters {
                cpu_milliseconds: output.timing.cpu_milliseconds,
                gpu_milliseconds: output.timing.gpu_milliseconds,
                memory_bytes: 22 * 1024 * 1024
                    + output.active_sounds.len() as u64 * 128 * 1024
                    + output.district_soundscape.layers.len() as u64 * 96 * 1024
                    + output.bus_outputs.len() as u64 * 64 * 1024,
            })
            .unwrap_or(PerformanceCounters {
                cpu_milliseconds: 0.18,
                gpu_milliseconds: 0.0,
                memory_bytes: 22 * 1024 * 1024,
            })
    }

    fn save_state(&self) -> Option<ModuleStateRecord> {
        let descriptor = self.descriptor();
        let mut entries = vec![format!("max_active:{}", self.max_active_sounds)];
        entries.extend(
            self.mixed_sound_events
                .iter()
                .map(|event_id| format!("mixed:{event_id}")),
        );
        entries.extend(
            self.mixed_soundscape_events
                .iter()
                .map(|event_id| format!("soundscape:{event_id}")),
        );
        Some(ModuleStateRecord::new(
            descriptor.module_id,
            descriptor.schema,
            AUDIO_MIXER_MODULE_STATE_VERSION,
            entries,
        ))
    }

    fn load_state(&mut self, state: &ModuleStateRecord) {
        if state.state_version != AUDIO_MIXER_MODULE_STATE_VERSION {
            return;
        }
        if let Some(max_active) = state
            .entries_with_prefix("max_active:")
            .next()
            .and_then(|value| value.parse::<usize>().ok())
        {
            self.max_active_sounds = max_active;
        }
        self.mixed_sound_events = state
            .entries_with_prefix("mixed:")
            .filter_map(|event_id| event_id.parse::<WorldEventId>().ok())
            .collect();
        self.mixed_soundscape_events = state
            .entries_with_prefix("soundscape:")
            .filter_map(|event_id| event_id.parse::<WorldEventId>().ok())
            .collect();
        self.last_output = None;
    }
}

pub fn speech_request_from_dialogue_event(
    event_id: WorldEventId,
    speaker: EntityId,
    text: String,
    emotion: EmotionState,
    voice_persona: Option<VoicePersonaId>,
    location: Vec3,
) -> SpeechRequest {
    SpeechRequest {
        request_id: event_id,
        speaker,
        voice_persona: voice_persona.unwrap_or(speaker),
        text,
        emotion,
        intent: "dialogue".to_string(),
        urgency: 0.8,
        loudness: 0.65,
        language: 1,
        interruption_policy: "finish_current_phrase".to_string(),
        location: Some(location),
        priority: SpeechPriority::Important,
        mode: SpeechGenerationMode::CachedGenerated,
        max_latency_ms: 50,
        safety_context: VoiceSafetyContext::default(),
    }
}

fn apply_voice_quality_to_request(
    mut request: SpeechRequest,
    quality_tier: QualityTier,
) -> SpeechRequest {
    match quality_tier {
        QualityTier::Disabled | QualityTier::BackgroundApproximation => {
            request.priority = SpeechPriority::Background;
            request.mode = SpeechGenerationMode::CrowdChatter;
            request.max_latency_ms = request.max_latency_ms.min(25);
            request.urgency = (request.urgency * 0.75).clamp(0.0, 1.0);
            request.loudness = (request.loudness * 0.85).clamp(0.25, 1.0);
        }
        QualityTier::NormalRuntime => {}
        QualityTier::HeroHighFidelityRuntime => {
            request.priority = request.priority.max(SpeechPriority::Important);
            request.mode = SpeechGenerationMode::OnDemandGenerated;
            request.max_latency_ms = request.max_latency_ms.max(120);
        }
        QualityTier::ReferenceOfflineValidation => {
            request.priority = SpeechPriority::Critical;
            request.mode = SpeechGenerationMode::OnDemandGenerated;
            request.max_latency_ms = request.max_latency_ms.max(500);
        }
    }
    request
}

#[derive(Clone, Copy, Debug)]
struct SpeechFidelityProfile {
    phonemes_per_word: usize,
    viseme_chunk_size: usize,
    prosody_seconds_per_frame: f32,
    duration_scale: f32,
    phoneme_confidence: f32,
    viseme_weight_scale: f32,
    cpu_milliseconds: f32,
    memory_bytes: u64,
}

fn speech_fidelity_profile(request: &SpeechRequest) -> SpeechFidelityProfile {
    match request.mode {
        SpeechGenerationMode::PreAuthored => SpeechFidelityProfile {
            phonemes_per_word: 3,
            viseme_chunk_size: 2,
            prosody_seconds_per_frame: 0.25,
            duration_scale: 1.0,
            phoneme_confidence: 0.96,
            viseme_weight_scale: 1.0,
            cpu_milliseconds: 0.03,
            memory_bytes: 64 * 1024,
        },
        SpeechGenerationMode::CachedGenerated => SpeechFidelityProfile {
            phonemes_per_word: 3,
            viseme_chunk_size: 2,
            prosody_seconds_per_frame: 0.25,
            duration_scale: 1.0,
            phoneme_confidence: 0.92,
            viseme_weight_scale: 1.0,
            cpu_milliseconds: 0.12,
            memory_bytes: 256 * 1024,
        },
        SpeechGenerationMode::OnDemandGenerated => SpeechFidelityProfile {
            phonemes_per_word: 4,
            viseme_chunk_size: 1,
            prosody_seconds_per_frame: 0.18,
            duration_scale: 1.05,
            phoneme_confidence: 0.96,
            viseme_weight_scale: 1.08,
            cpu_milliseconds: 0.22,
            memory_bytes: 384 * 1024,
        },
        SpeechGenerationMode::CrowdChatter => SpeechFidelityProfile {
            phonemes_per_word: 1,
            viseme_chunk_size: 3,
            prosody_seconds_per_frame: 0.5,
            duration_scale: 0.9,
            phoneme_confidence: 0.72,
            viseme_weight_scale: 0.65,
            cpu_milliseconds: 0.04,
            memory_bytes: 64 * 1024,
        },
        SpeechGenerationMode::RadioComms => SpeechFidelityProfile {
            phonemes_per_word: 2,
            viseme_chunk_size: 4,
            prosody_seconds_per_frame: 0.4,
            duration_scale: 0.95,
            phoneme_confidence: 0.82,
            viseme_weight_scale: 0.75,
            cpu_milliseconds: 0.06,
            memory_bytes: 96 * 1024,
        },
    }
}

pub fn synthesize_speech(request: &SpeechRequest) -> SpeechResult {
    let persona = persona_for_voice(request.voice_persona);
    synthesize_speech_with_persona(request, &persona)
}

pub fn synthesize_speech_with_persona(
    request: &SpeechRequest,
    persona: &VoicePersona,
) -> SpeechResult {
    let validation_report = validate_speech_request(request, persona);
    let text = if validation_report.passed {
        request.text.as_str()
    } else {
        "I cannot use that voice right now."
    };
    let words = text
        .split_whitespace()
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let fidelity = speech_fidelity_profile(request);
    let phoneme_count = (words.len() * fidelity.phonemes_per_word).max(1);
    let emotional_speed = (1.0 + request.emotion.urgency * 0.25 + request.emotion.fear * 0.1)
        / persona.speaking_rate.max(0.3);
    let duration_seconds = (words.len() as f32 * 0.22 * emotional_speed * fidelity.duration_scale
        / request.loudness.max(0.25))
    .max(0.35);
    let phoneme_step = duration_seconds / phoneme_count as f32;

    let phonemes = (0..phoneme_count)
        .map(|index| {
            let start_seconds = index as f32 * phoneme_step;
            let phoneme = match (index + persona.id as usize) % 7 {
                0 => "AI",
                1 => "S",
                2 => "TH",
                3 => "EH",
                4 => "M",
                5 => "T",
                _ => "AH",
            };
            PhonemeTiming {
                phoneme: phoneme.to_string(),
                start_seconds,
                end_seconds: start_seconds + phoneme_step,
                confidence: if validation_report.passed {
                    fidelity.phoneme_confidence
                } else {
                    0.55
                },
            }
        })
        .collect::<Vec<_>>();

    let visemes = phonemes
        .chunks(fidelity.viseme_chunk_size)
        .enumerate()
        .map(|(index, chunk)| VisemeTiming {
            viseme: match index % 5 {
                0 => "open",
                1 => "narrow",
                2 => "teeth",
                3 => "tongue",
                _ => "closed",
            }
            .to_string(),
            start_seconds: chunk
                .first()
                .map(|timing| timing.start_seconds)
                .unwrap_or(0.0),
            end_seconds: chunk
                .last()
                .map(|timing| timing.end_seconds)
                .unwrap_or(duration_seconds),
            weight: ((0.72 + request.emotion.urgency * 0.18) * fidelity.viseme_weight_scale)
                .clamp(0.25, 1.0),
        })
        .collect();

    SpeechResult {
        request_id: request.request_id,
        speaker: request.speaker,
        voice_persona: request.voice_persona,
        audio_clip: AudioClipHandle(stable_clip_id(&cache_key(request))),
        phonemes,
        visemes,
        emotion_curve: vec![
            (0.0, request.emotion.clone()),
            (duration_seconds, request.emotion.clone()),
        ],
        prosody: build_prosody(
            request,
            persona,
            duration_seconds,
            fidelity.prosody_seconds_per_frame,
        ),
        duration_seconds,
        cache_status: if validation_report.passed {
            SpeechCacheStatus::Generated
        } else {
            SpeechCacheStatus::Fallback
        },
        validation_report,
        generation_cost: PerformanceCounters {
            cpu_milliseconds: fidelity.cpu_milliseconds,
            gpu_milliseconds: 0.0,
            memory_bytes: fidelity.memory_bytes,
        },
    }
}

pub fn speech_result_from_performance_capture(
    request: &SpeechRequest,
    persona: &VoicePersona,
    clip: &PerformanceCaptureClip,
) -> SpeechResult {
    let speech_report = validate_speech_request(request, persona);
    let capture_report = validate_performance_capture_clip(request, clip);
    let validation_report = speech_validation_from_capture_reports(&speech_report, &capture_report);

    if !validation_report.passed {
        let mut fallback = fallback_speech_result(request, persona);
        fallback.validation_report = validation_report;
        return fallback;
    }

    SpeechResult {
        request_id: request.request_id,
        speaker: request.speaker,
        voice_persona: request.voice_persona,
        audio_clip: clip.audio_track,
        phonemes: clip.phonemes.clone(),
        visemes: clip.visemes.clone(),
        emotion_curve: clip.emotion_curve.clone(),
        prosody: clip.prosody.clone(),
        duration_seconds: clip.duration_seconds,
        cache_status: SpeechCacheStatus::CapturedPerformance,
        validation_report,
        generation_cost: PerformanceCounters {
            cpu_milliseconds: 0.04,
            gpu_milliseconds: 0.0,
            memory_bytes: performance_capture_memory_bytes(clip),
        },
    }
}

pub fn generated_performance_capture_clip_from_speech_result(
    request: &SpeechRequest,
    result: &SpeechResult,
) -> PerformanceCaptureClip {
    let duration_seconds = if result.duration_seconds.is_finite() && result.duration_seconds > 0.0 {
        result.duration_seconds
    } else {
        0.35
    };
    let mut face_curves = Vec::new();
    for (index, viseme) in result.visemes.iter().enumerate() {
        let start = viseme.start_seconds.clamp(0.0, duration_seconds);
        let end = viseme.end_seconds.clamp(start, duration_seconds);
        let midpoint = ((start + end) * 0.5).clamp(0.0, duration_seconds);
        let weight = viseme.weight.clamp(0.0, 1.0);
        face_curves.push(PerformanceCurveSample {
            channel: PerformanceCurveChannel::JawOpen,
            time_seconds: start,
            value: (0.18 + weight * 0.62).clamp(0.0, 1.0),
            confidence: 0.88,
        });
        face_curves.push(PerformanceCurveSample {
            channel: match index % 3 {
                0 => PerformanceCurveChannel::LipRound,
                1 => PerformanceCurveChannel::LipSpread,
                _ => PerformanceCurveChannel::BrowRaise,
            },
            time_seconds: midpoint,
            value: weight,
            confidence: 0.84,
        });
    }
    if face_curves.is_empty() {
        face_curves.push(PerformanceCurveSample {
            channel: PerformanceCurveChannel::JawOpen,
            time_seconds: 0.0,
            value: 0.24,
            confidence: 0.8,
        });
    }

    let mut body_curves = result
        .prosody
        .frames
        .iter()
        .enumerate()
        .map(|(index, frame)| PerformanceCurveSample {
            channel: if index % 2 == 0 {
                PerformanceCurveChannel::ChestBreath
            } else {
                PerformanceCurveChannel::HeadPitch
            },
            time_seconds: frame.time_seconds.clamp(0.0, duration_seconds),
            value: (frame.energy * 0.35 + frame.breathiness * 0.45).clamp(-1.0, 1.0),
            confidence: 0.72,
        })
        .collect::<Vec<_>>();
    if body_curves.is_empty() {
        body_curves.push(PerformanceCurveSample {
            channel: PerformanceCurveChannel::ChestBreath,
            time_seconds: 0.0,
            value: 0.16,
            confidence: 0.7,
        });
    }

    PerformanceCaptureClip {
        clip_id: stable_clip_id(&cache_key(request)).saturating_add(0xC0DE),
        speaker: request.speaker,
        voice_persona: request.voice_persona,
        source: PerformanceCaptureSource::AiGeneratedAnimationHints,
        provenance: PerformanceCaptureProvenance {
            actor_or_source_label: "ashfall_generated_performance_system".to_string(),
            session_label: format!("speech_request_{}", request.request_id),
            rights_verified: true,
            consent_verified: true,
            retargeting_allowed: true,
            notes: "deterministic generated capture derived from speech timing".to_string(),
        },
        timecode: PerformanceCaptureTimecode {
            start_seconds: 0.0,
            frame_rate: 60.0,
        },
        audio_track: result.audio_clip,
        duration_seconds,
        phonemes: result.phonemes.clone(),
        visemes: result.visemes.clone(),
        emotion_curve: result.emotion_curve.clone(),
        prosody: result.prosody.clone(),
        face_curves,
        body_curves,
        calibration: PerformanceCaptureCalibration {
            audio_sample_rate_hz: 48_000,
            video_frame_rate: 60.0,
            face_solve_confidence: 0.86,
            body_solve_confidence: 0.72,
            audio_video_offset_seconds: 0.018,
        },
        target_rig: PerformanceRigMapping {
            target_rig_label: "ashfall_runtime_human_v1".to_string(),
            viseme_set_label: "ashfall_viseme_v1".to_string(),
            mapped_face_channels: vec![
                PerformanceCurveChannel::JawOpen,
                PerformanceCurveChannel::LipRound,
                PerformanceCurveChannel::LipSpread,
                PerformanceCurveChannel::BrowRaise,
                PerformanceCurveChannel::EyeSquint,
            ],
            mapped_body_channels: vec![
                PerformanceCurveChannel::ChestBreath,
                PerformanceCurveChannel::HeadPitch,
                PerformanceCurveChannel::BodyLean,
            ],
        },
        validation_notes: vec![
            "generated from speech phoneme and viseme timing".to_string(),
            "approved synthetic capture path with provenance metadata".to_string(),
        ],
    }
}

pub fn validate_performance_capture_clip(
    request: &SpeechRequest,
    clip: &PerformanceCaptureClip,
) -> PerformanceCaptureValidationReport {
    let mut issues = Vec::new();

    if clip.speaker != request.speaker {
        issues.push(capture_issue(
            SpeechValidationSeverity::Error,
            "speaker_mismatch",
            "performance capture speaker must match the speech request speaker",
        ));
    }
    if clip.voice_persona != request.voice_persona {
        issues.push(capture_issue(
            SpeechValidationSeverity::Error,
            "voice_persona_mismatch",
            "performance capture voice persona must match the speech request persona",
        ));
    }
    if request.safety_context.licensed_persona_required
        && (!clip.provenance.rights_verified || !clip.provenance.consent_verified)
    {
        issues.push(capture_issue(
            SpeechValidationSeverity::Error,
            "capture_provenance_unverified",
            "performance capture requires verified rights and consent",
        ));
    }
    if !clip.provenance.retargeting_allowed {
        issues.push(capture_issue(
            SpeechValidationSeverity::Error,
            "retargeting_disallowed",
            "performance capture must allow retargeting to the requested runtime rig",
        ));
    }
    if matches!(clip.source, PerformanceCaptureSource::Placeholder)
        && request.priority >= SpeechPriority::Important
    {
        issues.push(capture_issue(
            SpeechValidationSeverity::Warning,
            "placeholder_capture_for_important_line",
            "important dialogue should use actor, manual, or approved generated capture",
        ));
    }
    if !clip.duration_seconds.is_finite() || clip.duration_seconds <= 0.0 {
        issues.push(capture_issue(
            SpeechValidationSeverity::Error,
            "invalid_capture_duration",
            "performance capture duration must be positive and finite",
        ));
    }
    if !clip.timecode.frame_rate.is_finite() || clip.timecode.frame_rate <= 0.0 {
        issues.push(capture_issue(
            SpeechValidationSeverity::Error,
            "invalid_timecode",
            "performance capture timecode needs a positive frame rate",
        ));
    }
    if clip.calibration.audio_sample_rate_hz == 0 {
        issues.push(capture_issue(
            SpeechValidationSeverity::Error,
            "missing_audio_sample_rate",
            "performance capture calibration must include audio sample rate",
        ));
    }
    if !clip.calibration.video_frame_rate.is_finite() || clip.calibration.video_frame_rate <= 0.0 {
        issues.push(capture_issue(
            SpeechValidationSeverity::Warning,
            "missing_video_frame_rate",
            "face/body capture calibration should include video frame rate",
        ));
    }
    let sync_error = clip.calibration.audio_video_offset_seconds.abs();
    if sync_error > 0.16 {
        issues.push(capture_issue(
            SpeechValidationSeverity::Error,
            "capture_sync_error",
            "audio and video capture are too far apart for reliable lip sync",
        ));
    } else if sync_error > 0.075 {
        issues.push(capture_issue(
            SpeechValidationSeverity::Warning,
            "capture_sync_warning",
            "audio/video capture offset is visible enough to need review",
        ));
    }
    if clip.calibration.face_solve_confidence < 0.55 {
        issues.push(capture_issue(
            SpeechValidationSeverity::Error,
            "low_face_solve_confidence",
            "face solve confidence is too low for close-up lip sync",
        ));
    } else if clip.calibration.face_solve_confidence < 0.75 {
        issues.push(capture_issue(
            SpeechValidationSeverity::Warning,
            "soft_face_solve_confidence",
            "face solve confidence is below the desired capture quality",
        ));
    }
    if clip.calibration.body_solve_confidence < 0.4 {
        issues.push(capture_issue(
            SpeechValidationSeverity::Warning,
            "soft_body_solve_confidence",
            "body solve confidence is low enough to risk stiff or noisy posture",
        ));
    }
    if clip.audio_track.0 == 0 {
        issues.push(capture_issue(
            SpeechValidationSeverity::Error,
            "missing_audio_track",
            "performance capture must reference an audio clip or stream",
        ));
    }
    if clip.phonemes.is_empty() || clip.visemes.is_empty() {
        issues.push(capture_issue(
            SpeechValidationSeverity::Error,
            "missing_timing_tracks",
            "performance capture must include phoneme and viseme timing tracks",
        ));
    }
    if !timing_track_is_valid(
        clip.phonemes
            .iter()
            .map(|timing| (timing.start_seconds, timing.end_seconds)),
        clip.duration_seconds,
    ) {
        issues.push(capture_issue(
            SpeechValidationSeverity::Error,
            "invalid_phoneme_timing",
            "phoneme timing must be finite, ordered, and inside the capture duration",
        ));
    }
    if !timing_track_is_valid(
        clip.visemes
            .iter()
            .map(|timing| (timing.start_seconds, timing.end_seconds)),
        clip.duration_seconds,
    ) {
        issues.push(capture_issue(
            SpeechValidationSeverity::Error,
            "invalid_viseme_timing",
            "viseme timing must be finite, ordered, and inside the capture duration",
        ));
    }
    if clip.emotion_curve.is_empty()
        || clip
            .emotion_curve
            .iter()
            .any(|(time, _)| !time.is_finite() || *time < 0.0 || *time > clip.duration_seconds)
    {
        issues.push(capture_issue(
            SpeechValidationSeverity::Error,
            "invalid_emotion_curve",
            "performance capture must include an emotion curve inside the clip duration",
        ));
    }
    if clip.prosody.frames.len() < 2
        || clip.prosody.frames.iter().any(|frame| {
            !frame.time_seconds.is_finite()
                || frame.time_seconds < 0.0
                || frame.time_seconds > clip.duration_seconds
                || !frame.pitch_hz.is_finite()
                || !frame.energy.is_finite()
                || !frame.breathiness.is_finite()
        })
    {
        issues.push(capture_issue(
            SpeechValidationSeverity::Error,
            "invalid_capture_prosody",
            "performance capture must include finite prosody frames inside the clip duration",
        ));
    }
    if !curve_samples_are_valid(&clip.face_curves, clip.duration_seconds) {
        issues.push(capture_issue(
            SpeechValidationSeverity::Error,
            "invalid_face_curves",
            "face capture curves must be finite and inside the clip duration",
        ));
    }
    if !curve_samples_are_valid(&clip.body_curves, clip.duration_seconds) {
        issues.push(capture_issue(
            SpeechValidationSeverity::Warning,
            "invalid_body_curves",
            "body capture curves contain values that should be reviewed before close-up use",
        ));
    }
    if !clip
        .target_rig
        .mapped_face_channels
        .contains(&PerformanceCurveChannel::JawOpen)
        || clip.target_rig.viseme_set_label.trim().is_empty()
    {
        issues.push(capture_issue(
            SpeechValidationSeverity::Error,
            "incomplete_face_rig_mapping",
            "target rig mapping must include jaw/viseme controls for lip sync",
        ));
    }
    if clip.target_rig.mapped_body_channels.is_empty() {
        issues.push(capture_issue(
            SpeechValidationSeverity::Warning,
            "missing_body_rig_mapping",
            "body capture should include a target rig mapping for posture and breathing",
        ));
    }

    PerformanceCaptureValidationReport {
        passed: !issues
            .iter()
            .any(|issue| issue.severity == SpeechValidationSeverity::Error),
        issues,
    }
}

pub fn validate_speech_request(
    request: &SpeechRequest,
    persona: &VoicePersona,
) -> SpeechValidationReport {
    let mut issues = Vec::new();
    if request.text.trim().is_empty() {
        issues.push(speech_issue(
            SpeechValidationSeverity::Error,
            "empty_text",
            "speech request text must not be empty",
        ));
    }
    if request.safety_context.licensed_persona_required
        && (!persona.provenance.rights_verified || !persona.provenance.actor_consent)
    {
        issues.push(speech_issue(
            SpeechValidationSeverity::Error,
            "voice_provenance_unverified",
            "voice persona must have verified rights and consent",
        ));
    }
    if !request.safety_context.allow_generated_voice
        && matches!(persona.provenance.source, VoiceSource::GeneratedOriginal)
    {
        issues.push(speech_issue(
            SpeechValidationSeverity::Error,
            "generated_voice_disallowed",
            "request disallows generated voice for this line",
        ));
    }
    if request.text.len() > 240 && request.max_latency_ms < 100 {
        issues.push(speech_issue(
            SpeechValidationSeverity::Warning,
            "long_line_low_latency",
            "long dynamic speech should be cached or pre-generated",
        ));
    }

    SpeechValidationReport {
        passed: !issues
            .iter()
            .any(|issue| issue.severity == SpeechValidationSeverity::Error),
        issues,
    }
}

pub fn evaluate_speech_comfort(
    result: &SpeechResult,
    persona: &VoicePersona,
) -> SpeechComfortReport {
    let mut issues = Vec::new();
    let duration_seconds = result.duration_seconds;
    let phonemes_per_second = rate_per_second(result.phonemes.len(), duration_seconds);
    let visemes_per_second = rate_per_second(result.visemes.len(), duration_seconds);
    let average_phoneme_confidence = average_f32(result.phonemes.iter().map(|phoneme| {
        if phoneme.confidence.is_finite() {
            phoneme.confidence
        } else {
            0.0
        }
    }))
    .unwrap_or(0.0);
    let max_pitch_jump_hz = max_pitch_jump_hz(&result.prosody.frames);
    let max_energy = max_prosody_value(&result.prosody.frames, |frame| frame.energy);
    let max_breathiness = max_prosody_value(&result.prosody.frames, |frame| frame.breathiness);

    if !result.validation_report.passed {
        push_speech_comfort_issue(
            &mut issues,
            SpeechComfortIssueSeverity::Error,
            "speech_validation_failed",
            "speech validation failed; comfort evaluation must use a valid generated result",
        );
    }
    for validation_issue in &result.validation_report.issues {
        let severity = match validation_issue.severity {
            SpeechValidationSeverity::Info => SpeechComfortIssueSeverity::Info,
            SpeechValidationSeverity::Warning => SpeechComfortIssueSeverity::Warning,
            SpeechValidationSeverity::Error => SpeechComfortIssueSeverity::Error,
        };
        push_speech_comfort_issue(
            &mut issues,
            severity,
            format!("validation_{}", validation_issue.code),
            validation_issue.message.clone(),
        );
    }
    if result.cache_status == SpeechCacheStatus::Fallback {
        push_speech_comfort_issue(
            &mut issues,
            SpeechComfortIssueSeverity::Warning,
            "fallback_voice_used",
            "fallback speech is acceptable for latency, but should not dominate important dialogue",
        );
    }
    if !duration_seconds.is_finite() || duration_seconds <= 0.0 {
        push_speech_comfort_issue(
            &mut issues,
            SpeechComfortIssueSeverity::Error,
            "invalid_duration",
            "speech duration must be positive and finite",
        );
    } else if duration_seconds > 12.0 {
        push_speech_comfort_issue(
            &mut issues,
            SpeechComfortIssueSeverity::Warning,
            "line_too_long",
            "very long generated lines should be split or pre-authored for listening comfort",
        );
    }
    if result.phonemes.is_empty() || result.visemes.is_empty() {
        push_speech_comfort_issue(
            &mut issues,
            SpeechComfortIssueSeverity::Error,
            "missing_timing_tracks",
            "speech comfort requires phoneme and viseme timing tracks",
        );
    }
    if !(2.0..=22.0).contains(&phonemes_per_second) {
        let severity = if !(1.0..=32.0).contains(&phonemes_per_second) {
            SpeechComfortIssueSeverity::Error
        } else {
            SpeechComfortIssueSeverity::Warning
        };
        push_speech_comfort_issue(
            &mut issues,
            severity,
            "uncomfortable_phoneme_rate",
            "phoneme density is outside the comfortable speech range",
        );
    }
    if visemes_per_second > 18.0 {
        push_speech_comfort_issue(
            &mut issues,
            SpeechComfortIssueSeverity::Warning,
            "busy_viseme_track",
            "viseme changes are dense enough to risk chattery facial motion",
        );
    }
    if average_phoneme_confidence < 0.55 {
        push_speech_comfort_issue(
            &mut issues,
            SpeechComfortIssueSeverity::Error,
            "low_phoneme_confidence",
            "phoneme timing confidence is too low for comfortable lip sync",
        );
    } else if average_phoneme_confidence < 0.75 {
        push_speech_comfort_issue(
            &mut issues,
            SpeechComfortIssueSeverity::Warning,
            "soft_phoneme_confidence",
            "phoneme timing confidence is below the desired comfort range",
        );
    }
    if result.prosody.frames.len() < 2 {
        push_speech_comfort_issue(
            &mut issues,
            SpeechComfortIssueSeverity::Error,
            "missing_prosody",
            "speech comfort requires at least two prosody frames",
        );
    }
    if result.prosody.frames.iter().any(|frame| {
        !frame.pitch_hz.is_finite()
            || !frame.energy.is_finite()
            || !frame.breathiness.is_finite()
            || frame.pitch_hz < persona.pitch_range.min_hz
            || frame.pitch_hz > persona.pitch_range.max_hz
    }) {
        push_speech_comfort_issue(
            &mut issues,
            SpeechComfortIssueSeverity::Error,
            "invalid_prosody_frame",
            "prosody frames must stay finite and inside the persona pitch range",
        );
    }
    if max_pitch_jump_hz > 80.0 {
        push_speech_comfort_issue(
            &mut issues,
            SpeechComfortIssueSeverity::Error,
            "pitch_jump_artifact",
            "large pitch jumps sound synthetic and uncomfortable",
        );
    } else if max_pitch_jump_hz > 48.0 {
        push_speech_comfort_issue(
            &mut issues,
            SpeechComfortIssueSeverity::Warning,
            "sharp_pitch_jump",
            "pitch movement is sharper than desired for long listening comfort",
        );
    }
    if max_energy > 1.0 {
        push_speech_comfort_issue(
            &mut issues,
            SpeechComfortIssueSeverity::Error,
            "energy_clipped",
            "prosody energy exceeds normalized output range",
        );
    } else if max_energy > 0.95 {
        push_speech_comfort_issue(
            &mut issues,
            SpeechComfortIssueSeverity::Warning,
            "hot_delivery",
            "delivery energy is close to the comfort limit",
        );
    }
    if max_breathiness > 0.8 {
        push_speech_comfort_issue(
            &mut issues,
            SpeechComfortIssueSeverity::Warning,
            "excessive_breathiness",
            "breathiness is high enough to become tiring over long conversations",
        );
    }

    let score = speech_comfort_score(&issues);
    let passed = score >= 0.72
        && !issues
            .iter()
            .any(|issue| issue.severity == SpeechComfortIssueSeverity::Error);
    SpeechComfortReport {
        passed,
        score,
        duration_seconds,
        phonemes_per_second,
        visemes_per_second,
        average_phoneme_confidence,
        max_pitch_jump_hz,
        max_energy,
        max_breathiness,
        issue_count: issues.len(),
        issues,
    }
}

pub fn evaluate_conversation_comfort<'a>(
    results: impl IntoIterator<Item = &'a SpeechResult>,
) -> ConversationComfortReport {
    let results = results.into_iter().collect::<Vec<_>>();
    let mut issues = Vec::new();
    if results.is_empty() {
        push_speech_comfort_issue(
            &mut issues,
            SpeechComfortIssueSeverity::Error,
            "missing_speech_results",
            "conversation comfort requires at least one generated speech result",
        );
        return ConversationComfortReport {
            passed: false,
            line_count: 0,
            unique_voice_persona_count: 0,
            total_duration_seconds: 0.0,
            average_line_score: 0.0,
            minimum_line_score: 0.0,
            projected_ten_minute_score: 0.0,
            fallback_count: 0,
            cache_hit_count: 0,
            repeated_clip_count: 0,
            issue_count: issues.len(),
            issues,
        };
    }

    let mut personas = BTreeSet::new();
    let mut clip_counts = BTreeMap::<AudioClipHandle, usize>::new();
    let mut fallback_count = 0usize;
    let mut cache_hit_count = 0usize;
    let mut total_duration_seconds = 0.0f32;
    let mut line_scores = Vec::with_capacity(results.len());

    for result in &results {
        personas.insert(result.voice_persona);
        *clip_counts.entry(result.audio_clip).or_default() += 1;
        total_duration_seconds += result.duration_seconds.max(0.0);
        match result.cache_status {
            SpeechCacheStatus::Fallback => fallback_count += 1,
            SpeechCacheStatus::CacheHit => cache_hit_count += 1,
            SpeechCacheStatus::Generated | SpeechCacheStatus::CapturedPerformance => {}
        }
        let persona = persona_for_voice(result.voice_persona);
        let comfort = evaluate_speech_comfort(result, &persona);
        issues.extend(comfort.issues.clone());
        line_scores.push(comfort.score);
    }

    let repeated_clip_count = clip_counts
        .values()
        .map(|count| count.saturating_sub(1))
        .sum::<usize>();
    let average_line_score = average_f32(line_scores.iter().copied()).unwrap_or(0.0);
    let minimum_line_score = line_scores.iter().copied().reduce(f32::min).unwrap_or(0.0);
    let fallback_ratio = fallback_count as f32 / results.len() as f32;
    let repeated_ratio = repeated_clip_count as f32 / results.len() as f32;
    let cache_support = if cache_hit_count > 0 || results.len() <= 2 {
        1.0
    } else {
        0.96
    };
    let projected_ten_minute_score = (average_line_score
        * (1.0 - fallback_ratio * 0.3)
        * (1.0 - repeated_ratio * 0.2)
        * cache_support)
        .clamp(0.0, 1.0);

    if fallback_ratio > 0.5 {
        push_speech_comfort_issue(
            &mut issues,
            SpeechComfortIssueSeverity::Error,
            "conversation_fallback_dominates",
            "fallback speech dominates the conversation",
        );
    } else if fallback_count > 0 {
        push_speech_comfort_issue(
            &mut issues,
            SpeechComfortIssueSeverity::Warning,
            "conversation_uses_fallback",
            "some lines used fallback speech; cache or pre-generate likely dialogue",
        );
    }
    if repeated_ratio > 0.6 {
        push_speech_comfort_issue(
            &mut issues,
            SpeechComfortIssueSeverity::Warning,
            "repeated_clip_fatigue",
            "many repeated clips may become annoying in a long conversation",
        );
    }
    if projected_ten_minute_score < 0.75 {
        push_speech_comfort_issue(
            &mut issues,
            SpeechComfortIssueSeverity::Error,
            "low_ten_minute_comfort_projection",
            "conversation projection does not meet the long-listening comfort target",
        );
    }

    let passed = !issues
        .iter()
        .any(|issue| issue.severity == SpeechComfortIssueSeverity::Error);
    ConversationComfortReport {
        passed,
        line_count: results.len(),
        unique_voice_persona_count: personas.len(),
        total_duration_seconds,
        average_line_score,
        minimum_line_score,
        projected_ten_minute_score,
        fallback_count,
        cache_hit_count,
        repeated_clip_count,
        issue_count: issues.len(),
        issues,
    }
}

fn rate_per_second(count: usize, duration_seconds: f32) -> f32 {
    if duration_seconds > 0.0 && duration_seconds.is_finite() {
        count as f32 / duration_seconds
    } else {
        0.0
    }
}

fn average_f32(values: impl IntoIterator<Item = f32>) -> Option<f32> {
    let mut total = 0.0f32;
    let mut count = 0usize;
    for value in values {
        if value.is_finite() {
            total += value;
            count += 1;
        }
    }
    (count > 0).then_some(total / count as f32)
}

fn max_pitch_jump_hz(frames: &[ProsodyFrame]) -> f32 {
    frames
        .windows(2)
        .filter_map(|pair| {
            let delta = (pair[1].pitch_hz - pair[0].pitch_hz).abs();
            delta.is_finite().then_some(delta)
        })
        .fold(0.0, f32::max)
}

fn max_prosody_value(frames: &[ProsodyFrame], value: impl Fn(&ProsodyFrame) -> f32) -> f32 {
    frames
        .iter()
        .filter_map(|frame| {
            let value = value(frame);
            value.is_finite().then_some(value)
        })
        .fold(0.0, f32::max)
}

fn speech_comfort_score(issues: &[SpeechComfortIssue]) -> f32 {
    let penalty = issues
        .iter()
        .map(|issue| match issue.severity {
            SpeechComfortIssueSeverity::Info => 0.03,
            SpeechComfortIssueSeverity::Warning => 0.12,
            SpeechComfortIssueSeverity::Error => 0.36,
        })
        .sum::<f32>();
    (1.0 - penalty).clamp(0.0, 1.0)
}

fn push_speech_comfort_issue(
    issues: &mut Vec<SpeechComfortIssue>,
    severity: SpeechComfortIssueSeverity,
    code: impl Into<String>,
    message: impl Into<String>,
) {
    issues.push(SpeechComfortIssue {
        severity,
        code: code.into(),
        message: message.into(),
    });
}

pub fn persona_for_voice(id: VoicePersonaId) -> VoicePersona {
    if id == 300 {
        mara_voice_persona()
    } else {
        VoicePersona {
            id,
            display_name: format!("Generated Persona {id}"),
            timbre_profile: TimbreProfile {
                brightness: 0.55,
                roughness: 0.2,
                warmth: 0.5,
                nasality: 0.2,
            },
            pitch_range: PitchRange {
                min_hz: 120.0,
                resting_hz: 180.0,
                max_hz: 260.0,
            },
            speaking_rate: 1.0,
            accent_profile: AccentProfile {
                label: "neutral synthetic".to_string(),
                rhythm: "even".to_string(),
            },
            emotional_range: EmotionalRange {
                supports_fear: true,
                supports_anger: true,
                supports_sadness: true,
                supports_urgency: true,
            },
            breath_profile: BreathProfile {
                breaths_per_minute: 14.0,
                audible_breathiness: 0.18,
                panic_multiplier: 1.8,
            },
            hesitation_style: HesitationStyle::ShortPauses,
            fatigue_response: FatigueResponse {
                pitch_drop_hz: 12.0,
                speaking_rate_multiplier: 0.9,
                breathiness_gain: 0.2,
            },
            injury_response: InjuryVoiceResponse {
                pain_pitch_shift_hz: 28.0,
                breath_interruptions: 0.2,
                volume_drop: 0.18,
            },
            provenance: VoiceProvenance {
                source: VoiceSource::GeneratedOriginal,
                rights_verified: true,
                actor_consent: true,
                generated_model_label: "ashfall-original-voice-v1".to_string(),
                replacement_allowed: true,
                notes: "fictional generated persona".to_string(),
            },
        }
    }
}

pub fn mara_voice_persona() -> VoicePersona {
    VoicePersona {
        id: 300,
        display_name: "Mara".to_string(),
        timbre_profile: TimbreProfile {
            brightness: 0.58,
            roughness: 0.28,
            warmth: 0.42,
            nasality: 0.16,
        },
        pitch_range: PitchRange {
            min_hz: 145.0,
            resting_hz: 205.0,
            max_hz: 310.0,
        },
        speaking_rate: 1.08,
        accent_profile: AccentProfile {
            label: "urban alley resident".to_string(),
            rhythm: "fast clipped warnings".to_string(),
        },
        emotional_range: EmotionalRange {
            supports_fear: true,
            supports_anger: true,
            supports_sadness: true,
            supports_urgency: true,
        },
        breath_profile: BreathProfile {
            breaths_per_minute: 16.0,
            audible_breathiness: 0.22,
            panic_multiplier: 2.1,
        },
        hesitation_style: HesitationStyle::BrokenPhrases,
        fatigue_response: FatigueResponse {
            pitch_drop_hz: 18.0,
            speaking_rate_multiplier: 0.86,
            breathiness_gain: 0.28,
        },
        injury_response: InjuryVoiceResponse {
            pain_pitch_shift_hz: 34.0,
            breath_interruptions: 0.35,
            volume_drop: 0.22,
        },
        provenance: VoiceProvenance {
            source: VoiceSource::GeneratedOriginal,
            rights_verified: true,
            actor_consent: true,
            generated_model_label: "ashfall-mara-original-v1".to_string(),
            replacement_allowed: true,
            notes: "fictional project-owned voice, not a real-person clone".to_string(),
        },
    }
}

pub fn spatialize_sound(input: SoundSpatializationInput) -> ActiveSoundInfo {
    spatialize_sound_with_acoustic_context(input, AcousticSoundContext::default())
}

#[derive(Default)]
struct AcousticSoundContext<'a> {
    environment: AcousticEnvironment,
    zone_entity: Option<EntityId>,
    occlusion_sources: Vec<AudioOcclusionReport>,
    material_descriptor: Option<&'a MaterialDescriptor>,
    material_state: Option<MaterialState>,
}

fn spatialize_sound_with_acoustic_context(
    input: SoundSpatializationInput,
    context: AcousticSoundContext<'_>,
) -> ActiveSoundInfo {
    let material_profile = material_sound_profile_from_descriptor(
        input.kind,
        input.material_id,
        context.material_descriptor,
        context.material_state,
    );
    let distance = input.location.distance(input.listener);
    let radius = input.radius_meters.max(0.1);
    let falloff = (1.0 - (distance / radius).clamp(0.0, 1.0) * 0.25).clamp(0.0, 1.0);
    let occlusion = input.occlusion_hint.clamp(0.0, 1.0);
    let material_muffle = material_profile.absorption * 0.12 + material_profile.wet_modifier * 0.08;
    let environmental_muffle =
        context.environment.air_absorption * (distance / radius).clamp(0.0, 1.0);
    let spatial = SpatialAudioParams {
        gain: falloff,
        pan: ((input.location.x - input.listener.x) / radius).clamp(-1.0, 1.0),
        low_pass_hz: (18_000.0
            * (1.0
                - occlusion.clamp(0.0, 0.95) * 0.55
                - material_muffle
                - environmental_muffle * 0.2))
            .max(2_600.0),
        reverb_send: reverb_send_for(input.kind, distance, context.environment, &material_profile),
        early_reflection_gain: context.environment.early_reflection_gain,
        reverb_tail_seconds: context.environment.reverb_tail_seconds,
        air_absorption: context.environment.air_absorption,
        occlusion,
    };
    let environment_gain = (1.0 + context.environment.early_reflection_gain * 0.08
        - context.environment.air_absorption * 0.18)
        .clamp(0.72, 1.08);
    let effective_intensity = material_aware_intensity_for_profile(
        input.kind,
        &material_profile,
        input.intensity,
        occlusion,
    ) * spatial.gain
        * environment_gain;

    ActiveSoundInfo {
        event_id: input.event_id,
        source_entity: input.source_entity,
        event_kind: input.kind,
        material_id: input.material_id,
        clip: AudioClipHandle((input.event_id & u64::MAX as u128) + 70_000),
        bus: bus_for_sound(input.kind),
        priority: priority_for_sound(input.kind),
        location: input.location,
        distance_to_listener_meters: distance,
        effective_intensity,
        radius_meters: input.radius_meters,
        remaining_seconds: sound_duration_seconds(input.kind),
        spatial,
        material_profile,
        acoustic_environment: context.environment,
        acoustic_zone_entity: context.zone_entity,
        occlusion_sources: context.occlusion_sources,
    }
}

pub fn material_sound_profile(
    kind: AudioEventKind,
    material_id: Option<MaterialId>,
) -> MaterialSoundProfile {
    match (kind, material_id) {
        (AudioEventKind::GlassImpact | AudioEventKind::GlassShatter, Some(_)) => {
            MaterialSoundProfile {
                family: SoundFamily::Glass,
                absorption: 0.08,
                resonance: 0.85,
                wet_modifier: 0.05,
                break_brightness: 0.95,
            }
        }
        (AudioEventKind::WaterSplash, _) => MaterialSoundProfile {
            family: SoundFamily::Water,
            absorption: 0.5,
            resonance: 0.1,
            wet_modifier: 1.0,
            break_brightness: 0.25,
        },
        (AudioEventKind::MetalBend, Some(_)) => MaterialSoundProfile {
            family: SoundFamily::Metal,
            absorption: 0.14,
            resonance: 0.75,
            wet_modifier: 0.12,
            break_brightness: 0.7,
        },
        (AudioEventKind::ConcreteCrack | AudioEventKind::Footstep, Some(_)) => {
            MaterialSoundProfile {
                family: SoundFamily::Concrete,
                absorption: 0.42,
                resonance: 0.22,
                wet_modifier: 0.4,
                break_brightness: 0.35,
            }
        }
        (AudioEventKind::VoiceSpeech | AudioEventKind::HumanBreath, _) => MaterialSoundProfile {
            family: SoundFamily::Human,
            absorption: 0.62,
            resonance: 0.18,
            wet_modifier: 0.0,
            break_brightness: 0.0,
        },
        (AudioEventKind::ElectricBuzz | AudioEventKind::NeonHum, _) => MaterialSoundProfile {
            family: SoundFamily::Electric,
            absorption: 0.18,
            resonance: 0.48,
            wet_modifier: 0.22,
            break_brightness: 0.5,
        },
        _ => MaterialSoundProfile {
            family: SoundFamily::Generic,
            absorption: 0.35,
            resonance: 0.25,
            wet_modifier: 0.1,
            break_brightness: 0.3,
        },
    }
}

pub fn material_sound_profile_from_descriptor(
    kind: AudioEventKind,
    material_id: Option<MaterialId>,
    descriptor: Option<&MaterialDescriptor>,
    state: Option<MaterialState>,
) -> MaterialSoundProfile {
    let mut profile = material_sound_profile(kind, material_id);

    if let Some(descriptor) = descriptor {
        profile.family = sound_family_from_material(kind, descriptor);
        profile.absorption = descriptor.acoustic.absorption.clamp(0.0, 1.0);
        profile.resonance = descriptor.acoustic.resonance.clamp(0.0, 1.0);
        profile.wet_modifier = descriptor.acoustic.wetness_muffle.clamp(0.0, 1.0);
        profile.break_brightness = descriptor.acoustic.impact_brightness.clamp(0.0, 1.0);
    }

    if let Some(state) = state {
        profile.absorption =
            (profile.absorption + state.moisture * 0.22 + state.soot * 0.08).clamp(0.0, 1.0);
        profile.resonance = (profile.resonance * (1.0 - state.crack_density * 0.35)
            + state.plastic_strain.min(1.0) * 0.08)
            .clamp(0.0, 1.0);
        profile.wet_modifier = profile.wet_modifier.max(state.moisture.clamp(0.0, 1.0));
        profile.break_brightness = (profile.break_brightness
            - state.moisture * 0.18
            - state.corrosion * 0.08
            - state.soot * 0.06)
            .clamp(0.05, 1.0);
    }

    profile
}

pub fn material_aware_intensity(
    kind: AudioEventKind,
    material_id: Option<MaterialId>,
    intensity: f32,
    occlusion_hint: f32,
) -> f32 {
    let profile = material_sound_profile(kind, material_id);
    material_aware_intensity_for_profile(kind, &profile, intensity, occlusion_hint)
}

pub fn material_aware_intensity_for_profile(
    _kind: AudioEventKind,
    profile: &MaterialSoundProfile,
    intensity: f32,
    occlusion_hint: f32,
) -> f32 {
    let material_gain = match profile.family {
        SoundFamily::Glass => 1.12,
        SoundFamily::Water => 0.82,
        SoundFamily::Concrete => 0.95,
        SoundFamily::Metal => 1.05,
        SoundFamily::Electric => 1.02,
        SoundFamily::Human | SoundFamily::Generic => 1.0,
    };
    let acoustic_gain =
        (material_gain + profile.resonance * 0.08 - profile.absorption * 0.12).clamp(0.5, 1.25);
    (intensity * acoustic_gain * (1.0 - occlusion_hint.clamp(0.0, 0.9) * 0.5)).clamp(0.0, 1.25)
}

pub fn evaluate_acoustic_scene(
    snapshot: &WorldSnapshot,
    source: Vec3,
    listener: Vec3,
) -> AcousticSceneEvaluation {
    let (environment, zone_entity, source_inside_zone, listener_inside_zone) =
        acoustic_environment_at(snapshot, source, listener);
    let occlusion_sources = audio_occlusion_reports(snapshot, source, listener);
    let world_occlusion = occlusion_sources
        .iter()
        .fold(0.0, |acc, report| combine_occlusion(acc, report.occlusion));

    AcousticSceneEvaluation {
        environment,
        zone_entity,
        source_inside_zone,
        listener_inside_zone,
        world_occlusion,
        occlusion_sources,
    }
}

fn acoustic_environment_at(
    snapshot: &WorldSnapshot,
    source: Vec3,
    listener: Vec3,
) -> (AcousticEnvironment, Option<EntityId>, bool, bool) {
    let mut best: Option<(u8, EntityId, AcousticEnvironment, bool, bool)> = None;

    for (entity, tags) in snapshot.tags.iter() {
        let Some(space) = acoustic_space_from_tags(tags) else {
            continue;
        };
        let Some(transform) = snapshot.transforms.find(*entity) else {
            continue;
        };
        let source_inside = point_inside_zone(source, *transform);
        let listener_inside = point_inside_zone(listener, *transform);
        if !source_inside && !listener_inside {
            continue;
        }

        let score = u8::from(source_inside) + u8::from(listener_inside);
        let environment = acoustic_environment_for_space(space);
        if best
            .as_ref()
            .is_none_or(|(best_score, best_entity, _, _, _)| {
                score > *best_score || (score == *best_score && *entity < *best_entity)
            })
        {
            best = Some((score, *entity, environment, source_inside, listener_inside));
        }
    }

    best.map(|(_, entity, environment, source_inside, listener_inside)| {
        (environment, Some(entity), source_inside, listener_inside)
    })
    .unwrap_or((AcousticEnvironment::default(), None, false, false))
}

fn acoustic_space_from_tags(tags: &TagSet) -> Option<AcousticSpaceKind> {
    for tag in tags {
        let normalized = tag.to_ascii_lowercase();
        if normalized == "audio_zone:alley" || normalized.contains("rain alley") {
            return Some(AcousticSpaceKind::NarrowAlley);
        }
        if normalized == "audio_zone:indoor_small" || normalized == "audio_zone:room" {
            return Some(AcousticSpaceKind::SmallInterior);
        }
        if normalized == "audio_zone:indoor_large" || normalized == "audio_zone:atrium" {
            return Some(AcousticSpaceKind::LargeInterior);
        }
        if normalized == "audio_zone:tunnel" || normalized == "audio_zone:sewer" {
            return Some(AcousticSpaceKind::Tunnel);
        }
        if normalized == "audio_zone:damp_utility" {
            return Some(AcousticSpaceKind::DampUtility);
        }
        if normalized == "audio_zone:outdoor" {
            return Some(AcousticSpaceKind::Outdoor);
        }
    }
    None
}

fn acoustic_environment_for_space(space: AcousticSpaceKind) -> AcousticEnvironment {
    match space {
        AcousticSpaceKind::Outdoor => AcousticEnvironment::default(),
        AcousticSpaceKind::NarrowAlley => AcousticEnvironment {
            space,
            reverb_send: 0.34,
            early_reflection_gain: 0.42,
            reverb_tail_seconds: 1.7,
            air_absorption: 0.07,
            obstruction_density: 0.12,
        },
        AcousticSpaceKind::SmallInterior => AcousticEnvironment {
            space,
            reverb_send: 0.22,
            early_reflection_gain: 0.48,
            reverb_tail_seconds: 0.9,
            air_absorption: 0.11,
            obstruction_density: 0.2,
        },
        AcousticSpaceKind::LargeInterior => AcousticEnvironment {
            space,
            reverb_send: 0.46,
            early_reflection_gain: 0.32,
            reverb_tail_seconds: 2.4,
            air_absorption: 0.08,
            obstruction_density: 0.15,
        },
        AcousticSpaceKind::Tunnel => AcousticEnvironment {
            space,
            reverb_send: 0.58,
            early_reflection_gain: 0.5,
            reverb_tail_seconds: 3.1,
            air_absorption: 0.1,
            obstruction_density: 0.3,
        },
        AcousticSpaceKind::DampUtility => AcousticEnvironment {
            space,
            reverb_send: 0.4,
            early_reflection_gain: 0.36,
            reverb_tail_seconds: 1.9,
            air_absorption: 0.16,
            obstruction_density: 0.28,
        },
    }
}

fn audio_occlusion_reports(
    snapshot: &WorldSnapshot,
    source: Vec3,
    listener: Vec3,
) -> Vec<AudioOcclusionReport> {
    let mut reports = Vec::new();

    for (entity, tags) in snapshot.tags.iter() {
        let Some((tag_strength, label)) = occluder_strength_from_tags(tags) else {
            continue;
        };
        let Some(transform) = snapshot.transforms.find(*entity) else {
            continue;
        };
        let distance_to_ray =
            distance_point_to_segment(transform.translation_meters, source, listener);
        let radius = occluder_radius(*transform);
        if distance_to_ray > radius {
            continue;
        }

        let material_factor = snapshot
            .material_states
            .find(*entity)
            .map(|state| {
                (1.0 - state.crack_density * 0.65 - state.plastic_strain.min(1.0) * 0.25)
                    .clamp(0.15, 1.0)
            })
            .unwrap_or(1.0);
        let proximity_factor = (1.0 - distance_to_ray / radius).clamp(0.0, 1.0);
        let occlusion = (tag_strength * material_factor * proximity_factor).clamp(0.0, 0.85);
        if occlusion > 0.01 {
            reports.push(AudioOcclusionReport {
                occluder_entity: *entity,
                occluder_label: label,
                occlusion,
                distance_to_sound_ray_meters: distance_to_ray,
            });
        }
    }

    reports.sort_by(|left, right| {
        right
            .occlusion
            .partial_cmp(&left.occlusion)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.occluder_entity.cmp(&right.occluder_entity))
    });
    reports
}

fn occluder_strength_from_tags(tags: &TagSet) -> Option<(f32, String)> {
    let mut strongest: Option<(f32, String)> = None;

    for tag in tags {
        let normalized = tag.to_ascii_lowercase();
        let candidate = if normalized == "audio_occluder"
            || normalized == "audio_occluder:wall"
            || normalized == "wall"
        {
            Some((0.42, "wall".to_string()))
        } else if normalized == "audio_occluder:glass" || normalized == "glass_wall" {
            Some((0.24, "glass".to_string()))
        } else if normalized == "audio_occluder:door" || normalized == "door" {
            Some((0.36, "door".to_string()))
        } else if normalized == "audio_occluder:smoke"
            || normalized == "audio_occluder:steam"
            || normalized == "smoke"
            || normalized == "steam"
        {
            Some((0.18, "volumetric".to_string()))
        } else {
            None
        };

        if let Some(candidate) = candidate
            && strongest
                .as_ref()
                .is_none_or(|(strength, _)| candidate.0 > *strength)
        {
            strongest = Some(candidate);
        }
    }

    strongest
}

fn point_inside_zone(point: Vec3, transform: Transform) -> bool {
    let half_x = (transform.scale.x.abs() * 0.5).max(0.5);
    let half_y = (transform.scale.y.abs() * 0.5).max(0.5);
    let half_z = (transform.scale.z.abs() * 0.5).max(0.5);
    (point.x - transform.translation_meters.x).abs() <= half_x
        && (point.y - transform.translation_meters.y).abs() <= half_y
        && (point.z - transform.translation_meters.z).abs() <= half_z
}

fn occluder_radius(transform: Transform) -> f32 {
    (transform
        .scale
        .x
        .abs()
        .max(transform.scale.y.abs())
        .max(transform.scale.z.abs())
        * 0.5)
        .max(0.35)
}

fn distance_point_to_segment(point: Vec3, a: Vec3, b: Vec3) -> f32 {
    let ab = Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z);
    let ap = Vec3::new(point.x - a.x, point.y - a.y, point.z - a.z);
    let ab_len_sq = ab.x * ab.x + ab.y * ab.y + ab.z * ab.z;
    if ab_len_sq <= f32::EPSILON {
        return point.distance(a);
    }
    let t = ((ap.x * ab.x + ap.y * ab.y + ap.z * ab.z) / ab_len_sq).clamp(0.0, 1.0);
    point.distance(Vec3::new(a.x + ab.x * t, a.y + ab.y * t, a.z + ab.z * t))
}

fn combine_occlusion(existing: f32, added: f32) -> f32 {
    let existing = existing.clamp(0.0, 1.0);
    let added = added.clamp(0.0, 1.0);
    (1.0 - (1.0 - existing) * (1.0 - added)).clamp(0.0, 1.0)
}

fn sound_family_from_material(
    kind: AudioEventKind,
    descriptor: &MaterialDescriptor,
) -> SoundFamily {
    match kind {
        AudioEventKind::GlassImpact | AudioEventKind::GlassShatter => return SoundFamily::Glass,
        AudioEventKind::MetalBend => return SoundFamily::Metal,
        AudioEventKind::WaterSplash | AudioEventKind::SteamLeak => return SoundFamily::Water,
        AudioEventKind::VoiceSpeech | AudioEventKind::HumanBreath => return SoundFamily::Human,
        AudioEventKind::ElectricBuzz | AudioEventKind::NeonHum => return SoundFamily::Electric,
        _ => {}
    }

    let name = descriptor.name.to_ascii_lowercase();
    if name.contains("glass") {
        SoundFamily::Glass
    } else if name.contains("metal")
        || name.contains("chrome")
        || name.contains("steel")
        || name.contains("neon")
    {
        SoundFamily::Metal
    } else if name.contains("water") || name.contains("rain") || name.contains("puddle") {
        SoundFamily::Water
    } else if name.contains("concrete") || name.contains("asphalt") || name.contains("stone") {
        SoundFamily::Concrete
    } else if name.contains("skin") || name.contains("human") {
        SoundFamily::Human
    } else {
        SoundFamily::Generic
    }
}

fn build_prosody(
    request: &SpeechRequest,
    persona: &VoicePersona,
    duration_seconds: f32,
    seconds_per_frame: f32,
) -> ProsodyFrameSet {
    let frame_count = ((duration_seconds / seconds_per_frame.max(0.05)).ceil() as usize).max(2);
    let frames = (0..frame_count)
        .map(|index| {
            let t = index as f32 / (frame_count - 1) as f32;
            let urgency = request.urgency.clamp(0.0, 1.0);
            let fear = request.emotion.fear.clamp(0.0, 1.0);
            ProsodyFrame {
                time_seconds: t * duration_seconds,
                pitch_hz: (persona.pitch_range.resting_hz
                    + fear * 28.0
                    + urgency * 12.0
                    + (t * std::f32::consts::TAU).sin() * 4.0)
                    .clamp(persona.pitch_range.min_hz, persona.pitch_range.max_hz),
                energy: (request.loudness + urgency * 0.18).clamp(0.0, 1.0),
                breathiness: (persona.breath_profile.audible_breathiness
                    + fear * persona.breath_profile.panic_multiplier * 0.08)
                    .clamp(0.0, 1.0),
            }
        })
        .collect();
    ProsodyFrameSet { frames }
}

fn cache_key(request: &SpeechRequest) -> SpeechCacheKey {
    SpeechCacheKey {
        voice_persona: request.voice_persona,
        normalized_text: request
            .text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase(),
        intent: request.intent.to_ascii_lowercase(),
        language: request.language,
        mode: request.mode,
        emotion_bucket: ((request.emotion.fear + request.emotion.anger + request.emotion.urgency)
            * 10.0)
            .round() as u8,
        loudness_bucket: (request.loudness.clamp(0.0, 1.0) * 10.0).round() as u8,
    }
}

fn stable_clip_id(key: &SpeechCacheKey) -> AssetId {
    60_000
        + stable_u64(&format!(
            "{}:{}:{}:{}:{}:{}:{}",
            key.voice_persona,
            key.normalized_text,
            key.intent,
            key.language,
            speech_mode_label(key.mode),
            key.emotion_bucket,
            key.loudness_bucket
        )) as u128
}

fn listener_location(frame: &FrameContext) -> Vec3 {
    frame
        .snapshot
        .tags
        .iter()
        .find_map(|(entity, tags)| {
            tags.iter()
                .any(|tag| tag == "player")
                .then(|| frame.snapshot.transforms.find(*entity))
                .flatten()
                .map(|transform| transform.translation_meters)
        })
        .unwrap_or(Vec3::ZERO)
}

fn bus_for_sound(kind: AudioEventKind) -> AudioBusKind {
    match kind {
        AudioEventKind::VoiceSpeech | AudioEventKind::HumanBreath => AudioBusKind::Voice,
        AudioEventKind::GlassImpact
        | AudioEventKind::GlassShatter
        | AudioEventKind::ConcreteCrack
        | AudioEventKind::Explosion
        | AudioEventKind::Gunshot => AudioBusKind::Impact,
        AudioEventKind::Footstep
        | AudioEventKind::WaterSplash
        | AudioEventKind::ClothRustle
        | AudioEventKind::DoorOpen => AudioBusKind::Foley,
        AudioEventKind::ElectricBuzz | AudioEventKind::NeonHum | AudioEventKind::SteamLeak => {
            AudioBusKind::Mechanical
        }
        _ => AudioBusKind::Ambience,
    }
}

fn priority_for_sound(kind: AudioEventKind) -> AudioPriority {
    match kind {
        AudioEventKind::VoiceSpeech | AudioEventKind::Explosion | AudioEventKind::Gunshot => {
            AudioPriority::Critical
        }
        AudioEventKind::GlassShatter
        | AudioEventKind::ConcreteCrack
        | AudioEventKind::MetalBend => AudioPriority::High,
        AudioEventKind::WaterSplash | AudioEventKind::Footstep | AudioEventKind::DoorOpen => {
            AudioPriority::Normal
        }
        _ => AudioPriority::Low,
    }
}

fn reverb_send_for(
    kind: AudioEventKind,
    distance: f32,
    acoustic_environment: AcousticEnvironment,
    material_profile: &MaterialSoundProfile,
) -> f32 {
    let base = match kind {
        AudioEventKind::GlassShatter | AudioEventKind::Gunshot | AudioEventKind::Explosion => 0.55,
        AudioEventKind::VoiceSpeech => 0.18,
        AudioEventKind::WaterSplash => 0.25,
        _ => 0.32,
    };
    (base + acoustic_environment.reverb_send + material_profile.resonance * 0.08
        - material_profile.absorption * 0.04
        + (distance / 40.0).clamp(0.0, 0.25))
    .clamp(0.0, 1.0)
}

fn mix_buses_with_soundscape(
    active_sounds: &[ActiveSoundInfo],
    soundscape_layers: &[SoundscapeLayer],
    adaptive_score: &AdaptiveScoreOutput,
) -> Vec<AudioBusMix> {
    let mut buses: BTreeMap<AudioBusKindKey, AudioBusMix> = BTreeMap::new();
    for sound in active_sounds {
        let key = AudioBusKindKey(sound.bus);
        let entry = buses.entry(key).or_insert(AudioBusMix {
            bus: sound.bus,
            sound_count: 0,
            peak_intensity: 0.0,
        });
        entry.sound_count += 1;
        entry.peak_intensity = entry.peak_intensity.max(sound.effective_intensity);
    }
    for layer in soundscape_layers {
        let key = AudioBusKindKey(layer.bus);
        let entry = buses.entry(key).or_insert(AudioBusMix {
            bus: layer.bus,
            sound_count: 0,
            peak_intensity: 0.0,
        });
        entry.sound_count += 1;
        entry.peak_intensity = entry.peak_intensity.max(layer.intensity);
    }
    for stem in &adaptive_score.stems {
        let key = AudioBusKindKey(AudioBusKind::Music);
        let entry = buses.entry(key).or_insert(AudioBusMix {
            bus: AudioBusKind::Music,
            sound_count: 0,
            peak_intensity: 0.0,
        });
        entry.sound_count += 1;
        entry.peak_intensity = entry.peak_intensity.max(stem.intensity);
    }
    buses.into_values().collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct AudioBusKindKey(AudioBusKind);

fn sound_duration_seconds(kind: AudioEventKind) -> f32 {
    match kind {
        AudioEventKind::GlassShatter => 1.4,
        AudioEventKind::WaterSplash => 0.7,
        AudioEventKind::VoiceSpeech => 2.8,
        AudioEventKind::Explosion => 3.2,
        AudioEventKind::Gunshot => 0.8,
        AudioEventKind::NeonHum | AudioEventKind::ElectricBuzz => 4.0,
        _ => 1.0,
    }
}

fn speech_issue(
    severity: SpeechValidationSeverity,
    code: &'static str,
    message: &'static str,
) -> SpeechValidationIssue {
    SpeechValidationIssue {
        severity,
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn capture_issue(
    severity: SpeechValidationSeverity,
    code: &'static str,
    message: &'static str,
) -> PerformanceCaptureValidationIssue {
    PerformanceCaptureValidationIssue {
        severity,
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn speech_validation_from_capture_reports(
    speech_report: &SpeechValidationReport,
    capture_report: &PerformanceCaptureValidationReport,
) -> SpeechValidationReport {
    let mut issues = speech_report.issues.clone();
    issues.extend(
        capture_report
            .issues
            .iter()
            .map(|issue| SpeechValidationIssue {
                severity: issue.severity,
                code: if issue.code.starts_with("capture_") {
                    issue.code.clone()
                } else {
                    format!("capture_{}", issue.code)
                },
                message: issue.message.clone(),
            }),
    );
    SpeechValidationReport {
        passed: !issues
            .iter()
            .any(|issue| issue.severity == SpeechValidationSeverity::Error),
        issues,
    }
}

fn performance_capture_memory_bytes(clip: &PerformanceCaptureClip) -> u64 {
    let track_count = clip.phonemes.len()
        + clip.visemes.len()
        + clip.prosody.frames.len()
        + clip.emotion_curve.len()
        + clip.face_curves.len()
        + clip.body_curves.len();
    (track_count as u64)
        .saturating_mul(48)
        .saturating_add(64 * 1024)
}

fn timing_track_is_valid(
    timings: impl IntoIterator<Item = (f32, f32)>,
    duration_seconds: f32,
) -> bool {
    if !duration_seconds.is_finite() || duration_seconds <= 0.0 {
        return false;
    }

    let mut previous_end = 0.0_f32;
    for (start, end) in timings {
        if !start.is_finite()
            || !end.is_finite()
            || start < -0.001
            || end <= start
            || end > duration_seconds + 0.001
            || start + 0.001 < previous_end
        {
            return false;
        }
        previous_end = end;
    }
    true
}

fn curve_samples_are_valid(samples: &[PerformanceCurveSample], duration_seconds: f32) -> bool {
    if samples.is_empty() {
        return false;
    }
    samples.iter().all(|sample| {
        sample.time_seconds.is_finite()
            && sample.value.is_finite()
            && sample.confidence.is_finite()
            && sample.time_seconds >= 0.0
            && sample.time_seconds <= duration_seconds + 0.001
            && sample.confidence >= 0.0
            && sample.confidence <= 1.0
    })
}

fn audio_gpu_resource(
    graph: &mut GpuGraphBuilder,
    label: &'static str,
    kind: GpuResourceKind,
    byte_len: u64,
    lifetime: GpuResourceLifetime,
) -> GpuResourceHandle {
    graph.declare_resource(
        GpuResourceDesc::new(label, kind, byte_len)
            .owned_by(65)
            .with_lifetime(lifetime),
    )
}

fn audio_compute_pipeline_with_permutation(
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

fn audio_shader_permutation(
    defines: impl IntoIterator<Item = impl Into<String>>,
) -> GpuShaderPermutation {
    GpuShaderPermutation::new(defines)
}

fn audio_resource_bytes(item_count: u64, bytes_per_item: u64) -> u64 {
    item_count
        .saturating_mul(bytes_per_item)
        .max(bytes_per_item)
}

fn audio_output_buffer_bytes(output: &AudioFrameOutput) -> u64 {
    let tail_seconds = output
        .active_sounds
        .iter()
        .map(|sound| {
            sound
                .remaining_seconds
                .max(sound.spatial.reverb_tail_seconds)
        })
        .fold(0.25_f32, f32::max)
        .clamp(0.25, 8.0);
    let sample_count = (output.sample_rate_hz as f32 * tail_seconds).ceil() as u64;
    sample_count
        .saturating_mul(2)
        .saturating_mul(std::mem::size_of::<f32>() as u64)
        .max(512 * 1024)
}

fn estimate_audio_gpu_milliseconds(
    active_sounds: &[ActiveSoundInfo],
    soundscape_layer_count: usize,
) -> f32 {
    if active_sounds.is_empty() && soundscape_layer_count == 0 {
        return 0.0;
    }
    let active_count = active_sounds.len() as f32;
    let soundscape_count = soundscape_layer_count as f32;
    let occluded_count = active_sounds
        .iter()
        .filter(|sound| sound.spatial.occlusion > 0.01 || !sound.occlusion_sources.is_empty())
        .count() as f32;
    let reverb_count = active_sounds
        .iter()
        .filter(|sound| {
            sound.spatial.reverb_send > 0.01 || sound.spatial.reverb_tail_seconds > 0.25
        })
        .count() as f32;
    (0.05
        + active_count * 0.01
        + soundscape_count * 0.006
        + occluded_count * 0.012
        + reverb_count * 0.015)
        .min(0.9)
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

fn should_defer_speech_generation(request: &SpeechRequest) -> bool {
    !matches!(request.mode, SpeechGenerationMode::PreAuthored)
        && request.max_latency_ms < 100
        && request.text.len() > 160
}

fn fallback_speech_result(request: &SpeechRequest, persona: &VoicePersona) -> SpeechResult {
    let mut fallback_request = request.clone();
    fallback_request.text = DEFERRED_SPEECH_TEXT.to_string();
    fallback_request.mode = SpeechGenerationMode::PreAuthored;
    fallback_request.max_latency_ms = fallback_request.max_latency_ms.min(25);
    let mut result = synthesize_speech_with_persona(&fallback_request, persona);
    result.request_id = request.request_id;
    result.speaker = request.speaker;
    result.voice_persona = request.voice_persona;
    result.cache_status = SpeechCacheStatus::Fallback;
    result.generation_cost = PerformanceCounters {
        cpu_milliseconds: 0.02,
        gpu_milliseconds: 0.0,
        memory_bytes: 32 * 1024,
    };
    result
}

fn stable_u64(input: &str) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for byte in input.bytes() {
        value ^= u64::from(byte);
        value = value.wrapping_mul(0x1000_0000_01b3);
    }
    value
}

fn parse_finite_f32(value: &str) -> Option<f32> {
    let value = value.parse::<f32>().ok()?;
    value.is_finite().then_some(value)
}

fn encode_state_text(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'%' | b'|' | b',' | b'\n' | b'\r' => {
                encoded.push_str(&format!("%{byte:02X}"));
            }
            0x20..=0x7E => encoded.push(byte as char),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn decode_state_text(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = *bytes.get(index + 1)?;
            let low = *bytes.get(index + 2)?;
            decoded.push((hex_digit(high)? << 4) | hex_digit(low)?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn encode_speech_request(request: &SpeechRequest) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        request.request_id,
        request.speaker,
        request.voice_persona,
        encode_state_text(&request.text),
        request.emotion.fear,
        request.emotion.anger,
        request.emotion.urgency,
        request.emotion.trust,
        encode_state_text(&request.intent),
        request.urgency,
        request.loudness,
        request.language,
        encode_state_text(&request.interruption_policy),
        encode_location(request.location),
        speech_priority_label(request.priority),
        speech_mode_label(request.mode),
        request.max_latency_ms,
        request.safety_context.allow_generated_voice,
        request.safety_context.content_review_required,
        request.safety_context.licensed_persona_required
    )
}

fn parse_speech_request(value: &str) -> Option<SpeechRequest> {
    let fields = value.split('|').collect::<Vec<_>>();
    if fields.len() != 20 {
        return None;
    }
    Some(SpeechRequest {
        request_id: fields[0].parse().ok()?,
        speaker: fields[1].parse().ok()?,
        voice_persona: fields[2].parse().ok()?,
        text: decode_state_text(fields[3])?,
        emotion: EmotionState {
            fear: parse_finite_f32(fields[4])?.clamp(0.0, 1.0),
            anger: parse_finite_f32(fields[5])?.clamp(0.0, 1.0),
            urgency: parse_finite_f32(fields[6])?.clamp(0.0, 1.0),
            trust: parse_finite_f32(fields[7])?.clamp(0.0, 1.0),
        },
        intent: decode_state_text(fields[8])?,
        urgency: parse_finite_f32(fields[9])?.clamp(0.0, 1.0),
        loudness: parse_finite_f32(fields[10])?.clamp(0.0, 1.5),
        language: fields[11].parse().ok()?,
        interruption_policy: decode_state_text(fields[12])?,
        location: parse_location(fields[13])?,
        priority: parse_speech_priority(fields[14])?,
        mode: parse_speech_mode(fields[15])?,
        max_latency_ms: fields[16].parse().ok()?,
        safety_context: VoiceSafetyContext {
            allow_generated_voice: fields[17].parse().ok()?,
            content_review_required: fields[18].parse().ok()?,
            licensed_persona_required: fields[19].parse().ok()?,
        },
    })
}

fn encode_location(location: Option<Vec3>) -> String {
    location
        .map(|location| format!("{},{},{}", location.x, location.y, location.z))
        .unwrap_or_else(|| "none".to_string())
}

fn parse_location(value: &str) -> Option<Option<Vec3>> {
    if value == "none" {
        return Some(None);
    }
    let fields = value.split(',').collect::<Vec<_>>();
    if fields.len() != 3 {
        return None;
    }
    Some(Some(Vec3::new(
        parse_finite_f32(fields[0])?,
        parse_finite_f32(fields[1])?,
        parse_finite_f32(fields[2])?,
    )))
}

fn speech_priority_label(priority: SpeechPriority) -> &'static str {
    match priority {
        SpeechPriority::Background => "background",
        SpeechPriority::Normal => "normal",
        SpeechPriority::Important => "important",
        SpeechPriority::Critical => "critical",
    }
}

fn parse_speech_priority(value: &str) -> Option<SpeechPriority> {
    match value {
        "background" => Some(SpeechPriority::Background),
        "normal" => Some(SpeechPriority::Normal),
        "important" => Some(SpeechPriority::Important),
        "critical" => Some(SpeechPriority::Critical),
        _ => None,
    }
}

fn speech_mode_label(mode: SpeechGenerationMode) -> &'static str {
    match mode {
        SpeechGenerationMode::PreAuthored => "preauthored",
        SpeechGenerationMode::CachedGenerated => "cached_generated",
        SpeechGenerationMode::OnDemandGenerated => "on_demand_generated",
        SpeechGenerationMode::CrowdChatter => "crowd_chatter",
        SpeechGenerationMode::RadioComms => "radio_comms",
    }
}

fn parse_speech_mode(value: &str) -> Option<SpeechGenerationMode> {
    match value {
        "preauthored" => Some(SpeechGenerationMode::PreAuthored),
        "cached_generated" => Some(SpeechGenerationMode::CachedGenerated),
        "on_demand_generated" => Some(SpeechGenerationMode::OnDemandGenerated),
        "crowd_chatter" => Some(SpeechGenerationMode::CrowdChatter),
        "radio_comms" => Some(SpeechGenerationMode::RadioComms),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ashfall_core::world::{CommandSink, EntityTemplate, PhysicalBody, Renderable, WorldState};

    #[test]
    fn rodio_preview_tone_uses_audio_source_library() {
        let preview = rodio_preview_tone(440.0, 0.05, 0.25);

        assert_eq!(preview.sample_rate_hz, 48_000);
        assert_eq!(preview.channels, 1);
        assert_eq!(preview.preview_samples.len(), 32);
        assert!(
            preview
                .preview_samples
                .iter()
                .any(|sample| sample.abs() > 0.0)
        );
    }

    #[test]
    fn rodio_event_cue_maps_sound_events_to_material_aware_preview() {
        let loud = sound_event(50, AudioEventKind::GlassShatter, Some(2), Some(1), 0.95);
        let quiet = sound_event(51, AudioEventKind::GlassShatter, Some(2), Some(1), 0.15);

        let loud_cue =
            rodio_event_cue_for_world_event(&loud).expect("glass sound should produce cue");
        let quiet_cue =
            rodio_event_cue_for_world_event(&quiet).expect("quiet glass sound should produce cue");
        let preview = loud_cue.preview();

        assert_eq!(loud_cue.label, "GlassShatter");
        assert!(loud_cue.frequency_hz > 1_000.0);
        assert!(loud_cue.gain > quiet_cue.gain);
        assert_eq!(preview.preview_samples.len(), 32);
    }

    #[test]
    fn rodio_event_cue_maps_dialogue_and_security_events() {
        let dialogue = WorldEvent {
            event_id: 52,
            tick: 1,
            location_meters: Vec3::new(5.0, 0.0, 0.0),
            actors: vec![3],
            kind: WorldEventKind::DialogueEmitted {
                speaker: 3,
                target: Some(1),
                text: "I saw that.".to_string(),
                emotion: EmotionState::alarmed(),
                voice_persona: Some(300),
            },
            physical_evidence: Vec::new(),
            narrative_tags: vec!["dialogue".to_string()],
        };
        let security = WorldEvent {
            event_id: 53,
            tick: 1,
            location_meters: Vec3::new(0.0, 0.0, 0.0),
            actors: vec![1],
            kind: WorldEventKind::SecurityAlertRaised {
                faction: 700,
                source_event: 52,
                threat: 1,
                severity: 0.72,
            },
            physical_evidence: Vec::new(),
            narrative_tags: vec!["security".to_string()],
        };
        let blocked = WorldEvent {
            event_id: 54,
            tick: 1,
            location_meters: Vec3::new(3.5, 0.0, 0.0),
            actors: vec![3],
            kind: WorldEventKind::NavigationMoveBlocked { entity: 3 },
            physical_evidence: vec!["navigation_blocked".to_string()],
            narrative_tags: vec!["navigation".to_string()],
        };

        let dialogue_cue =
            rodio_event_cue_for_world_event(&dialogue).expect("dialogue should produce voice cue");
        let security_cue = rodio_event_cue_for_world_event(&security)
            .expect("security alert should produce alert cue");
        let blocked_cue = rodio_event_cue_for_world_event(&blocked)
            .expect("blocked navigation should produce a cue");

        assert_eq!(dialogue_cue.label, "voice cue");
        assert_eq!(security_cue.label, "security alert");
        assert_eq!(blocked_cue.label, "blocked step");
        assert!(security_cue.frequency_hz > dialogue_cue.frequency_hz);
        assert!(blocked_cue.frequency_hz < dialogue_cue.frequency_hz);
    }

    fn dialogue_request(text: &str) -> SpeechRequest {
        speech_request_from_dialogue_event(
            10,
            3,
            text.to_string(),
            EmotionState::alarmed(),
            Some(300),
            Vec3::new(5.0, 0.0, 0.0),
        )
    }

    fn dialogue_frame(
        event_id: WorldEventId,
        quality_tier: QualityTier,
        text: &str,
    ) -> FrameContext {
        let world = WorldState::default();
        let tick = event_id as u64;
        let sim_time = SimTime::new(tick as f64 / 60.0, tick);
        FrameContext {
            frame_id: tick,
            sim_time,
            dt_seconds: 1.0 / 60.0,
            quality_tier,
            snapshot: world.snapshot(tick, sim_time),
            recent_events: vec![WorldEvent {
                event_id,
                tick,
                location_meters: Vec3::new(5.0, 0.0, 0.0),
                actors: vec![3],
                kind: WorldEventKind::DialogueEmitted {
                    speaker: 3,
                    target: None,
                    text: text.to_string(),
                    emotion: EmotionState::alarmed(),
                    voice_persona: Some(300),
                },
                physical_evidence: Vec::new(),
                narrative_tags: vec!["dialogue".to_string()],
            }],
            forces: Vec::new(),
        }
    }

    fn performance_capture_clip(request: &SpeechRequest) -> PerformanceCaptureClip {
        let persona = persona_for_voice(request.voice_persona);
        let duration_seconds = 1.4;
        PerformanceCaptureClip {
            clip_id: 90_001,
            speaker: request.speaker,
            voice_persona: request.voice_persona,
            source: PerformanceCaptureSource::ActorSession,
            provenance: PerformanceCaptureProvenance {
                actor_or_source_label: "actor:mara-session".to_string(),
                session_label: "capture-session-07".to_string(),
                rights_verified: true,
                consent_verified: true,
                retargeting_allowed: true,
                notes: "approved close-up performance capture".to_string(),
            },
            timecode: PerformanceCaptureTimecode {
                start_seconds: 12.0,
                frame_rate: 60.0,
            },
            audio_track: AudioClipHandle(70_001),
            duration_seconds,
            phonemes: vec![
                PhonemeTiming {
                    phoneme: "AH".to_string(),
                    start_seconds: 0.0,
                    end_seconds: 0.22,
                    confidence: 0.96,
                },
                PhonemeTiming {
                    phoneme: "S".to_string(),
                    start_seconds: 0.22,
                    end_seconds: 0.48,
                    confidence: 0.95,
                },
                PhonemeTiming {
                    phoneme: "M".to_string(),
                    start_seconds: 0.48,
                    end_seconds: 0.86,
                    confidence: 0.94,
                },
                PhonemeTiming {
                    phoneme: "EH".to_string(),
                    start_seconds: 0.86,
                    end_seconds: 1.4,
                    confidence: 0.93,
                },
            ],
            visemes: vec![
                VisemeTiming {
                    viseme: "open".to_string(),
                    start_seconds: 0.0,
                    end_seconds: 0.36,
                    weight: 0.92,
                },
                VisemeTiming {
                    viseme: "narrow".to_string(),
                    start_seconds: 0.36,
                    end_seconds: 0.82,
                    weight: 0.86,
                },
                VisemeTiming {
                    viseme: "closed".to_string(),
                    start_seconds: 0.82,
                    end_seconds: 1.4,
                    weight: 0.78,
                },
            ],
            emotion_curve: vec![
                (0.0, request.emotion.clone()),
                (duration_seconds, request.emotion.clone()),
            ],
            prosody: build_prosody(request, &persona, duration_seconds, 0.35),
            face_curves: vec![
                PerformanceCurveSample {
                    channel: PerformanceCurveChannel::JawOpen,
                    time_seconds: 0.0,
                    value: 0.12,
                    confidence: 0.94,
                },
                PerformanceCurveSample {
                    channel: PerformanceCurveChannel::LipRound,
                    time_seconds: 0.42,
                    value: 0.48,
                    confidence: 0.92,
                },
                PerformanceCurveSample {
                    channel: PerformanceCurveChannel::BrowRaise,
                    time_seconds: 0.9,
                    value: 0.32,
                    confidence: 0.9,
                },
            ],
            body_curves: vec![
                PerformanceCurveSample {
                    channel: PerformanceCurveChannel::ChestBreath,
                    time_seconds: 0.0,
                    value: 0.2,
                    confidence: 0.86,
                },
                PerformanceCurveSample {
                    channel: PerformanceCurveChannel::HeadPitch,
                    time_seconds: 0.7,
                    value: -0.08,
                    confidence: 0.82,
                },
            ],
            calibration: PerformanceCaptureCalibration {
                audio_sample_rate_hz: 48_000,
                video_frame_rate: 60.0,
                face_solve_confidence: 0.91,
                body_solve_confidence: 0.78,
                audio_video_offset_seconds: 0.018,
            },
            target_rig: PerformanceRigMapping {
                target_rig_label: "ashfall_hero_human_v1".to_string(),
                viseme_set_label: "ashfall_viseme_v1".to_string(),
                mapped_face_channels: vec![
                    PerformanceCurveChannel::JawOpen,
                    PerformanceCurveChannel::LipRound,
                    PerformanceCurveChannel::LipSpread,
                    PerformanceCurveChannel::BrowRaise,
                ],
                mapped_body_channels: vec![
                    PerformanceCurveChannel::ChestBreath,
                    PerformanceCurveChannel::HeadPitch,
                ],
            },
            validation_notes: vec!["operator reviewed lip-sync solve".to_string()],
        }
    }

    #[test]
    fn speech_generation_returns_lip_sync_and_uses_cache() {
        let mut module = VoiceAudioModule::default();
        let request = dialogue_request("I saw that. Security will hear about this.");

        let first = module.process_speech_request(&request);
        let mut repeated = request.clone();
        repeated.request_id = 11;
        let second = module.process_speech_request(&repeated);

        assert!(first.validation_report.passed);
        assert_eq!(first.cache_status, SpeechCacheStatus::Generated);
        assert_eq!(second.cache_status, SpeechCacheStatus::CacheHit);
        assert_eq!(first.audio_clip, second.audio_clip);
        assert!(!first.phonemes.is_empty());
        assert!(!first.visemes.is_empty());
        assert!(first.visemes.iter().all(|viseme| viseme.weight > 0.0));
        assert!(first.prosody.frames.len() >= 2);
        assert_eq!(module.cache_len(), 1);
    }

    #[test]
    fn performance_capture_maps_to_speech_result_with_lip_sync_tracks() {
        let request = dialogue_request("I saw that. Security will hear about this.");
        let persona = persona_for_voice(request.voice_persona);
        let clip = performance_capture_clip(&request);

        let capture_report = validate_performance_capture_clip(&request, &clip);
        let result = speech_result_from_performance_capture(&request, &persona, &clip);

        assert!(capture_report.passed);
        assert!(result.validation_report.passed);
        assert_eq!(result.cache_status, SpeechCacheStatus::CapturedPerformance);
        assert_eq!(result.audio_clip, clip.audio_track);
        assert_eq!(result.phonemes, clip.phonemes);
        assert_eq!(result.visemes, clip.visemes);
        assert_eq!(result.emotion_curve, clip.emotion_curve);
        assert_eq!(result.prosody, clip.prosody);
        assert_eq!(result.duration_seconds, clip.duration_seconds);
        assert!(result.generation_cost.memory_bytes > 64 * 1024);

        let comfort = evaluate_speech_comfort(&result, &persona);
        assert!(comfort.passed);
        assert!(comfort.average_phoneme_confidence > 0.9);
    }

    #[test]
    fn performance_capture_rejects_unlicensed_or_misaligned_data_and_falls_back() {
        let request = dialogue_request("I saw that. Security will hear about this.");
        let persona = persona_for_voice(request.voice_persona);
        let mut clip = performance_capture_clip(&request);
        clip.provenance.rights_verified = false;
        clip.provenance.retargeting_allowed = false;
        clip.calibration.audio_video_offset_seconds = 0.24;
        clip.calibration.face_solve_confidence = 0.42;
        clip.target_rig.mapped_face_channels.clear();
        clip.visemes[1].start_seconds = 0.1;

        let capture_report = validate_performance_capture_clip(&request, &clip);
        let result = speech_result_from_performance_capture(&request, &persona, &clip);

        assert!(!capture_report.passed);
        assert_eq!(result.cache_status, SpeechCacheStatus::Fallback);
        assert!(!result.validation_report.passed);
        for expected in [
            "capture_provenance_unverified",
            "capture_retargeting_disallowed",
            "capture_sync_error",
            "capture_low_face_solve_confidence",
            "capture_incomplete_face_rig_mapping",
            "capture_invalid_viseme_timing",
        ] {
            assert!(
                result
                    .validation_report
                    .issues
                    .iter()
                    .any(|issue| issue.code == expected),
                "{expected} should be reported"
            );
        }
    }

    #[test]
    fn speech_comfort_scores_generated_lines_and_conversation_projection() {
        let mut module = VoiceAudioModule::default();
        let request = dialogue_request("I saw that. Security will hear about this.");

        let first = module.process_speech_request(&request);
        let mut repeated = request.clone();
        repeated.request_id = 11;
        let second = module.process_speech_request(&repeated);

        let persona = persona_for_voice(first.voice_persona);
        let line_report = evaluate_speech_comfort(&first, &persona);
        assert!(line_report.passed);
        assert!(line_report.score > 0.85);
        assert!(line_report.phonemes_per_second > 2.0);
        assert!(line_report.max_pitch_jump_hz < 48.0);

        let conversation = evaluate_conversation_comfort([&first, &second]);
        assert!(conversation.passed);
        assert_eq!(conversation.line_count, 2);
        assert_eq!(conversation.unique_voice_persona_count, 1);
        assert_eq!(conversation.cache_hit_count, 1);
        assert!(conversation.projected_ten_minute_score > 0.8);
    }

    #[test]
    fn speech_comfort_flags_fallback_low_confidence_and_pitch_artifacts() {
        let request = dialogue_request("Unsafe clone");
        let persona = persona_for_voice(request.voice_persona);
        let mut result = synthesize_speech(&request);
        result.cache_status = SpeechCacheStatus::Fallback;
        for phoneme in &mut result.phonemes {
            phoneme.confidence = 0.42;
        }
        if let Some(frame) = result.prosody.frames.get_mut(1) {
            frame.pitch_hz = persona.pitch_range.max_hz + 120.0;
            frame.energy = 1.1;
            frame.breathiness = 0.9;
        }

        let report = evaluate_speech_comfort(&result, &persona);

        assert!(!report.passed);
        assert!(report.score < 0.5);
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == "fallback_voice_used")
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == "low_phoneme_confidence")
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == "invalid_prosody_frame")
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == "energy_clipped")
        );
    }

    #[test]
    fn voice_quality_tiers_change_speech_fidelity_and_cost() {
        let request = dialogue_request("Keep your head down while I call the patrol.");
        let background =
            apply_voice_quality_to_request(request.clone(), QualityTier::BackgroundApproximation);
        let hero =
            apply_voice_quality_to_request(request.clone(), QualityTier::HeroHighFidelityRuntime);

        assert_eq!(background.priority, SpeechPriority::Background);
        assert_eq!(background.mode, SpeechGenerationMode::CrowdChatter);
        assert_eq!(hero.priority, SpeechPriority::Important);
        assert_eq!(hero.mode, SpeechGenerationMode::OnDemandGenerated);

        let normal_result = synthesize_speech(&request);
        let background_result = synthesize_speech(&background);
        let hero_result = synthesize_speech(&hero);

        assert!(background_result.phonemes.len() < normal_result.phonemes.len());
        assert!(background_result.visemes.len() < normal_result.visemes.len());
        assert!(
            background_result.generation_cost.cpu_milliseconds
                < normal_result.generation_cost.cpu_milliseconds
        );
        assert!(hero_result.phonemes.len() > normal_result.phonemes.len());
        assert!(hero_result.visemes.len() > normal_result.visemes.len());
        assert!(
            hero_result.generation_cost.cpu_milliseconds
                > normal_result.generation_cost.cpu_milliseconds
        );
    }

    #[test]
    fn dialogue_tick_uses_frame_quality_and_keeps_cache_tiers_separate() {
        let mut module = VoiceAudioModule::default();
        let text = "Keep your head down while I call the patrol.";
        let mut sink = CommandSink::default();

        module.tick(
            &dialogue_frame(41, QualityTier::BackgroundApproximation, text),
            &mut sink,
        );
        let background = module
            .speech_results
            .get(&41)
            .expect("background speech should be synthesized")
            .clone();

        module.tick(
            &dialogue_frame(42, QualityTier::NormalRuntime, text),
            &mut sink,
        );
        let normal = module
            .speech_results
            .get(&42)
            .expect("normal speech should be synthesized");

        assert!(background.phonemes.len() < normal.phonemes.len());
        assert!(
            background.generation_cost.cpu_milliseconds < normal.generation_cost.cpu_milliseconds
        );
        assert_ne!(background.audio_clip, normal.audio_clip);
        assert_eq!(module.cache_len(), 2);
        assert!(sink.events.iter().any(|event| {
            matches!(
                event.kind,
                WorldEventKind::SpeechSynthesized {
                    speaker: 3,
                    phoneme_count,
                    ..
                } if phoneme_count == background.phonemes.len()
            )
        }));
    }

    #[test]
    fn dialogue_tick_emits_distinct_events_for_multiple_same_tick_lines() {
        let mut module = VoiceAudioModule::default();
        let mut frame = dialogue_frame(41, QualityTier::NormalRuntime, "I saw that.");
        frame.recent_events.push(WorldEvent {
            event_id: 42,
            tick: frame.sim_time.tick,
            location_meters: Vec3::new(5.0, 0.0, 0.0),
            actors: vec![3],
            kind: WorldEventKind::DialogueEmitted {
                speaker: 3,
                target: None,
                text: "Gas! Get clear!".to_string(),
                emotion: EmotionState::alarmed(),
                voice_persona: Some(300),
            },
            physical_evidence: Vec::new(),
            narrative_tags: vec!["dialogue".to_string()],
        });
        let mut sink = CommandSink::default();

        module.tick(&frame, &mut sink);

        let voice_line_ids = sink
            .events
            .iter()
            .filter_map(|event| {
                matches!(event.kind, WorldEventKind::VoiceLineSpoken { speaker: 3 })
                    .then_some(event.event_id)
            })
            .collect::<Vec<_>>();
        let speech_ids = sink
            .events
            .iter()
            .filter_map(|event| {
                matches!(
                    event.kind,
                    WorldEventKind::SpeechSynthesized { speaker: 3, .. }
                )
                .then_some(event.event_id)
            })
            .collect::<Vec<_>>();

        assert_eq!(voice_line_ids.len(), 2);
        assert_eq!(speech_ids.len(), 2);
        assert_ne!(voice_line_ids[0], voice_line_ids[1]);
        assert_ne!(speech_ids[0], speech_ids[1]);
        assert_eq!(module.speech_results.len(), 2);
        assert_eq!(sink.commands.len(), 2);
    }

    #[test]
    fn long_low_latency_speech_uses_fallback_until_cache_job_completes() {
        let mut module = VoiceAudioModule::default();
        let mut request = dialogue_request(
            "I need to explain every detail of the alley surveillance route, the broken glass, \
             the bystander report, the backup patrol timing, and the evidence chain before \
             anyone makes a bad call in the next few seconds.",
        );
        request.mode = SpeechGenerationMode::OnDemandGenerated;
        request.max_latency_ms = 40;
        request.priority = SpeechPriority::Important;

        let fallback = module.process_speech_request(&request);
        assert_eq!(fallback.cache_status, SpeechCacheStatus::Fallback);
        assert!(fallback.duration_seconds < 1.0);
        assert_eq!(module.cache_len(), 0);
        assert_eq!(module.pending_generation_len(), 1);

        let mut repeated = request.clone();
        repeated.request_id = 11;
        let cached = module.process_speech_request(&repeated);
        assert_eq!(cached.cache_status, SpeechCacheStatus::CacheHit);
        assert_eq!(module.cache_len(), 1);
        assert_eq!(module.pending_generation_len(), 0);
        assert_eq!(cached.audio_clip, synthesize_speech(&request).audio_clip);
        assert!(cached.duration_seconds > fallback.duration_seconds);
    }

    #[test]
    fn voice_state_restores_spoken_events_and_speech_cache() {
        let mut module = VoiceAudioModule::default();
        let request = dialogue_request("I saw that. Security will hear about this.");
        let first = module.process_speech_request(&request);
        module.spoken_dialogue_events.insert(99);

        let state = module.save_state().expect("voice module should save state");
        assert!(state.entries.iter().any(|entry| entry == "spoken:99"));
        assert!(
            state
                .entries
                .iter()
                .any(|entry| entry.starts_with("cache_request:"))
        );

        let mut restored = VoiceAudioModule::default();
        restored.load_state(&state);
        assert_eq!(restored.cache_len(), 1);
        assert!(restored.spoken_dialogue_events.contains(&99));

        let mut repeated = request.clone();
        repeated.request_id = 12;
        let second = restored.process_speech_request(&repeated);
        assert_eq!(second.cache_status, SpeechCacheStatus::CacheHit);
        assert_eq!(first.audio_clip, second.audio_clip);
    }

    #[test]
    fn unverified_voice_persona_is_rejected_without_panicking() {
        let mut persona = mara_voice_persona();
        persona.provenance.rights_verified = false;
        let result = synthesize_speech_with_persona(&dialogue_request("Unsafe clone"), &persona);

        assert!(!result.validation_report.passed);
        assert_eq!(result.cache_status, SpeechCacheStatus::Fallback);
        assert!(result.validation_report.issues.iter().any(|issue| {
            issue.severity == SpeechValidationSeverity::Error
                && issue.code == "voice_provenance_unverified"
        }));
    }

    #[test]
    fn spatial_audio_uses_material_and_distance() {
        let sound = spatialize_sound(SoundSpatializationInput {
            event_id: 7,
            kind: AudioEventKind::GlassShatter,
            source_entity: Some(2),
            material_id: Some(1),
            location: Vec3::new(2.0, 0.0, 0.0),
            listener: Vec3::ZERO,
            intensity: 0.95,
            radius_meters: 28.0,
            occlusion_hint: 0.1,
        });

        assert_eq!(sound.bus, AudioBusKind::Impact);
        assert_eq!(sound.priority, AudioPriority::High);
        assert_eq!(sound.material_profile.family, SoundFamily::Glass);
        assert!(sound.effective_intensity > 0.9);
        assert!(sound.spatial.reverb_send > 0.0);
    }

    #[test]
    fn material_descriptor_and_state_shape_foley_profile() {
        let material = wet_concrete_material(42);
        let profile = material_sound_profile_from_descriptor(
            AudioEventKind::Footstep,
            Some(material.id),
            Some(&material),
            Some(MaterialState {
                moisture: 0.9,
                soot: 0.2,
                ..MaterialState::default()
            }),
        );

        assert_eq!(profile.family, SoundFamily::Concrete);
        assert!(profile.absorption > material.acoustic.absorption);
        assert_eq!(profile.wet_modifier, 0.9);
        assert!(profile.break_brightness < material.acoustic.impact_brightness);
    }

    #[test]
    fn district_soundscape_reacts_to_power_gas_and_alerts() {
        let events = [
            world_event(
                700,
                WorldEventKind::PowerTransformerOverheated { entity: 55 },
                vec!["power".to_string()],
            ),
            world_event(
                701,
                WorldEventKind::ToxicGasReleased,
                vec!["gas".to_string()],
            ),
            world_event(
                702,
                WorldEventKind::SecurityAlertRaised {
                    faction: 8,
                    source_event: 701,
                    threat: 1,
                    severity: 0.82,
                },
                vec!["security".to_string()],
            ),
        ];

        let state = district_audio_state_from_events(77, events.iter());
        assert_eq!(state.weather, SoundscapeWeatherState::GasHazard);
        assert!(state.power_level < 0.4);
        assert!(state.alertness > 0.75);
        assert!(state.pollution_level > 0.8);
        assert_eq!(state.active_events.len(), 3);

        let soundscape = generate_district_soundscape(&state);
        assert!(soundscape.validation_report.passed);
        assert!(soundscape.tension_score >= 0.78);
        assert!(soundscape.music_ducking > 0.25);
        assert!(soundscape.adaptive_score.validation_report.passed);
        assert!(soundscape.adaptive_score.input.danger_level > 0.8);
        assert_eq!(
            soundscape.adaptive_score.input.gameplay_state,
            MusicGameplayState::Combat
        );
        assert_eq!(
            soundscape.adaptive_score.transition,
            MusicTransitionKind::SilenceDrop
        );
        assert!(
            soundscape
                .adaptive_score
                .stems
                .iter()
                .any(|stem| stem.kind == MusicStemKind::Percussion)
        );
        assert!(
            soundscape
                .adaptive_score
                .diegetic_sources
                .iter()
                .any(|source| source.radio_filtered)
        );
        assert!(
            soundscape
                .layers
                .iter()
                .any(|layer| layer.kind == SoundscapeLayerKind::EmergencyAlarm)
        );
        assert!(
            soundscape
                .layers
                .iter()
                .any(|layer| layer.kind == SoundscapeLayerKind::Ventilation)
        );
        assert!(
            soundscape
                .layers
                .iter()
                .any(|layer| layer.kind == SoundscapeLayerKind::PoliceScanner)
        );
    }

    #[test]
    fn adaptive_score_crossfades_social_radio_and_layers_tension() {
        let mut state = DistrictAudioState::baseline(12);
        state.crowd_density = 0.86;
        state.alertness = 0.28;
        state.crime_pressure = 0.22;
        state.active_events = vec![444, 445];

        let input = adaptive_score_input_from_district_audio_state(&state);
        assert_eq!(input.gameplay_state, MusicGameplayState::Social);
        assert!(input.story_thread_intensity > 0.35);

        let score = generate_adaptive_score(&input);

        assert!(score.validation_report.passed);
        assert_eq!(score.transition, MusicTransitionKind::Crossfade);
        assert!(score.radio_station.enabled);
        assert!(
            score
                .stems
                .iter()
                .any(|stem| stem.kind == MusicStemKind::Melody)
        );
        assert!(
            score
                .stems
                .iter()
                .any(|stem| stem.kind == MusicStemKind::RadioBed && stem.diegetic)
        );
        assert_eq!(score.diegetic_sources.len(), 1);
    }

    #[test]
    fn mixer_outputs_voice_impact_and_foley_buses() {
        let mut world = WorldState::default();
        world
            .spawn_entity_template(EntityTemplate {
                entity_id: Some(1),
                name: "player".to_string(),
                transform: Transform::at(Vec3::ZERO),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 80.0,
                    material_id: 4,
                    dynamic: true,
                    fragile: false,
                }),
                material_state: None,
                human: None,
                agent: None,
                tags: vec!["player".to_string()],
            })
            .expect("player should spawn");
        world
            .spawn_entity_template(EntityTemplate {
                entity_id: Some(2),
                name: "glass".to_string(),
                transform: Transform::at(Vec3::new(2.0, 0.0, 0.0)),
                renderable: Some(Renderable {
                    mesh: MeshAssetHandle(2_000),
                    material: 1,
                    visible: true,
                    fracture_replacement_mesh: None,
                }),
                physical_body: None,
                material_state: None,
                human: None,
                agent: None,
                tags: Vec::new(),
            })
            .expect("glass should spawn");

        let frame = FrameContext {
            frame_id: 1,
            sim_time: SimTime::new(1.0 / 60.0, 1),
            dt_seconds: 1.0 / 60.0,
            quality_tier: QualityTier::NormalRuntime,
            snapshot: world.snapshot(1, SimTime::new(1.0 / 60.0, 1)),
            recent_events: vec![
                sound_event(1, AudioEventKind::GlassShatter, Some(2), Some(1), 0.95),
                sound_event(2, AudioEventKind::WaterSplash, Some(2), Some(2), 0.35),
                sound_event(3, AudioEventKind::VoiceSpeech, Some(3), None, 0.65),
            ],
            forces: Vec::new(),
        };
        let mut mixer = AudioMixerModule::default();
        let mut sink = CommandSink::default();

        mixer.tick(&frame, &mut sink);

        let output = mixer.last_output().expect("audio frame should mix");
        assert_eq!(output.sample_rate_hz, 48_000);
        assert_eq!(output.active_sounds.len(), 3);
        assert!(output.peak_intensity > 0.9);
        assert!(
            output
                .bus_outputs
                .iter()
                .any(|bus| bus.bus == AudioBusKind::Impact)
        );
        assert!(
            output
                .bus_outputs
                .iter()
                .any(|bus| bus.bus == AudioBusKind::Foley)
        );
        assert!(
            output
                .bus_outputs
                .iter()
                .any(|bus| bus.bus == AudioBusKind::Voice)
        );
        assert!(
            output
                .bus_outputs
                .iter()
                .any(|bus| bus.bus == AudioBusKind::Music)
        );
        assert!(sink.events.iter().any(|event| {
            matches!(
                event.kind,
                WorldEventKind::AudioFrameMixed {
                    active_sound_count: 3,
                    material_aware_sound_count: 2,
                    peak_intensity,
                    ..
                } if peak_intensity > 0.9
            )
        }));
    }

    #[test]
    fn mixer_outputs_state_driven_soundscape_without_one_shot_sound() {
        let mut world = WorldState::default();
        world
            .spawn_entity_template(EntityTemplate {
                entity_id: Some(1),
                name: "player".to_string(),
                transform: Transform::at(Vec3::ZERO),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 80.0,
                    material_id: 4,
                    dynamic: true,
                    fragile: false,
                }),
                material_state: None,
                human: None,
                agent: None,
                tags: vec!["player".to_string()],
            })
            .expect("player should spawn");

        let frame = FrameContext {
            frame_id: 3,
            sim_time: SimTime::new(3.0 / 60.0, 3),
            dt_seconds: 1.0 / 60.0,
            quality_tier: QualityTier::NormalRuntime,
            snapshot: world.snapshot(3, SimTime::new(3.0 / 60.0, 3)),
            recent_events: vec![
                world_event(
                    710,
                    WorldEventKind::PowerTransformerOverheated { entity: 55 },
                    vec!["power".to_string()],
                ),
                world_event(
                    711,
                    WorldEventKind::SecurityAlertRaised {
                        faction: 8,
                        source_event: 710,
                        threat: 1,
                        severity: 0.74,
                    },
                    vec!["security".to_string()],
                ),
            ],
            forces: Vec::new(),
        };
        let mut mixer = AudioMixerModule::default();
        let mut sink = CommandSink::default();

        mixer.tick(&frame, &mut sink);

        let output = mixer
            .last_output()
            .expect("soundscape-only audio frame should mix");
        assert!(output.active_sounds.is_empty());
        assert!(
            output
                .district_soundscape
                .layers
                .iter()
                .any(|layer| layer.kind == SoundscapeLayerKind::EmergencyAlarm)
        );
        assert!(
            output
                .bus_outputs
                .iter()
                .any(|bus| bus.bus == AudioBusKind::Mechanical)
        );
        assert!(
            output
                .bus_outputs
                .iter()
                .any(|bus| bus.bus == AudioBusKind::Music)
        );
        assert!(output.peak_intensity > 0.4);
        assert!(sink.events.iter().any(|event| {
            matches!(
                event.kind,
                WorldEventKind::AudioFrameMixed {
                    active_sound_count: 0,
                    material_aware_sound_count: 0,
                    peak_intensity,
                    ..
                } if peak_intensity > 0.4
            )
        }));

        let services = ashfall_core::gpu::GpuServices::new_vulkan();
        let mut graph = services.begin_frame(65);
        mixer.schedule_gpu(&mut graph);
        assert!(
            graph
                .passes()
                .iter()
                .any(|pass| pass.name == "audio_soundscape_layers")
        );
        let report = services.submit(graph);
        assert!(report.validation.passed);
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(65)
                && usage.label == "audio district soundscape layer buffer"
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "audio_soundscape_layers")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "audio_soundscape_layers"
                && pipeline.shader_key == "audio/soundscape_layers.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "DISTRICT_SOUNDSCAPE")
        }));
    }

    #[test]
    fn mixer_schedules_gpu_audio_graph_for_active_sounds() {
        let mut world = WorldState::default();
        world
            .spawn_entity_template(EntityTemplate {
                entity_id: Some(1),
                name: "player".to_string(),
                transform: Transform::at(Vec3::ZERO),
                renderable: None,
                physical_body: Some(PhysicalBody {
                    mass_kg: 80.0,
                    material_id: 4,
                    dynamic: true,
                    fragile: false,
                }),
                material_state: None,
                human: None,
                agent: None,
                tags: vec!["player".to_string()],
            })
            .expect("player should spawn");
        let frame = FrameContext {
            frame_id: 4,
            sim_time: SimTime::new(4.0 / 60.0, 4),
            dt_seconds: 1.0 / 60.0,
            quality_tier: QualityTier::NormalRuntime,
            snapshot: world.snapshot(4, SimTime::new(4.0 / 60.0, 4)),
            recent_events: vec![
                sound_event(300, AudioEventKind::GlassShatter, Some(2), Some(1), 0.95),
                sound_event(301, AudioEventKind::WaterSplash, Some(2), Some(2), 0.35),
                sound_event(302, AudioEventKind::VoiceSpeech, Some(3), None, 0.65),
            ],
            forces: Vec::new(),
        };
        let mut mixer = AudioMixerModule::default();
        let mut sink = CommandSink::default();
        mixer.tick(&frame, &mut sink);
        assert!(
            mixer
                .last_output()
                .expect("audio frame should mix")
                .timing
                .gpu_milliseconds
                > 0.0
        );

        let services = ashfall_core::gpu::GpuServices::new_vulkan();
        let mut graph = services.begin_frame(65);
        mixer.schedule_gpu(&mut graph);

        for expected in [
            "audio_spatialization",
            "audio_occlusion_filter",
            "audio_reverb_convolution",
            "audio_soundscape_layers",
            "audio_bus_mixdown",
            "audio_final_mix",
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
            usage.owner == Some(65)
                && usage.label == "audio reverb tail buffer"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "audio_reverb_convolution")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(65)
                && usage.label == "audio final mix output buffer"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "audio_final_mix")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "audio_spatialization"
                && pipeline.shader_key == "audio/spatialization.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "SPATIAL_AUDIO")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "audio_reverb_convolution"
                && pipeline.shader_key == "audio/reverb_convolution.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "CONVOLUTION_REVERB")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "audio_soundscape_layers"
                && pipeline.shader_key == "audio/soundscape_layers.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "STATE_DRIVEN_AMBIENCE")
        }));
    }

    #[test]
    fn mixer_applies_alley_zone_reverb_and_world_occlusion() {
        let mut world = WorldState::default();
        world
            .spawn_entity_template(EntityTemplate {
                entity_id: Some(1),
                name: "player".to_string(),
                transform: Transform::at(Vec3::ZERO),
                renderable: None,
                physical_body: None,
                material_state: None,
                human: None,
                agent: None,
                tags: vec!["player".to_string()],
            })
            .expect("player should spawn");
        world
            .spawn_entity_template(EntityTemplate {
                entity_id: Some(10),
                name: "alley acoustic zone".to_string(),
                transform: Transform {
                    translation_meters: Vec3::new(2.0, 0.0, 0.0),
                    scale: Vec3::new(8.0, 3.0, 3.0),
                    ..Transform::default()
                },
                renderable: None,
                physical_body: None,
                material_state: None,
                human: None,
                agent: None,
                tags: vec!["audio_zone:alley".to_string()],
            })
            .expect("zone should spawn");
        world
            .spawn_entity_template(EntityTemplate {
                entity_id: Some(11),
                name: "concrete occluder".to_string(),
                transform: Transform {
                    translation_meters: Vec3::new(2.0, 0.0, 0.0),
                    scale: Vec3::new(0.8, 3.0, 3.0),
                    ..Transform::default()
                },
                renderable: None,
                physical_body: None,
                material_state: Some(MaterialState::default()),
                human: None,
                agent: None,
                tags: vec!["audio_occluder:wall".to_string()],
            })
            .expect("occluder should spawn");

        let frame = FrameContext {
            frame_id: 2,
            sim_time: SimTime::new(2.0 / 60.0, 2),
            dt_seconds: 1.0 / 60.0,
            quality_tier: QualityTier::NormalRuntime,
            snapshot: world.snapshot(2, SimTime::new(2.0 / 60.0, 2)),
            recent_events: vec![sound_event_at(
                99,
                AudioEventKind::GlassShatter,
                Some(12),
                Some(1),
                0.9,
                Vec3::new(4.0, 0.0, 0.0),
            )],
            forces: Vec::new(),
        };
        let scene = evaluate_acoustic_scene(&frame.snapshot, Vec3::new(4.0, 0.0, 0.0), Vec3::ZERO);
        assert_eq!(scene.environment.space, AcousticSpaceKind::NarrowAlley);
        assert_eq!(scene.zone_entity, Some(10));
        assert!(scene.world_occlusion > 0.3);
        assert_eq!(scene.occlusion_sources[0].occluder_entity, 11);

        let mut mixer = AudioMixerModule::default();
        let mut sink = CommandSink::default();
        mixer.tick(&frame, &mut sink);

        let output = mixer.last_output().expect("audio frame should mix");
        let sound = output
            .active_sounds
            .first()
            .expect("sound should be active");
        let outdoor = spatialize_sound(SoundSpatializationInput {
            event_id: 99,
            kind: AudioEventKind::GlassShatter,
            source_entity: Some(12),
            material_id: Some(1),
            location: Vec3::new(4.0, 0.0, 0.0),
            listener: Vec3::ZERO,
            intensity: 0.9,
            radius_meters: 28.0,
            occlusion_hint: 0.0,
        });

        assert_eq!(
            sound.acoustic_environment.space,
            AcousticSpaceKind::NarrowAlley
        );
        assert_eq!(sound.acoustic_zone_entity, Some(10));
        assert!(sound.spatial.occlusion > 0.3);
        assert!(sound.spatial.low_pass_hz < outdoor.spatial.low_pass_hz);
        assert!(sound.spatial.reverb_send > outdoor.spatial.reverb_send);
        assert!(sound.spatial.reverb_tail_seconds > 1.0);
        assert_eq!(sound.occlusion_sources[0].occluder_entity, 11);
    }

    #[test]
    fn dropped_sounds_report_original_event_ids() {
        let mut world = WorldState::default();
        world
            .spawn_entity_template(EntityTemplate {
                entity_id: Some(1),
                name: "player".to_string(),
                transform: Transform::at(Vec3::ZERO),
                renderable: None,
                physical_body: None,
                material_state: None,
                human: None,
                agent: None,
                tags: vec!["player".to_string()],
            })
            .expect("player should spawn");
        let frame = FrameContext {
            frame_id: 3,
            sim_time: SimTime::new(3.0 / 60.0, 3),
            dt_seconds: 1.0 / 60.0,
            quality_tier: QualityTier::NormalRuntime,
            snapshot: world.snapshot(3, SimTime::new(3.0 / 60.0, 3)),
            recent_events: vec![
                sound_event(200, AudioEventKind::VoiceSpeech, Some(2), None, 0.7),
                sound_event(201, AudioEventKind::NeonHum, Some(3), None, 0.2),
            ],
            forces: Vec::new(),
        };
        let mut mixer = AudioMixerModule {
            max_active_sounds: 1,
            ..AudioMixerModule::default()
        };
        let mut sink = CommandSink::default();

        mixer.tick(&frame, &mut sink);

        let output = mixer.last_output().expect("audio frame should mix");
        assert_eq!(output.active_sounds.len(), 1);
        assert_eq!(output.active_sounds[0].event_id, 200);
        assert_eq!(output.dropped_sounds.len(), 1);
        assert_eq!(output.dropped_sounds[0].event_id, 201);
    }

    fn world_event(
        event_id: WorldEventId,
        kind: WorldEventKind,
        narrative_tags: Vec<String>,
    ) -> WorldEvent {
        WorldEvent {
            event_id,
            tick: 1,
            location_meters: Vec3::new(2.0, 0.0, 0.0),
            actors: Vec::new(),
            kind,
            physical_evidence: Vec::new(),
            narrative_tags,
        }
    }

    fn sound_event(
        event_id: WorldEventId,
        kind: AudioEventKind,
        source_entity: Option<EntityId>,
        material_id: Option<MaterialId>,
        intensity: f32,
    ) -> WorldEvent {
        sound_event_at(
            event_id,
            kind,
            source_entity,
            material_id,
            intensity,
            Vec3::new(2.0, 0.0, 0.0),
        )
    }

    fn sound_event_at(
        event_id: WorldEventId,
        kind: AudioEventKind,
        source_entity: Option<EntityId>,
        material_id: Option<MaterialId>,
        intensity: f32,
        location: Vec3,
    ) -> WorldEvent {
        WorldEvent {
            event_id,
            tick: 1,
            location_meters: location,
            actors: source_entity.into_iter().collect(),
            kind: WorldEventKind::SoundEmitted {
                kind,
                source_entity,
                material_id,
                intensity,
                radius_meters: 28.0,
                occlusion_hint: 0.1,
            },
            physical_evidence: Vec::new(),
            narrative_tags: vec!["audio".to_string()],
        }
    }

    fn wet_concrete_material(id: MaterialId) -> MaterialDescriptor {
        MaterialDescriptor {
            id,
            name: "wet cracked concrete".to_string(),
            visual: VisualMaterial {
                base_color_linear: [0.2, 0.2, 0.18, 1.0],
                roughness: 0.8,
                metallic: 0.0,
                transmission: 0.0,
                subsurface: 0.0,
                emission_linear: [0.0, 0.0, 0.0],
                anisotropy: 0.02,
                clearcoat: 0.08,
                normal_displacement_strength: 0.42,
                layer_count: 3,
                transparency: 0.0,
            },
            physical: PhysicalMaterial {
                density_kg_per_m3: 2_400.0,
                young_modulus: 28_000_000_000.0,
                poisson_ratio: 0.2,
                yield_stress: 30_000_000.0,
                fracture_toughness: 0.8,
                hardness: 0.7,
                viscosity: 0.0,
                surface_tension: 0.0,
                restitution: 0.2,
                friction_static: 0.9,
                friction_dynamic: 0.7,
            },
            acoustic: AcousticMaterial {
                impact_brightness: 0.45,
                resonance: 0.18,
                absorption: 0.48,
                wetness_muffle: 0.65,
            },
            thermal: ThermalMaterial {
                heat_capacity: 880.0,
                conductivity: 1.4,
                ignition_temperature: 1_200.0,
            },
            electrical: ElectricalMaterial {
                conductivity: 0.02,
                dielectric_strength: 3_000_000.0,
            },
            procedural_source: None,
        }
    }
}
