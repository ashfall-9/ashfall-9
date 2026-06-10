#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($name:ident, $inner:ty) => {
        #[repr(transparent)]
        #[derive(
            Clone,
            Copy,
            Debug,
            Default,
            Hash,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Serialize,
            Deserialize,
        )]
        pub struct $name(pub $inner);
    };
}

id_type!(EntityId, u64);
id_type!(AssetId, u128);
id_type!(MaterialId, u64);
id_type!(GeometryId, u64);
id_type!(SimObjectId, u64);
id_type!(CharacterId, u64);
id_type!(SceneSnapshotId, u64);
id_type!(PhysicsSnapshotId, u64);
id_type!(CharacterSnapshotId, u64);
id_type!(RenderSceneId, u64);
id_type!(RenderFrameId, u64);
id_type!(RenderCaptureId, u64);
id_type!(PresentationSurfaceId, u64);
id_type!(SwapchainId, u64);
id_type!(SubmissionId, u64);
id_type!(TimelineValue, u64);
id_type!(DescriptorEpoch, u64);

#[repr(transparent)]
#[derive(
    Clone, Copy, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct ResourceGeneration(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct InterfaceVersion {
    pub name: &'static str,
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl InterfaceVersion {
    pub const fn new(name: &'static str, major: u16, minor: u16, patch: u16) -> Self {
        Self {
            name,
            major,
            minor,
            patch,
        }
    }
}

pub const ENGINE_CORE_VERSION: InterfaceVersion = InterfaceVersion::new("engine_core", 0, 1, 0);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraState {
    pub position_m: [f32; 3],
    pub forward: [f32; 3],
    pub up: [f32; 3],
    pub fov_y_degrees: f32,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            position_m: [0.0, 0.0, 0.0],
            forward: [0.0, 0.0, 1.0],
            up: [0.0, 1.0, 0.0],
            fov_y_degrees: 60.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraExposure {
    pub fixed: bool,
    pub exposure_value: f32,
    pub white_balance_kelvin: f32,
}

impl Default for CameraExposure {
    fn default() -> Self {
        Self {
            fixed: true,
            exposure_value: 13.5,
            white_balance_kelvin: 6500.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeatherState {
    pub cloud_coverage: f32,
    pub haze: f32,
}

impl Default for WeatherState {
    fn default() -> Self {
        Self {
            cloud_coverage: 0.0,
            haze: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderQualityProfile {
    #[default]
    Smoke,
    Low,
    Medium,
    High,
    Certification,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrameSnapshot {
    pub frame_index: u64,
    pub sim_time_s: f64,
    pub dt_s: f32,
    pub camera: CameraState,
    pub exposure: CameraExposure,
    pub weather: WeatherState,
    pub scene_id: SceneSnapshotId,
    pub physics_id: PhysicsSnapshotId,
    pub character_id: CharacterSnapshotId,
    pub render_quality: RenderQualityProfile,
}

impl Default for FrameSnapshot {
    fn default() -> Self {
        Self {
            frame_index: 0,
            sim_time_s: 0.0,
            dt_s: 0.0,
            camera: CameraState::default(),
            exposure: CameraExposure::default(),
            weather: WeatherState::default(),
            scene_id: SceneSnapshotId(0),
            physics_id: PhysicsSnapshotId(0),
            character_id: CharacterSnapshotId(0),
            render_quality: RenderQualityProfile::Smoke,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PresentationState {
    Unavailable,
    Available,
    Minimized,
    ResizePending,
    SurfaceLost,
    DeviceLost,
}

pub trait PresentationHost {
    fn presentation_state(&self) -> PresentationState;
    fn surface_id(&self) -> Option<PresentationSurfaceId>;
    fn current_extent(&self) -> Option<[u32; 2]>;
    fn request_redraw(&self);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueueClass {
    Graphics,
    Compute,
    Transfer,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuResourceKind {
    Buffer,
    Image,
    ImageView,
    Sampler,
    AccelerationStructure,
    DescriptorTable,
    Pipeline,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuResourceId {
    pub index: u32,
    pub generation: ResourceGeneration,
    pub kind: GpuResourceKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceRetirement {
    pub resource: GpuResourceId,
    pub retire_after: TimelineValue,
    pub descriptor_epoch: Option<DescriptorEpoch>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RayTracingMode {
    Unsupported,
    RayQueryOnly,
    FullPipeline,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RendererGpuTier {
    Unavailable,
    Vulkan13Portable,
    DescriptorIndexing,
    RayQuery,
    FullRayTracing,
    Vulkan14Preferred,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GpuFeatureReport {
    pub dynamic_rendering: bool,
    pub synchronization2: bool,
    pub timeline_semaphore: bool,
    pub descriptor_indexing: bool,
    pub descriptor_buffer: bool,
    pub buffer_device_address: bool,
    pub ray_query: bool,
    pub ray_tracing_pipeline: bool,
    pub acceleration_structure: bool,
}

impl GpuFeatureReport {
    pub const fn unavailable() -> Self {
        Self {
            dynamic_rendering: false,
            synchronization2: false,
            timeline_semaphore: false,
            descriptor_indexing: false,
            descriptor_buffer: false,
            buffer_device_address: false,
            ray_query: false,
            ray_tracing_pipeline: false,
            acceleration_structure: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryHeapReport {
    pub size_mb: u64,
    pub device_local: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GpuCapabilities {
    pub schema_version: String,
    pub device_name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub api_version: String,
    pub driver_version: String,
    pub selected_tier: RendererGpuTier,
    pub features: GpuFeatureReport,
    pub memory_heaps: Vec<MemoryHeapReport>,
    pub max_image_dimension_2d: u32,
    pub max_storage_buffer_range: u64,
    pub timestamp_period_ns: f32,
}

impl GpuCapabilities {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            schema_version: "gpu_info.v1".to_string(),
            device_name: format!("unavailable: {}", reason.into()),
            vendor_id: 0,
            device_id: 0,
            api_version: "unknown".to_string(),
            driver_version: "unknown".to_string(),
            selected_tier: RendererGpuTier::Unavailable,
            features: GpuFeatureReport::unavailable(),
            memory_heaps: Vec::new(),
            max_image_dimension_2d: 0,
            max_storage_buffer_range: 0,
            timestamp_period_ns: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComponentHealth {
    Ok,
    Degraded { reason: String },
    Disabled { reason: String },
    Failed { reason: String },
}

impl ComponentHealth {
    pub const fn ok() -> Self {
        Self::Ok
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ModuleTelemetry {
    pub module_name: String,
    pub cpu_ms: f32,
    pub gpu_ms: Option<f32>,
    pub queued_jobs: u32,
    pub failed_jobs: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleRegistry {
    module_names: Vec<String>,
}

impl ModuleRegistry {
    pub fn register(&mut self, name: impl Into<String>) {
        self.module_names.push(name.into());
    }

    pub fn module_names(&self) -> &[String] {
        &self.module_names
    }
}

#[derive(Debug, Default)]
pub struct InitContext {
    pub frame_index: u64,
}

#[derive(Debug, Default)]
pub struct UpdateContext {
    pub frame: FrameSnapshot,
    pub events: Vec<EngineEvent>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ModuleOutput {
    pub events: Vec<EngineEvent>,
    pub render_tasks: Vec<RenderTask>,
}

pub trait EngineModule: Send {
    fn name(&self) -> &'static str;
    fn interface_version(&self) -> InterfaceVersion;
    fn register(&mut self, registry: &mut ModuleRegistry) -> anyhow::Result<()>;
    fn initialize(&mut self, ctx: &mut InitContext) -> anyhow::Result<()>;
    fn begin_frame(&mut self, frame: &FrameSnapshot) -> anyhow::Result<()>;
    fn update(&mut self, ctx: &mut UpdateContext) -> anyhow::Result<ModuleOutput>;
    fn end_frame(&mut self, frame: &FrameSnapshot) -> anyhow::Result<()>;
    fn health(&self) -> ComponentHealth;
    fn telemetry(&self) -> ModuleTelemetry;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineEvent {
    AppCloseRequested {
        frame_index: u64,
    },
    WindowResized {
        frame_index: u64,
        extent: [u32; 2],
    },
    RenderCaptureReady {
        frame_index: u64,
        capture: RenderCaptureId,
    },
    Diagnostic {
        frame_index: u64,
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderTask {
    RenderMainView,
    RenderCapture { request_id: u64 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum JobStatus<T> {
    Queued,
    Running { progress: Option<f32> },
    Complete(T),
    Failed { message: String },
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct EngineRuntime {
    frame_index: u64,
    sim_time_s: f64,
    quality: RenderQualityProfile,
}

impl EngineRuntime {
    pub fn new(quality: RenderQualityProfile) -> Self {
        Self {
            frame_index: 0,
            sim_time_s: 0.0,
            quality,
        }
    }

    pub fn frame_index(&self) -> u64 {
        self.frame_index
    }

    pub fn advance_frame(&mut self, dt_s: f32) -> FrameSnapshot {
        let snapshot = FrameSnapshot {
            frame_index: self.frame_index,
            sim_time_s: self.sim_time_s,
            dt_s,
            render_quality: self.quality,
            ..FrameSnapshot::default()
        };
        self.frame_index += 1;
        self.sim_time_s += f64::from(dt_s);
        snapshot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_opaque_and_orderable() {
        assert!(EntityId(1) < EntityId(2));
        assert_eq!(AssetId(9), AssetId(9));
    }

    #[test]
    fn runtime_advances_immutable_frame_snapshots() {
        let mut runtime = EngineRuntime::new(RenderQualityProfile::Smoke);

        let first = runtime.advance_frame(1.0 / 60.0);
        let second = runtime.advance_frame(1.0 / 60.0);

        assert_eq!(first.frame_index, 0);
        assert_eq!(second.frame_index, 1);
        assert!(second.sim_time_s > first.sim_time_s);
    }
}
