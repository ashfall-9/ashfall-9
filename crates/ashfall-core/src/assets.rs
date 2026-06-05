use std::collections::{BTreeMap, VecDeque};

use crate::core::{AssetId, FrameId, ModuleId, QualityTier, SchemaVersion};
use crate::validation::ValidationReport;

pub const ASSET_PACKAGE_MANIFEST_SCHEMA_VERSION: u32 = 4;
pub const ASSET_PACKAGE_MANIFEST_SCHEMA: SchemaVersion = SchemaVersion {
    name: "AssetPackageManifest",
    version: ASSET_PACKAGE_MANIFEST_SCHEMA_VERSION,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AssetKind {
    Mesh,
    Texture,
    MaterialGraph,
    AudioClip,
    Shader,
    WorldChunk,
    GeneratedBundle,
    Other(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AssetLoadState {
    #[default]
    Unloaded,
    Queued,
    Loading,
    Resident,
    Failed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AssetPriority {
    Background,
    #[default]
    Visible,
    Hero,
    Critical,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetStreamingRequest {
    pub asset_id: AssetId,
    pub requester: ModuleId,
    pub requested_quality: QualityTier,
    pub priority: AssetPriority,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetStreamingCompletion {
    pub asset_id: AssetId,
    pub requester: ModuleId,
    pub priority: AssetPriority,
    pub requested_quality: QualityTier,
    pub quality_tier: QualityTier,
    pub frame_id: FrameId,
    pub byte_len: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetStreamingRecord {
    pub request: AssetStreamingRequest,
    pub state: AssetLoadState,
    pub last_touched_frame: Option<FrameId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetRecord {
    pub id: AssetId,
    pub kind: AssetKind,
    pub label: String,
    pub provenance: String,
    pub dependencies: Vec<AssetId>,
    pub byte_len: Option<u64>,
    pub generated: bool,
    pub quality_tier: QualityTier,
    pub load_state: AssetLoadState,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssetPackageManifest {
    pub package_asset: AssetId,
    pub manifest_schema_version: u32,
    pub asset_kind: AssetKind,
    pub label: String,
    pub provenance: String,
    pub schema_name: String,
    pub schema_version: u32,
    pub dependencies: Vec<AssetId>,
    pub chunks: Vec<AssetPackageChunkManifest>,
    pub total_uncompressed_bytes: u64,
    pub generated: bool,
    pub quality_tier: QualityTier,
    pub validation: ValidationReport,
}

impl AssetPackageManifest {
    pub fn validate(&self, component: ModuleId) -> ValidationReport {
        validate_asset_package_manifest(self, component)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetPackageChunkManifest {
    pub chunk_id: u64,
    pub usage_label: String,
    pub uncompressed_size: u64,
    pub compressed_size: u64,
    pub content_hash: u128,
}

pub fn validate_asset_package_manifest(
    manifest: &AssetPackageManifest,
    component: ModuleId,
) -> ValidationReport {
    let mut report = ValidationReport::for_asset(
        manifest.package_asset,
        component,
        ASSET_PACKAGE_MANIFEST_SCHEMA,
    );

    if manifest.package_asset == 0 {
        report.add_error(
            "missing_package_asset",
            "asset package manifest must reference a stable package asset id",
            "package_asset",
        );
    }
    if manifest.manifest_schema_version != ASSET_PACKAGE_MANIFEST_SCHEMA_VERSION {
        report.add_error(
            "manifest_schema_mismatch",
            "asset package manifest schema version does not match the shared contract",
            "manifest_schema_version",
        );
    }
    if manifest.label.trim().is_empty() {
        report.add_error(
            "missing_label",
            "asset package manifest must carry a human-readable label",
            "label",
        );
    }
    if manifest.schema_name.trim().is_empty() || manifest.schema_version == 0 {
        report.add_error(
            "missing_runtime_schema",
            "asset package manifest must declare the runtime schema name and version it contains",
            "schema",
        );
    }
    if manifest.dependencies.contains(&0) {
        report.add_error(
            "invalid_dependency",
            "asset package manifest dependencies must not contain zero handles",
            "dependencies",
        );
    }
    if manifest.chunks.is_empty() {
        report.add_error(
            "missing_chunks",
            "asset package manifest must declare at least one runtime chunk",
            "chunks",
        );
    }

    let chunk_total = manifest
        .chunks
        .iter()
        .map(|chunk| chunk.uncompressed_size)
        .sum::<u64>();
    if chunk_total != manifest.total_uncompressed_bytes {
        report.add_error(
            "chunk_byte_total_mismatch",
            "asset package manifest total bytes must match declared chunks",
            "total_uncompressed_bytes",
        );
    }
    for chunk in &manifest.chunks {
        if chunk.usage_label.trim().is_empty() {
            report.add_error(
                "missing_chunk_usage",
                "asset package manifest chunks must describe their runtime usage",
                format!("chunk:{}", chunk.chunk_id),
            );
        }
        if chunk.uncompressed_size == 0 {
            report.add_warning(
                "empty_chunk",
                "asset package manifest chunk has zero uncompressed bytes",
                format!("chunk:{}", chunk.chunk_id),
            );
        }
    }

    if manifest.validation.asset != Some(manifest.package_asset) {
        report.add_error(
            "validation_asset_mismatch",
            "attached validation report must reference the same package asset",
            "validation.asset",
        );
    }
    if !manifest.validation.passed {
        report.add_error(
            "attached_validation_failed",
            "attached validation report failed",
            "validation",
        );
    }

    report.add_metric(
        "dependency_count",
        manifest.dependencies.len() as f64,
        "count",
        None,
    );
    report.add_metric("chunk_count", manifest.chunks.len() as f64, "count", None);
    report.add_metric(
        "total_uncompressed_bytes",
        manifest.total_uncompressed_bytes as f64,
        "bytes",
        None,
    );

    report
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GeneratedAssetRecipe {
    pub generator: ModuleId,
    pub kind: AssetKind,
    pub label: String,
    pub recipe_schema: String,
    pub recipe_version: u32,
    pub recipe_key: String,
    pub source_assets: Vec<AssetId>,
    pub quality_tier: QualityTier,
    pub byte_len: Option<u64>,
}

impl GeneratedAssetRecipe {
    pub fn new(
        generator: ModuleId,
        kind: AssetKind,
        label: impl Into<String>,
        recipe_schema: impl Into<String>,
        recipe_version: u32,
        recipe_key: impl Into<String>,
        quality_tier: QualityTier,
    ) -> Self {
        Self {
            generator,
            kind,
            label: label.into(),
            recipe_schema: recipe_schema.into(),
            recipe_version,
            recipe_key: recipe_key.into(),
            source_assets: Vec::new(),
            quality_tier,
            byte_len: None,
        }
    }

    pub fn with_source_assets(mut self, source_assets: Vec<AssetId>) -> Self {
        self.source_assets = source_assets;
        self
    }

    pub fn with_byte_len(mut self, byte_len: u64) -> Self {
        self.byte_len = Some(byte_len);
        self
    }

    pub fn provenance(&self) -> String {
        format!(
            "generated:{}:{}@v{}:{}",
            self.generator, self.recipe_schema, self.recipe_version, self.recipe_key
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedAssetCacheEntry {
    pub asset_id: AssetId,
    pub recipe: GeneratedAssetRecipe,
    pub created_frame: Option<FrameId>,
    pub last_used_frame: Option<FrameId>,
    pub request_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedAssetRegistration {
    pub asset_id: AssetId,
    pub cache_hit: bool,
    pub entry: GeneratedAssetCacheEntry,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetStreamingBudget {
    pub max_resident_assets_per_frame: usize,
    pub max_bytes_per_frame: u64,
}

impl Default for AssetStreamingBudget {
    fn default() -> Self {
        Self {
            max_resident_assets_per_frame: 4,
            max_bytes_per_frame: 64 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetStreamingFrameReport {
    pub frame_id: FrameId,
    pub total_registered_assets: usize,
    pub queued_assets: usize,
    pub loading_assets: usize,
    pub resident_assets: usize,
    pub failed_assets: usize,
    pub pending_assets: usize,
    pub generated_pending_assets: usize,
    pub bytes_streamed_this_frame: u64,
    pub bytes_pending: u64,
    pub completions: Vec<AssetStreamingCompletion>,
    pub by_requester: Vec<AssetRequesterStreamingCost>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetRequesterStreamingCost {
    pub requester: ModuleId,
    pub requested_assets: usize,
    pub pending_assets: usize,
    pub completed_assets: usize,
    pub bytes_pending: u64,
    pub bytes_streamed_this_frame: u64,
    pub highest_priority: AssetPriority,
    pub highest_quality: QualityTier,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetRegistry {
    records: BTreeMap<AssetId, AssetRecord>,
    generated_cache: BTreeMap<AssetId, GeneratedAssetCacheEntry>,
    streaming_records: BTreeMap<AssetId, AssetStreamingRecord>,
    streaming_queue: VecDeque<AssetStreamingRequest>,
}

impl AssetRegistry {
    pub fn register_asset(
        &mut self,
        kind: AssetKind,
        label: impl Into<String>,
        provenance: impl Into<String>,
        dependencies: Vec<AssetId>,
        quality_tier: QualityTier,
    ) -> AssetId {
        let label = label.into();
        let provenance = provenance.into();
        let id = content_id(&kind, &label, &provenance, &dependencies);
        self.records.insert(
            id,
            AssetRecord {
                id,
                kind,
                label,
                provenance,
                dependencies,
                byte_len: None,
                generated: false,
                quality_tier,
                load_state: AssetLoadState::Resident,
            },
        );
        id
    }

    pub fn register_known(&mut self, record: AssetRecord) {
        if !record.generated {
            self.generated_cache.remove(&record.id);
        }
        self.records.insert(record.id, record);
    }

    pub fn register_generated_asset(
        &mut self,
        recipe: GeneratedAssetRecipe,
        frame_id: Option<FrameId>,
    ) -> GeneratedAssetRegistration {
        let asset_id = generated_asset_id(&recipe);

        if let Some(entry) = self.generated_cache.get_mut(&asset_id) {
            entry.last_used_frame = frame_id;
            entry.request_count = entry.request_count.saturating_add(1);
            if let Some(record) = self.records.get_mut(&asset_id)
                && recipe.quality_tier > record.quality_tier
            {
                record.quality_tier = recipe.quality_tier;
            }
            return GeneratedAssetRegistration {
                asset_id,
                cache_hit: true,
                entry: entry.clone(),
            };
        }

        let record = AssetRecord {
            id: asset_id,
            kind: recipe.kind.clone(),
            label: recipe.label.clone(),
            provenance: recipe.provenance(),
            dependencies: recipe.source_assets.clone(),
            byte_len: recipe.byte_len,
            generated: true,
            quality_tier: recipe.quality_tier,
            load_state: AssetLoadState::Unloaded,
        };
        self.records.insert(asset_id, record);

        let entry = GeneratedAssetCacheEntry {
            asset_id,
            recipe,
            created_frame: frame_id,
            last_used_frame: frame_id,
            request_count: 1,
        };
        self.generated_cache.insert(asset_id, entry.clone());

        GeneratedAssetRegistration {
            asset_id,
            cache_hit: false,
            entry,
        }
    }

    pub fn generated_cache_entry(&self, asset_id: AssetId) -> Option<&GeneratedAssetCacheEntry> {
        self.generated_cache.get(&asset_id)
    }

    pub fn generated_assets(&self) -> impl Iterator<Item = &GeneratedAssetCacheEntry> {
        self.generated_cache.values()
    }

    pub fn generated_cache_len(&self) -> usize {
        self.generated_cache.len()
    }

    pub fn get(&self, id: AssetId) -> Option<&AssetRecord> {
        self.records.get(&id)
    }

    pub fn all(&self) -> impl Iterator<Item = &AssetRecord> {
        self.records.values()
    }

    pub fn load_state(&self, id: AssetId) -> Option<AssetLoadState> {
        self.records.get(&id).map(|record| record.load_state)
    }

    pub fn streaming_records(&self) -> impl Iterator<Item = &AssetStreamingRecord> {
        self.streaming_records.values()
    }

    pub fn streaming_queue_len(&self) -> usize {
        self.streaming_queue.len()
    }

    pub fn queue_streaming(&mut self, request: AssetStreamingRequest) -> bool {
        let Some(record) = self.records.get_mut(&request.asset_id) else {
            return false;
        };

        if record.load_state == AssetLoadState::Resident {
            return false;
        }

        if let Some(existing) = self.streaming_records.get_mut(&request.asset_id) {
            existing.last_touched_frame = None;
            if request.priority > existing.request.priority
                || request.requested_quality > existing.request.requested_quality
            {
                existing.request = request;
            }
            return false;
        }

        record.load_state = AssetLoadState::Queued;
        self.streaming_records.insert(
            request.asset_id,
            AssetStreamingRecord {
                request: request.clone(),
                state: AssetLoadState::Queued,
                last_touched_frame: None,
            },
        );
        self.streaming_queue.push_back(request);
        true
    }

    pub fn mark_loading(&mut self, id: AssetId, frame_id: FrameId) -> bool {
        let Some(record) = self.records.get_mut(&id) else {
            return false;
        };
        record.load_state = AssetLoadState::Loading;
        if let Some(streaming) = self.streaming_records.get_mut(&id) {
            streaming.state = AssetLoadState::Loading;
            streaming.last_touched_frame = Some(frame_id);
        }
        true
    }

    pub fn mark_resident(&mut self, id: AssetId, frame_id: FrameId) -> bool {
        let Some(record) = self.records.get_mut(&id) else {
            return false;
        };
        record.load_state = AssetLoadState::Resident;
        if let Some(streaming) = self.streaming_records.get_mut(&id) {
            streaming.state = AssetLoadState::Resident;
            streaming.last_touched_frame = Some(frame_id);
        }
        true
    }

    pub fn advance_streaming(
        &mut self,
        frame_id: FrameId,
        max_resident_assets: usize,
    ) -> Vec<AssetStreamingCompletion> {
        self.advance_streaming_with_budget(
            frame_id,
            AssetStreamingBudget {
                max_resident_assets_per_frame: max_resident_assets,
                ..AssetStreamingBudget::default()
            },
        )
        .completions
    }

    pub fn advance_streaming_with_budget(
        &mut self,
        frame_id: FrameId,
        budget: AssetStreamingBudget,
    ) -> AssetStreamingFrameReport {
        let mut pending = self.streaming_queue.drain(..).collect::<Vec<_>>();
        for request in &mut pending {
            if let Some(record) = self.streaming_records.get(&request.asset_id) {
                *request = record.request.clone();
            }
        }
        pending.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| right.requested_quality.cmp(&left.requested_quality))
                .then_with(|| left.asset_id.cmp(&right.asset_id))
        });

        let mut remaining = VecDeque::new();
        let mut completions = Vec::new();
        let mut bytes_streamed_this_frame: u64 = 0;
        for request in pending {
            if completions.len() >= budget.max_resident_assets_per_frame {
                remaining.push_back(request);
                continue;
            }

            let Some(record) = self.records.get(&request.asset_id) else {
                continue;
            };
            if record.load_state == AssetLoadState::Resident {
                continue;
            }
            if !self.dependencies_are_resident(record) {
                remaining.push_back(request);
                continue;
            }

            let byte_len = estimated_asset_bytes(record, request.requested_quality);
            if bytes_streamed_this_frame > 0
                && bytes_streamed_this_frame.saturating_add(byte_len) > budget.max_bytes_per_frame
            {
                remaining.push_back(request);
                continue;
            }

            let Some(record) = self.records.get_mut(&request.asset_id) else {
                continue;
            };
            record.load_state = AssetLoadState::Resident;
            if request.requested_quality > record.quality_tier {
                record.quality_tier = request.requested_quality;
            }
            if let Some(streaming) = self.streaming_records.get_mut(&request.asset_id) {
                streaming.state = AssetLoadState::Resident;
                streaming.last_touched_frame = Some(frame_id);
            }
            bytes_streamed_this_frame = bytes_streamed_this_frame.saturating_add(byte_len);
            completions.push(AssetStreamingCompletion {
                asset_id: request.asset_id,
                requester: request.requester,
                priority: request.priority,
                requested_quality: request.requested_quality,
                quality_tier: record.quality_tier,
                frame_id,
                byte_len,
            });
        }

        self.streaming_queue = remaining;
        self.streaming_report(frame_id, completions, bytes_streamed_this_frame)
    }

    pub fn streaming_report(
        &self,
        frame_id: FrameId,
        completions: Vec<AssetStreamingCompletion>,
        bytes_streamed_this_frame: u64,
    ) -> AssetStreamingFrameReport {
        let mut queued_assets = 0;
        let mut loading_assets = 0;
        let mut resident_assets = 0;
        let mut failed_assets = 0;
        let mut pending_assets = 0;
        let mut generated_pending_assets = 0;
        let mut bytes_pending: u64 = 0;
        let mut by_requester: BTreeMap<ModuleId, AssetRequesterStreamingCost> = BTreeMap::new();

        for record in self.records.values() {
            match record.load_state {
                AssetLoadState::Unloaded => {}
                AssetLoadState::Queued => queued_assets += 1,
                AssetLoadState::Loading => loading_assets += 1,
                AssetLoadState::Resident => resident_assets += 1,
                AssetLoadState::Failed => failed_assets += 1,
            }
        }

        for streaming in self.streaming_records.values() {
            let Some(record) = self.records.get(&streaming.request.asset_id) else {
                continue;
            };
            let estimated_bytes =
                estimated_asset_bytes(record, streaming.request.requested_quality);
            let requester = by_requester
                .entry(streaming.request.requester)
                .or_insert_with(|| AssetRequesterStreamingCost {
                    requester: streaming.request.requester,
                    ..AssetRequesterStreamingCost::default()
                });
            requester.requested_assets += 1;
            requester.highest_priority = requester.highest_priority.max(streaming.request.priority);
            requester.highest_quality = requester
                .highest_quality
                .max(streaming.request.requested_quality);

            if record.load_state == AssetLoadState::Resident {
                continue;
            }

            pending_assets += 1;
            bytes_pending = bytes_pending.saturating_add(estimated_bytes);
            requester.pending_assets += 1;
            requester.bytes_pending = requester.bytes_pending.saturating_add(estimated_bytes);
            if record.generated {
                generated_pending_assets += 1;
            }
        }

        for completion in &completions {
            let requester = by_requester.entry(completion.requester).or_insert_with(|| {
                AssetRequesterStreamingCost {
                    requester: completion.requester,
                    ..AssetRequesterStreamingCost::default()
                }
            });
            requester.completed_assets += 1;
            requester.bytes_streamed_this_frame = requester
                .bytes_streamed_this_frame
                .saturating_add(completion.byte_len);
            requester.highest_priority = requester.highest_priority.max(completion.priority);
            requester.highest_quality = requester.highest_quality.max(completion.requested_quality);
        }

        AssetStreamingFrameReport {
            frame_id,
            total_registered_assets: self.records.len(),
            queued_assets,
            loading_assets,
            resident_assets,
            failed_assets,
            pending_assets,
            generated_pending_assets,
            bytes_streamed_this_frame,
            bytes_pending,
            completions,
            by_requester: by_requester.into_values().collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    fn dependencies_are_resident(&self, record: &AssetRecord) -> bool {
        record.dependencies.iter().all(|dependency| {
            self.records
                .get(dependency)
                .is_some_and(|dependency| dependency.load_state == AssetLoadState::Resident)
        })
    }
}

fn estimated_asset_bytes(record: &AssetRecord, requested_quality: QualityTier) -> u64 {
    let base = record.byte_len.unwrap_or(match record.kind {
        AssetKind::Mesh => 6 * 1024 * 1024,
        AssetKind::Texture => 8 * 1024 * 1024,
        AssetKind::MaterialGraph => 2 * 1024 * 1024,
        AssetKind::AudioClip => 1024 * 1024,
        AssetKind::Shader => 512 * 1024,
        AssetKind::WorldChunk => 16 * 1024 * 1024,
        AssetKind::GeneratedBundle => 24 * 1024 * 1024,
        AssetKind::Other(_) => 4 * 1024 * 1024,
    });
    let numerator = match requested_quality {
        QualityTier::Disabled => 0,
        QualityTier::BackgroundApproximation => 1,
        QualityTier::NormalRuntime => 2,
        QualityTier::HeroHighFidelityRuntime => 3,
        QualityTier::ReferenceOfflineValidation => 6,
    };

    base.saturating_mul(numerator).saturating_div(2).max(1)
}

fn content_id(
    kind: &AssetKind,
    label: &str,
    provenance: &str,
    dependencies: &[AssetId],
) -> AssetId {
    let mut high = StableAssetHasher::new(0x9e37_79b9_7f4a_7c15);
    high.write_str("ashfall-content-high");
    high.write_asset_kind(kind);
    high.write_str(label);
    high.write_str(provenance);
    high.write_asset_ids(dependencies);

    let mut low = StableAssetHasher::new(0xc2b2_ae3d_27d4_eb4f);
    low.write_str("ashfall-content-low");
    dependencies
        .iter()
        .rev()
        .for_each(|dependency| low.write_u128(*dependency));
    low.write_str(provenance);
    low.write_str(label);
    low.write_asset_kind(kind);

    ((high.finish() as u128) << 64) | low.finish() as u128
}

fn generated_asset_id(recipe: &GeneratedAssetRecipe) -> AssetId {
    let mut high = StableAssetHasher::new(0x94d0_49bb_1331_11eb);
    high.write_str("ashfall-generated-high");
    high.write_u64(recipe.generator);
    high.write_asset_kind(&recipe.kind);
    high.write_str(&recipe.label);
    high.write_str(&recipe.recipe_schema);
    high.write_u32(recipe.recipe_version);
    high.write_str(&recipe.recipe_key);
    high.write_quality(recipe.quality_tier);
    high.write_asset_ids(&recipe.source_assets);

    let mut low = StableAssetHasher::new(0x2545_f491_4f6c_dd1d);
    low.write_str("ashfall-generated-low");
    low.write_str(&recipe.recipe_key);
    low.write_str(&recipe.recipe_schema);
    low.write_u32(recipe.recipe_version);
    recipe
        .source_assets
        .iter()
        .rev()
        .for_each(|source| low.write_u128(*source));
    low.write_asset_kind(&recipe.kind);
    low.write_u64(recipe.generator);
    low.write_quality(recipe.quality_tier);

    ((high.finish() as u128) << 64) | low.finish() as u128
}

#[derive(Clone, Copy, Debug)]
struct StableAssetHasher {
    state: u64,
}

impl StableAssetHasher {
    const FNV_PRIME: u64 = 1_099_511_628_211;

    fn new(seed: u64) -> Self {
        Self {
            state: 14_695_981_039_346_656_037 ^ seed,
        }
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        self.write_usize(bytes.len());
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(Self::FNV_PRIME);
        }
    }

    fn write_str(&mut self, value: &str) {
        self.write_bytes(value.as_bytes());
    }

    fn write_u8(&mut self, value: u8) {
        self.write_bytes(&[value]);
    }

    fn write_u32(&mut self, value: u32) {
        self.write_bytes(&value.to_le_bytes());
    }

    fn write_u64(&mut self, value: u64) {
        self.write_bytes(&value.to_le_bytes());
    }

    fn write_u128(&mut self, value: u128) {
        self.write_bytes(&value.to_le_bytes());
    }

    fn write_usize(&mut self, value: usize) {
        self.state ^= value as u64;
        self.state = self.state.wrapping_mul(Self::FNV_PRIME);
    }

    fn write_asset_ids(&mut self, ids: &[AssetId]) {
        self.write_usize(ids.len());
        for id in ids {
            self.write_u128(*id);
        }
    }

    fn write_asset_kind(&mut self, kind: &AssetKind) {
        match kind {
            AssetKind::Mesh => self.write_u8(1),
            AssetKind::Texture => self.write_u8(2),
            AssetKind::MaterialGraph => self.write_u8(3),
            AssetKind::AudioClip => self.write_u8(4),
            AssetKind::Shader => self.write_u8(5),
            AssetKind::WorldChunk => self.write_u8(6),
            AssetKind::GeneratedBundle => self.write_u8(7),
            AssetKind::Other(value) => {
                self.write_u8(255);
                self.write_str(value);
            }
        }
    }

    fn write_quality(&mut self, quality_tier: QualityTier) {
        let value = match quality_tier {
            QualityTier::Disabled => 0,
            QualityTier::BackgroundApproximation => 1,
            QualityTier::NormalRuntime => 2,
            QualityTier::HeroHighFidelityRuntime => 3,
            QualityTier::ReferenceOfflineValidation => 4,
        };
        self.write_u8(value);
    }

    fn finish(self) -> u64 {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: AssetId, kind: AssetKind, state: AssetLoadState, byte_len: u64) -> AssetRecord {
        AssetRecord {
            id,
            kind,
            label: format!("asset {id}"),
            provenance: "unit-test".to_string(),
            dependencies: Vec::new(),
            byte_len: Some(byte_len),
            generated: false,
            quality_tier: QualityTier::NormalRuntime,
            load_state: state,
        }
    }

    #[test]
    fn streaming_report_tracks_bytes_and_requesters() {
        let mut registry = AssetRegistry::default();
        registry.register_known(record(1, AssetKind::Mesh, AssetLoadState::Unloaded, 1024));
        registry.register_known(record(
            2,
            AssetKind::Texture,
            AssetLoadState::Unloaded,
            2048,
        ));

        assert!(registry.queue_streaming(AssetStreamingRequest {
            asset_id: 1,
            requester: 10,
            requested_quality: QualityTier::NormalRuntime,
            priority: AssetPriority::Visible,
            reason: "visible mesh".to_string(),
        }));
        assert!(registry.queue_streaming(AssetStreamingRequest {
            asset_id: 2,
            requester: 30,
            requested_quality: QualityTier::HeroHighFidelityRuntime,
            priority: AssetPriority::Hero,
            reason: "material cache".to_string(),
        }));

        let report = registry.advance_streaming_with_budget(
            4,
            AssetStreamingBudget {
                max_resident_assets_per_frame: 1,
                max_bytes_per_frame: u64::MAX,
            },
        );

        assert_eq!(report.completions.len(), 1);
        assert_eq!(report.completions[0].asset_id, 2);
        assert_eq!(report.completions[0].requester, 30);
        assert!(report.bytes_streamed_this_frame > 0);
        assert_eq!(report.pending_assets, 1);
        assert!(report.by_requester.iter().any(|cost| {
            cost.requester == 10 && cost.pending_assets == 1 && cost.bytes_pending > 0
        }));
        assert!(
            report
                .by_requester
                .iter()
                .any(|cost| { cost.requester == 30 && cost.completed_assets == 1 })
        );
    }

    #[test]
    fn streaming_waits_for_dependencies() {
        let mut registry = AssetRegistry::default();
        registry.register_known(record(
            1,
            AssetKind::Texture,
            AssetLoadState::Unloaded,
            1024,
        ));
        let mut dependent = record(
            2,
            AssetKind::GeneratedBundle,
            AssetLoadState::Unloaded,
            2048,
        );
        dependent.dependencies = vec![1];
        dependent.generated = true;
        registry.register_known(dependent);

        registry.queue_streaming(AssetStreamingRequest {
            asset_id: 2,
            requester: 40,
            requested_quality: QualityTier::HeroHighFidelityRuntime,
            priority: AssetPriority::Hero,
            reason: "dependent bundle".to_string(),
        });
        let blocked = registry.advance_streaming_with_budget(1, AssetStreamingBudget::default());
        assert!(blocked.completions.is_empty());
        assert_eq!(blocked.pending_assets, 1);
        assert_eq!(blocked.generated_pending_assets, 1);

        registry.queue_streaming(AssetStreamingRequest {
            asset_id: 1,
            requester: 30,
            requested_quality: QualityTier::NormalRuntime,
            priority: AssetPriority::Critical,
            reason: "dependency".to_string(),
        });
        let dependency = registry.advance_streaming_with_budget(2, AssetStreamingBudget::default());
        assert!(
            dependency
                .completions
                .iter()
                .any(|completion| completion.asset_id == 1)
        );

        if !dependency
            .completions
            .iter()
            .any(|completion| completion.asset_id == 2)
        {
            let unblocked =
                registry.advance_streaming_with_budget(3, AssetStreamingBudget::default());
            assert!(
                unblocked
                    .completions
                    .iter()
                    .any(|completion| completion.asset_id == 2)
            );
        }
    }

    #[test]
    fn generated_asset_cache_reuses_recipe_ids_and_streams_outputs() {
        let mut registry = AssetRegistry::default();
        registry.register_known(record(
            1,
            AssetKind::Texture,
            AssetLoadState::Resident,
            1024,
        ));

        let recipe = GeneratedAssetRecipe::new(
            30,
            AssetKind::GeneratedBundle,
            "wet asphalt reflection cache",
            "MaterialCacheRecipe",
            2,
            "asphalt:wetness=0.9:quality=hero",
            QualityTier::HeroHighFidelityRuntime,
        )
        .with_source_assets(vec![1])
        .with_byte_len(4096);

        let first = registry.register_generated_asset(recipe.clone(), Some(7));
        let second = registry.register_generated_asset(recipe, Some(8));

        assert!(!first.cache_hit);
        assert!(second.cache_hit);
        assert_eq!(first.asset_id, second.asset_id);
        assert_eq!(registry.generated_cache_len(), 1);
        assert_eq!(second.entry.request_count, 2);
        assert_eq!(second.entry.created_frame, Some(7));
        assert_eq!(second.entry.last_used_frame, Some(8));

        let asset = registry
            .get(first.asset_id)
            .expect("generated asset should be registered");
        assert!(asset.generated);
        assert_eq!(asset.load_state, AssetLoadState::Unloaded);
        assert_eq!(asset.dependencies, vec![1]);
        assert_eq!(asset.byte_len, Some(4096));

        assert!(registry.queue_streaming(AssetStreamingRequest {
            asset_id: first.asset_id,
            requester: 30,
            requested_quality: QualityTier::HeroHighFidelityRuntime,
            priority: AssetPriority::Hero,
            reason: "generated cache should stream through registry".to_string(),
        }));

        let report = registry.advance_streaming_with_budget(
            9,
            AssetStreamingBudget {
                max_resident_assets_per_frame: 1,
                max_bytes_per_frame: u64::MAX,
            },
        );

        assert_eq!(report.completions.len(), 1);
        assert_eq!(report.completions[0].asset_id, first.asset_id);
        assert_eq!(
            registry.load_state(first.asset_id),
            Some(AssetLoadState::Resident)
        );
    }

    #[test]
    fn generated_asset_cache_is_deterministic_across_registries() {
        let recipe = GeneratedAssetRecipe::new(
            40,
            AssetKind::Mesh,
            "fractured glass shard cluster",
            "FractureMeshRecipe",
            1,
            "glass-wall:seed=42:impulse=high",
            QualityTier::NormalRuntime,
        )
        .with_source_assets(vec![10, 11]);

        let mut first_registry = AssetRegistry::default();
        let mut second_registry = AssetRegistry::default();

        let first = first_registry.register_generated_asset(recipe.clone(), Some(1));
        let second = second_registry.register_generated_asset(recipe, Some(99));

        assert_eq!(first.asset_id, second.asset_id);
        assert_eq!(
            first_registry
                .generated_cache_entry(first.asset_id)
                .expect("entry should exist")
                .recipe
                .recipe_schema,
            "FractureMeshRecipe"
        );
    }

    #[test]
    fn package_manifest_validation_checks_shared_contract() {
        let validation =
            ValidationReport::for_asset(88, 80, crate::validation::VALIDATION_REPORT_SCHEMA);
        let manifest = AssetPackageManifest {
            package_asset: 88,
            manifest_schema_version: ASSET_PACKAGE_MANIFEST_SCHEMA_VERSION,
            asset_kind: AssetKind::WorldChunk,
            label: "city cell".to_string(),
            provenance: "unit-test".to_string(),
            schema_name: "CityCellPackage".to_string(),
            schema_version: 4,
            dependencies: vec![10, 11],
            chunks: vec![AssetPackageChunkManifest {
                chunk_id: 1,
                usage_label: "world_cell".to_string(),
                uncompressed_size: 4096,
                compressed_size: 2048,
                content_hash: 99,
            }],
            total_uncompressed_bytes: 4096,
            generated: true,
            quality_tier: QualityTier::NormalRuntime,
            validation,
        };

        let report = manifest.validate(80);

        assert!(report.passed);
        assert_eq!(report.asset, Some(88));
        assert!(report.errors.is_empty());
        assert!(report.metrics.iter().any(|metric| {
            metric.name == "dependency_count" && (metric.value - 2.0).abs() < f64::EPSILON
        }));
    }
}
