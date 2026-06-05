use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use crate::assets::{
    AssetKind, AssetLoadState, AssetRecord, AssetRegistry, AssetStreamingFrameReport,
    AssetStreamingRequest, GeneratedAssetRecipe,
};
use crate::budget::{
    BudgetFramePlan, BudgetPressure, BudgetPressureSeverity, ModuleBudget, ModuleBudgetEvaluation,
};
use crate::core::*;
use crate::gpu::{
    GpuBarrier, GpuCapabilityReport, GpuDescriptorReport, GpuExecutionPlan, GpuFrameReport,
    GpuGraphBuilder, GpuGraphValidationReport, GpuMemoryPlan, GpuPassDesc, GpuPassTiming,
    GpuPipelineReport, GpuQueueWorkload, GpuRegisteredResourceUsage, GpuResourceRecord,
    GpuResourceUsage, GpuServices, GpuValidationSeverity,
};
use crate::module_abi::{
    ASHFALL_DYNAMIC_MODULE_API_VERSION, DYNAMIC_ASSET_KIND_AUDIO_CLIP,
    DYNAMIC_ASSET_KIND_GENERATED_BUNDLE, DYNAMIC_ASSET_KIND_MATERIAL_GRAPH,
    DYNAMIC_ASSET_KIND_MESH, DYNAMIC_ASSET_KIND_OTHER, DYNAMIC_ASSET_KIND_SHADER,
    DYNAMIC_ASSET_KIND_TEXTURE, DYNAMIC_ASSET_KIND_WORLD_CHUNK, DYNAMIC_ASSET_LOAD_FAILED,
    DYNAMIC_ASSET_LOAD_RESIDENT, DYNAMIC_ASSET_LOAD_UNLOADED, DYNAMIC_ASSET_MAX_DEPENDENCIES,
    DYNAMIC_HOST_STATUS_INVALID_ENUM, DYNAMIC_HOST_STATUS_INVALID_HANDLE,
    DYNAMIC_HOST_STATUS_INVALID_TEXT, DYNAMIC_HOST_STATUS_INVALID_VERSION,
    DYNAMIC_HOST_STATUS_NULL_HOST, DYNAMIC_HOST_STATUS_NULL_PAYLOAD,
    DYNAMIC_HOST_STATUS_NULL_RESULT, DYNAMIC_HOST_STATUS_OK,
    DYNAMIC_HOST_STATUS_TOO_MANY_DEPENDENCIES, DynamicAssetRegistrationResultV1,
    DynamicAssetRegistrationV1, DynamicCommandSinkV1, DynamicEngineHostV1, DynamicFrameInputV1,
    DynamicGeneratedAssetRecipeV1, DynamicGpuGraphBuilderV1, DynamicModuleApiV1,
    DynamicModuleHandle, DynamicModuleValidationReport, DynamicSchemaMigrationStepV1,
    DynamicSchemaRegistrationV1, DynamicU128V1, quality_tier_from_abi,
};
use crate::schema::{
    SchemaCompatibilityReport, SchemaMigrationStep, SchemaRegistry, SchemaRequirement,
};
use crate::world::{
    CommandError, CommandSink, EventFilter, EventValidationError, WorldCommand, WorldEvent,
    WorldEventKind, WorldSaveData, WorldSnapshot, WorldState,
};

#[derive(Clone, Debug, PartialEq)]
pub struct ModuleDescriptor {
    pub module_id: ModuleId,
    pub name: &'static str,
    pub schema: SchemaVersion,
    pub default_quality: QualityTier,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleStateRecord {
    pub module_id: ModuleId,
    pub schema: SchemaVersion,
    pub state_version: u32,
    pub entries: Vec<String>,
}

impl ModuleStateRecord {
    pub fn new(
        module_id: ModuleId,
        schema: SchemaVersion,
        state_version: u32,
        entries: Vec<String>,
    ) -> Self {
        Self {
            module_id,
            schema,
            state_version,
            entries,
        }
    }

    pub fn entries_with_prefix<'a>(
        &'a self,
        prefix: &'a str,
    ) -> impl Iterator<Item = &'a str> + 'a {
        self.entries
            .iter()
            .filter_map(move |entry| entry.strip_prefix(prefix))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleQualityState {
    pub module_id: ModuleId,
    pub quality_tier: QualityTier,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModuleRegistryReport {
    pub static_module_count: usize,
    pub dynamic_module_api_version: u32,
    pub modules: Vec<RegisteredModuleReport>,
    pub schema_compatibility: SchemaCompatibilityReport,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RegisteredModuleReport {
    pub descriptor: ModuleDescriptor,
    pub schema_requirements: Vec<SchemaRequirement>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModuleReplacementReport {
    pub previous: ModuleDescriptor,
    pub replacement: ModuleDescriptor,
    pub schema_compatibility: SchemaCompatibilityReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleReplacementError {
    MissingModule(ModuleId),
    SchemaNameMismatch {
        module_id: ModuleId,
        expected: &'static str,
        replacement: &'static str,
    },
    SchemaVersionTooOld {
        module_id: ModuleId,
        expected_minimum: u32,
        replacement: u32,
    },
}

impl fmt::Display for ModuleReplacementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingModule(module_id) => {
                write!(f, "module {module_id} is not registered")
            }
            Self::SchemaNameMismatch {
                module_id,
                expected,
                replacement,
            } => write!(
                f,
                "replacement for module {module_id} exports schema {replacement}, but {expected} is required"
            ),
            Self::SchemaVersionTooOld {
                module_id,
                expected_minimum,
                replacement,
            } => write!(
                f,
                "replacement for module {module_id} exports schema v{replacement}, but v{expected_minimum}+ is required"
            ),
        }
    }
}

impl Error for ModuleReplacementError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DynamicModuleRegistrationError {
    Validation(DynamicModuleValidationReport),
}

impl fmt::Display for DynamicModuleRegistrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(report) => write!(f, "{report}"),
        }
    }
}

impl Error for DynamicModuleRegistrationError {}

/// Registers a schema exported by a dynamic module through the opaque host handle.
///
/// # Safety
///
/// `host` must point to a `DynamicEngineHostV1` created by the Ashfall runtime for the
/// current dynamic module init call. `schema` must point to a valid registration payload
/// for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ashfall_dynamic_host_register_schema_v1(
    host: *mut DynamicEngineHostV1,
    schema: *const DynamicSchemaRegistrationV1,
) -> u32 {
    let Ok(context) = (unsafe { dynamic_engine_context_from_host(host) }) else {
        return unsafe { dynamic_engine_context_error(host) };
    };
    let Some(schema) = (unsafe { schema.as_ref() }) else {
        return DYNAMIC_HOST_STATUS_NULL_PAYLOAD;
    };
    if schema.version == 0 {
        return DYNAMIC_HOST_STATUS_INVALID_VERSION;
    }
    let Ok(name) = decode_dynamic_text(&schema.name) else {
        return DYNAMIC_HOST_STATUS_INVALID_TEXT;
    };

    context.schemas.register(
        context.module_id,
        SchemaVersion {
            name: Box::leak(name.into_boxed_str()),
            version: schema.version,
        },
    );

    DYNAMIC_HOST_STATUS_OK
}

/// Registers a schema migration step exported by a dynamic module.
///
/// # Safety
///
/// `host` must point to a live dynamic host created by the runtime. `step` must point to
/// a valid migration payload for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ashfall_dynamic_host_register_schema_migration_v1(
    host: *mut DynamicEngineHostV1,
    step: *const DynamicSchemaMigrationStepV1,
) -> u32 {
    let Ok(context) = (unsafe { dynamic_engine_context_from_host(host) }) else {
        return unsafe { dynamic_engine_context_error(host) };
    };
    let Some(step) = (unsafe { step.as_ref() }) else {
        return DYNAMIC_HOST_STATUS_NULL_PAYLOAD;
    };
    if step.from_version == 0 || step.to_version <= step.from_version {
        return DYNAMIC_HOST_STATUS_INVALID_VERSION;
    }
    let Ok(schema_name) = decode_dynamic_text(&step.schema_name) else {
        return DYNAMIC_HOST_STATUS_INVALID_TEXT;
    };
    let Ok(description) = decode_dynamic_text(&step.description) else {
        return DYNAMIC_HOST_STATUS_INVALID_TEXT;
    };

    context.schemas.register_migration(SchemaMigrationStep {
        schema_name,
        from_version: step.from_version,
        to_version: step.to_version,
        description,
        lossless: step.lossless != 0,
    });

    DYNAMIC_HOST_STATUS_OK
}

/// Registers a content-addressed asset through the runtime-owned asset registry.
///
/// # Safety
///
/// `host` must point to a live dynamic host created by the runtime. `asset` and `out`
/// must point to valid payload/result storage for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ashfall_dynamic_host_register_asset_v1(
    host: *mut DynamicEngineHostV1,
    asset: *const DynamicAssetRegistrationV1,
    out: *mut DynamicAssetRegistrationResultV1,
) -> u32 {
    let Ok(context) = (unsafe { dynamic_engine_context_from_host(host) }) else {
        return unsafe { dynamic_engine_context_error(host) };
    };
    let Some(asset) = (unsafe { asset.as_ref() }) else {
        return DYNAMIC_HOST_STATUS_NULL_PAYLOAD;
    };
    let Some(out) = (unsafe { out.as_mut() }) else {
        return DYNAMIC_HOST_STATUS_NULL_RESULT;
    };
    let Ok(label) = decode_dynamic_text(&asset.label) else {
        return DYNAMIC_HOST_STATUS_INVALID_TEXT;
    };
    let Ok(provenance) = decode_dynamic_text(&asset.provenance) else {
        return DYNAMIC_HOST_STATUS_INVALID_TEXT;
    };
    let Some(kind) = dynamic_asset_kind_from_abi(asset.kind, &label) else {
        return DYNAMIC_HOST_STATUS_INVALID_ENUM;
    };
    let Some(quality_tier) = quality_tier_from_abi(asset.quality_tier) else {
        return DYNAMIC_HOST_STATUS_INVALID_ENUM;
    };
    let Some(load_state) = dynamic_asset_load_state_from_abi(asset.load_state) else {
        return DYNAMIC_HOST_STATUS_INVALID_ENUM;
    };
    let Ok(dependencies) = dynamic_asset_ids(&asset.dependencies, asset.dependency_count) else {
        return DYNAMIC_HOST_STATUS_TOO_MANY_DEPENDENCIES;
    };
    let byte_len = (asset.has_byte_len != 0).then_some(asset.byte_len);
    let asset_id = context.assets.register_asset(
        kind.clone(),
        label.clone(),
        provenance.clone(),
        dependencies.clone(),
        quality_tier,
    );
    context.assets.register_known(AssetRecord {
        id: asset_id,
        kind,
        label,
        provenance,
        dependencies,
        byte_len,
        generated: false,
        quality_tier,
        load_state,
    });
    out.asset_id = DynamicU128V1::from_u128(asset_id);
    out.cache_hit = 0;

    DYNAMIC_HOST_STATUS_OK
}

/// Registers or reuses a generated asset cache entry through the runtime-owned asset registry.
///
/// # Safety
///
/// `host` must point to a live dynamic host created by the runtime. `recipe` and `out`
/// must point to valid payload/result storage for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ashfall_dynamic_host_register_generated_asset_v1(
    host: *mut DynamicEngineHostV1,
    recipe: *const DynamicGeneratedAssetRecipeV1,
    out: *mut DynamicAssetRegistrationResultV1,
) -> u32 {
    let Ok(context) = (unsafe { dynamic_engine_context_from_host(host) }) else {
        return unsafe { dynamic_engine_context_error(host) };
    };
    let Some(recipe) = (unsafe { recipe.as_ref() }) else {
        return DYNAMIC_HOST_STATUS_NULL_PAYLOAD;
    };
    let Some(out) = (unsafe { out.as_mut() }) else {
        return DYNAMIC_HOST_STATUS_NULL_RESULT;
    };
    if recipe.recipe_version == 0 {
        return DYNAMIC_HOST_STATUS_INVALID_VERSION;
    }
    let Ok(label) = decode_dynamic_text(&recipe.label) else {
        return DYNAMIC_HOST_STATUS_INVALID_TEXT;
    };
    let Ok(recipe_schema) = decode_dynamic_text(&recipe.recipe_schema) else {
        return DYNAMIC_HOST_STATUS_INVALID_TEXT;
    };
    let Ok(recipe_key) = decode_dynamic_text(&recipe.recipe_key) else {
        return DYNAMIC_HOST_STATUS_INVALID_TEXT;
    };
    let Some(kind) = dynamic_asset_kind_from_abi(recipe.kind, &label) else {
        return DYNAMIC_HOST_STATUS_INVALID_ENUM;
    };
    let Some(quality_tier) = quality_tier_from_abi(recipe.quality_tier) else {
        return DYNAMIC_HOST_STATUS_INVALID_ENUM;
    };
    let Ok(source_assets) = dynamic_asset_ids(&recipe.source_assets, recipe.source_asset_count)
    else {
        return DYNAMIC_HOST_STATUS_TOO_MANY_DEPENDENCIES;
    };

    let mut generated_recipe = GeneratedAssetRecipe::new(
        context.module_id,
        kind,
        label,
        recipe_schema,
        recipe.recipe_version,
        recipe_key,
        quality_tier,
    )
    .with_source_assets(source_assets);
    if recipe.has_byte_len != 0 {
        generated_recipe = generated_recipe.with_byte_len(recipe.byte_len);
    }
    let registration = context
        .assets
        .register_generated_asset(generated_recipe, None);
    out.asset_id = DynamicU128V1::from_u128(registration.asset_id);
    out.cache_hit = u8::from(registration.cache_hit);

    DYNAMIC_HOST_STATUS_OK
}

unsafe fn dynamic_engine_context_from_host<'a>(
    host: *mut DynamicEngineHostV1,
) -> Result<&'a mut EngineContext<'a>, u32> {
    let Some(host) = (unsafe { host.as_ref() }) else {
        return Err(DYNAMIC_HOST_STATUS_NULL_HOST);
    };
    if host.api_version != ASHFALL_DYNAMIC_MODULE_API_VERSION {
        return Err(DYNAMIC_HOST_STATUS_INVALID_VERSION);
    }
    if host.host_handle == 0 {
        return Err(DYNAMIC_HOST_STATUS_INVALID_HANDLE);
    }

    let context = host.host_handle as *mut EngineContext<'a>;
    unsafe { context.as_mut() }.ok_or(DYNAMIC_HOST_STATUS_INVALID_HANDLE)
}

unsafe fn dynamic_engine_context_error(host: *mut DynamicEngineHostV1) -> u32 {
    let Some(host) = (unsafe { host.as_ref() }) else {
        return DYNAMIC_HOST_STATUS_NULL_HOST;
    };
    if host.api_version != ASHFALL_DYNAMIC_MODULE_API_VERSION {
        DYNAMIC_HOST_STATUS_INVALID_VERSION
    } else {
        DYNAMIC_HOST_STATUS_INVALID_HANDLE
    }
}

fn decode_dynamic_text(bytes: &[u8]) -> Result<String, ()> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let value = std::str::from_utf8(&bytes[..end]).map_err(|_| ())?;
    if value.trim().is_empty() {
        Err(())
    } else {
        Ok(value.to_string())
    }
}

fn dynamic_asset_kind_from_abi(value: u32, label: &str) -> Option<AssetKind> {
    match value {
        DYNAMIC_ASSET_KIND_MESH => Some(AssetKind::Mesh),
        DYNAMIC_ASSET_KIND_TEXTURE => Some(AssetKind::Texture),
        DYNAMIC_ASSET_KIND_MATERIAL_GRAPH => Some(AssetKind::MaterialGraph),
        DYNAMIC_ASSET_KIND_AUDIO_CLIP => Some(AssetKind::AudioClip),
        DYNAMIC_ASSET_KIND_SHADER => Some(AssetKind::Shader),
        DYNAMIC_ASSET_KIND_WORLD_CHUNK => Some(AssetKind::WorldChunk),
        DYNAMIC_ASSET_KIND_GENERATED_BUNDLE => Some(AssetKind::GeneratedBundle),
        DYNAMIC_ASSET_KIND_OTHER => Some(AssetKind::Other(label.to_string())),
        _ => None,
    }
}

fn dynamic_asset_load_state_from_abi(value: u32) -> Option<AssetLoadState> {
    match value {
        DYNAMIC_ASSET_LOAD_UNLOADED => Some(AssetLoadState::Unloaded),
        DYNAMIC_ASSET_LOAD_RESIDENT => Some(AssetLoadState::Resident),
        DYNAMIC_ASSET_LOAD_FAILED => Some(AssetLoadState::Failed),
        _ => None,
    }
}

fn dynamic_asset_ids(
    assets: &[DynamicU128V1; DYNAMIC_ASSET_MAX_DEPENDENCIES],
    count: u32,
) -> Result<Vec<AssetId>, u32> {
    let count = count as usize;
    if count > DYNAMIC_ASSET_MAX_DEPENDENCIES {
        return Err(DYNAMIC_HOST_STATUS_TOO_MANY_DEPENDENCIES);
    }

    Ok(assets[..count]
        .iter()
        .map(|asset| asset.to_u128())
        .collect())
}

pub struct DynamicEngineModule {
    api: DynamicModuleApiV1,
    descriptor: ModuleDescriptor,
    handle: Option<DynamicModuleHandle>,
}

impl DynamicEngineModule {
    pub fn try_new(api: DynamicModuleApiV1) -> Result<Self, DynamicModuleRegistrationError> {
        let dynamic_descriptor = api
            .descriptor()
            .map_err(DynamicModuleRegistrationError::Validation)?;
        let descriptor = ModuleDescriptor {
            module_id: dynamic_descriptor.module_id,
            name: leak_dynamic_text(dynamic_descriptor.name),
            schema: SchemaVersion {
                name: leak_dynamic_text(dynamic_descriptor.schema_name),
                version: dynamic_descriptor.schema_version,
            },
            default_quality: dynamic_descriptor.default_quality,
        };

        Ok(Self {
            api,
            descriptor,
            handle: None,
        })
    }

    pub fn handle(&self) -> Option<DynamicModuleHandle> {
        self.handle
    }
}

impl EngineModule for DynamicEngineModule {
    fn descriptor(&self) -> ModuleDescriptor {
        self.descriptor.clone()
    }

    fn init(&mut self, ctx: &mut EngineContext<'_>) {
        if self.handle.is_some() {
            return;
        }
        let Some(init) = self.api.init else {
            return;
        };
        let mut host = DynamicEngineHostV1 {
            api_version: ASHFALL_DYNAMIC_MODULE_API_VERSION,
            host_handle: ctx as *mut EngineContext<'_> as usize,
        };
        let handle = unsafe { init(&mut host) };
        if !handle.is_null() {
            self.handle = Some(handle);
        }
    }

    fn tick(&mut self, frame: &FrameContext, out: &mut CommandSink) {
        let (Some(handle), Some(tick)) = (self.handle, self.api.tick) else {
            return;
        };
        let input = DynamicFrameInputV1 {
            frame_id: frame.frame_id,
            tick: frame.sim_time.tick,
            dt_seconds: frame.dt_seconds,
            snapshot_handle: &frame.snapshot as *const WorldSnapshot as usize,
            recent_event_count: frame.recent_events.len().min(u32::MAX as usize) as u32,
            force_count: frame.forces.len().min(u32::MAX as usize) as u32,
        };
        let mut sink = DynamicCommandSinkV1 {
            sink_handle: out as *mut CommandSink as usize,
        };
        unsafe { tick(handle, &input, &mut sink) };
    }

    fn schedule_gpu(&mut self, graph: &mut GpuGraphBuilder) {
        let (Some(handle), Some(schedule_gpu)) = (self.handle, self.api.schedule_gpu) else {
            return;
        };
        let mut graph = DynamicGpuGraphBuilderV1 {
            frame_id: graph.frame_id,
            graph_handle: graph as *mut GpuGraphBuilder as usize,
        };
        unsafe { schedule_gpu(handle, &mut graph) };
    }

    fn shutdown(&mut self) {
        let (Some(handle), Some(shutdown)) = (self.handle.take(), self.api.shutdown) else {
            return;
        };
        unsafe { shutdown(handle) };
    }
}

fn leak_dynamic_text(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

pub struct EngineContext<'a> {
    pub schema_version: u32,
    pub module_id: ModuleId,
    pub schemas: &'a mut SchemaRegistry,
    pub assets: &'a mut AssetRegistry,
}

#[derive(Clone, Debug)]
pub struct FrameContext {
    pub frame_id: FrameId,
    pub sim_time: SimTime,
    pub dt_seconds: f32,
    pub quality_tier: QualityTier,
    pub snapshot: WorldSnapshot,
    pub recent_events: Vec<WorldEvent>,
    pub forces: Vec<ForceCommand>,
}

pub trait EngineModule {
    fn descriptor(&self) -> ModuleDescriptor;

    fn schema_requirements(&self) -> Vec<SchemaRequirement> {
        Vec::new()
    }

    fn init(&mut self, _ctx: &mut EngineContext<'_>) {}

    fn tick(&mut self, _frame: &FrameContext, _out: &mut CommandSink) {}

    fn schedule_gpu(&mut self, _graph: &mut GpuGraphBuilder) {}

    fn performance_counters(&self) -> PerformanceCounters {
        PerformanceCounters::default()
    }

    fn save_state(&self) -> Option<ModuleStateRecord> {
        None
    }

    fn load_state(&mut self, _state: &ModuleStateRecord) {}

    fn shutdown(&mut self) {}
}

#[derive(Debug)]
pub enum RuntimeError {
    Command(CommandError),
    Event(EventValidationError),
    Schema(SchemaCompatibilityReport),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command(error) => write!(f, "{error}"),
            Self::Event(error) => write!(f, "{error}"),
            Self::Schema(report) => write!(f, "{report}"),
        }
    }
}

impl Error for RuntimeError {}

impl From<CommandError> for RuntimeError {
    fn from(value: CommandError) -> Self {
        Self::Command(value)
    }
}

impl From<EventValidationError> for RuntimeError {
    fn from(value: EventValidationError) -> Self {
        Self::Event(value)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FrameReport {
    pub frame_id: FrameId,
    pub sim_time: SimTime,
    pub events: Vec<WorldEvent>,
    pub profiler: FrameProfilerReport,
    pub gpu_backend: crate::gpu::GpuBackend,
    pub gpu_capability_report: GpuCapabilityReport,
    pub gpu_passes: Vec<GpuPassDesc>,
    pub gpu_barriers: Vec<GpuBarrier>,
    pub gpu_timing: Vec<GpuPassTiming>,
    pub gpu_validation: GpuGraphValidationReport,
    pub gpu_resource_usage: Vec<GpuResourceUsage>,
    pub gpu_declared_resources: Vec<GpuResourceRecord>,
    pub gpu_registered_resource_usage: Vec<GpuRegisteredResourceUsage>,
    pub gpu_pipeline_report: GpuPipelineReport,
    pub gpu_descriptor_report: GpuDescriptorReport,
    pub gpu_memory_plan: GpuMemoryPlan,
    pub gpu_execution_plan: GpuExecutionPlan,
    pub gpu_queue_workloads: Vec<GpuQueueWorkload>,
    pub gpu_transient_memory_bytes: u64,
    pub gpu_total_milliseconds: f32,
    pub module_reports: Vec<ModuleFrameReport>,
    pub budget_pressure: Vec<BudgetPressure>,
    pub budget_plan: BudgetFramePlan,
    pub asset_streaming: AssetStreamingFrameReport,
    pub schema_compatibility: SchemaCompatibilityReport,
}

impl FrameReport {
    pub fn inspect(&self) -> FrameInspectionReport {
        FrameInspectionReport::from_frame(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FrameInspectionSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameInspectionIssue {
    pub severity: FrameInspectionSeverity,
    pub code: String,
    pub subject: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModuleInspectionReport {
    pub module_id: ModuleId,
    pub module_name: &'static str,
    pub quality_tier: QualityTier,
    pub cpu_milliseconds: f32,
    pub gpu_milliseconds: f32,
    pub memory_bytes: u64,
    pub budget_pressure_count: usize,
    pub streaming_requested_assets: usize,
    pub streaming_pending_assets: usize,
    pub streaming_completed_assets: usize,
    pub streaming_bytes_pending: u64,
    pub streaming_bytes_this_frame: u64,
    pub gpu_resource_count: usize,
    pub gpu_resource_bytes: u64,
    pub gpu_descriptor_count: usize,
    pub gpu_bindless_descriptor_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FrameInspectionReport {
    pub frame_id: FrameId,
    pub tick: u64,
    pub passed: bool,
    pub issues: Vec<FrameInspectionIssue>,
    pub modules: Vec<ModuleInspectionReport>,
    pub total_streaming_bytes: u64,
    pub total_gpu_resource_bytes: u64,
    pub total_gpu_descriptor_bindings: usize,
    pub total_gpu_bindless_bindings: usize,
    pub gpu_descriptor_table_bytes: u64,
    pub total_gpu_pipeline_count: usize,
    pub gpu_pipeline_cache_hit_count: usize,
    pub gpu_unknown_pipeline_count: usize,
    pub total_gpu_transient_bytes: u64,
    pub gpu_memory_saved_bytes: u64,
    pub gpu_scheduled_wall_milliseconds: f32,
    pub gpu_overlapped_work_milliseconds: f32,
}

impl FrameInspectionReport {
    pub fn from_frame(report: &FrameReport) -> Self {
        let mut streaming_by_requester = BTreeMap::new();
        for cost in &report.asset_streaming.by_requester {
            streaming_by_requester.insert(cost.requester, cost);
        }

        let mut gpu_resources_by_owner: BTreeMap<ModuleId, (usize, u64)> = BTreeMap::new();
        for usage in &report.gpu_registered_resource_usage {
            if let Some(owner) = usage.owner {
                let entry = gpu_resources_by_owner.entry(owner).or_default();
                entry.0 += 1;
                entry.1 = entry.1.saturating_add(usage.byte_len);
            }
        }
        let mut gpu_descriptors_by_owner: BTreeMap<ModuleId, (usize, usize)> = BTreeMap::new();
        for binding in &report.gpu_descriptor_report.bindings {
            if let Some(owner) = binding.owner {
                let entry = gpu_descriptors_by_owner.entry(owner).or_default();
                entry.0 += 1;
                entry.1 += usize::from(binding.bindless_index.is_some());
            }
        }

        let modules = report
            .module_reports
            .iter()
            .map(|module| {
                let module_id = module.descriptor.module_id;
                let streaming = streaming_by_requester.get(&module_id).copied();
                let (gpu_resource_count, gpu_resource_bytes) = gpu_resources_by_owner
                    .get(&module_id)
                    .copied()
                    .unwrap_or_default();
                let (gpu_descriptor_count, gpu_bindless_descriptor_count) =
                    gpu_descriptors_by_owner
                        .get(&module_id)
                        .copied()
                        .unwrap_or_default();

                ModuleInspectionReport {
                    module_id,
                    module_name: module.descriptor.name,
                    quality_tier: module.quality_tier,
                    cpu_milliseconds: module.counters.cpu_milliseconds,
                    gpu_milliseconds: module.counters.gpu_milliseconds,
                    memory_bytes: module.counters.memory_bytes,
                    budget_pressure_count: module.pressure.len(),
                    streaming_requested_assets: streaming
                        .map(|cost| cost.requested_assets)
                        .unwrap_or_default(),
                    streaming_pending_assets: streaming
                        .map(|cost| cost.pending_assets)
                        .unwrap_or_default(),
                    streaming_completed_assets: streaming
                        .map(|cost| cost.completed_assets)
                        .unwrap_or_default(),
                    streaming_bytes_pending: streaming
                        .map(|cost| cost.bytes_pending)
                        .unwrap_or_default(),
                    streaming_bytes_this_frame: streaming
                        .map(|cost| cost.bytes_streamed_this_frame)
                        .unwrap_or_default(),
                    gpu_resource_count,
                    gpu_resource_bytes,
                    gpu_descriptor_count,
                    gpu_bindless_descriptor_count,
                }
            })
            .collect::<Vec<_>>();

        let mut issues = Vec::new();
        issues.extend(report.schema_compatibility.issues.iter().map(|issue| {
            FrameInspectionIssue {
                severity: match issue.severity {
                    crate::schema::SchemaCompatibilitySeverity::Info => {
                        FrameInspectionSeverity::Info
                    }
                    crate::schema::SchemaCompatibilitySeverity::Warning => {
                        FrameInspectionSeverity::Warning
                    }
                    crate::schema::SchemaCompatibilitySeverity::Error => {
                        FrameInspectionSeverity::Error
                    }
                },
                code: issue.code.clone(),
                subject: issue.schema_name.clone(),
                message: issue.message.clone(),
            }
        }));
        issues.extend(report.gpu_validation.issues.iter().map(|issue| {
            FrameInspectionIssue {
                severity: match issue.severity {
                    GpuValidationSeverity::Info => FrameInspectionSeverity::Info,
                    GpuValidationSeverity::Warning => FrameInspectionSeverity::Warning,
                    GpuValidationSeverity::Error => FrameInspectionSeverity::Error,
                },
                code: issue.code.clone(),
                subject: issue
                    .pass_name
                    .clone()
                    .unwrap_or_else(|| "gpu_graph".to_string()),
                message: issue.message.clone(),
            }
        }));

        for pressure in &report.budget_pressure {
            issues.push(FrameInspectionIssue {
                severity: match pressure.severity() {
                    BudgetPressureSeverity::Info => FrameInspectionSeverity::Info,
                    BudgetPressureSeverity::Warning => FrameInspectionSeverity::Warning,
                    BudgetPressureSeverity::Critical => FrameInspectionSeverity::Error,
                },
                code: format!("budget_{:?}", pressure.kind).to_lowercase(),
                subject: format!("module:{}", pressure.module_id),
                message: format!(
                    "measured {:.2} against budget {:.2}",
                    pressure.measured, pressure.budget
                ),
            });
        }

        if report.asset_streaming.pending_assets > 0 {
            issues.push(FrameInspectionIssue {
                severity: FrameInspectionSeverity::Warning,
                code: "asset_streaming_pending".to_string(),
                subject: "asset_streaming".to_string(),
                message: format!(
                    "{} assets still pending ({} bytes)",
                    report.asset_streaming.pending_assets, report.asset_streaming.bytes_pending
                ),
            });
        }

        let total_gpu_resource_bytes = modules
            .iter()
            .map(|module| module.gpu_resource_bytes)
            .sum::<u64>();
        let passed = !issues
            .iter()
            .any(|issue| issue.severity == FrameInspectionSeverity::Error);

        Self {
            frame_id: report.frame_id,
            tick: report.sim_time.tick,
            passed,
            issues,
            modules,
            total_streaming_bytes: report.asset_streaming.bytes_streamed_this_frame,
            total_gpu_resource_bytes,
            total_gpu_descriptor_bindings: report.gpu_descriptor_report.bindings.len(),
            total_gpu_bindless_bindings: report.gpu_descriptor_report.bindless_binding_count,
            gpu_descriptor_table_bytes: report.gpu_descriptor_report.descriptor_table_bytes,
            total_gpu_pipeline_count: report.gpu_pipeline_report.pipelines.len(),
            gpu_pipeline_cache_hit_count: report.gpu_pipeline_report.cache_hit_count,
            gpu_unknown_pipeline_count: report.gpu_pipeline_report.unknown_pipeline_count,
            total_gpu_transient_bytes: report.gpu_memory_plan.total_transient_bytes,
            gpu_memory_saved_bytes: report.gpu_memory_plan.aliased_bytes_saved,
            gpu_scheduled_wall_milliseconds: report.gpu_execution_plan.estimated_wall_milliseconds,
            gpu_overlapped_work_milliseconds: report
                .gpu_execution_plan
                .overlapped_work_milliseconds,
        }
    }

    pub fn module(&self, module_id: ModuleId) -> Option<&ModuleInspectionReport> {
        self.modules
            .iter()
            .find(|module| module.module_id == module_id)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderFrameReport {
    pub render_frame_id: FrameId,
    pub sim_frame_id: FrameId,
    pub sim_time: SimTime,
    pub render_time_seconds: f64,
    pub delta_seconds: f32,
    pub fixed_dt_seconds: f32,
    pub interpolation_alpha: f32,
    pub snapshot: WorldSnapshot,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeSaveData {
    pub world: WorldSaveData,
    pub asset_registry: AssetRegistry,
    pub schema_registry: SchemaRegistry,
    pub module_states: Vec<ModuleStateRecord>,
    pub module_quality: Vec<ModuleQualityState>,
    pub queued_commands: Vec<WorldCommand>,
    pub replay_log: ReplayLog,
    pub frame_id: FrameId,
    pub render_frame_id: FrameId,
    pub sim_time: SimTime,
    pub render_time_seconds: f64,
    pub render_since_last_sim_seconds: f64,
    pub fixed_dt_seconds: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FrameProfilerReport {
    pub frame_id: FrameId,
    pub module_costs: Vec<ModuleCostReport>,
    pub total_module_cpu_milliseconds: f32,
    pub total_module_gpu_milliseconds: f32,
    pub gpu_graph_milliseconds: f32,
    pub total_module_memory_bytes: u64,
    pub asset_streaming_bytes: u64,
    pub asset_streaming_pending: usize,
    pub budget_pressure_count: usize,
}

impl FrameProfilerReport {
    pub fn from_parts(
        frame_id: FrameId,
        module_reports: &[ModuleFrameReport],
        gpu_report: &GpuFrameReport,
        asset_streaming: &AssetStreamingFrameReport,
    ) -> Self {
        let module_costs = module_reports
            .iter()
            .map(ModuleCostReport::from_module_report)
            .collect::<Vec<_>>();
        let total_module_cpu_milliseconds = module_costs
            .iter()
            .map(|cost| cost.cpu_milliseconds)
            .sum::<f32>();
        let total_module_gpu_milliseconds = module_costs
            .iter()
            .map(|cost| cost.gpu_milliseconds)
            .sum::<f32>();
        let total_module_memory_bytes = module_costs.iter().map(|cost| cost.memory_bytes).sum();
        let budget_pressure_count = module_costs
            .iter()
            .map(|cost| cost.budget_pressure_count)
            .sum();

        Self {
            frame_id,
            module_costs,
            total_module_cpu_milliseconds,
            total_module_gpu_milliseconds,
            gpu_graph_milliseconds: gpu_report.total_gpu_milliseconds,
            total_module_memory_bytes,
            asset_streaming_bytes: asset_streaming.bytes_streamed_this_frame,
            asset_streaming_pending: asset_streaming.pending_assets,
            budget_pressure_count,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModuleCostReport {
    pub module_id: ModuleId,
    pub module_name: &'static str,
    pub quality_tier: QualityTier,
    pub cpu_milliseconds: f32,
    pub gpu_milliseconds: f32,
    pub memory_bytes: u64,
    pub budget_cpu_milliseconds: f32,
    pub budget_gpu_milliseconds: f32,
    pub budget_memory_bytes: u64,
    pub budget_streaming_bytes: u64,
    pub budget_latency_milliseconds: f32,
    pub budget_pressure_count: usize,
}

impl ModuleCostReport {
    pub fn from_module_report(report: &ModuleFrameReport) -> Self {
        Self {
            module_id: report.descriptor.module_id,
            module_name: report.descriptor.name,
            quality_tier: report.quality_tier,
            cpu_milliseconds: report.counters.cpu_milliseconds,
            gpu_milliseconds: report.counters.gpu_milliseconds,
            memory_bytes: report.counters.memory_bytes,
            budget_cpu_milliseconds: report.budget.cpu_milliseconds,
            budget_gpu_milliseconds: report.budget.gpu_milliseconds,
            budget_memory_bytes: report.budget.memory_bytes,
            budget_streaming_bytes: report.budget.streaming_bytes,
            budget_latency_milliseconds: report.budget.latency_milliseconds,
            budget_pressure_count: report.pressure.len(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModuleFrameReport {
    pub descriptor: ModuleDescriptor,
    pub quality_tier: QualityTier,
    pub counters: PerformanceCounters,
    pub budget: ModuleBudget,
    pub pressure: Vec<BudgetPressure>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReplayFrame {
    pub frame_id: FrameId,
    pub tick: u64,
    pub queued_commands: Vec<WorldCommand>,
    pub events: Vec<WorldEvent>,
}

impl ReplayFrame {
    pub fn summary(&self) -> ReplayFrameSummary {
        ReplayFrameSummary {
            frame_id: self.frame_id,
            tick: self.tick,
            queued_command_count: self.queued_commands.len(),
            queued_command_fingerprints: self
                .queued_commands
                .iter()
                .map(|command| format!("{command:?}"))
                .collect(),
            event_count: self.events.len(),
            event_fingerprints: self.events.iter().map(replay_event_fingerprint).collect(),
        }
    }

    pub fn events_matching(&self, filter: &EventFilter) -> Vec<&WorldEvent> {
        filter.select(self.events.iter())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReplayLog {
    frames: Vec<ReplayFrame>,
}

impl ReplayLog {
    pub fn new(frames: Vec<ReplayFrame>) -> Self {
        Self { frames }
    }

    pub fn frames(&self) -> &[ReplayFrame] {
        &self.frames
    }

    pub fn frame(&self, index: usize) -> Option<&ReplayFrame> {
        self.frames.get(index)
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn summaries(&self) -> Vec<ReplayFrameSummary> {
        self.frames.iter().map(ReplayFrame::summary).collect()
    }

    pub fn capture(&self, filter: &ReplayCaptureFilter) -> ReplayCapture {
        if filter.limit == Some(0) {
            return ReplayCapture::default();
        }

        let mut frame_summaries = Vec::new();
        for frame in &self.frames {
            if filter.matches(frame) {
                frame_summaries.push(frame.summary());
                if filter
                    .limit
                    .is_some_and(|limit| frame_summaries.len() == limit)
                {
                    break;
                }
            }
        }

        ReplayCapture { frame_summaries }
    }

    pub fn compare(&self, actual: &ReplayLog) -> ReplayComparisonReport {
        let compared_frames = self.frames.len().min(actual.frames.len());
        let max_frames = self.frames.len().max(actual.frames.len());
        let mut mismatches = Vec::new();

        for frame_index in 0..max_frames {
            let expected = self.frames.get(frame_index).map(ReplayFrame::summary);
            let actual = actual.frames.get(frame_index).map(ReplayFrame::summary);
            let status = compare_replay_frame_summaries(expected.as_ref(), actual.as_ref());

            if status != ReplayComparisonStatus::Matched {
                mismatches.push(ReplayMismatch {
                    frame_index,
                    status,
                    expected,
                    actual,
                });
            }
        }

        ReplayComparisonReport {
            passed: mismatches.is_empty(),
            compared_frames,
            mismatches,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReplayCaptureFilter {
    pub start_frame_id: Option<FrameId>,
    pub end_frame_id: Option<FrameId>,
    pub start_tick: Option<u64>,
    pub end_tick: Option<u64>,
    pub event_filter: Option<EventFilter>,
    pub event_kind_label: Option<String>,
    pub min_event_count: Option<usize>,
    pub limit: Option<usize>,
}

impl ReplayCaptureFilter {
    pub fn with_frame_range(mut self, start_frame_id: FrameId, end_frame_id: FrameId) -> Self {
        self.start_frame_id = Some(start_frame_id);
        self.end_frame_id = Some(end_frame_id);
        self
    }

    pub fn with_tick_range(mut self, start_tick: u64, end_tick: u64) -> Self {
        self.start_tick = Some(start_tick);
        self.end_tick = Some(end_tick);
        self
    }

    pub fn with_event_filter(mut self, event_filter: EventFilter) -> Self {
        self.event_filter = Some(event_filter);
        self
    }

    pub fn with_event_kind_label(mut self, event_kind_label: impl Into<String>) -> Self {
        self.event_kind_label = Some(event_kind_label.into());
        self
    }

    pub fn with_min_event_count(mut self, min_event_count: usize) -> Self {
        self.min_event_count = Some(min_event_count);
        self
    }

    pub fn limited_to(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn matches(&self, frame: &ReplayFrame) -> bool {
        if self
            .start_frame_id
            .is_some_and(|start_frame_id| frame.frame_id < start_frame_id)
        {
            return false;
        }
        if self
            .end_frame_id
            .is_some_and(|end_frame_id| frame.frame_id > end_frame_id)
        {
            return false;
        }
        if self
            .start_tick
            .is_some_and(|start_tick| frame.tick < start_tick)
        {
            return false;
        }
        if self.end_tick.is_some_and(|end_tick| frame.tick > end_tick) {
            return false;
        }
        if self
            .min_event_count
            .is_some_and(|min_event_count| frame.events.len() < min_event_count)
        {
            return false;
        }
        if self.event_filter.is_some() || self.event_kind_label.is_some() {
            return frame.events.iter().any(|event| {
                let event_filter_matches = match &self.event_filter {
                    Some(event_filter) => event_filter.matches(event),
                    None => true,
                };
                let kind_label_matches = match self.event_kind_label.as_deref() {
                    Some(kind_label) => event_kind_label(&event.kind) == kind_label,
                    None => true,
                };

                event_filter_matches && kind_label_matches
            });
        }

        true
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayCapture {
    pub frame_summaries: Vec<ReplayFrameSummary>,
}

impl ReplayCapture {
    pub fn len(&self) -> usize {
        self.frame_summaries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frame_summaries.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayFrameSummary {
    pub frame_id: FrameId,
    pub tick: u64,
    pub queued_command_count: usize,
    pub queued_command_fingerprints: Vec<String>,
    pub event_count: usize,
    pub event_fingerprints: Vec<ReplayEventFingerprint>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayEventFingerprint {
    pub event_id: WorldEventId,
    pub tick: u64,
    pub location_meters: String,
    pub actors: Vec<EntityId>,
    pub kind_label: String,
    pub kind_fingerprint: String,
    pub evidence_count: usize,
    pub physical_evidence: EvidenceRefs,
    pub narrative_tags: TagSet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayComparisonStatus {
    Matched,
    MissingFrame,
    ExtraFrame,
    FrameIdMismatch,
    TickMismatch,
    CommandCountMismatch,
    CommandPayloadMismatch,
    EventCountMismatch,
    EventKindMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayMismatch {
    pub frame_index: usize,
    pub status: ReplayComparisonStatus,
    pub expected: Option<ReplayFrameSummary>,
    pub actual: Option<ReplayFrameSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayComparisonReport {
    pub passed: bool,
    pub compared_frames: usize,
    pub mismatches: Vec<ReplayMismatch>,
}

pub struct EngineRuntime {
    pub world: WorldState,
    asset_registry: AssetRegistry,
    schema_registry: SchemaRegistry,
    gpu: GpuServices,
    modules: Vec<Box<dyn EngineModule>>,
    module_quality: BTreeMap<ModuleId, QualityTier>,
    queued_commands: Vec<WorldCommand>,
    replay_log: ReplayLog,
    frame_id: FrameId,
    render_frame_id: FrameId,
    sim_time: SimTime,
    render_time_seconds: f64,
    render_since_last_sim_seconds: f64,
    fixed_dt_seconds: f32,
}

impl EngineRuntime {
    pub fn new(world: WorldState) -> Self {
        Self {
            world,
            asset_registry: AssetRegistry::default(),
            schema_registry: SchemaRegistry::default(),
            gpu: GpuServices::new_vulkan(),
            modules: Vec::new(),
            module_quality: BTreeMap::new(),
            queued_commands: Vec::new(),
            replay_log: ReplayLog::default(),
            frame_id: 0,
            render_frame_id: 0,
            sim_time: SimTime::default(),
            render_time_seconds: 0.0,
            render_since_last_sim_seconds: 0.0,
            fixed_dt_seconds: 1.0 / 60.0,
        }
    }

    pub fn register_module<M>(&mut self, module: M)
    where
        M: EngineModule + 'static,
    {
        let descriptor = module.descriptor();
        self.module_quality
            .entry(descriptor.module_id)
            .or_insert(descriptor.default_quality);
        self.modules.push(Box::new(module));
    }

    pub fn register_dynamic_module(
        &mut self,
        api: DynamicModuleApiV1,
    ) -> Result<ModuleDescriptor, DynamicModuleRegistrationError> {
        let module = DynamicEngineModule::try_new(api)?;
        let descriptor = module.descriptor();
        self.register_module(module);
        Ok(descriptor)
    }

    pub fn replace_module<M>(
        &mut self,
        mut replacement: M,
    ) -> Result<ModuleReplacementReport, ModuleReplacementError>
    where
        M: EngineModule + 'static,
    {
        let replacement_descriptor = replacement.descriptor();
        let index = self
            .modules
            .iter()
            .position(|module| module.descriptor().module_id == replacement_descriptor.module_id)
            .ok_or(ModuleReplacementError::MissingModule(
                replacement_descriptor.module_id,
            ))?;
        let previous_descriptor = self.modules[index].descriptor();

        validate_module_replacement(previous_descriptor.clone(), replacement_descriptor.clone())?;

        self.modules[index].shutdown();
        self.schema_registry.register(
            replacement_descriptor.module_id,
            replacement_descriptor.schema.clone(),
        );
        let mut context = EngineContext {
            schema_version: 1,
            module_id: replacement_descriptor.module_id,
            schemas: &mut self.schema_registry,
            assets: &mut self.asset_registry,
        };
        replacement.init(&mut context);
        self.modules[index] = Box::new(replacement);
        self.module_quality.insert(
            replacement_descriptor.module_id,
            replacement_descriptor.default_quality,
        );

        Ok(ModuleReplacementReport {
            previous: previous_descriptor,
            replacement: replacement_descriptor,
            schema_compatibility: self.schema_compatibility_report(),
        })
    }

    pub fn init_modules(&mut self) {
        for module in &mut self.modules {
            let descriptor = module.descriptor();
            self.schema_registry
                .register(descriptor.module_id, descriptor.schema);
            let mut context = EngineContext {
                schema_version: 1,
                module_id: descriptor.module_id,
                schemas: &mut self.schema_registry,
                assets: &mut self.asset_registry,
            };
            module.init(&mut context);
        }
    }

    pub fn queue_command(&mut self, command: WorldCommand) {
        self.queued_commands.push(command);
    }

    pub fn module_descriptors(&self) -> Vec<ModuleDescriptor> {
        self.modules
            .iter()
            .map(|module| module.descriptor())
            .collect()
    }

    pub fn module_quality(&self, module_id: ModuleId) -> Option<QualityTier> {
        self.module_quality.get(&module_id).copied()
    }

    pub fn set_module_quality(
        &mut self,
        module_id: ModuleId,
        quality_tier: QualityTier,
    ) -> Option<QualityTier> {
        let current = self.module_quality.get_mut(&module_id)?;
        let previous = *current;
        *current = quality_tier;
        Some(previous)
    }

    pub fn module_registry_report(&self) -> ModuleRegistryReport {
        let modules = self
            .modules
            .iter()
            .map(|module| RegisteredModuleReport {
                descriptor: module.descriptor(),
                schema_requirements: module.schema_requirements(),
            })
            .collect::<Vec<_>>();
        let requirements = modules
            .iter()
            .flat_map(|module| module.schema_requirements.iter().cloned())
            .collect::<Vec<_>>();

        ModuleRegistryReport {
            static_module_count: modules.len(),
            dynamic_module_api_version: ASHFALL_DYNAMIC_MODULE_API_VERSION,
            modules,
            schema_compatibility: self.schema_registry.check_requirements(&requirements),
        }
    }

    pub fn assets(&self) -> &AssetRegistry {
        &self.asset_registry
    }

    pub fn assets_mut(&mut self) -> &mut AssetRegistry {
        &mut self.asset_registry
    }

    pub fn schemas(&self) -> &SchemaRegistry {
        &self.schema_registry
    }

    pub fn schema_compatibility_report(&self) -> SchemaCompatibilityReport {
        let requirements = self
            .modules
            .iter()
            .flat_map(|module| module.schema_requirements())
            .collect::<Vec<_>>();
        self.schema_registry.check_requirements(&requirements)
    }

    pub fn replay_log(&self) -> &ReplayLog {
        &self.replay_log
    }

    pub fn save_world(&self) -> WorldSaveData {
        self.world.save_data()
    }

    pub fn load_world(&mut self, save_data: WorldSaveData) {
        self.world = WorldState::load_data(save_data);
    }

    pub fn save_runtime(&self) -> RuntimeSaveData {
        RuntimeSaveData {
            world: self.world.save_data(),
            asset_registry: self.asset_registry.clone(),
            schema_registry: self.schema_registry.clone(),
            module_states: self
                .modules
                .iter()
                .filter_map(|module| module.save_state())
                .collect(),
            module_quality: self
                .module_quality
                .iter()
                .map(|(module_id, quality_tier)| ModuleQualityState {
                    module_id: *module_id,
                    quality_tier: *quality_tier,
                })
                .collect(),
            queued_commands: self.queued_commands.clone(),
            replay_log: self.replay_log.clone(),
            frame_id: self.frame_id,
            render_frame_id: self.render_frame_id,
            sim_time: self.sim_time,
            render_time_seconds: self.render_time_seconds,
            render_since_last_sim_seconds: self.render_since_last_sim_seconds,
            fixed_dt_seconds: self.fixed_dt_seconds,
        }
    }

    pub fn load_runtime(&mut self, save_data: RuntimeSaveData) {
        let RuntimeSaveData {
            world,
            asset_registry,
            schema_registry,
            module_states,
            module_quality,
            queued_commands,
            replay_log,
            frame_id,
            render_frame_id,
            sim_time,
            render_time_seconds,
            render_since_last_sim_seconds,
            fixed_dt_seconds,
        } = save_data;

        self.world = WorldState::load_data(world);
        self.asset_registry = asset_registry;
        self.schema_registry = schema_registry;
        self.module_quality = module_quality
            .into_iter()
            .map(|state| (state.module_id, state.quality_tier))
            .collect();
        self.queued_commands = queued_commands;
        self.replay_log = replay_log;
        self.frame_id = frame_id;
        self.render_frame_id = render_frame_id;
        self.sim_time = sim_time;
        self.render_time_seconds = render_time_seconds;
        self.render_since_last_sim_seconds = render_since_last_sim_seconds;
        self.fixed_dt_seconds = fixed_dt_seconds;

        for module in &mut self.modules {
            let module_id = module.descriptor().module_id;
            self.module_quality
                .entry(module_id)
                .or_insert_with(|| module.descriptor().default_quality);
            if let Some(state) = module_states
                .iter()
                .find(|state| state.module_id == module_id)
            {
                module.load_state(state);
            }
        }
    }

    pub fn fixed_dt_seconds(&self) -> f32 {
        self.fixed_dt_seconds
    }

    pub fn render_frame(&mut self, delta_seconds: f32) -> RenderFrameReport {
        let delta_seconds = delta_seconds.max(0.0);
        self.render_frame_id += 1;
        self.render_time_seconds += f64::from(delta_seconds);
        self.render_since_last_sim_seconds += f64::from(delta_seconds);

        let interpolation_alpha = if self.fixed_dt_seconds <= 0.0 {
            0.0
        } else {
            (self.render_since_last_sim_seconds / f64::from(self.fixed_dt_seconds)).clamp(0.0, 1.0)
                as f32
        };

        RenderFrameReport {
            render_frame_id: self.render_frame_id,
            sim_frame_id: self.frame_id,
            sim_time: self.sim_time,
            render_time_seconds: self.render_time_seconds,
            delta_seconds,
            fixed_dt_seconds: self.fixed_dt_seconds,
            interpolation_alpha,
            snapshot: self.world.snapshot(self.frame_id, self.sim_time),
        }
    }

    pub fn step(&mut self) -> Result<FrameReport, RuntimeError> {
        let schema_compatibility = self.schema_compatibility_report();
        if !schema_compatibility.passed {
            return Err(RuntimeError::Schema(schema_compatibility));
        }
        self.world
            .validate_commands(&self.queued_commands, self.sim_time.tick + 1)?;

        self.frame_id += 1;
        self.sim_time.tick += 1;
        self.sim_time.seconds += f64::from(self.fixed_dt_seconds);
        self.render_since_last_sim_seconds =
            (self.render_since_last_sim_seconds - f64::from(self.fixed_dt_seconds)).max(0.0);

        let queued_for_replay = self.queued_commands.clone();
        let mut emitted_events = Vec::new();
        let queued_commands = std::mem::take(&mut self.queued_commands);
        self.commit_commands(queued_commands, &mut emitted_events)?;

        let frame_forces = self.world.take_forces();
        let mut visible_events = self.world.event_ledger.recent(32);
        let mut module_reports = Vec::new();
        let mut budget_pressure = Vec::new();
        let mut budget_evaluations = Vec::new();
        let active_module_quality = self.module_quality.clone();

        for module in &mut self.modules {
            let descriptor = module.descriptor();
            let quality_tier = active_module_quality
                .get(&descriptor.module_id)
                .copied()
                .unwrap_or(descriptor.default_quality);

            if quality_tier == QualityTier::Disabled {
                let counters = PerformanceCounters::default();
                let evaluation = ModuleBudgetEvaluation::evaluate(
                    descriptor.module_id,
                    quality_tier,
                    counters.clone(),
                );
                let budget = evaluation.budget.clone();
                let pressure = evaluation.pressure.clone();
                budget_pressure.extend(pressure.clone());
                budget_evaluations.push(evaluation);
                module_reports.push(ModuleFrameReport {
                    descriptor,
                    quality_tier,
                    counters,
                    budget,
                    pressure,
                });
                continue;
            }

            let frame = FrameContext {
                frame_id: self.frame_id,
                sim_time: self.sim_time,
                dt_seconds: self.fixed_dt_seconds,
                quality_tier,
                snapshot: self.world.snapshot(self.frame_id, self.sim_time),
                recent_events: visible_events.clone(),
                forces: frame_forces.clone(),
            };

            let mut sink = CommandSink::default();
            module.tick(&frame, &mut sink);

            self.world
                .validate_commands(&sink.commands, self.sim_time.tick)?;
            self.world
                .validate_events(&sink.events, self.sim_time.tick, &visible_events)?;

            for event in sink.events {
                record_visible_event(
                    &mut self.world,
                    &mut self.asset_registry,
                    event,
                    &mut visible_events,
                    &mut emitted_events,
                );
            }

            let produced = self
                .world
                .apply_commands_transactional(sink.commands, self.sim_time.tick)?;
            for event in produced {
                record_visible_event(
                    &mut self.world,
                    &mut self.asset_registry,
                    event,
                    &mut visible_events,
                    &mut emitted_events,
                );
            }

            let counters = module.performance_counters();
            let evaluation = ModuleBudgetEvaluation::evaluate(
                descriptor.module_id,
                quality_tier,
                counters.clone(),
            );
            let budget = evaluation.budget.clone();
            let pressure = evaluation.pressure.clone();
            budget_pressure.extend(pressure.clone());
            budget_evaluations.push(evaluation);
            module_reports.push(ModuleFrameReport {
                descriptor,
                quality_tier,
                counters,
                budget,
                pressure,
            });
        }

        let asset_streaming =
            self.advance_asset_streaming(&mut visible_events, &mut emitted_events);

        let gpu_report = self.schedule_gpu();
        let budget_plan = BudgetFramePlan::from_module_evaluations(&budget_evaluations);
        self.apply_budget_plan(&budget_plan);
        let profiler = FrameProfilerReport::from_parts(
            self.frame_id,
            &module_reports,
            &gpu_report,
            &asset_streaming,
        );
        self.replay_log.frames.push(ReplayFrame {
            frame_id: self.frame_id,
            tick: self.sim_time.tick,
            queued_commands: queued_for_replay,
            events: emitted_events.clone(),
        });

        Ok(FrameReport {
            frame_id: self.frame_id,
            sim_time: self.sim_time,
            events: emitted_events,
            profiler,
            gpu_backend: gpu_report.backend,
            gpu_capability_report: gpu_report.capability_report,
            gpu_passes: gpu_report.passes,
            gpu_barriers: gpu_report.barriers,
            gpu_timing: gpu_report.timing,
            gpu_validation: gpu_report.validation,
            gpu_resource_usage: gpu_report.resource_usage,
            gpu_declared_resources: gpu_report.declared_resources,
            gpu_registered_resource_usage: gpu_report.registered_resource_usage,
            gpu_pipeline_report: gpu_report.pipeline_report,
            gpu_descriptor_report: gpu_report.descriptor_report,
            gpu_memory_plan: gpu_report.memory_plan,
            gpu_execution_plan: gpu_report.execution_plan,
            gpu_queue_workloads: gpu_report.queue_workloads,
            gpu_transient_memory_bytes: gpu_report.transient_memory_bytes,
            gpu_total_milliseconds: gpu_report.total_gpu_milliseconds,
            module_reports,
            budget_pressure,
            budget_plan,
            asset_streaming,
            schema_compatibility,
        })
    }

    pub fn shutdown(&mut self) {
        for module in &mut self.modules {
            module.shutdown();
        }
    }

    fn commit_commands(
        &mut self,
        commands: Vec<WorldCommand>,
        emitted_events: &mut Vec<WorldEvent>,
    ) -> Result<(), RuntimeError> {
        let produced = self
            .world
            .apply_commands_transactional(commands, self.sim_time.tick)?;
        for event in produced {
            record_report_event(
                &mut self.world,
                &mut self.asset_registry,
                event,
                emitted_events,
            );
        }
        Ok(())
    }

    fn advance_asset_streaming(
        &mut self,
        visible_events: &mut Vec<WorldEvent>,
        emitted_events: &mut Vec<WorldEvent>,
    ) -> AssetStreamingFrameReport {
        let report = self
            .asset_registry
            .advance_streaming_with_budget(self.frame_id, Default::default());
        for completion in &report.completions {
            let event = self.world.system_event(
                self.sim_time.tick,
                Vec3::ZERO,
                Vec::new(),
                WorldEventKind::AssetBecameResident {
                    asset_id: completion.asset_id,
                    quality_tier: completion.quality_tier,
                },
                vec!["streamed_asset".to_string()],
                vec!["asset_streaming".to_string(), "runtime".to_string()],
            );
            record_visible_event(
                &mut self.world,
                &mut self.asset_registry,
                event,
                visible_events,
                emitted_events,
            );
        }
        report
    }

    fn schedule_gpu(&mut self) -> GpuFrameReport {
        let mut graph = self.gpu.begin_frame(self.frame_id);
        let active_module_quality = self.module_quality.clone();
        for module in &mut self.modules {
            let descriptor = module.descriptor();
            let quality_tier = active_module_quality
                .get(&descriptor.module_id)
                .copied()
                .unwrap_or(descriptor.default_quality);
            if quality_tier == QualityTier::Disabled {
                continue;
            }
            let previous_owner = graph.push_component_owner(descriptor.module_id);
            module.schedule_gpu(&mut graph);
            graph.pop_component_owner(previous_owner);
        }
        self.gpu.submit(graph)
    }

    fn apply_budget_plan(&mut self, budget_plan: &BudgetFramePlan) {
        for directive in &budget_plan.directives {
            if directive.recommended_quality != directive.current_quality {
                self.module_quality
                    .insert(directive.module_id, directive.recommended_quality);
            }
        }
    }
}

fn record_report_event(
    world: &mut WorldState,
    asset_registry: &mut AssetRegistry,
    event: WorldEvent,
    emitted_events: &mut Vec<WorldEvent>,
) {
    handle_asset_event(asset_registry, &event);
    world.record_event(event.clone());
    emitted_events.push(event);
}

fn record_visible_event(
    world: &mut WorldState,
    asset_registry: &mut AssetRegistry,
    event: WorldEvent,
    visible_events: &mut Vec<WorldEvent>,
    emitted_events: &mut Vec<WorldEvent>,
) {
    handle_asset_event(asset_registry, &event);
    visible_events.push(event.clone());
    world.record_event(event.clone());
    emitted_events.push(event);
}

fn handle_asset_event(asset_registry: &mut AssetRegistry, event: &WorldEvent) {
    let WorldEventKind::AssetStreamingRequested {
        asset_id,
        requester,
        requested_quality,
        priority,
        reason,
    } = &event.kind
    else {
        return;
    };

    asset_registry.queue_streaming(AssetStreamingRequest {
        asset_id: *asset_id,
        requester: *requester,
        requested_quality: *requested_quality,
        priority: *priority,
        reason: reason.clone(),
    });
}

fn validate_module_replacement(
    previous: ModuleDescriptor,
    replacement: ModuleDescriptor,
) -> Result<(), ModuleReplacementError> {
    if previous.schema.name != replacement.schema.name {
        return Err(ModuleReplacementError::SchemaNameMismatch {
            module_id: previous.module_id,
            expected: previous.schema.name,
            replacement: replacement.schema.name,
        });
    }

    if replacement.schema.version < previous.schema.version {
        return Err(ModuleReplacementError::SchemaVersionTooOld {
            module_id: previous.module_id,
            expected_minimum: previous.schema.version,
            replacement: replacement.schema.version,
        });
    }

    Ok(())
}

fn compare_replay_frame_summaries(
    expected: Option<&ReplayFrameSummary>,
    actual: Option<&ReplayFrameSummary>,
) -> ReplayComparisonStatus {
    let (Some(expected), Some(actual)) = (expected, actual) else {
        return match (expected.is_some(), actual.is_some()) {
            (true, false) => ReplayComparisonStatus::MissingFrame,
            (false, true) => ReplayComparisonStatus::ExtraFrame,
            (false, false) => ReplayComparisonStatus::Matched,
            (true, true) => unreachable!("handled by let-else pattern"),
        };
    };

    if expected.frame_id != actual.frame_id {
        return ReplayComparisonStatus::FrameIdMismatch;
    }
    if expected.tick != actual.tick {
        return ReplayComparisonStatus::TickMismatch;
    }
    if expected.queued_command_count != actual.queued_command_count {
        return ReplayComparisonStatus::CommandCountMismatch;
    }
    if expected.queued_command_fingerprints != actual.queued_command_fingerprints {
        return ReplayComparisonStatus::CommandPayloadMismatch;
    }
    if expected.event_count != actual.event_count {
        return ReplayComparisonStatus::EventCountMismatch;
    }
    if expected.event_fingerprints != actual.event_fingerprints {
        return ReplayComparisonStatus::EventKindMismatch;
    }

    ReplayComparisonStatus::Matched
}

fn replay_event_fingerprint(event: &WorldEvent) -> ReplayEventFingerprint {
    ReplayEventFingerprint {
        event_id: event.event_id,
        tick: event.tick,
        location_meters: replay_vec3_key(event.location_meters),
        actors: event.actors.clone(),
        kind_label: event_kind_label(&event.kind).to_string(),
        kind_fingerprint: format!("{:?}", event.kind),
        evidence_count: event.physical_evidence.len(),
        physical_evidence: event.physical_evidence.clone(),
        narrative_tags: event.narrative_tags.clone(),
    }
}

fn replay_vec3_key(value: Vec3) -> String {
    format!("{:.3},{:.3},{:.3}", value.x, value.y, value.z)
}

fn event_kind_label(kind: &WorldEventKind) -> &'static str {
    match kind {
        WorldEventKind::EntitySpawned { .. } => "EntitySpawned",
        WorldEventKind::EntityDespawned { .. } => "EntityDespawned",
        WorldEventKind::TransformChanged { .. } => "TransformChanged",
        WorldEventKind::NavigationMoveBlocked { .. } => "NavigationMoveBlocked",
        WorldEventKind::AgentStateChanged { .. } => "AgentStateChanged",
        WorldEventKind::ForceApplied { .. } => "ForceApplied",
        WorldEventKind::DamageApplied { .. } => "DamageApplied",
        WorldEventKind::MeshReplaced { .. } => "MeshReplaced",
        WorldEventKind::MaterialStateChanged { .. } => "MaterialStateChanged",
        WorldEventKind::GlassWallFractured { .. } => "GlassWallFractured",
        WorldEventKind::MetalBent { .. } => "MetalBent",
        WorldEventKind::FlexibleConstraintResolved { .. } => "FlexibleConstraintResolved",
        WorldEventKind::PowerTransformerOverheated { .. } => "PowerTransformerOverheated",
        WorldEventKind::StreetFlooded => "StreetFlooded",
        WorldEventKind::ToxicGasReleased => "ToxicGasReleased",
        WorldEventKind::NpcWitnessedCrime { .. } => "NpcWitnessedCrime",
        WorldEventKind::NpcHeardSound { .. } => "NpcHeardSound",
        WorldEventKind::AgentMemoryUpdated { .. } => "AgentMemoryUpdated",
        WorldEventKind::AgentIntentProposed { .. } => "AgentIntentProposed",
        WorldEventKind::AgentDecisionExplained { .. } => "AgentDecisionExplained",
        WorldEventKind::FactionLostTerritory { .. } => "FactionLostTerritory",
        WorldEventKind::FactionReputationChanged { .. } => "FactionReputationChanged",
        WorldEventKind::SecurityAlertRaised { .. } => "SecurityAlertRaised",
        WorldEventKind::SurveillanceIncreased { .. } => "SurveillanceIncreased",
        WorldEventKind::VoiceLineSpoken { .. } => "VoiceLineSpoken",
        WorldEventKind::SpeechSynthesized { .. } => "SpeechSynthesized",
        WorldEventKind::FacialAnimationApplied { .. } => "FacialAnimationApplied",
        WorldEventKind::HumanAppearanceUpdated { .. } => "HumanAppearanceUpdated",
        WorldEventKind::PlayerIdentityExposed => "PlayerIdentityExposed",
        WorldEventKind::SoundEmitted { .. } => "SoundEmitted",
        WorldEventKind::AudioFrameMixed { .. } => "AudioFrameMixed",
        WorldEventKind::DialogueEmitted { .. } => "DialogueEmitted",
        WorldEventKind::StoryEventEmitted { .. } => "StoryEventEmitted",
        WorldEventKind::AssetStreamingRequested { .. } => "AssetStreamingRequested",
        WorldEventKind::AssetBecameResident { .. } => "AssetBecameResident",
        WorldEventKind::Custom(_) => "Custom",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::BudgetDirectiveAction;
    use crate::module_abi::{
        DYNAMIC_ASSET_KIND_GENERATED_BUNDLE, DYNAMIC_ASSET_KIND_SHADER,
        DYNAMIC_ASSET_LOAD_UNLOADED, DYNAMIC_ASSET_MAX_DEPENDENCIES, DYNAMIC_GPU_DISPATCH_COMPUTE,
        DYNAMIC_GPU_MAX_PASS_RESOURCES, DYNAMIC_GPU_PIPELINE_COMPUTE, DYNAMIC_GPU_QUEUE_COMPUTE,
        DYNAMIC_GPU_RESOURCE_KIND_BUFFER, DYNAMIC_GPU_RESOURCE_LIFETIME_IMPORTED,
        DYNAMIC_GPU_RESOURCE_LIFETIME_TRANSIENT, DYNAMIC_GRAPH_STATUS_OK, DYNAMIC_HOST_STATUS_OK,
        DYNAMIC_MODULE_TEXT_BYTES, DYNAMIC_SINK_STATUS_OK, DynamicAssetRegistrationResultV1,
        DynamicAssetRegistrationV1, DynamicCommandSinkV1, DynamicCustomEventV1,
        DynamicEngineHostV1, DynamicFrameInputV1, DynamicGeneratedAssetRecipeV1,
        DynamicGpuGraphBuilderV1, DynamicGpuPassV1, DynamicGpuPipelineResultV1,
        DynamicGpuPipelineV1, DynamicGpuResourceResultV1, DynamicGpuResourceV1, DynamicModuleApiV1,
        DynamicModuleHandle, DynamicModuleHandlersV1, DynamicSchemaMigrationStepV1,
        DynamicSchemaRegistrationV1, DynamicU128V1, DynamicVec3V1,
        ashfall_dynamic_gpu_graph_add_pass_v1, ashfall_dynamic_gpu_graph_create_pipeline_v1,
        ashfall_dynamic_gpu_graph_declare_resource_v1, ashfall_dynamic_sink_push_custom_event_v1,
        quality_tier_to_abi,
    };
    use crate::schema::{SchemaCompatibilitySeverity, SchemaMigrationStep, SchemaRequirement};
    use crate::world::{EntityTemplate, EventFilter, PhysicalEventType};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static DYNAMIC_INIT_COUNT: AtomicUsize = AtomicUsize::new(0);
    static DYNAMIC_TICK_COUNT: AtomicUsize = AtomicUsize::new(0);
    static DYNAMIC_GPU_COUNT: AtomicUsize = AtomicUsize::new(0);
    static DYNAMIC_SHUTDOWN_COUNT: AtomicUsize = AtomicUsize::new(0);
    static DYNAMIC_HOST_API_VERSION: AtomicUsize = AtomicUsize::new(0);
    static DYNAMIC_TICK_FRAME: AtomicUsize = AtomicUsize::new(0);
    static DYNAMIC_TICK_TICK: AtomicUsize = AtomicUsize::new(0);
    static DYNAMIC_TICK_EVENT_COUNT: AtomicUsize = AtomicUsize::new(0);
    static DYNAMIC_TICK_FORCE_COUNT: AtomicUsize = AtomicUsize::new(0);
    static DYNAMIC_SINK_HANDLE: AtomicUsize = AtomicUsize::new(0);
    static DYNAMIC_GPU_FRAME: AtomicUsize = AtomicUsize::new(0);

    struct ProducerModule;

    impl EngineModule for ProducerModule {
        fn descriptor(&self) -> ModuleDescriptor {
            ModuleDescriptor {
                module_id: 1,
                name: "producer",
                schema: SchemaVersion {
                    name: "ProducerOutput",
                    version: 1,
                },
                default_quality: QualityTier::BackgroundApproximation,
            }
        }
    }

    struct ConsumerModule {
        required_schema: &'static str,
    }

    impl EngineModule for ConsumerModule {
        fn descriptor(&self) -> ModuleDescriptor {
            ModuleDescriptor {
                module_id: 2,
                name: "consumer",
                schema: SchemaVersion {
                    name: "ConsumerOutput",
                    version: 1,
                },
                default_quality: QualityTier::BackgroundApproximation,
            }
        }

        fn schema_requirements(&self) -> Vec<SchemaRequirement> {
            vec![SchemaRequirement::required(
                2,
                self.required_schema,
                1,
                "consumer test dependency",
            )]
        }
    }

    struct MigratingProducerModule;

    impl EngineModule for MigratingProducerModule {
        fn descriptor(&self) -> ModuleDescriptor {
            ModuleDescriptor {
                module_id: 11,
                name: "migrating_producer",
                schema: SchemaVersion {
                    name: "MigratingOutput",
                    version: 1,
                },
                default_quality: QualityTier::BackgroundApproximation,
            }
        }

        fn init(&mut self, ctx: &mut EngineContext<'_>) {
            ctx.schemas.register_migration(SchemaMigrationStep {
                schema_name: "MigratingOutput".to_string(),
                from_version: 1,
                to_version: 2,
                description: "add stable debug metadata".to_string(),
                lossless: true,
            });
        }
    }

    struct V2ConsumerModule;

    impl EngineModule for V2ConsumerModule {
        fn descriptor(&self) -> ModuleDescriptor {
            ModuleDescriptor {
                module_id: 12,
                name: "v2_consumer",
                schema: SchemaVersion {
                    name: "V2ConsumerOutput",
                    version: 1,
                },
                default_quality: QualityTier::BackgroundApproximation,
            }
        }

        fn schema_requirements(&self) -> Vec<SchemaRequirement> {
            vec![SchemaRequirement::required(
                12,
                "MigratingOutput",
                2,
                "consumer can read migrated producer output",
            )]
        }
    }

    struct OverBudgetModule {
        quality: QualityTier,
        counters: PerformanceCounters,
    }

    impl EngineModule for OverBudgetModule {
        fn descriptor(&self) -> ModuleDescriptor {
            ModuleDescriptor {
                module_id: 3,
                name: "over_budget",
                schema: SchemaVersion {
                    name: "OverBudgetOutput",
                    version: 1,
                },
                default_quality: self.quality,
            }
        }

        fn performance_counters(&self) -> PerformanceCounters {
            self.counters.clone()
        }
    }

    struct ProbeModule {
        module_id: ModuleId,
        name: &'static str,
        schema_name: &'static str,
        schema_version: u32,
        init_count: Arc<AtomicUsize>,
        shutdown_count: Arc<AtomicUsize>,
    }

    impl ProbeModule {
        fn new(
            module_id: ModuleId,
            name: &'static str,
            schema_name: &'static str,
            schema_version: u32,
            init_count: Arc<AtomicUsize>,
            shutdown_count: Arc<AtomicUsize>,
        ) -> Self {
            Self {
                module_id,
                name,
                schema_name,
                schema_version,
                init_count,
                shutdown_count,
            }
        }
    }

    struct StateProbeModule {
        value: Arc<AtomicUsize>,
    }

    impl EngineModule for StateProbeModule {
        fn descriptor(&self) -> ModuleDescriptor {
            ModuleDescriptor {
                module_id: 8,
                name: "state_probe",
                schema: SchemaVersion {
                    name: "StateProbeOutput",
                    version: 1,
                },
                default_quality: QualityTier::BackgroundApproximation,
            }
        }

        fn save_state(&self) -> Option<ModuleStateRecord> {
            let descriptor = self.descriptor();
            Some(ModuleStateRecord::new(
                descriptor.module_id,
                descriptor.schema,
                1,
                vec![format!("value:{}", self.value.load(Ordering::SeqCst))],
            ))
        }

        fn load_state(&mut self, state: &ModuleStateRecord) {
            if state.state_version != 1 {
                return;
            }
            if let Some(value) = state
                .entries_with_prefix("value:")
                .next()
                .and_then(|value| value.parse::<usize>().ok())
            {
                self.value.store(value, Ordering::SeqCst);
            }
        }
    }

    unsafe extern "C" fn dynamic_init(host: *mut DynamicEngineHostV1) -> DynamicModuleHandle {
        assert!(!host.is_null());
        let host_ptr = host;
        let host = unsafe { &*host_ptr };
        DYNAMIC_INIT_COUNT.fetch_add(1, Ordering::SeqCst);
        DYNAMIC_HOST_API_VERSION.store(host.api_version as usize, Ordering::SeqCst);
        let schema = DynamicSchemaRegistrationV1 {
            name: dynamic_text("DynamicProbeDebug"),
            version: 1,
        };
        let migration = DynamicSchemaMigrationStepV1 {
            schema_name: dynamic_text("DynamicProbeDebug"),
            from_version: 1,
            to_version: 2,
            description: dynamic_text("add dynamic debug metadata"),
            lossless: 1,
            _reserved: [0; 7],
        };
        let asset = DynamicAssetRegistrationV1 {
            kind: DYNAMIC_ASSET_KIND_SHADER,
            label: dynamic_text("dynamic shader"),
            provenance: dynamic_text("dynamic module init"),
            quality_tier: quality_tier_to_abi(QualityTier::NormalRuntime),
            load_state: DYNAMIC_ASSET_LOAD_UNLOADED,
            byte_len: 2048,
            has_byte_len: 1,
            ..DynamicAssetRegistrationV1::default()
        };
        let mut asset_result = DynamicAssetRegistrationResultV1::default();
        let schema_status = unsafe { ashfall_dynamic_host_register_schema_v1(host_ptr, &schema) };
        let migration_status =
            unsafe { ashfall_dynamic_host_register_schema_migration_v1(host_ptr, &migration) };
        let asset_status =
            unsafe { ashfall_dynamic_host_register_asset_v1(host_ptr, &asset, &mut asset_result) };
        assert_eq!(schema_status, DYNAMIC_HOST_STATUS_OK);
        assert_eq!(migration_status, DYNAMIC_HOST_STATUS_OK);
        assert_eq!(asset_status, DYNAMIC_HOST_STATUS_OK);
        assert_ne!(asset_result.asset_id.to_u128(), 0);

        let mut source_assets = [DynamicU128V1::default(); DYNAMIC_ASSET_MAX_DEPENDENCIES];
        source_assets[0] = asset_result.asset_id;
        let generated_recipe = DynamicGeneratedAssetRecipeV1 {
            kind: DYNAMIC_ASSET_KIND_GENERATED_BUNDLE,
            label: dynamic_text("dynamic generated bundle"),
            recipe_schema: dynamic_text("DynamicRecipe"),
            recipe_version: 1,
            recipe_key: dynamic_text("dynamic-probe-cache"),
            source_asset_count: 1,
            source_assets,
            quality_tier: quality_tier_to_abi(QualityTier::NormalRuntime),
            byte_len: 4096,
            has_byte_len: 1,
            _reserved: [0; 7],
        };
        let mut generated_result = DynamicAssetRegistrationResultV1::default();
        let generated_status = unsafe {
            ashfall_dynamic_host_register_generated_asset_v1(
                host_ptr,
                &generated_recipe,
                &mut generated_result,
            )
        };
        assert_eq!(generated_status, DYNAMIC_HOST_STATUS_OK);
        assert_ne!(generated_result.asset_id.to_u128(), 0);
        DynamicModuleHandle::new(77)
    }

    unsafe extern "C" fn dynamic_tick(
        module: DynamicModuleHandle,
        frame: *const DynamicFrameInputV1,
        out: *mut DynamicCommandSinkV1,
    ) {
        assert_eq!(module, DynamicModuleHandle::new(77));
        assert!(!frame.is_null());
        assert!(!out.is_null());
        let frame = unsafe { &*frame };
        let out = unsafe { &mut *out };
        DYNAMIC_TICK_COUNT.fetch_add(1, Ordering::SeqCst);
        DYNAMIC_TICK_FRAME.store(frame.frame_id as usize, Ordering::SeqCst);
        DYNAMIC_TICK_TICK.store(frame.tick as usize, Ordering::SeqCst);
        DYNAMIC_TICK_EVENT_COUNT.store(frame.recent_event_count as usize, Ordering::SeqCst);
        DYNAMIC_TICK_FORCE_COUNT.store(frame.force_count as usize, Ordering::SeqCst);
        DYNAMIC_SINK_HANDLE.store(out.sink_handle, Ordering::SeqCst);
        let mut label = [0; DYNAMIC_MODULE_TEXT_BYTES];
        let label_bytes = b"dynamic_probe_tick";
        label[..label_bytes.len()].copy_from_slice(label_bytes);
        let event = DynamicCustomEventV1 {
            event_id: DynamicU128V1::from_u128(((frame.tick as u128) << 64) | 700),
            tick: frame.tick,
            location_meters: DynamicVec3V1::default(),
            label,
            ..DynamicCustomEventV1::default()
        };
        let status = unsafe { ashfall_dynamic_sink_push_custom_event_v1(out, &event) };
        assert_eq!(status, DYNAMIC_SINK_STATUS_OK);
    }

    unsafe extern "C" fn dynamic_schedule_gpu(
        module: DynamicModuleHandle,
        graph: *mut DynamicGpuGraphBuilderV1,
    ) {
        assert_eq!(module, DynamicModuleHandle::new(77));
        assert!(!graph.is_null());
        let graph = unsafe { &mut *graph };
        DYNAMIC_GPU_COUNT.fetch_add(1, Ordering::SeqCst);
        DYNAMIC_GPU_FRAME.store(graph.frame_id as usize, Ordering::SeqCst);
        let mut name = [0; DYNAMIC_MODULE_TEXT_BYTES];
        let name_bytes = b"dynamic_probe_gpu";
        name[..name_bytes.len()].copy_from_slice(name_bytes);
        let source = DynamicGpuResourceV1 {
            label: dynamic_text("dynamic gpu input"),
            kind: DYNAMIC_GPU_RESOURCE_KIND_BUFFER,
            byte_len: 2048,
            owner: 70,
            has_owner: 1,
            lifetime: DYNAMIC_GPU_RESOURCE_LIFETIME_IMPORTED,
            bindless: 1,
            _reserved: [0; 2],
        };
        let target = DynamicGpuResourceV1 {
            label: dynamic_text("dynamic gpu output"),
            kind: DYNAMIC_GPU_RESOURCE_KIND_BUFFER,
            byte_len: 4096,
            owner: 70,
            has_owner: 1,
            lifetime: DYNAMIC_GPU_RESOURCE_LIFETIME_TRANSIENT,
            bindless: 0,
            _reserved: [0; 2],
        };
        let mut source_result = DynamicGpuResourceResultV1::default();
        let mut target_result = DynamicGpuResourceResultV1::default();
        let source_status = unsafe {
            ashfall_dynamic_gpu_graph_declare_resource_v1(graph, &source, &mut source_result)
        };
        let target_status = unsafe {
            ashfall_dynamic_gpu_graph_declare_resource_v1(graph, &target, &mut target_result)
        };
        assert_eq!(source_status, DYNAMIC_GRAPH_STATUS_OK);
        assert_eq!(target_status, DYNAMIC_GRAPH_STATUS_OK);
        assert_eq!(source_result.has_bindless_index, 1);
        let pipeline = DynamicGpuPipelineV1 {
            label: dynamic_text("dynamic probe pipeline"),
            shader_key: dynamic_text("dynamic/probe.comp"),
            kind: DYNAMIC_GPU_PIPELINE_COMPUTE,
            quality_tier: quality_tier_to_abi(QualityTier::BackgroundApproximation),
        };
        let mut pipeline_result = DynamicGpuPipelineResultV1::default();
        let pipeline_status = unsafe {
            ashfall_dynamic_gpu_graph_create_pipeline_v1(graph, &pipeline, &mut pipeline_result)
        };
        assert_eq!(pipeline_status, DYNAMIC_GRAPH_STATUS_OK);
        assert_ne!(pipeline_result.pipeline.to_u128(), 0);
        let mut reads = [DynamicU128V1::default(); DYNAMIC_GPU_MAX_PASS_RESOURCES];
        let mut writes = [DynamicU128V1::default(); DYNAMIC_GPU_MAX_PASS_RESOURCES];
        reads[0] = source_result.resource;
        writes[0] = target_result.resource;
        let pass = DynamicGpuPassV1 {
            name,
            queue: DYNAMIC_GPU_QUEUE_COMPUTE,
            dispatch_kind: DYNAMIC_GPU_DISPATCH_COMPUTE,
            pipeline: pipeline_result.pipeline,
            budget_hint: quality_tier_to_abi(QualityTier::BackgroundApproximation),
            read_count: 1,
            reads,
            write_count: 1,
            writes,
        };
        let status = unsafe { ashfall_dynamic_gpu_graph_add_pass_v1(graph, &pass) };
        assert_eq!(status, DYNAMIC_GRAPH_STATUS_OK);
    }

    unsafe extern "C" fn dynamic_shutdown(module: DynamicModuleHandle) {
        assert_eq!(module, DynamicModuleHandle::new(77));
        DYNAMIC_SHUTDOWN_COUNT.fetch_add(1, Ordering::SeqCst);
    }

    fn dynamic_handlers() -> DynamicModuleHandlersV1 {
        DynamicModuleHandlersV1 {
            init: Some(dynamic_init),
            tick: Some(dynamic_tick),
            schedule_gpu: Some(dynamic_schedule_gpu),
            shutdown: Some(dynamic_shutdown),
        }
    }

    fn reset_dynamic_counters() {
        for counter in [
            &DYNAMIC_INIT_COUNT,
            &DYNAMIC_TICK_COUNT,
            &DYNAMIC_GPU_COUNT,
            &DYNAMIC_SHUTDOWN_COUNT,
            &DYNAMIC_HOST_API_VERSION,
            &DYNAMIC_TICK_FRAME,
            &DYNAMIC_TICK_TICK,
            &DYNAMIC_TICK_EVENT_COUNT,
            &DYNAMIC_TICK_FORCE_COUNT,
            &DYNAMIC_SINK_HANDLE,
            &DYNAMIC_GPU_FRAME,
        ] {
            counter.store(0, Ordering::SeqCst);
        }
    }

    fn dynamic_text(value: &str) -> [u8; DYNAMIC_MODULE_TEXT_BYTES] {
        let mut out = [0; DYNAMIC_MODULE_TEXT_BYTES];
        let copy_len = value.len().min(DYNAMIC_MODULE_TEXT_BYTES - 1);
        out[..copy_len].copy_from_slice(&value.as_bytes()[..copy_len]);
        out
    }

    struct InvalidCommandModule;

    impl EngineModule for InvalidCommandModule {
        fn descriptor(&self) -> ModuleDescriptor {
            ModuleDescriptor {
                module_id: 6,
                name: "invalid_command_module",
                schema: SchemaVersion {
                    name: "InvalidCommandOutput",
                    version: 1,
                },
                default_quality: QualityTier::BackgroundApproximation,
            }
        }

        fn tick(&mut self, frame: &FrameContext, out: &mut CommandSink) {
            out.event(WorldEvent {
                event_id: 1,
                tick: frame.sim_time.tick,
                location_meters: Vec3::ZERO,
                actors: Vec::new(),
                kind: WorldEventKind::Custom("should_not_commit".to_string()),
                physical_evidence: Vec::new(),
                narrative_tags: vec!["invalid_module".to_string()],
            });
            out.command(WorldCommand::ApplyForce(ForceCommand {
                entity: 999,
                vector_newtons: Vec3::new(1.0, 0.0, 0.0),
                impulse_newton_seconds: 1.0,
                source: None,
            }));
        }
    }

    struct InvalidEventModule;

    impl EngineModule for InvalidEventModule {
        fn descriptor(&self) -> ModuleDescriptor {
            ModuleDescriptor {
                module_id: 7,
                name: "invalid_event_module",
                schema: SchemaVersion {
                    name: "InvalidEventOutput",
                    version: 1,
                },
                default_quality: QualityTier::BackgroundApproximation,
            }
        }

        fn tick(&mut self, frame: &FrameContext, out: &mut CommandSink) {
            out.event(WorldEvent {
                event_id: 2,
                tick: frame.sim_time.tick,
                location_meters: Vec3::ZERO,
                actors: vec![999],
                kind: WorldEventKind::AgentIntentProposed {
                    agent: 999,
                    action: "Teleport".to_string(),
                    source_event: None,
                    validation_passed: true,
                },
                physical_evidence: vec!["invalid_event".to_string()],
                narrative_tags: vec!["invalid_module".to_string()],
            });
        }
    }

    impl EngineModule for ProbeModule {
        fn descriptor(&self) -> ModuleDescriptor {
            ModuleDescriptor {
                module_id: self.module_id,
                name: self.name,
                schema: SchemaVersion {
                    name: self.schema_name,
                    version: self.schema_version,
                },
                default_quality: QualityTier::BackgroundApproximation,
            }
        }

        fn init(&mut self, _ctx: &mut EngineContext<'_>) {
            self.init_count.fetch_add(1, Ordering::SeqCst);
        }

        fn shutdown(&mut self) {
            self.shutdown_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn runtime_rejects_missing_required_schema_before_frame_runs() {
        let mut runtime = EngineRuntime::new(WorldState::default());
        runtime.register_module(ConsumerModule {
            required_schema: "MissingOutput",
        });
        runtime.init_modules();

        let error = runtime.step().expect_err("schema preflight should fail");

        let RuntimeError::Schema(report) = error else {
            panic!("expected schema compatibility error");
        };
        assert!(!report.passed);
        assert_eq!(runtime.replay_log().frames().len(), 0);
        assert!(report.issues.iter().any(|issue| {
            issue.severity == SchemaCompatibilitySeverity::Error && issue.code == "schema_missing"
        }));
    }

    #[test]
    fn runtime_includes_schema_compatibility_report_in_successful_frame() {
        let mut runtime = EngineRuntime::new(WorldState::default());
        runtime.register_module(ProducerModule);
        runtime.register_module(ConsumerModule {
            required_schema: "ProducerOutput",
        });
        runtime.init_modules();

        let registry = runtime.module_registry_report();
        assert_eq!(registry.static_module_count, 2);
        assert_eq!(
            registry.dynamic_module_api_version,
            ASHFALL_DYNAMIC_MODULE_API_VERSION
        );
        assert_eq!(registry.modules[1].schema_requirements.len(), 1);
        assert!(registry.schema_compatibility.passed);

        let report = runtime.step().expect("schemas should be compatible");

        assert!(report.schema_compatibility.passed);
        assert_eq!(report.schema_compatibility.checked_requirements, 1);
        assert!(report.schema_compatibility.issues.is_empty());
        assert_eq!(report.profiler.frame_id, report.frame_id);
        assert_eq!(report.profiler.module_costs.len(), 2);
        assert_eq!(report.profiler.budget_pressure_count, 0);
        assert_eq!(report.budget_plan.total_pressure_count, 0);
        assert!(report.budget_plan.demotions().next().is_none());
    }

    #[test]
    fn runtime_accepts_lossless_schema_migration_policy() {
        let mut runtime = EngineRuntime::new(WorldState::default());
        runtime.register_module(MigratingProducerModule);
        runtime.register_module(V2ConsumerModule);
        runtime.init_modules();

        let registry = runtime.module_registry_report();
        assert!(registry.schema_compatibility.passed);
        assert!(registry.schema_compatibility.issues.iter().any(|issue| {
            issue.code == "schema_migration_available"
                && issue.schema_name == "MigratingOutput"
                && issue.migration_path.len() == 1
        }));

        let report = runtime
            .step()
            .expect("lossless schema migration should satisfy preflight");

        assert!(report.schema_compatibility.passed);
        assert!(report.schema_compatibility.issues.iter().any(|issue| {
            issue.code == "schema_migration_available"
                && issue.severity == SchemaCompatibilitySeverity::Warning
        }));
    }

    #[test]
    fn runtime_registers_and_drives_dynamic_module_lifecycle() {
        reset_dynamic_counters();
        let api = DynamicModuleApiV1::from_parts(
            70,
            "dynamic_probe",
            "DynamicProbeOutput",
            1,
            QualityTier::NormalRuntime,
            dynamic_handlers(),
        );
        let mut runtime = EngineRuntime::new(WorldState::default());

        let descriptor = runtime
            .register_dynamic_module(api)
            .expect("valid dynamic API should register");
        assert_eq!(descriptor.module_id, 70);
        assert_eq!(descriptor.name, "dynamic_probe");
        assert_eq!(descriptor.schema.name, "DynamicProbeOutput");
        assert_eq!(runtime.module_quality(70), Some(QualityTier::NormalRuntime));
        assert_eq!(
            runtime.set_module_quality(70, QualityTier::ReferenceOfflineValidation),
            Some(QualityTier::NormalRuntime)
        );
        assert_eq!(
            runtime.module_quality(70),
            Some(QualityTier::ReferenceOfflineValidation)
        );
        assert_eq!(runtime.set_module_quality(404, QualityTier::Disabled), None);
        assert_eq!(
            runtime.set_module_quality(70, QualityTier::NormalRuntime),
            Some(QualityTier::ReferenceOfflineValidation)
        );

        runtime.init_modules();
        assert!(
            runtime
                .schemas()
                .require_at_least("DynamicProbeDebug", 1)
                .is_ok()
        );
        assert_eq!(
            runtime
                .schemas()
                .migration_path("DynamicProbeDebug", 1, 2)
                .expect("dynamic migration should be registered")
                .len(),
            1
        );
        assert!(runtime.assets().all().any(|asset| {
            asset.label == "dynamic shader" && asset.load_state == AssetLoadState::Unloaded
        }));
        assert_eq!(runtime.assets().generated_cache_len(), 1);
        let report = runtime.step().expect("dynamic module frame should run");

        assert_eq!(DYNAMIC_INIT_COUNT.load(Ordering::SeqCst), 1);
        assert_eq!(
            DYNAMIC_HOST_API_VERSION.load(Ordering::SeqCst),
            ASHFALL_DYNAMIC_MODULE_API_VERSION as usize
        );
        assert_eq!(DYNAMIC_TICK_COUNT.load(Ordering::SeqCst), 1);
        assert_eq!(DYNAMIC_TICK_FRAME.load(Ordering::SeqCst), 1);
        assert_eq!(DYNAMIC_TICK_TICK.load(Ordering::SeqCst), 1);
        assert_eq!(DYNAMIC_TICK_EVENT_COUNT.load(Ordering::SeqCst), 0);
        assert_eq!(DYNAMIC_TICK_FORCE_COUNT.load(Ordering::SeqCst), 0);
        assert_ne!(DYNAMIC_SINK_HANDLE.load(Ordering::SeqCst), 0);
        assert_eq!(DYNAMIC_GPU_COUNT.load(Ordering::SeqCst), 1);
        assert_eq!(DYNAMIC_GPU_FRAME.load(Ordering::SeqCst), 1);
        assert!(
            report
                .gpu_passes
                .iter()
                .any(|pass| pass.name == "dynamic_probe_gpu")
        );
        assert_eq!(report.gpu_pipeline_report.unknown_pipeline_count, 0);
        assert!(report.gpu_pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "dynamic probe pipeline"
                && pipeline.shader_key == "dynamic/probe.comp"
                && pipeline.pass_names == vec!["dynamic_probe_gpu".to_string()]
        }));
        assert!(report.gpu_registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(70)
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "dynamic_probe_gpu")
        }));
        assert!(report.module_reports.iter().any(|module| {
            module.descriptor.module_id == 70 && module.descriptor.name == "dynamic_probe"
        }));
        assert!(report.events.iter().any(|event| {
            matches!(&event.kind, WorldEventKind::Custom(label) if label == "dynamic_probe_tick")
        }));

        runtime.shutdown();
        assert_eq!(DYNAMIC_SHUTDOWN_COUNT.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn runtime_rejects_invalid_dynamic_module_registration() {
        let mut runtime = EngineRuntime::new(WorldState::default());
        let api = DynamicModuleApiV1::from_parts(
            0,
            "",
            "BrokenOutput",
            0,
            QualityTier::NormalRuntime,
            DynamicModuleHandlersV1::default(),
        );

        let error = runtime
            .register_dynamic_module(api)
            .expect_err("invalid ABI should be rejected before registration");

        assert!(matches!(
            error,
            DynamicModuleRegistrationError::Validation(report)
                if !report.passed
                    && report
                        .issues
                        .iter()
                        .any(|issue| issue.code == "missing_module_id")
        ));
        assert!(runtime.module_descriptors().is_empty());
    }

    #[test]
    fn render_frame_tracks_variable_time_without_advancing_simulation() {
        let mut runtime = EngineRuntime::new(WorldState::default());
        let half_tick = runtime.fixed_dt_seconds() * 0.5;

        let first = runtime.render_frame(half_tick);
        let second = runtime.render_frame(half_tick * 0.5);

        assert_eq!(first.render_frame_id, 1);
        assert_eq!(second.render_frame_id, 2);
        assert_eq!(first.sim_time.tick, 0);
        assert_eq!(second.sim_time.tick, 0);
        assert_eq!(runtime.replay_log().len(), 0);
        assert!((first.interpolation_alpha - 0.5).abs() < f32::EPSILON);
        assert!((second.interpolation_alpha - 0.75).abs() < f32::EPSILON);
        assert_eq!(second.snapshot.frame_id, 0);
    }

    #[test]
    fn fixed_simulation_step_consumes_render_interpolation_accumulator() {
        let mut runtime = EngineRuntime::new(WorldState::default());
        runtime.render_frame(runtime.fixed_dt_seconds() * 0.5);

        let sim_report = runtime.step().expect("fixed simulation step should run");
        let render_report = runtime.render_frame(0.0);

        assert_eq!(sim_report.sim_time.tick, 1);
        assert_eq!(render_report.sim_time.tick, 1);
        assert_eq!(render_report.sim_frame_id, sim_report.frame_id);
        assert_eq!(render_report.interpolation_alpha, 0.0);
        assert_eq!(render_report.snapshot.frame_id, sim_report.frame_id);
    }

    #[test]
    fn runtime_save_data_restores_clock_replay_render_and_queued_commands() {
        let mut world = WorldState::default();
        let entity = world
            .spawn_entity_template(EntityTemplate::new(
                "runtime save prop",
                Transform::at(Vec3::ZERO),
            ))
            .expect("entity should spawn");
        let mut runtime = EngineRuntime::new(world);
        runtime.queue_command(WorldCommand::ApplyForce(ForceCommand {
            entity,
            vector_newtons: Vec3::new(2.0, 0.0, 0.0),
            impulse_newton_seconds: 1.0,
            source: None,
        }));
        runtime.step().expect("first frame should run");
        runtime.render_frame(runtime.fixed_dt_seconds() * 0.5);
        runtime.queue_command(WorldCommand::SetTransform(
            entity,
            Transform::at(Vec3::new(4.0, 0.0, 0.0)),
        ));

        let save_data = runtime.save_runtime();
        assert_eq!(save_data.frame_id, 1);
        assert_eq!(save_data.render_frame_id, 1);
        assert_eq!(save_data.sim_time.tick, 1);
        assert_eq!(save_data.queued_commands.len(), 1);
        assert_eq!(save_data.replay_log.len(), 1);

        let mut restored = EngineRuntime::new(WorldState::default());
        restored.load_runtime(save_data);
        let render_frame = restored.render_frame(0.0);

        assert_eq!(render_frame.render_frame_id, 2);
        assert_eq!(render_frame.sim_frame_id, 1);
        assert_eq!(render_frame.sim_time.tick, 1);
        assert!((render_frame.interpolation_alpha - 0.5).abs() < f32::EPSILON);
        assert_eq!(restored.replay_log().len(), 1);

        let report = restored
            .step()
            .expect("queued transform from save should apply");
        assert_eq!(report.sim_time.tick, 2);
        assert!(report
            .events
            .iter()
            .any(|event| matches!(event.kind, WorldEventKind::TransformChanged { entity: id } if id == entity)));
    }

    #[test]
    fn runtime_save_data_restores_registered_module_state() {
        let saved_value = Arc::new(AtomicUsize::new(37));
        let mut runtime = EngineRuntime::new(WorldState::default());
        runtime.register_module(StateProbeModule {
            value: Arc::clone(&saved_value),
        });
        runtime.init_modules();

        let save_data = runtime.save_runtime();
        assert_eq!(save_data.module_states.len(), 1);
        assert_eq!(save_data.module_states[0].module_id, 8);

        let restored_value = Arc::new(AtomicUsize::new(0));
        let mut restored = EngineRuntime::new(WorldState::default());
        restored.register_module(StateProbeModule {
            value: Arc::clone(&restored_value),
        });
        restored.init_modules();
        restored.load_runtime(save_data);

        assert_eq!(restored_value.load(Ordering::SeqCst), 37);
    }

    #[test]
    fn queued_command_batch_rejects_without_partial_world_mutation() {
        let mut world = WorldState::default();
        let entity = world
            .spawn_entity_template(EntityTemplate::new(
                "transaction prop",
                Transform::at(Vec3::ZERO),
            ))
            .expect("entity should spawn");
        let mut runtime = EngineRuntime::new(world);
        runtime.queue_command(WorldCommand::ApplyForce(ForceCommand {
            entity,
            vector_newtons: Vec3::new(1.0, 0.0, 0.0),
            impulse_newton_seconds: 1.0,
            source: None,
        }));
        runtime.queue_command(WorldCommand::ApplyForce(ForceCommand {
            entity: 999,
            vector_newtons: Vec3::new(1.0, 0.0, 0.0),
            impulse_newton_seconds: 1.0,
            source: None,
        }));

        let error = runtime
            .step()
            .expect_err("queued batch should fail before mutation");

        assert!(matches!(
            error,
            RuntimeError::Command(CommandError::MissingEntity(999))
        ));
        assert!(runtime.world.save_data().pending_forces.is_empty());
        assert!(runtime.world.event_ledger.all().is_empty());
        assert_eq!(runtime.render_frame(0.0).sim_time.tick, 0);
        assert!(runtime.replay_log().is_empty());
    }

    #[test]
    fn invalid_module_commands_do_not_commit_direct_events() {
        let mut runtime = EngineRuntime::new(WorldState::default());
        runtime.register_module(InvalidCommandModule);
        runtime.init_modules();

        let error = runtime
            .step()
            .expect_err("module command should fail validation");

        assert!(matches!(
            error,
            RuntimeError::Command(CommandError::MissingEntity(999))
        ));
        assert!(runtime.world.event_ledger.all().is_empty());
        assert!(runtime.replay_log().is_empty());
    }

    #[test]
    fn invalid_module_events_do_not_commit_to_ledger() {
        let mut runtime = EngineRuntime::new(WorldState::default());
        runtime.register_module(InvalidEventModule);
        runtime.init_modules();

        let error = runtime
            .step()
            .expect_err("module event should fail validation");

        assert!(matches!(
            error,
            RuntimeError::Event(EventValidationError::MissingEntity { entity: 999, .. })
        ));
        assert!(runtime.world.event_ledger.all().is_empty());
        assert!(runtime.replay_log().is_empty());
    }

    #[test]
    fn runtime_budget_plan_recommends_quality_control_for_over_budget_modules() {
        let mut runtime = EngineRuntime::new(WorldState::default());
        runtime.register_module(OverBudgetModule {
            quality: QualityTier::BackgroundApproximation,
            counters: PerformanceCounters {
                cpu_milliseconds: 1.0,
                gpu_milliseconds: 0.0,
                memory_bytes: 1024,
            },
        });
        runtime.init_modules();

        let report = runtime.step().expect("frame should run");
        let directive = report
            .budget_plan
            .directive_for(3)
            .expect("module should have a budget directive");

        assert_eq!(report.budget_pressure.len(), 1);
        assert_eq!(report.budget_plan.total_pressure_count, 1);
        assert_eq!(report.budget_plan.critical_pressure_count, 1);
        assert_eq!(directive.action, BudgetDirectiveAction::Disable);
        assert_eq!(directive.recommended_quality, QualityTier::Disabled);
        assert_eq!(runtime.module_quality(3), Some(QualityTier::Disabled));
    }

    #[test]
    fn runtime_applies_budget_quality_to_following_frame() {
        let mut runtime = EngineRuntime::new(WorldState::default());
        runtime.register_module(OverBudgetModule {
            quality: QualityTier::BackgroundApproximation,
            counters: PerformanceCounters {
                cpu_milliseconds: 1.0,
                gpu_milliseconds: 0.0,
                memory_bytes: 1024,
            },
        });
        runtime.init_modules();

        runtime
            .step()
            .expect("first frame should issue quality directive");
        let second = runtime
            .step()
            .expect("second frame should honor disabled quality");
        let module_report = second
            .module_reports
            .iter()
            .find(|report| report.descriptor.module_id == 3)
            .expect("module report should exist");
        let directive = second
            .budget_plan
            .directive_for(3)
            .expect("budget directive should exist");

        assert_eq!(module_report.quality_tier, QualityTier::Disabled);
        assert_eq!(module_report.counters, PerformanceCounters::default());
        assert!(module_report.pressure.is_empty());
        assert_eq!(directive.action, BudgetDirectiveAction::Maintain);
        assert_eq!(runtime.module_quality(3), Some(QualityTier::Disabled));
    }

    #[test]
    fn runtime_replaces_registered_module_with_compatible_stub() {
        let old_init_count = Arc::new(AtomicUsize::new(0));
        let old_shutdown_count = Arc::new(AtomicUsize::new(0));
        let new_init_count = Arc::new(AtomicUsize::new(0));
        let new_shutdown_count = Arc::new(AtomicUsize::new(0));
        let mut runtime = EngineRuntime::new(WorldState::default());
        runtime.register_module(ProbeModule::new(
            4,
            "probe_real",
            "ProbeOutput",
            1,
            old_init_count.clone(),
            old_shutdown_count.clone(),
        ));
        runtime.init_modules();

        let replacement = ProbeModule::new(
            4,
            "probe_stub",
            "ProbeOutput",
            2,
            new_init_count.clone(),
            new_shutdown_count.clone(),
        );
        let report = runtime
            .replace_module(replacement)
            .expect("same-schema replacement should succeed");

        assert_eq!(report.previous.name, "probe_real");
        assert_eq!(report.replacement.name, "probe_stub");
        assert!(report.schema_compatibility.passed);
        assert_eq!(old_init_count.load(Ordering::SeqCst), 1);
        assert_eq!(old_shutdown_count.load(Ordering::SeqCst), 1);
        assert_eq!(new_init_count.load(Ordering::SeqCst), 1);
        assert_eq!(new_shutdown_count.load(Ordering::SeqCst), 0);
        assert_eq!(
            runtime
                .schemas()
                .require_at_least("ProbeOutput", 2)
                .expect("schema should be upgraded")
                .version
                .version,
            2
        );
        assert!(
            runtime
                .module_descriptors()
                .iter()
                .any(|descriptor| descriptor.name == "probe_stub")
        );
    }

    #[test]
    fn runtime_rejects_incompatible_module_replacement() {
        let init_count = Arc::new(AtomicUsize::new(0));
        let shutdown_count = Arc::new(AtomicUsize::new(0));
        let mut runtime = EngineRuntime::new(WorldState::default());
        runtime.register_module(ProbeModule::new(
            5,
            "probe_real",
            "ProbeOutput",
            2,
            init_count.clone(),
            shutdown_count.clone(),
        ));
        runtime.init_modules();

        let wrong_schema = runtime
            .replace_module(ProbeModule::new(
                5,
                "probe_wrong_schema",
                "OtherOutput",
                3,
                Arc::new(AtomicUsize::new(0)),
                Arc::new(AtomicUsize::new(0)),
            ))
            .expect_err("schema name mismatch should fail");
        assert_eq!(
            wrong_schema,
            ModuleReplacementError::SchemaNameMismatch {
                module_id: 5,
                expected: "ProbeOutput",
                replacement: "OtherOutput",
            }
        );

        let too_old = runtime
            .replace_module(ProbeModule::new(
                5,
                "probe_too_old",
                "ProbeOutput",
                1,
                Arc::new(AtomicUsize::new(0)),
                Arc::new(AtomicUsize::new(0)),
            ))
            .expect_err("older schema should fail");
        assert_eq!(
            too_old,
            ModuleReplacementError::SchemaVersionTooOld {
                module_id: 5,
                expected_minimum: 2,
                replacement: 1,
            }
        );
        assert_eq!(shutdown_count.load(Ordering::SeqCst), 0);
        assert!(
            runtime
                .module_descriptors()
                .iter()
                .any(|descriptor| descriptor.name == "probe_real")
        );
    }

    #[test]
    fn replay_log_compare_accepts_identical_logs() {
        let expected = replay_test_log(WorldEventKind::Custom("same".to_string()), 1);
        let actual = expected.clone();

        let report = expected.compare(&actual);

        assert!(report.passed);
        assert_eq!(report.compared_frames, 1);
        assert!(report.mismatches.is_empty());

        let summary = expected.frame(0).expect("frame should exist").summary();
        assert_eq!(summary.event_fingerprints[0].kind_label, "Custom");
        assert_eq!(summary.event_fingerprints[0].evidence_count, 1);
    }

    #[test]
    fn replay_log_compare_reports_tick_and_event_kind_mismatches() {
        let expected = replay_test_log(WorldEventKind::Custom("same".to_string()), 1);
        let shifted_tick = replay_test_log(WorldEventKind::Custom("same".to_string()), 2);
        let changed_kind = replay_test_log(WorldEventKind::StreetFlooded, 1);

        let tick_report = expected.compare(&shifted_tick);
        assert!(!tick_report.passed);
        assert_eq!(
            tick_report.mismatches[0].status,
            ReplayComparisonStatus::TickMismatch
        );

        let kind_report = expected.compare(&changed_kind);
        assert!(!kind_report.passed);
        assert_eq!(
            kind_report.mismatches[0].status,
            ReplayComparisonStatus::EventKindMismatch
        );
    }

    #[test]
    fn replay_capture_filters_frames_with_event_ledger_queries() {
        let log = ReplayLog::new(vec![
            ReplayFrame {
                frame_id: 1,
                tick: 10,
                queued_commands: Vec::new(),
                events: vec![replay_event(
                    1,
                    10,
                    vec![7],
                    WorldEventKind::GlassWallFractured { entity: 7 },
                    &["fracture_impulse", "glass_shards"],
                    &["crime", "loud"],
                )],
            },
            ReplayFrame {
                frame_id: 2,
                tick: 11,
                queued_commands: Vec::new(),
                events: vec![replay_event(
                    2,
                    11,
                    vec![8],
                    WorldEventKind::StreetFlooded,
                    &["water_flow"],
                    &["hazard"],
                )],
            },
            ReplayFrame {
                frame_id: 3,
                tick: 12,
                queued_commands: Vec::new(),
                events: vec![replay_event(
                    3,
                    12,
                    vec![7],
                    WorldEventKind::SecurityAlertRaised {
                        faction: 700,
                        source_event: 1,
                        threat: 7,
                        severity: 0.9,
                    },
                    &["camera_feed", "security_feed"],
                    &["security", "crime"],
                )],
            },
        ]);

        let capture = log.capture(
            &ReplayCaptureFilter::default()
                .with_tick_range(10, 12)
                .with_event_filter(
                    EventFilter::default()
                        .with_actor(7)
                        .with_story_tag("crime")
                        .with_physical_type(PhysicalEventType::Fracture),
                )
                .with_event_kind_label("GlassWallFractured"),
        );
        assert_eq!(capture.len(), 1);
        assert_eq!(capture.frame_summaries[0].frame_id, 1);

        let matching_events = log
            .frame(0)
            .expect("first replay frame should exist")
            .events_matching(&EventFilter::default().with_physical_evidence("glass_shards"));
        assert_eq!(matching_events.len(), 1);
        assert_eq!(matching_events[0].event_id, 1);

        let limited = log.capture(
            &ReplayCaptureFilter::default()
                .with_min_event_count(1)
                .limited_to(2),
        );
        assert_eq!(limited.len(), 2);
        assert!(
            log.capture(&ReplayCaptureFilter::default().limited_to(0))
                .is_empty()
        );
    }

    #[test]
    fn runtime_replay_summary_records_queued_commands_and_events() {
        let mut world = WorldState::default();
        let entity = world
            .spawn_entity_template(EntityTemplate::new(
                "replay test prop",
                Transform::at(Vec3::ZERO),
            ))
            .expect("entity should spawn");
        let mut runtime = EngineRuntime::new(world);
        runtime.queue_command(WorldCommand::ApplyForce(ForceCommand {
            entity,
            vector_newtons: Vec3::new(12.0, 0.0, 0.0),
            impulse_newton_seconds: 1.0,
            source: None,
        }));

        let report = runtime.step().expect("frame should run");
        let summaries = runtime.replay_log().summaries();

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].frame_id, report.frame_id);
        assert_eq!(summaries[0].queued_command_count, 1);
        assert_eq!(summaries[0].event_count, report.events.len());
        assert_eq!(
            summaries[0].event_fingerprints[0].kind_label,
            "ForceApplied"
        );
    }

    fn replay_event(
        event_id: WorldEventId,
        tick: u64,
        actors: Vec<EntityId>,
        kind: WorldEventKind,
        physical_evidence: &[&str],
        narrative_tags: &[&str],
    ) -> WorldEvent {
        WorldEvent {
            event_id,
            tick,
            location_meters: Vec3::new(1.0, 2.0, 3.0),
            actors,
            kind,
            physical_evidence: physical_evidence
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            narrative_tags: narrative_tags
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
        }
    }

    fn replay_test_log(kind: WorldEventKind, tick: u64) -> ReplayLog {
        ReplayLog::new(vec![ReplayFrame {
            frame_id: 1,
            tick,
            queued_commands: Vec::new(),
            events: vec![replay_event(
                ((tick as u128) << 64) | 1,
                tick,
                vec![7],
                kind,
                &["test_evidence"],
                &["test_tag"],
            )],
        }])
    }
}
