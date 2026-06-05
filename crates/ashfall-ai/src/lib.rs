use std::collections::{BTreeMap, BTreeSet};

use ashfall_core::core::*;
use ashfall_core::gpu::{
    ComputePipelineHandle, GpuDispatchKind, GpuGraphBuilder, GpuPassDesc, GpuPipelineDesc,
    GpuQueueKind, GpuResourceDesc, GpuResourceHandle, GpuResourceKind, GpuResourceLifetime,
    GpuShaderPermutation,
};
use ashfall_core::runtime::{EngineModule, FrameContext, ModuleDescriptor, ModuleStateRecord};
use ashfall_core::world::{
    CommandSink, DialogueEvent, EventLedger, MoveEntityStepCommand, MoveEntityStepMode, StoryEvent,
    WorldCommand, WorldEvent, WorldEventKind, WorldSnapshot,
};

pub const DEFAULT_SECURITY_FACTION_ID: FactionId = 700;
pub const DEFAULT_OPPOSITION_FACTION_ID: FactionId = 701;
pub const DEFAULT_STORY_LOCATION_ID: LocationId = 100;
pub const ALLEY_SECURITY_FACTION_ID: FactionId = DEFAULT_SECURITY_FACTION_ID;
pub const ALLEY_STORY_LOCATION_ID: LocationId = DEFAULT_STORY_LOCATION_ID;
const STORY_DIRECTOR_MODULE_STATE_VERSION: u32 = 2;
const AI_CHARACTERS_MODULE_STATE_VERSION: u32 = 4;
const AI_DEEP_PLANNING_INTERVAL_TICKS: u64 = 30;
const AI_SOUND_REACTION_INTERVAL_TICKS: u64 = 8;
const AI_FLEE_STEP_METERS: f32 = 2.0;
const AI_INVESTIGATE_STEP_METERS: f32 = 1.5;
const AI_INVESTIGATE_STOP_RADIUS_METERS: f32 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoryDirectorConfig {
    pub security_faction: FactionId,
    pub opposition_faction: FactionId,
    pub story_location: LocationId,
}

impl Default for StoryDirectorConfig {
    fn default() -> Self {
        Self {
            security_faction: DEFAULT_SECURITY_FACTION_ID,
            opposition_faction: DEFAULT_OPPOSITION_FACTION_ID,
            story_location: DEFAULT_STORY_LOCATION_ID,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AiCharactersConfig {
    pub default_authority_faction: FactionId,
    pub fallback_navigation_location: LocationId,
}

impl Default for AiCharactersConfig {
    fn default() -> Self {
        Self {
            default_authority_faction: DEFAULT_SECURITY_FACTION_ID,
            fallback_navigation_location: DEFAULT_STORY_LOCATION_ID,
        }
    }
}

fn parse_agent_event_key(value: &str) -> Option<(EntityId, WorldEventId)> {
    let (agent, event_id) = value.split_once(':')?;
    Some((agent.parse().ok()?, event_id.parse().ok()?))
}

fn parse_agent_tick_key(value: &str) -> Option<(EntityId, u64)> {
    let (agent, tick) = value.split_once(':')?;
    Some((agent.parse().ok()?, tick.parse().ok()?))
}

fn agent_event_entries<'a>(
    prefix: &'static str,
    values: &'a BTreeSet<(EntityId, WorldEventId)>,
) -> impl Iterator<Item = String> + 'a {
    values
        .iter()
        .map(move |(agent, event_id)| format!("{prefix}{agent}:{event_id}"))
}

fn agent_tick_entries<'a>(
    prefix: &'static str,
    values: &'a BTreeMap<EntityId, u64>,
) -> impl Iterator<Item = String> + 'a {
    values
        .iter()
        .map(move |(agent, tick)| format!("{prefix}{agent}:{tick}"))
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

fn join_u64(values: &[u64]) -> String {
    values
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn join_u128(values: &[u128]) -> String {
    values
        .iter()
        .map(u128::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn parse_u64_list(value: &str) -> Option<Vec<u64>> {
    if value.is_empty() {
        return Some(Vec::new());
    }
    value.split(',').map(|item| item.parse().ok()).collect()
}

fn parse_u128_list(value: &str) -> Option<Vec<u128>> {
    if value.is_empty() {
        return Some(Vec::new());
    }
    value.split(',').map(|item| item.parse().ok()).collect()
}

fn encode_optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string())
}

fn parse_optional_u64(value: &str) -> Option<Option<u64>> {
    if value == "none" {
        return Some(None);
    }
    value.parse().ok().map(Some)
}

fn encode_text_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| encode_state_text(value))
        .collect::<Vec<_>>()
        .join(",")
}

fn parse_text_list(value: &str) -> Option<Vec<String>> {
    if value.is_empty() {
        return Some(Vec::new());
    }
    value.split(',').map(decode_state_text).collect()
}

fn encode_vec3(value: Option<Vec3>) -> String {
    value
        .map(|value| format!("{},{},{}", value.x, value.y, value.z))
        .unwrap_or_else(|| "none".to_string())
}

fn parse_vec3(value: &str) -> Option<Option<Vec3>> {
    if value == "none" {
        return Some(None);
    }
    let mut parts = value.split(',');
    let x = parse_finite_f32(parts.next()?)?;
    let y = parse_finite_f32(parts.next()?)?;
    let z = parse_finite_f32(parts.next()?)?;
    parts.next().is_none().then_some(Some(Vec3::new(x, y, z)))
}

fn encode_knowledge_source(source: &KnowledgeSource) -> String {
    match source {
        KnowledgeSource::DirectObservation(event_id) => format!("direct:{event_id}"),
        KnowledgeSource::HeardFrom(entity) => format!("heard:{entity}"),
        KnowledgeSource::FactionBroadcast(faction) => format!("faction:{faction}"),
        KnowledgeSource::Inference { based_on } => format!("inference:{}", join_u128(based_on)),
    }
}

fn parse_knowledge_source(value: &str) -> Option<KnowledgeSource> {
    if let Some(event_id) = value.strip_prefix("direct:") {
        return event_id
            .parse()
            .ok()
            .map(KnowledgeSource::DirectObservation);
    }
    if let Some(entity) = value.strip_prefix("heard:") {
        return entity.parse().ok().map(KnowledgeSource::HeardFrom);
    }
    if let Some(faction) = value.strip_prefix("faction:") {
        return faction.parse().ok().map(KnowledgeSource::FactionBroadcast);
    }
    if let Some(events) = value.strip_prefix("inference:") {
        return Some(KnowledgeSource::Inference {
            based_on: parse_u128_list(events)?,
        });
    }
    None
}

fn encode_decay_policy(policy: &MemoryDecayPolicy) -> String {
    match policy {
        MemoryDecayPolicy::Never => "never".to_string(),
        MemoryDecayPolicy::Gradual { half_life_seconds } => {
            format!("gradual:{half_life_seconds}")
        }
    }
}

fn parse_decay_policy(value: &str) -> Option<MemoryDecayPolicy> {
    if value == "never" {
        return Some(MemoryDecayPolicy::Never);
    }
    value
        .strip_prefix("gradual:")
        .and_then(parse_finite_f32)
        .filter(|half_life_seconds| *half_life_seconds > 0.0)
        .map(|half_life_seconds| MemoryDecayPolicy::Gradual { half_life_seconds })
}

fn parse_memory_kind(value: &str) -> Option<MemoryKind> {
    match value {
        "short_term" => Some(MemoryKind::ShortTerm),
        "long_term" => Some(MemoryKind::LongTerm),
        "social" => Some(MemoryKind::Social),
        "location" => Some(MemoryKind::Location),
        "evidence" => Some(MemoryKind::Evidence),
        _ => None,
    }
}

fn encode_memory_record(record: &MemoryRecord) -> String {
    format!(
        "memory_record:{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        record.memory_id,
        record.agent,
        memory_kind_label(&record.kind),
        encode_knowledge_source(&record.source),
        encode_state_text(&record.content),
        record.importance,
        record.confidence,
        record.emotional_weight,
        record.created_tick,
        encode_vec3(record.location),
        encode_decay_policy(&record.decay_policy),
    )
}

fn parse_memory_record(value: &str) -> Option<MemoryRecord> {
    let fields = value.split('|').collect::<Vec<_>>();
    if fields.len() != 11 {
        return None;
    }
    Some(MemoryRecord {
        memory_id: fields[0].parse().ok()?,
        agent: fields[1].parse().ok()?,
        kind: parse_memory_kind(fields[2])?,
        source: parse_knowledge_source(fields[3])?,
        content: decode_state_text(fields[4])?,
        importance: parse_finite_f32(fields[5])?.clamp(0.0, 1.0),
        confidence: parse_finite_f32(fields[6])?.clamp(0.0, 1.0),
        emotional_weight: parse_finite_f32(fields[7])?.clamp(0.0, 1.0),
        created_tick: fields[8].parse().ok()?,
        location: parse_vec3(fields[9])?,
        decay_policy: parse_decay_policy(fields[10])?,
    })
}

fn encode_relationship_edge(edge: &RelationshipEdge) -> String {
    format!(
        "relationship:{}|{}|{}|{}|{}|{}",
        edge.source,
        edge.target,
        edge.trust,
        edge.fear,
        edge.suspicion,
        encode_text_list(&edge.history),
    )
}

fn parse_relationship_edge(value: &str) -> Option<RelationshipEdge> {
    let fields = value.split('|').collect::<Vec<_>>();
    if fields.len() != 6 {
        return None;
    }
    Some(RelationshipEdge {
        source: fields[0].parse().ok()?,
        target: fields[1].parse().ok()?,
        trust: parse_finite_f32(fields[2])?.clamp(-1.0, 1.0),
        fear: parse_finite_f32(fields[3])?.clamp(0.0, 1.0),
        suspicion: parse_finite_f32(fields[4])?.clamp(0.0, 1.0),
        history: parse_text_list(fields[5])?,
    })
}

fn encode_story_thread(thread: &StoryThread) -> String {
    format!(
        "story_thread:{}|{}|{}|{}|{}|{}|{}",
        thread.thread_id,
        encode_state_text(&thread.label),
        join_u128(&thread.grounded_events),
        join_u64(&thread.involved_factions),
        join_u64(&thread.involved_agents),
        thread.pressure,
        story_thread_status_label(thread.status),
    )
}

fn parse_story_thread(value: &str) -> Option<StoryThread> {
    let fields = value.split('|').collect::<Vec<_>>();
    if fields.len() != 7 {
        return None;
    }
    Some(StoryThread {
        thread_id: fields[0].parse().ok()?,
        label: decode_state_text(fields[1])?,
        grounded_events: parse_u128_list(fields[2])?,
        involved_factions: parse_u64_list(fields[3])?,
        involved_agents: parse_u64_list(fields[4])?,
        pressure: parse_finite_f32(fields[5])?.clamp(0.0, 1.0),
        status: parse_story_thread_status(fields[6])?,
    })
}

fn encode_rumor_record(rumor: &RumorRecord) -> String {
    format!(
        "rumor:{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        rumor.rumor_id,
        rumor.source_event,
        rumor.origin_faction,
        rumor.credibility,
        rumor.spread,
        rumor.falsehood_risk,
        rumor.incomplete,
        encode_state_text(&rumor.label),
        join_u64(&rumor.known_to_factions),
        join_u64(&rumor.known_to_agents),
        join_u128(&rumor.grounded_events),
    )
}

fn parse_rumor_record(value: &str) -> Option<RumorRecord> {
    let fields = value.split('|').collect::<Vec<_>>();
    if fields.len() != 11 {
        return None;
    }
    let mut known_to_factions = parse_u64_list(fields[8])?;
    known_to_factions.sort_unstable();
    known_to_factions.dedup();
    let mut known_to_agents = parse_u64_list(fields[9])?;
    known_to_agents.sort_unstable();
    known_to_agents.dedup();
    let mut grounded_events = parse_u128_list(fields[10])?;
    grounded_events.sort_unstable();
    grounded_events.dedup();

    Some(RumorRecord {
        rumor_id: fields[0].parse().ok()?,
        source_event: fields[1].parse().ok()?,
        origin_faction: fields[2].parse().ok()?,
        credibility: parse_finite_f32(fields[3])?.clamp(0.0, 1.0),
        spread: parse_finite_f32(fields[4])?.clamp(0.0, 1.0),
        falsehood_risk: parse_finite_f32(fields[5])?.clamp(0.0, 1.0),
        incomplete: fields[6].parse().ok()?,
        label: decode_state_text(fields[7])?,
        known_to_factions,
        known_to_agents,
        grounded_events,
    })
}

fn encode_story_opportunity(opportunity: &StoryOpportunityRecord) -> String {
    format!(
        "opportunity:{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        opportunity.opportunity_id,
        story_opportunity_kind_label(opportunity.kind),
        encode_state_text(&opportunity.label),
        opportunity.grounded_event,
        join_u64(&opportunity.involved_factions),
        join_u64(&opportunity.involved_agents),
        opportunity.pressure,
        opportunity.urgency,
        opportunity.requires_world_validation,
        opportunity.offered,
        encode_optional_u64(opportunity.source_rumor),
    )
}

fn parse_story_opportunity(value: &str) -> Option<StoryOpportunityRecord> {
    let fields = value.split('|').collect::<Vec<_>>();
    if fields.len() != 11 {
        return None;
    }
    let mut involved_factions = parse_u64_list(fields[4])?;
    involved_factions.sort_unstable();
    involved_factions.dedup();
    let mut involved_agents = parse_u64_list(fields[5])?;
    involved_agents.sort_unstable();
    involved_agents.dedup();

    Some(StoryOpportunityRecord {
        opportunity_id: fields[0].parse().ok()?,
        kind: parse_story_opportunity_kind(fields[1])?,
        label: decode_state_text(fields[2])?,
        grounded_event: fields[3].parse().ok()?,
        involved_factions,
        involved_agents,
        pressure: parse_finite_f32(fields[6])?.clamp(0.0, 1.0),
        urgency: parse_finite_f32(fields[7])?.clamp(0.0, 1.0),
        requires_world_validation: fields[8].parse().ok()?,
        offered: fields[9].parse().ok()?,
        source_rumor: parse_optional_u64(fields[10])?,
    })
}

fn story_thread_status_label(status: StoryThreadStatus) -> &'static str {
    match status {
        StoryThreadStatus::Seeded => "seeded",
        StoryThreadStatus::Escalating => "escalating",
        StoryThreadStatus::CoolingDown => "cooling_down",
        StoryThreadStatus::Resolved => "resolved",
    }
}

fn parse_story_thread_status(value: &str) -> Option<StoryThreadStatus> {
    match value {
        "seeded" => Some(StoryThreadStatus::Seeded),
        "escalating" => Some(StoryThreadStatus::Escalating),
        "cooling_down" => Some(StoryThreadStatus::CoolingDown),
        "resolved" => Some(StoryThreadStatus::Resolved),
        _ => None,
    }
}

fn story_opportunity_kind_label(kind: StoryOpportunityKind) -> &'static str {
    match kind {
        StoryOpportunityKind::Job => "job",
        StoryOpportunityKind::MoralDilemma => "moral_dilemma",
        StoryOpportunityKind::FactionConflict => "faction_conflict",
        StoryOpportunityKind::Secret => "secret",
    }
}

fn parse_story_opportunity_kind(value: &str) -> Option<StoryOpportunityKind> {
    match value {
        "job" => Some(StoryOpportunityKind::Job),
        "moral_dilemma" => Some(StoryOpportunityKind::MoralDilemma),
        "faction_conflict" => Some(StoryOpportunityKind::FactionConflict),
        "secret" => Some(StoryOpportunityKind::Secret),
        _ => None,
    }
}

fn encode_faction_state(state: &FactionState) -> String {
    format!(
        "faction_state:{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        state.faction,
        state.resources.credits,
        state.resources.influence,
        state.resources.personnel,
        state.resources.data_access,
        state.resources.water_access,
        join_u64(&state.territory),
        join_u64(&state.allies),
        join_u64(&state.enemies),
        join_u64(&state.secrets),
        state.alertness,
        state.trust_in_player,
        state.fear_of_player,
        state.territory_pressure,
        state.reputation_with_player,
    )
}

fn parse_faction_state(value: &str) -> Option<FactionState> {
    let fields = value.split('|').collect::<Vec<_>>();
    if fields.len() != 15 {
        return None;
    }
    Some(FactionState {
        faction: fields[0].parse().ok()?,
        resources: ResourceState {
            credits: fields[1].parse().ok()?,
            influence: parse_finite_f32(fields[2])?.clamp(0.0, 1.0),
            personnel: fields[3].parse().ok()?,
            data_access: parse_finite_f32(fields[4])?.clamp(0.0, 1.0),
            water_access: parse_finite_f32(fields[5])?.clamp(0.0, 1.0),
        },
        territory: parse_u64_list(fields[6])?,
        allies: parse_u64_list(fields[7])?,
        enemies: parse_u64_list(fields[8])?,
        secrets: parse_u64_list(fields[9])?,
        alertness: parse_finite_f32(fields[10])?.clamp(0.0, 1.0),
        trust_in_player: parse_finite_f32(fields[11])?.clamp(-1.0, 1.0),
        fear_of_player: parse_finite_f32(fields[12])?.clamp(0.0, 1.0),
        territory_pressure: parse_finite_f32(fields[13])?.clamp(0.0, 1.0),
        reputation_with_player: parse_finite_f32(fields[14])?.clamp(-1.0, 1.0),
    })
}

pub type WorldSnapshotRef = u64;
pub type AgentView = Vec<EntityId>;
pub type RelationshipGraphRef = u64;
pub type FactionGraphRef = u64;
pub type AvailableActionSet = Vec<String>;
pub type AiBudget = QualityTier;
pub type ValueTag = String;
pub type FearTag = String;
pub type GoalTag = String;
pub type SecretRef = u64;
pub type SkillSet = Vec<String>;
pub type SpeechStyle = String;
pub type SafetyProfile = String;
pub type Goal = String;
pub type Plan = Vec<PlanStep>;
pub type MemoryStoreRef = u64;
pub type FactSetRef = u64;
pub type ThreatRef = WorldEventId;
pub type TradeOffer = String;
pub type HackMethod = String;
pub type CallReason = String;
pub type DialogueIntent = String;
pub type MemoryContent = String;
pub type AiTimingReport = PerformanceCounters;
pub type StoryThreadId = u64;
pub type RumorId = u64;
pub type StoryOpportunityId = u64;

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerState {
    pub entity: EntityId,
    pub known_identity_exposure: f32,
    pub reputation: ReputationState,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AiFrameInput {
    pub tick: u64,
    pub world: WorldSnapshotRef,
    pub recent_events: Vec<WorldEvent>,
    pub agents: AgentView,
    pub relationships: RelationshipGraphRef,
    pub factions: FactionGraphRef,
    pub player_state: PlayerState,
    pub available_actions: AvailableActionSet,
    pub budget: AiBudget,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AiFrameOutput {
    pub agent_intents: Vec<AgentIntent>,
    pub dialogue_requests: Vec<DialogueRequest>,
    pub memory_updates: Vec<MemoryUpdate>,
    pub relationship_updates: Vec<RelationshipDelta>,
    pub story_events: Vec<StoryEvent>,
    pub director_commands: Vec<DirectorCommand>,
    pub debug_decisions: Vec<AiDebugDecision>,
    pub intent_validation: AgentIntentValidationReport,
    pub ai_timing: AiTimingReport,
}

impl Default for AiFrameOutput {
    fn default() -> Self {
        Self {
            agent_intents: Vec::new(),
            dialogue_requests: Vec::new(),
            memory_updates: Vec::new(),
            relationship_updates: Vec::new(),
            story_events: Vec::new(),
            director_commands: Vec::new(),
            debug_decisions: Vec::new(),
            intent_validation: AgentIntentValidationReport::default(),
            ai_timing: PerformanceCounters {
                cpu_milliseconds: 0.0,
                gpu_milliseconds: 0.0,
                memory_bytes: 0,
            },
        }
    }
}

impl AiFrameOutput {
    pub fn is_empty(&self) -> bool {
        self.agent_intents.is_empty()
            && self.dialogue_requests.is_empty()
            && self.memory_updates.is_empty()
            && self.relationship_updates.is_empty()
            && self.story_events.is_empty()
            && self.director_commands.is_empty()
            && self.debug_decisions.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgentPersona {
    pub id: AgentPersonaId,
    pub name: String,
    pub background: String,
    pub values: Vec<ValueTag>,
    pub fears: Vec<FearTag>,
    pub ambitions: Vec<GoalTag>,
    pub faction: Option<FactionId>,
    pub secrets: Vec<SecretRef>,
    pub skills: SkillSet,
    pub speech_style: SpeechStyle,
    pub voice_persona: VoicePersonaId,
    pub safety_profile: SafetyProfile,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgentRuntimeState {
    pub entity: EntityId,
    pub persona: AgentPersonaId,
    pub current_goal: Option<Goal>,
    pub plan: Plan,
    pub emotional_state: EmotionState,
    pub relationships: RelationshipGraphRef,
    pub memory: MemoryStoreRef,
    pub known_world_facts: FactSetRef,
    pub ai_lod: AiLodTier,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AiLodTier {
    Dormant,
    Schedule,
    Reactive,
    Planning,
    Hero,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanStep {
    pub label: String,
    pub intent: Option<AgentIntent>,
    pub status: PlanStepStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanStepStatus {
    Proposed,
    WaitingForValidation,
    Complete,
    Blocked,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AgentIntent {
    MoveTo {
        agent: EntityId,
        location: LocationId,
    },
    SpeakTo {
        agent: EntityId,
        target: EntityId,
        act: SpeechAct,
    },
    UseObject {
        agent: EntityId,
        object: EntityId,
        action: ObjectAction,
    },
    Attack {
        agent: EntityId,
        target: EntityId,
        method: AttackMethod,
    },
    Flee {
        agent: EntityId,
        from: ThreatRef,
    },
    Investigate {
        agent: EntityId,
        event: WorldEventId,
    },
    Trade {
        agent: EntityId,
        target: EntityId,
        offer: TradeOffer,
    },
    Hack {
        agent: EntityId,
        target: EntityId,
        method: HackMethod,
    },
    ReportCrime {
        agent: EntityId,
        event: WorldEventId,
        authority: FactionId,
    },
    CallAlly {
        agent: EntityId,
        ally: EntityId,
        reason: CallReason,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum SpeechAct {
    Warn,
    Threaten,
    Question,
    Report,
    AskForHelp,
    RevealSecret,
    Bargain,
    Accuse,
    Apologize,
    Distract,
    Comfort,
}

pub type ObjectAction = String;
pub type AttackMethod = String;

#[derive(Clone, Debug, PartialEq)]
pub struct MemoryUpdate {
    pub agent: EntityId,
    pub memory_kind: MemoryKind,
    pub source_event: Option<WorldEventId>,
    pub content: MemoryContent,
    pub importance: f32,
    pub confidence: f32,
    pub emotional_weight: f32,
    pub decay_policy: MemoryDecayPolicy,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MemoryKind {
    ShortTerm,
    LongTerm,
    Social,
    Location,
    Evidence,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MemoryDecayPolicy {
    Never,
    Gradual { half_life_seconds: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemoryRecord {
    pub memory_id: u128,
    pub agent: EntityId,
    pub kind: MemoryKind,
    pub source: KnowledgeSource,
    pub content: MemoryContent,
    pub importance: f32,
    pub confidence: f32,
    pub emotional_weight: f32,
    pub created_tick: u64,
    pub location: Option<Vec3>,
    pub decay_policy: MemoryDecayPolicy,
}

#[derive(Clone, Debug, PartialEq)]
pub enum KnowledgeSource {
    DirectObservation(WorldEventId),
    HeardFrom(EntityId),
    FactionBroadcast(FactionId),
    Inference { based_on: Vec<WorldEventId> },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemoryStore {
    records: Vec<MemoryRecord>,
}

impl MemoryStore {
    pub fn remember(
        &mut self,
        update: MemoryUpdate,
        source: KnowledgeSource,
        created_tick: u64,
        location: Option<Vec3>,
    ) -> u128 {
        let memory_id = deterministic_record_id(created_tick, update.agent, self.records.len());
        self.records.push(MemoryRecord {
            memory_id,
            agent: update.agent,
            kind: update.memory_kind,
            source,
            content: update.content,
            importance: update.importance,
            confidence: update.confidence,
            emotional_weight: update.emotional_weight,
            created_tick,
            location,
            decay_policy: update.decay_policy,
        });
        memory_id
    }

    pub fn records(&self) -> &[MemoryRecord] {
        &self.records
    }

    pub fn for_agent(&self, agent: EntityId) -> Vec<&MemoryRecord> {
        self.records
            .iter()
            .filter(|record| record.agent == agent)
            .collect()
    }

    pub fn recall_evidence_about(&self, agent: EntityId, needle: &str) -> Vec<&MemoryRecord> {
        self.records
            .iter()
            .filter(|record| {
                record.agent == agent
                    && record.kind == MemoryKind::Evidence
                    && record.content.to_ascii_lowercase().contains(needle)
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RelationshipDelta {
    pub source: EntityId,
    pub target: EntityId,
    pub trust_delta: f32,
    pub fear_delta: f32,
    pub suspicion_delta: f32,
    pub reason: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RelationshipGraph {
    edges: BTreeMap<(EntityId, EntityId), RelationshipEdge>,
}

impl RelationshipGraph {
    pub fn apply_delta(&mut self, delta: RelationshipDelta) {
        let edge = self
            .edges
            .entry((delta.source, delta.target))
            .or_insert_with(|| RelationshipEdge::neutral(delta.source, delta.target));
        edge.trust = (edge.trust + delta.trust_delta).clamp(-1.0, 1.0);
        edge.fear = (edge.fear + delta.fear_delta).clamp(0.0, 1.0);
        edge.suspicion = (edge.suspicion + delta.suspicion_delta).clamp(0.0, 1.0);
        edge.history.push(delta.reason);
    }

    pub fn get(&self, source: EntityId, target: EntityId) -> Option<&RelationshipEdge> {
        self.edges.get(&(source, target))
    }

    pub fn for_agent(&self, agent: EntityId) -> Vec<&RelationshipEdge> {
        self.edges
            .values()
            .filter(|edge| edge.source == agent || edge.target == agent)
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RelationshipEdge {
    pub source: EntityId,
    pub target: EntityId,
    pub trust: f32,
    pub fear: f32,
    pub suspicion: f32,
    pub history: Vec<String>,
}

impl RelationshipEdge {
    pub fn neutral(source: EntityId, target: EntityId) -> Self {
        Self {
            source,
            target,
            trust: 0.0,
            fear: 0.0,
            suspicion: 0.0,
            history: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DialogueRequest {
    pub speaker: EntityId,
    pub target: Option<EntityId>,
    pub speech_act: SpeechAct,
    pub intended_meaning: DialogueIntent,
    pub text: Option<String>,
    pub emotion: EmotionState,
    pub urgency: f32,
    pub secrecy: f32,
    pub voice_persona: VoicePersonaId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NarrativeDirectorState {
    pub tension: f32,
    pub unresolved_threads: Vec<String>,
    pub active_factions: Vec<FactionId>,
    pub active_threads: Vec<StoryThread>,
    pub faction_tensions: FactionTensionGraph,
    pub unresolved_secrets: Vec<SecretRef>,
    pub active_rumors: Vec<RumorRecord>,
    pub grounded_opportunities: Vec<StoryOpportunityRecord>,
    pub player_reputation: ReputationState,
    pub city_alertness: AlertnessState,
    pub desired_pacing: PacingProfile,
}

impl Default for NarrativeDirectorState {
    fn default() -> Self {
        Self {
            tension: 0.0,
            unresolved_threads: Vec::new(),
            active_factions: Vec::new(),
            active_threads: Vec::new(),
            faction_tensions: FactionTensionGraph::default(),
            unresolved_secrets: Vec::new(),
            active_rumors: Vec::new(),
            grounded_opportunities: Vec::new(),
            player_reputation: ReputationState::default(),
            city_alertness: AlertnessState::default(),
            desired_pacing: PacingProfile::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoryThread {
    pub thread_id: StoryThreadId,
    pub label: String,
    pub grounded_events: Vec<WorldEventId>,
    pub involved_factions: Vec<FactionId>,
    pub involved_agents: Vec<EntityId>,
    pub pressure: f32,
    pub status: StoryThreadStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoryThreadStatus {
    Seeded,
    Escalating,
    CoolingDown,
    Resolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StoryOpportunityKind {
    Job,
    MoralDilemma,
    FactionConflict,
    Secret,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoryOpportunityRecord {
    pub opportunity_id: StoryOpportunityId,
    pub kind: StoryOpportunityKind,
    pub label: String,
    pub grounded_event: WorldEventId,
    pub involved_factions: Vec<FactionId>,
    pub involved_agents: Vec<EntityId>,
    pub pressure: f32,
    pub urgency: f32,
    pub requires_world_validation: bool,
    pub offered: bool,
    pub source_rumor: Option<RumorId>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FactionTensionGraph {
    tensions: BTreeMap<(FactionId, FactionId), f32>,
}

impl FactionTensionGraph {
    pub fn set_tension(&mut self, a: FactionId, b: FactionId, tension: f32) {
        self.tensions
            .insert(sorted_pair(a, b), tension.clamp(0.0, 1.0));
    }

    pub fn tension(&self, a: FactionId, b: FactionId) -> f32 {
        self.tensions
            .get(&sorted_pair(a, b))
            .copied()
            .unwrap_or(0.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReputationState {
    pub trust_by_faction: BTreeMap<FactionId, f32>,
    pub fear_by_faction: BTreeMap<FactionId, f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AlertnessState {
    pub security: f32,
    pub gangs: f32,
    pub residents: f32,
    pub media: f32,
}

impl Default for AlertnessState {
    fn default() -> Self {
        Self {
            security: 0.0,
            gangs: 0.0,
            residents: 0.0,
            media: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PacingProfile {
    pub target_tension: f32,
    pub cooldown_seconds: f32,
    pub allow_new_threads: bool,
}

impl Default for PacingProfile {
    fn default() -> Self {
        Self {
            target_tension: 0.45,
            cooldown_seconds: 30.0,
            allow_new_threads: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DirectorCommand {
    IncreasePressure { location: LocationId, amount: f32 },
    SeedRumor { faction: FactionId, label: String },
    DelayNonCriticalBeat,
    IntroduceRumor(RumorSpec),
    EscalateFactionConflict(FactionConflictSpec),
    SurfaceSecret(SecretRef),
    OfferJob(JobOpportunitySpec),
    TriggerInvestigation(InvestigationSpec),
    IncreaseSurveillance(LocationId),
    CreateMoralDilemma(DilemmaSpec),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RumorSpec {
    pub source_event: WorldEventId,
    pub faction: FactionId,
    pub label: String,
    pub credibility: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RumorRecord {
    pub rumor_id: RumorId,
    pub source_event: WorldEventId,
    pub origin_faction: FactionId,
    pub label: String,
    pub credibility: f32,
    pub spread: f32,
    pub known_to_factions: Vec<FactionId>,
    pub known_to_agents: Vec<EntityId>,
    pub grounded_events: Vec<WorldEventId>,
    pub incomplete: bool,
    pub falsehood_risk: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FactionConflictSpec {
    pub source_event: WorldEventId,
    pub aggressor: FactionId,
    pub target: FactionId,
    pub pressure: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct JobOpportunitySpec {
    pub label: String,
    pub faction: FactionId,
    pub grounded_event: WorldEventId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InvestigationSpec {
    pub source_event: WorldEventId,
    pub investigating_faction: FactionId,
    pub suspect: EntityId,
    pub severity: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DilemmaSpec {
    pub label: String,
    pub grounded_event: WorldEventId,
    pub involved_factions: Vec<FactionId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FactionState {
    pub faction: FactionId,
    pub resources: ResourceState,
    pub territory: Vec<LocationId>,
    pub allies: Vec<FactionId>,
    pub enemies: Vec<FactionId>,
    pub secrets: Vec<SecretRef>,
    pub alertness: f32,
    pub trust_in_player: f32,
    pub fear_of_player: f32,
    pub territory_pressure: f32,
    pub reputation_with_player: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResourceState {
    pub credits: u32,
    pub influence: f32,
    pub personnel: u32,
    pub data_access: f32,
    pub water_access: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FactionGraph {
    states: BTreeMap<FactionId, FactionState>,
}

impl FactionGraph {
    pub fn security_context(config: &StoryDirectorConfig) -> Self {
        let mut graph = Self::default();
        graph.states.insert(
            config.security_faction,
            FactionState {
                faction: config.security_faction,
                resources: ResourceState {
                    credits: 90_000,
                    influence: 0.72,
                    personnel: 24,
                    data_access: 0.7,
                    water_access: 0.4,
                },
                territory: vec![config.story_location],
                allies: Vec::new(),
                enemies: vec![config.opposition_faction],
                secrets: vec![7_001],
                alertness: 0.25,
                trust_in_player: 0.0,
                fear_of_player: 0.0,
                territory_pressure: 0.35,
                reputation_with_player: 0.0,
            },
        );
        graph
    }

    pub fn default_security_context() -> Self {
        Self::security_context(&StoryDirectorConfig::default())
    }

    pub fn default_alley() -> Self {
        Self::default_security_context()
    }

    pub fn get(&self, faction: FactionId) -> Option<&FactionState> {
        self.states.get(&faction)
    }

    pub fn apply_player_reputation(&mut self, faction: FactionId, delta: f32) {
        if let Some(state) = self.states.get_mut(&faction) {
            state.reputation_with_player = (state.reputation_with_player + delta).clamp(-1.0, 1.0);
            state.trust_in_player = (state.trust_in_player + delta).clamp(-1.0, 1.0);
            state.alertness = (state.alertness + delta.abs() * 0.6).clamp(0.0, 1.0);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgentObservation {
    pub agent: EntityId,
    pub event_id: WorldEventId,
    pub kind: ObservationKind,
    pub knowledge_source: KnowledgeSource,
    pub location: Vec3,
    pub distance_meters: f32,
    pub confidence: f32,
    pub evidence: EvidenceRefs,
    pub tags: TagSet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservationKind {
    Crime,
    Danger,
    Dialogue,
    Navigation,
    Sound,
    Story,
    Unknown,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgentDecision {
    pub agent: EntityId,
    pub goal: Goal,
    pub plan: Plan,
    pub intents: Vec<AgentIntent>,
    pub dialogue: Option<DialogueRequest>,
    pub memory_update: MemoryUpdate,
    pub relationship_delta: Option<RelationshipDelta>,
    pub director_command: Option<DirectorCommand>,
    pub debug: AiDebugDecision,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AiDebugDecision {
    pub agent: EntityId,
    pub event: Option<WorldEventId>,
    pub goal: Goal,
    pub selected_lod: AiLodTier,
    pub available_actions: Vec<String>,
    pub reasons: Vec<String>,
    pub rejected_actions: Vec<String>,
}

pub type AgentDecisionTraceSet = Vec<AgentDecisionTrace>;

#[derive(Clone, Debug, PartialEq)]
pub struct AgentDecisionInspection {
    pub agent: EntityId,
    pub latest_goal: Option<Goal>,
    pub decisions: AgentDecisionTraceSet,
    pub memories: Vec<AgentMemoryTrace>,
    pub intents: Vec<AgentIntentTrace>,
}

impl AgentDecisionInspection {
    pub fn latest_decision(&self) -> Option<&AgentDecisionTrace> {
        self.decisions.last()
    }

    pub fn has_accepted_action(&self, action: &str) -> bool {
        self.decisions.iter().any(|decision| {
            decision
                .accepted_actions
                .iter()
                .any(|candidate| candidate == action)
        })
    }

    pub fn why_lines(&self) -> Vec<String> {
        self.decisions
            .iter()
            .flat_map(|decision| {
                decision
                    .reasons
                    .iter()
                    .map(|reason| format!("{}: {}", decision.goal, reason))
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgentDebugInspection {
    pub agent: EntityId,
    pub latest_goal: Option<Goal>,
    pub goals: Vec<AgentGoalTrace>,
    pub decisions: AgentDecisionTraceSet,
    pub memories: Vec<MemoryRecord>,
    pub memory_events: Vec<AgentMemoryTrace>,
    pub relationships: Vec<RelationshipEdge>,
    pub intents: Vec<AgentIntentTrace>,
    pub why_lines: Vec<String>,
}

impl AgentDebugInspection {
    pub fn latest_decision(&self) -> Option<&AgentDecisionTrace> {
        self.decisions.last()
    }

    pub fn relationships_with(&self, entity: EntityId) -> Vec<&RelationshipEdge> {
        self.relationships
            .iter()
            .filter(|edge| edge.source == entity || edge.target == entity)
            .collect()
    }

    pub fn why_summary(&self) -> String {
        if self.why_lines.is_empty() {
            format!("agent {} has no recorded AI decisions", self.agent)
        } else {
            self.why_lines.join(" | ")
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgentGoalTrace {
    pub goal: Goal,
    pub first_tick: u64,
    pub latest_tick: u64,
    pub decision_count: usize,
    pub source_events: Vec<WorldEventId>,
    pub selected_lods: Vec<String>,
    pub accepted_actions: Vec<String>,
    pub rejected_actions: Vec<String>,
    pub reasons: Vec<String>,
}

impl AgentGoalTrace {
    fn from_decision(decision: &AgentDecisionTrace) -> Self {
        let mut trace = Self {
            goal: decision.goal.clone(),
            first_tick: decision.tick,
            latest_tick: decision.tick,
            decision_count: 0,
            source_events: Vec::new(),
            selected_lods: Vec::new(),
            accepted_actions: Vec::new(),
            rejected_actions: Vec::new(),
            reasons: Vec::new(),
        };
        trace.absorb_decision(decision);
        trace
    }

    fn absorb_decision(&mut self, decision: &AgentDecisionTrace) {
        self.first_tick = self.first_tick.min(decision.tick);
        self.latest_tick = self.latest_tick.max(decision.tick);
        self.decision_count += 1;
        if let Some(source_event) = decision.source_event {
            push_unique(&mut self.source_events, source_event);
        }
        push_unique(&mut self.selected_lods, decision.selected_lod.clone());
        for action in &decision.accepted_actions {
            push_unique(&mut self.accepted_actions, action.clone());
        }
        for action in &decision.rejected_actions {
            push_unique(&mut self.rejected_actions, action.clone());
        }
        for reason in &decision.reasons {
            push_unique(&mut self.reasons, reason.clone());
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgentDecisionTrace {
    pub event_id: WorldEventId,
    pub tick: u64,
    pub source_event: Option<WorldEventId>,
    pub location: Vec3,
    pub goal: Goal,
    pub selected_lod: String,
    pub available_actions: Vec<String>,
    pub accepted_actions: Vec<String>,
    pub rejected_actions: Vec<String>,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgentMemoryTrace {
    pub event_id: WorldEventId,
    pub tick: u64,
    pub source_event: Option<WorldEventId>,
    pub memory_kind: String,
    pub content: String,
    pub importance: f32,
    pub confidence: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgentIntentTrace {
    pub event_id: WorldEventId,
    pub tick: u64,
    pub source_event: Option<WorldEventId>,
    pub action: String,
    pub validation_passed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentIntentValidationError {
    MissingAgent(EntityId),
    MissingTarget(EntityId),
    MissingObject(EntityId),
    MissingEvent(WorldEventId),
    UnavailableAction(&'static str),
    EmptyMethod(&'static str),
    UnsupportedThreat(WorldEventId),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RejectedAgentIntent {
    pub intent: AgentIntent,
    pub error: AgentIntentValidationError,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgentIntentValidationReport {
    pub passed: bool,
    pub accepted: Vec<AgentIntent>,
    pub rejected: Vec<RejectedAgentIntent>,
}

impl Default for AgentIntentValidationReport {
    fn default() -> Self {
        Self {
            passed: true,
            accepted: Vec::new(),
            rejected: Vec::new(),
        }
    }
}

impl AgentIntentValidationReport {
    pub fn merge(&mut self, other: AgentIntentValidationReport) {
        self.accepted.extend(other.accepted);
        self.rejected.extend(other.rejected);
        self.passed = self.rejected.is_empty();
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AiActionTensorScore {
    pub action: String,
    pub score: f32,
    pub rank: usize,
}

pub fn score_agent_actions_with_candle(
    actions: &[String],
    action_scores: &[f32],
) -> candle_core::Result<Vec<AiActionTensorScore>> {
    if actions.len() != action_scores.len() {
        return Err(candle_core::Error::Msg(format!(
            "action count {} did not match score count {}",
            actions.len(),
            action_scores.len()
        )));
    }
    if actions.is_empty() {
        return Ok(Vec::new());
    }

    let device = candle_core::Device::Cpu;
    let logits = candle_core::Tensor::from_slice(action_scores, (action_scores.len(),), &device)?;
    let scores = logits.to_vec1::<f32>()?;
    let mut ranked = actions
        .iter()
        .cloned()
        .zip(scores)
        .map(|(action, score)| AiActionTensorScore {
            action,
            score,
            rank: 0,
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.action.cmp(&right.action))
    });
    for (rank, scored_action) in ranked.iter_mut().enumerate() {
        scored_action.rank = rank + 1;
    }
    Ok(ranked)
}

pub fn inspect_agent_decisions_from_ledger(
    ledger: &EventLedger,
    agent: EntityId,
) -> AgentDecisionInspection {
    inspect_agent_decisions(ledger.all(), agent)
}

pub fn inspect_agent_decisions(events: &[WorldEvent], agent: EntityId) -> AgentDecisionInspection {
    let mut inspection = AgentDecisionInspection {
        agent,
        latest_goal: None,
        decisions: Vec::new(),
        memories: Vec::new(),
        intents: Vec::new(),
    };

    for event in events {
        match &event.kind {
            WorldEventKind::AgentDecisionExplained {
                agent: decision_agent,
                source_event,
                goal,
                selected_lod,
                available_actions,
                accepted_actions,
                rejected_actions,
                reasons,
            } if *decision_agent == agent => {
                inspection.latest_goal = Some(goal.clone());
                inspection.decisions.push(AgentDecisionTrace {
                    event_id: event.event_id,
                    tick: event.tick,
                    source_event: *source_event,
                    location: event.location_meters,
                    goal: goal.clone(),
                    selected_lod: selected_lod.clone(),
                    available_actions: available_actions.clone(),
                    accepted_actions: accepted_actions.clone(),
                    rejected_actions: rejected_actions.clone(),
                    reasons: reasons.clone(),
                });
            }
            WorldEventKind::AgentMemoryUpdated {
                agent: memory_agent,
                source_event,
                memory_kind,
                content,
                importance,
                confidence,
            } if *memory_agent == agent => {
                inspection.memories.push(AgentMemoryTrace {
                    event_id: event.event_id,
                    tick: event.tick,
                    source_event: *source_event,
                    memory_kind: memory_kind.clone(),
                    content: content.clone(),
                    importance: *importance,
                    confidence: *confidence,
                });
            }
            WorldEventKind::AgentIntentProposed {
                agent: intent_agent,
                action,
                source_event,
                validation_passed,
            } if *intent_agent == agent => {
                inspection.intents.push(AgentIntentTrace {
                    event_id: event.event_id,
                    tick: event.tick,
                    source_event: *source_event,
                    action: action.clone(),
                    validation_passed: *validation_passed,
                });
            }
            _ => {}
        }
    }

    inspection
}

pub fn inspect_agent_debug_state_from_ledger(
    module: &AiCharactersModule,
    ledger: &EventLedger,
    agent: EntityId,
) -> AgentDebugInspection {
    inspect_agent_debug_state(module, ledger.all(), agent)
}

pub fn inspect_agent_debug_state(
    module: &AiCharactersModule,
    events: &[WorldEvent],
    agent: EntityId,
) -> AgentDebugInspection {
    let AgentDecisionInspection {
        agent,
        latest_goal,
        decisions,
        memories: memory_events,
        intents,
    } = inspect_agent_decisions(events, agent);

    let goals = build_goal_traces(&decisions);
    let memories = module
        .memory_store()
        .for_agent(agent)
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    let relationships = module
        .relationship_graph()
        .for_agent(agent)
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    let why_lines = build_agent_why_lines(
        agent,
        &decisions,
        &memory_events,
        &memories,
        &relationships,
        &intents,
    );

    AgentDebugInspection {
        agent,
        latest_goal,
        goals,
        decisions,
        memories,
        memory_events,
        relationships,
        intents,
        why_lines,
    }
}

fn build_goal_traces(decisions: &[AgentDecisionTrace]) -> Vec<AgentGoalTrace> {
    let mut traces = BTreeMap::<Goal, AgentGoalTrace>::new();
    for decision in decisions {
        traces
            .entry(decision.goal.clone())
            .and_modify(|trace| trace.absorb_decision(decision))
            .or_insert_with(|| AgentGoalTrace::from_decision(decision));
    }
    traces.into_values().collect()
}

fn build_agent_why_lines(
    agent: EntityId,
    decisions: &[AgentDecisionTrace],
    memory_events: &[AgentMemoryTrace],
    memories: &[MemoryRecord],
    relationships: &[RelationshipEdge],
    intents: &[AgentIntentTrace],
) -> Vec<String> {
    let mut lines = Vec::new();

    for decision in decisions {
        lines.push(format!(
            "tick {}: agent {} pursued '{}' for {}; LOD {}; accepted {}; because {}",
            decision.tick,
            agent,
            decision.goal,
            source_event_text(decision.source_event),
            decision.selected_lod,
            string_list_or(&decision.accepted_actions, "no actions"),
            string_list_or(&decision.reasons, "no recorded reason")
        ));
        if !decision.rejected_actions.is_empty() {
            lines.push(format!(
                "tick {}: rejected {}",
                decision.tick,
                string_list_or(&decision.rejected_actions, "no rejected actions")
            ));
        }
    }

    for memory in memories {
        lines.push(format!(
            "memory {}: {} from {} with confidence {:.2}",
            memory_kind_label(&memory.kind),
            memory.content,
            knowledge_source_text(&memory.source),
            memory.confidence
        ));
    }

    for memory in memory_events {
        lines.push(format!(
            "memory event tick {}: stored {} for {} with importance {:.2}",
            memory.tick,
            memory.memory_kind,
            source_event_text(memory.source_event),
            memory.importance
        ));
    }

    for relationship in relationships {
        let other = if relationship.source == agent {
            relationship.target
        } else {
            relationship.source
        };
        let direction = if relationship.source == agent {
            "toward"
        } else {
            "from"
        };
        let history = relationship
            .history
            .last()
            .map(String::as_str)
            .unwrap_or("no recorded relationship history");
        lines.push(format!(
            "relationship {} {}: trust {:.2}, fear {:.2}, suspicion {:.2}; latest reason {}",
            direction,
            other,
            relationship.trust,
            relationship.fear,
            relationship.suspicion,
            history
        ));
    }

    for intent in intents.iter().filter(|intent| !intent.validation_passed) {
        lines.push(format!(
            "intent {} for {} failed validation at tick {}",
            intent.action,
            source_event_text(intent.source_event),
            intent.tick
        ));
    }

    lines
}

fn source_event_text(source_event: Option<WorldEventId>) -> String {
    match source_event {
        Some(event_id) => format!("event {event_id}"),
        None => "no source event".to_string(),
    }
}

fn knowledge_source_text(source: &KnowledgeSource) -> String {
    match source {
        KnowledgeSource::DirectObservation(event_id) => {
            format!("direct observation of event {event_id}")
        }
        KnowledgeSource::HeardFrom(entity) => format!("entity {entity}"),
        KnowledgeSource::FactionBroadcast(faction) => format!("faction broadcast {faction}"),
        KnowledgeSource::Inference { based_on } => {
            format!("inference from events {}", join_u128(based_on))
        }
    }
}

fn string_list_or(values: &[String], fallback: &str) -> String {
    if values.is_empty() {
        fallback.to_string()
    } else {
        values.join(", ")
    }
}

fn push_unique<T: PartialEq>(values: &mut Vec<T>, value: T) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn unit_f32(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn deterministic_rumor_id(source_event: WorldEventId, faction: FactionId, label: &str) -> RumorId {
    let mut hash = (source_event as u64) ^ ((source_event >> 64) as u64).rotate_left(17);
    hash ^= faction.rotate_left(11);
    for byte in label.bytes().take(96) {
        hash ^= u64::from(byte);
        hash = hash.rotate_left(7).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
    hash.max(1)
}

fn deterministic_story_opportunity_id(
    grounded_event: WorldEventId,
    kind: StoryOpportunityKind,
    label: &str,
) -> StoryOpportunityId {
    let kind_seed = match kind {
        StoryOpportunityKind::Job => 0x4A4F_4201,
        StoryOpportunityKind::MoralDilemma => 0xD11E_0002,
        StoryOpportunityKind::FactionConflict => 0xFAAC_0003,
        StoryOpportunityKind::Secret => 0x5EC7_0004,
    };
    let mut hash =
        (grounded_event as u64) ^ ((grounded_event >> 64) as u64).rotate_left(23) ^ kind_seed;
    for byte in label.bytes().take(96) {
        hash ^= u64::from(byte);
        hash = hash.rotate_left(9).wrapping_mul(0xA24B_AED4_963E_E407);
    }
    hash.max(1)
}

fn rumor_record_from_spec(
    spec: &RumorSpec,
    known_to_agents: &[EntityId],
    extra_grounded_events: &[WorldEventId],
) -> RumorRecord {
    let credibility = unit_f32(spec.credibility);
    let mut known_to_factions = vec![spec.faction];
    known_to_factions.sort_unstable();
    known_to_factions.dedup();
    let mut known_to_agents = known_to_agents.to_vec();
    known_to_agents.sort_unstable();
    known_to_agents.dedup();
    let mut grounded_events = vec![spec.source_event];
    grounded_events.extend_from_slice(extra_grounded_events);
    grounded_events.sort_unstable();
    grounded_events.dedup();
    let spread =
        (0.08 + credibility * 0.18 + known_to_agents.len() as f32 * 0.035).clamp(0.05, 0.65);

    RumorRecord {
        rumor_id: deterministic_rumor_id(spec.source_event, spec.faction, &spec.label),
        source_event: spec.source_event,
        origin_faction: spec.faction,
        label: spec.label.clone(),
        credibility,
        spread,
        known_to_factions,
        known_to_agents,
        grounded_events,
        incomplete: credibility < 0.75,
        falsehood_risk: (1.0 - credibility) * 0.65,
    }
}

fn push_or_merge_rumor(
    state: &mut NarrativeDirectorState,
    spec: &RumorSpec,
    known_to_agents: &[EntityId],
    extra_grounded_events: &[WorldEventId],
) {
    let incoming = rumor_record_from_spec(spec, known_to_agents, extra_grounded_events);
    if let Some(existing) = state.active_rumors.iter_mut().find(|rumor| {
        rumor.source_event == incoming.source_event
            && rumor.origin_faction == incoming.origin_faction
            && rumor.label == incoming.label
    }) {
        existing.credibility = existing.credibility.max(incoming.credibility);
        existing.falsehood_risk = existing.falsehood_risk.min(incoming.falsehood_risk);
        existing.spread = (existing.spread.max(incoming.spread) + 0.04).clamp(0.0, 1.0);
        existing.incomplete &= incoming.incomplete;
        for faction in incoming.known_to_factions {
            push_unique(&mut existing.known_to_factions, faction);
        }
        for agent in incoming.known_to_agents {
            push_unique(&mut existing.known_to_agents, agent);
        }
        for event in incoming.grounded_events {
            push_unique(&mut existing.grounded_events, event);
        }
        existing.known_to_factions.sort_unstable();
        existing.known_to_agents.sort_unstable();
        existing.grounded_events.sort_unstable();
        return;
    }

    state.active_rumors.push(incoming);
    state.active_rumors.sort_by_key(|rumor| rumor.rumor_id);
}

struct StoryOpportunitySeed<'a> {
    kind: StoryOpportunityKind,
    label: &'a str,
    grounded_event: WorldEventId,
    involved_factions: &'a [FactionId],
    involved_agents: &'a [EntityId],
    pressure: f32,
    urgency: f32,
    requires_world_validation: bool,
    source_rumor: Option<RumorId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EnvironmentalLiquidStory {
    Flooding,
    OilSlick,
    Biohazard,
}

impl EnvironmentalLiquidStory {
    fn from_event(event: &WorldEvent) -> Self {
        if world_event_contains_any(
            event,
            &[
                "biohazard",
                "biological",
                "biological_contamination",
                "blood",
            ],
        ) {
            Self::Biohazard
        } else if world_event_contains_any(
            event,
            &["fuel", "grease", "hydraulic", "oil", "slick_surface"],
        ) {
            Self::OilSlick
        } else {
            Self::Flooding
        }
    }

    fn tension_delta(self) -> f32 {
        match self {
            Self::Flooding => 0.1,
            Self::OilSlick => 0.12,
            Self::Biohazard => 0.15,
        }
    }

    fn pressure(self) -> f32 {
        match self {
            Self::Flooding => 0.46,
            Self::OilSlick => 0.5,
            Self::Biohazard => 0.58,
        }
    }

    fn urgency(self) -> f32 {
        match self {
            Self::Flooding => 0.5,
            Self::OilSlick => 0.64,
            Self::Biohazard => 0.76,
        }
    }

    fn thread_salt(self) -> u64 {
        match self {
            Self::Flooding => 0xF100_D001,
            Self::OilSlick => 0x0115_11CC,
            Self::Biohazard => 0xB10A_2A4D,
        }
    }

    fn unresolved_thread(self) -> &'static str {
        match self {
            Self::Flooding => "flooded alley access route needs intervention",
            Self::OilSlick => "oil slick blocks safe alley movement",
            Self::Biohazard => "biological spill needs containment before it spreads",
        }
    }

    fn thread_label(self) -> &'static str {
        match self {
            Self::Flooding => "flooded route cleanup",
            Self::OilSlick => "oil slick containment",
            Self::Biohazard => "biohazard containment",
        }
    }

    fn opportunity_label(self) -> &'static str {
        match self {
            Self::Flooding => "pump out flooded service route before patrols close the block",
            Self::OilSlick => "contain the oil slick before bikes and foot traffic slide through",
            Self::Biohazard => {
                "contain the biological spill before residents track it through the block"
            }
        }
    }
}

fn world_event_contains_any(event: &WorldEvent, needles: &[&str]) -> bool {
    event
        .physical_evidence
        .iter()
        .chain(event.narrative_tags.iter())
        .any(|tag| {
            let normalized = tag.to_ascii_lowercase();
            needles.iter().any(|needle| normalized.contains(needle))
        })
}

fn push_or_merge_story_opportunity(
    state: &mut NarrativeDirectorState,
    seed: StoryOpportunitySeed<'_>,
) -> StoryOpportunityId {
    let label = seed.label.to_string();
    let opportunity_id =
        deterministic_story_opportunity_id(seed.grounded_event, seed.kind, seed.label);
    let mut involved_factions = seed.involved_factions.to_vec();
    involved_factions.sort_unstable();
    involved_factions.dedup();
    let mut involved_agents = seed.involved_agents.to_vec();
    involved_agents.sort_unstable();
    involved_agents.dedup();

    if let Some(existing) = state.grounded_opportunities.iter_mut().find(|opportunity| {
        opportunity.grounded_event == seed.grounded_event
            && opportunity.kind == seed.kind
            && opportunity.label == label
    }) {
        existing.pressure = existing.pressure.max(unit_f32(seed.pressure));
        existing.urgency = existing.urgency.max(unit_f32(seed.urgency));
        existing.requires_world_validation |= seed.requires_world_validation;
        existing.offered = true;
        existing.source_rumor = existing.source_rumor.or(seed.source_rumor);
        for faction in involved_factions {
            push_unique(&mut existing.involved_factions, faction);
        }
        for agent in involved_agents {
            push_unique(&mut existing.involved_agents, agent);
        }
        existing.involved_factions.sort_unstable();
        existing.involved_agents.sort_unstable();
        return existing.opportunity_id;
    }

    state.grounded_opportunities.push(StoryOpportunityRecord {
        opportunity_id,
        kind: seed.kind,
        label,
        grounded_event: seed.grounded_event,
        involved_factions,
        involved_agents,
        pressure: unit_f32(seed.pressure),
        urgency: unit_f32(seed.urgency),
        requires_world_validation: seed.requires_world_validation,
        offered: true,
        source_rumor: seed.source_rumor,
    });
    state
        .grounded_opportunities
        .sort_by_key(|opportunity| opportunity.opportunity_id);
    opportunity_id
}

pub struct StoryDirectorModule {
    config: StoryDirectorConfig,
    identity_exposures_handled: BTreeSet<WorldEventId>,
    sound_reports_handled: BTreeSet<WorldEventId>,
    opportunities_handled: BTreeSet<WorldEventId>,
    state: NarrativeDirectorState,
    factions: FactionGraph,
    commands: Vec<DirectorCommand>,
    last_workload: Option<StoryGpuWorkload>,
}

impl Default for StoryDirectorModule {
    fn default() -> Self {
        Self::new(StoryDirectorConfig::default())
    }
}

impl StoryDirectorModule {
    pub fn new(config: StoryDirectorConfig) -> Self {
        Self {
            config,
            identity_exposures_handled: BTreeSet::new(),
            sound_reports_handled: BTreeSet::new(),
            opportunities_handled: BTreeSet::new(),
            state: NarrativeDirectorState::default(),
            factions: FactionGraph::security_context(&config),
            commands: Vec::new(),
            last_workload: None,
        }
    }

    pub fn config(&self) -> StoryDirectorConfig {
        self.config
    }

    pub fn state(&self) -> &NarrativeDirectorState {
        &self.state
    }

    pub fn factions(&self) -> &FactionGraph {
        &self.factions
    }

    pub fn commands(&self) -> &[DirectorCommand] {
        &self.commands
    }

    fn record_identity_exposure(&mut self, event: &WorldEvent, threat: EntityId) {
        let security_faction = self.config.security_faction;
        let opposition_faction = self.config.opposition_faction;
        self.state.tension = (self.state.tension + 0.28).clamp(0.0, 1.0);
        self.state.city_alertness.security =
            (self.state.city_alertness.security + 0.35).clamp(0.0, 1.0);
        self.state.active_factions.push(security_faction);
        self.state.active_factions.sort_unstable();
        self.state.active_factions.dedup();
        self.state
            .player_reputation
            .trust_by_faction
            .insert(security_faction, -0.25);
        self.state
            .player_reputation
            .fear_by_faction
            .insert(security_faction, 0.12);
        self.state
            .faction_tensions
            .set_tension(security_faction, opposition_faction, 0.66);
        self.state
            .unresolved_threads
            .push("security investigation into alley vandalism".to_string());
        self.state.active_threads.push(StoryThread {
            thread_id: event.event_id as u64,
            label: "alley vandalism investigation".to_string(),
            grounded_events: vec![event.event_id],
            involved_factions: vec![security_faction],
            involved_agents: event.actors.clone(),
            pressure: 0.72,
            status: StoryThreadStatus::Escalating,
        });
        self.factions
            .apply_player_reputation(security_faction, -0.25);
        let rumor = RumorSpec {
            source_event: event.event_id,
            faction: security_faction,
            label: "a witness identified the glass-breaker".to_string(),
            credibility: 0.86,
        };
        let rumor_id = deterministic_rumor_id(rumor.source_event, rumor.faction, &rumor.label);
        push_or_merge_rumor(&mut self.state, &rumor, &event.actors, &[event.event_id]);
        push_or_merge_story_opportunity(
            &mut self.state,
            StoryOpportunitySeed {
                kind: StoryOpportunityKind::MoralDilemma,
                label: "erase the witness feed or let security pressure rise",
                grounded_event: event.event_id,
                involved_factions: &[security_faction, opposition_faction],
                involved_agents: &event.actors,
                pressure: 0.7,
                urgency: 0.78,
                requires_world_validation: true,
                source_rumor: Some(rumor_id),
            },
        );
        push_or_merge_story_opportunity(
            &mut self.state,
            StoryOpportunitySeed {
                kind: StoryOpportunityKind::FactionConflict,
                label: "security blames street opposition for the alley breach",
                grounded_event: event.event_id,
                involved_factions: &[security_faction, opposition_faction],
                involved_agents: &event.actors,
                pressure: 0.66,
                urgency: 0.62,
                requires_world_validation: true,
                source_rumor: Some(rumor_id),
            },
        );
        if let Some(secret) = self
            .factions
            .get(security_faction)
            .and_then(|faction| faction.secrets.first().copied())
        {
            push_unique(&mut self.state.unresolved_secrets, secret);
            push_or_merge_story_opportunity(
                &mut self.state,
                StoryOpportunitySeed {
                    kind: StoryOpportunityKind::Secret,
                    label: "security feed access can be traded as leverage",
                    grounded_event: event.event_id,
                    involved_factions: &[security_faction],
                    involved_agents: &event.actors,
                    pressure: 0.58,
                    urgency: 0.52,
                    requires_world_validation: true,
                    source_rumor: Some(rumor_id),
                },
            );
            self.commands.push(DirectorCommand::SurfaceSecret(secret));
        }
        self.commands
            .push(DirectorCommand::TriggerInvestigation(InvestigationSpec {
                source_event: event.event_id,
                investigating_faction: security_faction,
                suspect: threat,
                severity: 0.72,
            }));
        self.commands.push(DirectorCommand::IntroduceRumor(rumor));
        self.commands
            .push(DirectorCommand::CreateMoralDilemma(DilemmaSpec {
                label: "erase the witness feed or let security pressure rise".to_string(),
                grounded_event: event.event_id,
                involved_factions: vec![security_faction, opposition_faction],
            }));
        self.commands.push(DirectorCommand::EscalateFactionConflict(
            FactionConflictSpec {
                source_event: event.event_id,
                aggressor: security_faction,
                target: opposition_faction,
                pressure: 0.66,
            },
        ));
    }

    fn record_heard_sound_report(
        &mut self,
        event: &WorldEvent,
        source_event: WorldEventId,
        confidence: f32,
    ) -> f32 {
        let security_faction = self.config.security_faction;
        let story_location = self.config.story_location;
        let amount = (0.05 + confidence.clamp(0.0, 1.0) * 0.1).clamp(0.05, 0.16);
        self.state.tension = (self.state.tension + amount * 0.35).clamp(0.0, 1.0);
        self.state.city_alertness.security =
            (self.state.city_alertness.security + amount).clamp(0.0, 1.0);
        self.state.active_factions.push(security_faction);
        self.state.active_factions.sort_unstable();
        self.state.active_factions.dedup();
        if !self
            .state
            .unresolved_threads
            .iter()
            .any(|thread| thread == "security sweep prompted by unidentified sound")
        {
            self.state
                .unresolved_threads
                .push("security sweep prompted by unidentified sound".to_string());
        }
        self.state.active_threads.push(StoryThread {
            thread_id: (event.event_id as u64) ^ 0xA11D_0001,
            label: "unidentified sound investigation".to_string(),
            grounded_events: vec![source_event, event.event_id],
            involved_factions: vec![security_faction],
            involved_agents: event.actors.clone(),
            pressure: amount,
            status: StoryThreadStatus::Seeded,
        });
        self.commands
            .push(DirectorCommand::IncreaseSurveillance(story_location));
        self.commands.push(DirectorCommand::IncreasePressure {
            location: story_location,
            amount,
        });
        let rumor = RumorSpec {
            source_event,
            faction: security_faction,
            label: "an unidentified impact drew a security sweep".to_string(),
            credibility: 0.42 + confidence.clamp(0.0, 1.0) * 0.32,
        };
        let rumor_id = deterministic_rumor_id(rumor.source_event, rumor.faction, &rumor.label);
        push_or_merge_rumor(
            &mut self.state,
            &rumor,
            &event.actors,
            &[source_event, event.event_id],
        );
        self.commands.push(DirectorCommand::IntroduceRumor(rumor));
        push_or_merge_story_opportunity(
            &mut self.state,
            StoryOpportunitySeed {
                kind: StoryOpportunityKind::Job,
                label: "trace the source of the unidentified impact",
                grounded_event: source_event,
                involved_factions: &[security_faction],
                involved_agents: &event.actors,
                pressure: amount.max(0.18),
                urgency: confidence.clamp(0.0, 1.0),
                requires_world_validation: true,
                source_rumor: Some(rumor_id),
            },
        );
        self.commands
            .push(DirectorCommand::OfferJob(JobOpportunitySpec {
                label: "trace the source of the unidentified impact".to_string(),
                faction: security_faction,
                grounded_event: source_event,
            }));
        amount
    }

    fn record_environmental_opportunity(&mut self, event: &WorldEvent) {
        if !self.opportunities_handled.insert(event.event_id) {
            return;
        }

        let security_faction = self.config.security_faction;
        let opposition_faction = self.config.opposition_faction;
        push_unique(&mut self.state.active_factions, security_faction);
        self.state.active_factions.sort_unstable();
        self.state.city_alertness.residents =
            (self.state.city_alertness.residents + 0.12).clamp(0.0, 1.0);

        match &event.kind {
            WorldEventKind::StreetFlooded => {
                let liquid_story = EnvironmentalLiquidStory::from_event(event);
                self.state.tension =
                    (self.state.tension + liquid_story.tension_delta()).clamp(0.0, 1.0);
                push_unique(
                    &mut self.state.unresolved_threads,
                    liquid_story.unresolved_thread().to_string(),
                );
                self.state.active_threads.push(StoryThread {
                    thread_id: (event.event_id as u64) ^ liquid_story.thread_salt(),
                    label: liquid_story.thread_label().to_string(),
                    grounded_events: vec![event.event_id],
                    involved_factions: vec![security_faction],
                    involved_agents: event.actors.clone(),
                    pressure: liquid_story.pressure(),
                    status: StoryThreadStatus::Seeded,
                });
                push_or_merge_story_opportunity(
                    &mut self.state,
                    StoryOpportunitySeed {
                        kind: StoryOpportunityKind::Job,
                        label: liquid_story.opportunity_label(),
                        grounded_event: event.event_id,
                        involved_factions: &[security_faction],
                        involved_agents: &event.actors,
                        pressure: liquid_story.pressure(),
                        urgency: liquid_story.urgency(),
                        requires_world_validation: true,
                        source_rumor: None,
                    },
                );
                self.commands
                    .push(DirectorCommand::OfferJob(JobOpportunitySpec {
                        label: liquid_story.opportunity_label().to_string(),
                        faction: security_faction,
                        grounded_event: event.event_id,
                    }));
            }
            WorldEventKind::ToxicGasReleased => {
                self.state.tension = (self.state.tension + 0.16).clamp(0.0, 1.0);
                push_unique(
                    &mut self.state.unresolved_threads,
                    "toxic gas forces an evacuation choice".to_string(),
                );
                self.state.active_threads.push(StoryThread {
                    thread_id: (event.event_id as u64) ^ 0x6A5_D11E,
                    label: "toxic gas evacuation dilemma".to_string(),
                    grounded_events: vec![event.event_id],
                    involved_factions: vec![security_faction, opposition_faction],
                    involved_agents: event.actors.clone(),
                    pressure: 0.62,
                    status: StoryThreadStatus::Escalating,
                });
                push_or_merge_story_opportunity(
                    &mut self.state,
                    StoryOpportunitySeed {
                        kind: StoryOpportunityKind::MoralDilemma,
                        label: "choose a safe evacuation route that exposes or protects witnesses",
                        grounded_event: event.event_id,
                        involved_factions: &[security_faction, opposition_faction],
                        involved_agents: &event.actors,
                        pressure: 0.62,
                        urgency: 0.82,
                        requires_world_validation: true,
                        source_rumor: None,
                    },
                );
                self.commands
                    .push(DirectorCommand::CreateMoralDilemma(DilemmaSpec {
                        label: "choose a safe evacuation route that exposes or protects witnesses"
                            .to_string(),
                        grounded_event: event.event_id,
                        involved_factions: vec![security_faction, opposition_faction],
                    }));
            }
            WorldEventKind::PowerTransformerOverheated { .. } => {
                self.state.tension = (self.state.tension + 0.14).clamp(0.0, 1.0);
                self.state
                    .faction_tensions
                    .set_tension(security_faction, opposition_faction, 0.72);
                push_unique(
                    &mut self.state.unresolved_threads,
                    "blackout leverage changes patrol and faction pressure".to_string(),
                );
                self.state.active_threads.push(StoryThread {
                    thread_id: (event.event_id as u64) ^ 0xB1AC_0001,
                    label: "blackout leverage dispute".to_string(),
                    grounded_events: vec![event.event_id],
                    involved_factions: vec![security_faction, opposition_faction],
                    involved_agents: event.actors.clone(),
                    pressure: 0.6,
                    status: StoryThreadStatus::Escalating,
                });
                push_or_merge_story_opportunity(
                    &mut self.state,
                    StoryOpportunitySeed {
                        kind: StoryOpportunityKind::FactionConflict,
                        label: "blackout control becomes a faction bargaining chip",
                        grounded_event: event.event_id,
                        involved_factions: &[security_faction, opposition_faction],
                        involved_agents: &event.actors,
                        pressure: 0.6,
                        urgency: 0.66,
                        requires_world_validation: true,
                        source_rumor: None,
                    },
                );
                self.commands.push(DirectorCommand::EscalateFactionConflict(
                    FactionConflictSpec {
                        source_event: event.event_id,
                        aggressor: opposition_faction,
                        target: security_faction,
                        pressure: 0.6,
                    },
                ));
            }
            _ => {}
        }
    }
}

impl EngineModule for StoryDirectorModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            module_id: 55,
            name: "procedural_story_director",
            schema: SchemaVersion {
                name: "NarrativeDirectorState",
                version: 1,
            },
            default_quality: QualityTier::NormalRuntime,
        }
    }

    fn schema_requirements(&self) -> Vec<ashfall_core::schema::SchemaRequirement> {
        vec![ashfall_core::schema::SchemaRequirement::required(
            55,
            "AiFrameOutput",
            1,
            "story director consumes grounded AI observations and director command proposals",
        )]
    }

    fn tick(&mut self, frame: &FrameContext, out: &mut CommandSink) {
        self.commands.clear();
        if self.factions.states.is_empty() {
            self.factions = FactionGraph::security_context(&self.config);
        }

        for event in &frame.recent_events {
            match &event.kind {
                WorldEventKind::PlayerIdentityExposed => {
                    if !self.identity_exposures_handled.insert(event.event_id) {
                        continue;
                    }

                    let Some(threat) = event.actors.first().copied() else {
                        continue;
                    };

                    self.record_identity_exposure(event, threat);
                    out.event(WorldEvent {
                        event_id: deterministic_event_id(frame.sim_time.tick, 55, threat),
                        tick: frame.sim_time.tick,
                        location_meters: event.location_meters,
                        actors: event.actors.clone(),
                        kind: WorldEventKind::SecurityAlertRaised {
                            faction: self.config.security_faction,
                            source_event: event.event_id,
                            threat,
                            severity: 0.72,
                        },
                        physical_evidence: vec![
                            "npc_witness".to_string(),
                            "security_feed".to_string(),
                        ],
                        narrative_tags: vec![
                            "security".to_string(),
                            "story_consequence".to_string(),
                            "faction".to_string(),
                        ],
                    });
                    out.event(WorldEvent {
                        event_id: deterministic_event_id(frame.sim_time.tick, 56, threat),
                        tick: frame.sim_time.tick,
                        location_meters: event.location_meters,
                        actors: event.actors.clone(),
                        kind: WorldEventKind::FactionReputationChanged {
                            faction: self.config.security_faction,
                            subject: threat,
                            delta: -0.25,
                            reason: "witnessed vandalism in controlled alley".to_string(),
                        },
                        physical_evidence: vec!["witness_statement".to_string()],
                        narrative_tags: vec![
                            "reputation".to_string(),
                            "faction".to_string(),
                            "story_consequence".to_string(),
                        ],
                    });
                }
                WorldEventKind::NpcHeardSound {
                    event: source_event,
                    confidence,
                    ..
                } => {
                    if *confidence < 0.45 || !self.sound_reports_handled.insert(event.event_id) {
                        continue;
                    }
                    let amount = self.record_heard_sound_report(event, *source_event, *confidence);
                    out.event(WorldEvent {
                        event_id: deterministic_event_id(
                            frame.sim_time.tick,
                            57,
                            (event.event_id as u64).rotate_left(5),
                        ),
                        tick: frame.sim_time.tick,
                        location_meters: event.location_meters,
                        actors: event.actors.clone(),
                        kind: WorldEventKind::SurveillanceIncreased {
                            location: self.config.story_location,
                            source_event: *source_event,
                            amount,
                        },
                        physical_evidence: vec![
                            "heard_sound".to_string(),
                            "npc_audio_report".to_string(),
                        ],
                        narrative_tags: vec![
                            "security".to_string(),
                            "audio".to_string(),
                            "story_consequence".to_string(),
                        ],
                    });
                }
                WorldEventKind::StreetFlooded
                | WorldEventKind::ToxicGasReleased
                | WorldEventKind::PowerTransformerOverheated { .. } => {
                    self.record_environmental_opportunity(event);
                }
                _ => {}
            }
        }

        self.last_workload = Some(story_gpu_workload_from_state(
            frame,
            &self.state,
            self.commands.len(),
        ));
    }

    fn schedule_gpu(&mut self, graph: &mut GpuGraphBuilder) {
        let Some(workload) = self.last_workload.as_ref() else {
            return;
        };
        if workload.active_thread_count == 0
            && workload.active_faction_count == 0
            && workload.rumor_count == 0
            && workload.opportunity_count == 0
            && workload.command_count == 0
        {
            return;
        }

        let recent_event_count =
            u64::try_from(workload.recent_event_count.max(1)).unwrap_or(u64::MAX);
        let active_thread_count =
            u64::try_from(workload.active_thread_count.max(1)).unwrap_or(u64::MAX);
        let active_faction_count =
            u64::try_from(workload.active_faction_count.max(1)).unwrap_or(u64::MAX);
        let faction_tension_count =
            u64::try_from(workload.faction_tension_count.max(1)).unwrap_or(u64::MAX);
        let reputation_entry_count =
            u64::try_from(workload.reputation_entry_count.max(1)).unwrap_or(u64::MAX);
        let grounded_event_count =
            u64::try_from(workload.grounded_event_count.max(1)).unwrap_or(u64::MAX);
        let rumor_count = u64::try_from(workload.rumor_count.max(1)).unwrap_or(u64::MAX);
        let rumor_grounding_count =
            u64::try_from(workload.rumor_grounding_count.max(1)).unwrap_or(u64::MAX);
        let rumor_known_count = u64::try_from(
            workload
                .rumor_known_faction_count
                .saturating_add(workload.rumor_known_agent_count)
                .max(1),
        )
        .unwrap_or(u64::MAX);
        let opportunity_count =
            u64::try_from(workload.opportunity_count.max(1)).unwrap_or(u64::MAX);
        let opportunity_link_count = u64::try_from(
            workload
                .opportunity_faction_count
                .saturating_add(workload.opportunity_agent_count)
                .max(1),
        )
        .unwrap_or(u64::MAX);
        let command_count = u64::try_from(workload.command_count.max(1)).unwrap_or(u64::MAX);

        let event_ledger_window = story_gpu_resource(
            graph,
            "story event ledger window buffer",
            GpuResourceKind::Buffer,
            ai_resource_bytes(recent_event_count, 192).saturating_add(ai_resource_bytes(
                grounded_event_count
                    .saturating_add(rumor_grounding_count)
                    .saturating_add(opportunity_count),
                48,
            )),
            GpuResourceLifetime::Imported,
            false,
        );
        let active_thread_table = story_gpu_resource(
            graph,
            "story active thread table",
            GpuResourceKind::Buffer,
            ai_resource_bytes(active_thread_count, 256),
            GpuResourceLifetime::Persistent,
            true,
        );
        let faction_state_table = story_gpu_resource(
            graph,
            "story faction state table",
            GpuResourceKind::Buffer,
            ai_resource_bytes(active_faction_count, 192)
                .saturating_add(ai_resource_bytes(faction_tension_count, 64))
                .saturating_add(ai_resource_bytes(reputation_entry_count, 32)),
            GpuResourceLifetime::Persistent,
            true,
        );
        let pressure_signal_buffer = story_gpu_resource(
            graph,
            "story pressure signal buffer",
            GpuResourceKind::Buffer,
            ai_resource_bytes(active_thread_count, 96),
            GpuResourceLifetime::Transient,
            false,
        );

        let pressure_pipeline = story_compute_pipeline_with_permutation(
            graph,
            "story_thread_pressure_update",
            "story/thread_pressure_update.comp",
            QualityTier::NormalRuntime,
            ai_shader_permutation(["GROUNDED_THREADS", "PACING_PROFILE", "FACTION_PRESSURE"]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "story_thread_pressure_update",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(pressure_pipeline),
                QualityTier::NormalRuntime,
            )
            .reads([
                event_ledger_window,
                active_thread_table,
                faction_state_table,
            ])
            .writes([pressure_signal_buffer]),
        );

        let faction_influence_buffer = if workload.active_faction_count > 0 {
            let faction_influence_buffer = story_gpu_resource(
                graph,
                "story faction influence buffer",
                GpuResourceKind::Buffer,
                ai_resource_bytes(active_faction_count, 128),
                GpuResourceLifetime::Transient,
                false,
            );
            let faction_pipeline = story_compute_pipeline_with_permutation(
                graph,
                "story_faction_reputation_update",
                "story/faction_reputation_update.comp",
                QualityTier::NormalRuntime,
                ai_shader_permutation(["REPUTATION", "CITY_ALERTNESS", "TERRITORY_PRESSURE"]),
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "story_faction_reputation_update",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(faction_pipeline),
                    QualityTier::NormalRuntime,
                )
                .reads([faction_state_table, pressure_signal_buffer])
                .writes([faction_influence_buffer]),
            );
            Some(faction_influence_buffer)
        } else {
            None
        };

        let rumor_propagation_buffer = if workload.rumor_count > 0 {
            let grounded_rumor_table = story_gpu_resource(
                graph,
                "story grounded rumor table",
                GpuResourceKind::Buffer,
                ai_resource_bytes(rumor_count, 224)
                    .saturating_add(ai_resource_bytes(rumor_grounding_count, 48))
                    .saturating_add(ai_resource_bytes(rumor_known_count, 24)),
                GpuResourceLifetime::Persistent,
                true,
            );
            let rumor_propagation_buffer = story_gpu_resource(
                graph,
                "story rumor propagation buffer",
                GpuResourceKind::Buffer,
                ai_resource_bytes(rumor_count, 128),
                GpuResourceLifetime::Transient,
                false,
            );
            let mut rumor_defines = vec![
                "RUMOR_NETWORK",
                "KNOWLEDGE_GROUNDING",
                "NO_GLOBAL_KNOWLEDGE",
            ];
            if workload.incomplete_rumor_count > 0 {
                rumor_defines.push("INCOMPLETE_RUMORS");
            }
            let rumor_pipeline = story_compute_pipeline_with_permutation(
                graph,
                "story_rumor_propagation",
                "story/rumor_propagation.comp",
                QualityTier::BackgroundApproximation,
                ai_shader_permutation(rumor_defines),
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "story_rumor_propagation",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(rumor_pipeline),
                    QualityTier::BackgroundApproximation,
                )
                .reads([
                    event_ledger_window,
                    active_thread_table,
                    faction_state_table,
                    grounded_rumor_table,
                ])
                .writes([rumor_propagation_buffer]),
            );
            Some(rumor_propagation_buffer)
        } else {
            None
        };

        let opportunity_index = story_gpu_resource(
            graph,
            "story grounded opportunity index",
            GpuResourceKind::Buffer,
            ai_resource_bytes(
                active_thread_count
                    .saturating_add(command_count)
                    .saturating_add(rumor_count),
                160,
            )
            .saturating_add(ai_resource_bytes(opportunity_count, 96)),
            GpuResourceLifetime::Transient,
            false,
        );
        let opportunity_table = if workload.opportunity_count > 0 {
            Some(story_gpu_resource(
                graph,
                "story director opportunity table",
                GpuResourceKind::Buffer,
                ai_resource_bytes(opportunity_count, 224)
                    .saturating_add(ai_resource_bytes(opportunity_link_count, 32)),
                GpuResourceLifetime::Persistent,
                true,
            ))
        } else {
            None
        };
        let opportunity_pipeline = story_compute_pipeline_with_permutation(
            graph,
            "story_grounded_opportunity_index",
            "story/grounded_opportunity_index.comp",
            QualityTier::BackgroundApproximation,
            ai_shader_permutation([
                "GROUNDED_OPPORTUNITIES",
                "RUMOR_SEEDS",
                "INVESTIGATION_THREADS",
                "DIRECTOR_OPPORTUNITIES",
                "WORLD_VALIDATED_OFFERS",
            ]),
        );
        let mut opportunity_reads = vec![
            event_ledger_window,
            active_thread_table,
            pressure_signal_buffer,
        ];
        if let Some(opportunity_table) = opportunity_table {
            opportunity_reads.push(opportunity_table);
        }
        if let Some(faction_influence_buffer) = faction_influence_buffer {
            opportunity_reads.push(faction_influence_buffer);
        }
        if let Some(rumor_propagation_buffer) = rumor_propagation_buffer {
            opportunity_reads.push(rumor_propagation_buffer);
        }
        graph.add_pass(
            GpuPassDesc::new(
                "story_grounded_opportunity_index",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(opportunity_pipeline),
                QualityTier::BackgroundApproximation,
            )
            .reads(opportunity_reads)
            .writes([opportunity_index]),
        );

        if workload.command_count > 0 {
            let command_queue = story_gpu_resource(
                graph,
                "story director command queue",
                GpuResourceKind::Buffer,
                ai_resource_bytes(command_count, 192),
                GpuResourceLifetime::Imported,
                false,
            );
            let command_validation = story_gpu_resource(
                graph,
                "story grounded command validation buffer",
                GpuResourceKind::Buffer,
                ai_resource_bytes(command_count, 96),
                GpuResourceLifetime::Transient,
                false,
            );
            let command_pipeline = story_compute_pipeline_with_permutation(
                graph,
                "story_director_command_validation",
                "story/director_command_validation.comp",
                QualityTier::NormalRuntime,
                ai_shader_permutation([
                    "WORLD_FACT_GROUNDING",
                    "NO_FORCED_SCENES",
                    "DIRECTOR_COMMANDS",
                ]),
            );
            let mut command_reads = vec![
                event_ledger_window,
                active_thread_table,
                opportunity_index,
                command_queue,
            ];
            if let Some(rumor_propagation_buffer) = rumor_propagation_buffer {
                command_reads.push(rumor_propagation_buffer);
            }
            graph.add_pass(
                GpuPassDesc::new(
                    "story_director_command_validation",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(command_pipeline),
                    QualityTier::NormalRuntime,
                )
                .reads(command_reads)
                .writes([command_validation]),
            );
        }
    }

    fn performance_counters(&self) -> PerformanceCounters {
        self.last_workload
            .as_ref()
            .map(|workload| PerformanceCounters {
                cpu_milliseconds: 0.12
                    + workload.recent_event_count as f32 * 0.004
                    + workload.active_thread_count as f32 * 0.01
                    + workload.rumor_count as f32 * 0.006
                    + workload.opportunity_count as f32 * 0.006,
                gpu_milliseconds: estimate_story_gpu_milliseconds(workload),
                memory_bytes: 10 * 1024 * 1024
                    + workload.active_thread_count as u64 * 128 * 1024
                    + workload.active_faction_count as u64 * 96 * 1024
                    + workload.rumor_count as u64 * 64 * 1024
                    + workload.rumor_known_agent_count as u64 * 8 * 1024
                    + workload.opportunity_count as u64 * 48 * 1024
                    + workload.opportunity_agent_count as u64 * 4 * 1024,
            })
            .unwrap_or(PerformanceCounters {
                cpu_milliseconds: 0.15,
                gpu_milliseconds: 0.0,
                memory_bytes: 10 * 1024 * 1024,
            })
    }

    fn save_state(&self) -> Option<ModuleStateRecord> {
        let descriptor = self.descriptor();
        let mut entries = vec![
            format!("tension:{}", self.state.tension),
            format!(
                "alertness:{}|{}|{}|{}",
                self.state.city_alertness.security,
                self.state.city_alertness.gangs,
                self.state.city_alertness.residents,
                self.state.city_alertness.media,
            ),
            format!(
                "pacing:{}|{}|{}",
                self.state.desired_pacing.target_tension,
                self.state.desired_pacing.cooldown_seconds,
                self.state.desired_pacing.allow_new_threads,
            ),
        ];
        entries.extend(
            self.identity_exposures_handled
                .iter()
                .map(|event_id| format!("identity:{event_id}")),
        );
        entries.extend(
            self.sound_reports_handled
                .iter()
                .map(|event_id| format!("sound:{event_id}")),
        );
        entries.extend(
            self.opportunities_handled
                .iter()
                .map(|event_id| format!("opportunity_handled:{event_id}")),
        );
        entries.extend(
            self.state
                .active_factions
                .iter()
                .map(|faction| format!("active_faction:{faction}")),
        );
        entries.extend(
            self.state
                .unresolved_threads
                .iter()
                .map(|thread| format!("unresolved_thread:{}", encode_state_text(thread))),
        );
        entries.extend(self.state.active_threads.iter().map(encode_story_thread));
        entries.extend(
            self.state
                .faction_tensions
                .tensions
                .iter()
                .map(|((left, right), tension)| {
                    format!("faction_tension:{left}|{right}|{tension}")
                }),
        );
        entries.extend(
            self.state
                .unresolved_secrets
                .iter()
                .map(|secret| format!("secret:{secret}")),
        );
        entries.extend(self.state.active_rumors.iter().map(encode_rumor_record));
        entries.extend(
            self.state
                .grounded_opportunities
                .iter()
                .map(encode_story_opportunity),
        );
        entries.extend(
            self.state
                .player_reputation
                .trust_by_faction
                .iter()
                .map(|(faction, trust)| format!("rep_trust:{faction}|{trust}")),
        );
        entries.extend(
            self.state
                .player_reputation
                .fear_by_faction
                .iter()
                .map(|(faction, fear)| format!("rep_fear:{faction}|{fear}")),
        );
        entries.extend(self.factions.states.values().map(encode_faction_state));
        Some(ModuleStateRecord::new(
            descriptor.module_id,
            descriptor.schema,
            STORY_DIRECTOR_MODULE_STATE_VERSION,
            entries,
        ))
    }

    fn load_state(&mut self, state: &ModuleStateRecord) {
        if state.state_version != STORY_DIRECTOR_MODULE_STATE_VERSION {
            return;
        }
        if let Some(tension) = state
            .entries_with_prefix("tension:")
            .next()
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|value| value.is_finite())
        {
            self.state.tension = tension.clamp(0.0, 1.0);
        }
        self.identity_exposures_handled = state
            .entries_with_prefix("identity:")
            .filter_map(|event_id| event_id.parse::<WorldEventId>().ok())
            .collect();
        self.sound_reports_handled = state
            .entries_with_prefix("sound:")
            .filter_map(|event_id| event_id.parse::<WorldEventId>().ok())
            .collect();
        self.opportunities_handled = state
            .entries_with_prefix("opportunity_handled:")
            .filter_map(|event_id| event_id.parse::<WorldEventId>().ok())
            .collect();
        self.state.active_factions = state
            .entries_with_prefix("active_faction:")
            .filter_map(|faction| faction.parse::<FactionId>().ok())
            .collect();
        self.state.active_factions.sort_unstable();
        self.state.active_factions.dedup();
        self.state.unresolved_threads = state
            .entries_with_prefix("unresolved_thread:")
            .filter_map(decode_state_text)
            .collect();
        self.state.active_threads = state
            .entries_with_prefix("story_thread:")
            .filter_map(parse_story_thread)
            .collect();
        self.state.faction_tensions.tensions = state
            .entries_with_prefix("faction_tension:")
            .filter_map(|entry| {
                let fields = entry.split('|').collect::<Vec<_>>();
                if fields.len() != 3 {
                    return None;
                }
                Some((
                    sorted_pair(fields[0].parse().ok()?, fields[1].parse().ok()?),
                    parse_finite_f32(fields[2])?.clamp(0.0, 1.0),
                ))
            })
            .collect();
        self.state.unresolved_secrets = state
            .entries_with_prefix("secret:")
            .filter_map(|secret| secret.parse::<SecretRef>().ok())
            .collect();
        self.state.active_rumors = state
            .entries_with_prefix("rumor:")
            .filter_map(parse_rumor_record)
            .collect();
        self.state.active_rumors.sort_by_key(|rumor| rumor.rumor_id);
        self.state.grounded_opportunities = state
            .entries_with_prefix("opportunity:")
            .filter_map(parse_story_opportunity)
            .collect();
        self.state
            .grounded_opportunities
            .sort_by_key(|opportunity| opportunity.opportunity_id);
        self.state.player_reputation.trust_by_faction = state
            .entries_with_prefix("rep_trust:")
            .filter_map(|entry| {
                let (faction, trust) = entry.split_once('|')?;
                Some((
                    faction.parse::<FactionId>().ok()?,
                    parse_finite_f32(trust)?.clamp(-1.0, 1.0),
                ))
            })
            .collect();
        self.state.player_reputation.fear_by_faction = state
            .entries_with_prefix("rep_fear:")
            .filter_map(|entry| {
                let (faction, fear) = entry.split_once('|')?;
                Some((
                    faction.parse::<FactionId>().ok()?,
                    parse_finite_f32(fear)?.clamp(0.0, 1.0),
                ))
            })
            .collect();
        if let Some(alertness) = state.entries_with_prefix("alertness:").next() {
            let fields = alertness.split('|').collect::<Vec<_>>();
            if fields.len() == 4
                && let (Some(security), Some(gangs), Some(residents), Some(media)) = (
                    parse_finite_f32(fields[0]),
                    parse_finite_f32(fields[1]),
                    parse_finite_f32(fields[2]),
                    parse_finite_f32(fields[3]),
                )
            {
                self.state.city_alertness = AlertnessState {
                    security: security.clamp(0.0, 1.0),
                    gangs: gangs.clamp(0.0, 1.0),
                    residents: residents.clamp(0.0, 1.0),
                    media: media.clamp(0.0, 1.0),
                };
            }
        }
        if let Some(pacing) = state.entries_with_prefix("pacing:").next() {
            let fields = pacing.split('|').collect::<Vec<_>>();
            if fields.len() == 3
                && let (Some(target_tension), Some(cooldown_seconds), Ok(allow_new_threads)) = (
                    parse_finite_f32(fields[0]),
                    parse_finite_f32(fields[1]),
                    fields[2].parse::<bool>(),
                )
            {
                self.state.desired_pacing = PacingProfile {
                    target_tension: target_tension.clamp(0.0, 1.0),
                    cooldown_seconds: cooldown_seconds.max(0.0),
                    allow_new_threads,
                };
            }
        }
        self.factions.states = state
            .entries_with_prefix("faction_state:")
            .filter_map(parse_faction_state)
            .map(|state| (state.faction, state))
            .collect();
        if !self.state.active_factions.is_empty() && self.factions.states.is_empty() {
            self.factions = FactionGraph::security_context(&self.config);
        }
        self.commands.clear();
        self.last_workload = None;
    }
}

pub struct AiCharactersModule {
    config: AiCharactersConfig,
    witnessed: BTreeSet<(EntityId, WorldEventId)>,
    danger_reactions: BTreeSet<(EntityId, WorldEventId)>,
    navigation_reactions: BTreeSet<(EntityId, WorldEventId)>,
    sound_reactions: BTreeSet<(EntityId, WorldEventId)>,
    memories_recorded: BTreeSet<(EntityId, WorldEventId)>,
    story_events_suggested: BTreeSet<WorldEventId>,
    memory_store: MemoryStore,
    relationship_graph: RelationshipGraph,
    debug_decisions: Vec<AiDebugDecision>,
    last_output: Option<AiFrameOutput>,
    last_workload: Option<AiGpuWorkload>,
    deep_planning_ticks: BTreeMap<EntityId, u64>,
    sound_reaction_ticks: BTreeMap<EntityId, u64>,
}

impl Default for AiCharactersModule {
    fn default() -> Self {
        Self::new(AiCharactersConfig::default())
    }
}

impl AiCharactersModule {
    pub fn new(config: AiCharactersConfig) -> Self {
        Self {
            config,
            witnessed: BTreeSet::new(),
            danger_reactions: BTreeSet::new(),
            navigation_reactions: BTreeSet::new(),
            sound_reactions: BTreeSet::new(),
            memories_recorded: BTreeSet::new(),
            story_events_suggested: BTreeSet::new(),
            memory_store: MemoryStore::default(),
            relationship_graph: RelationshipGraph::default(),
            debug_decisions: Vec::new(),
            last_output: None,
            last_workload: None,
            deep_planning_ticks: BTreeMap::new(),
            sound_reaction_ticks: BTreeMap::new(),
        }
    }

    pub fn config(&self) -> AiCharactersConfig {
        self.config
    }

    pub fn memory_store(&self) -> &MemoryStore {
        &self.memory_store
    }

    pub fn relationship_graph(&self) -> &RelationshipGraph {
        &self.relationship_graph
    }

    pub fn debug_decisions(&self) -> &[AiDebugDecision] {
        &self.debug_decisions
    }

    pub fn last_output(&self) -> Option<&AiFrameOutput> {
        self.last_output.as_ref()
    }

    pub fn inspect_agent_debug_state(
        &self,
        events: &[WorldEvent],
        agent: EntityId,
    ) -> AgentDebugInspection {
        inspect_agent_debug_state(self, events, agent)
    }

    pub fn inspect_agent_debug_state_from_ledger(
        &self,
        ledger: &EventLedger,
        agent: EntityId,
    ) -> AgentDebugInspection {
        inspect_agent_debug_state_from_ledger(self, ledger, agent)
    }

    fn effective_quality_for_observation(
        &mut self,
        agent: EntityId,
        requested_quality: QualityTier,
        kind: ObservationKind,
        tick: u64,
    ) -> QualityTier {
        match kind {
            ObservationKind::Danger => requested_quality,
            ObservationKind::Crime => throttle_quality_by_interval(
                &mut self.deep_planning_ticks,
                agent,
                requested_quality,
                tick,
                AI_DEEP_PLANNING_INTERVAL_TICKS,
            ),
            ObservationKind::Sound => throttle_quality_by_interval(
                &mut self.sound_reaction_ticks,
                agent,
                requested_quality,
                tick,
                AI_SOUND_REACTION_INTERVAL_TICKS,
            ),
            ObservationKind::Dialogue
            | ObservationKind::Navigation
            | ObservationKind::Story
            | ObservationKind::Unknown => requested_quality,
        }
    }

    fn handle_crime_observation(
        &mut self,
        frame: &FrameContext,
        out: &mut CommandSink,
        output: &mut AiFrameOutput,
        event: &WorldEvent,
        damaged_entity: EntityId,
    ) {
        for (agent, state) in frame.snapshot.agents.iter() {
            let key = (*agent, event.event_id);
            if self.witnessed.contains(&key) {
                continue;
            }

            let Some(observation) = observe_event_for_agent(frame, *agent, event, 20.0, 30.0)
            else {
                continue;
            };

            self.witnessed.insert(key);
            let persona = persona_for_agent(state.persona);
            let effective_quality = self.effective_quality_for_observation(
                *agent,
                state.ai_lod.min(frame.quality_tier),
                observation.kind,
                frame.sim_time.tick,
            );
            let decision = plan_agent_response_with_config(
                *agent,
                damaged_entity,
                &persona,
                &observation,
                effective_quality,
                &self.config,
            );
            let should_escalate_story = supports_story_escalation(decision.debug.selected_lod);
            let should_emit_witness = supports_world_report(decision.debug.selected_lod);
            self.record_decision_output(frame, out, output, &observation, decision);

            if should_emit_witness {
                out.event(WorldEvent {
                    event_id: deterministic_event_id(frame.sim_time.tick, 50, *agent),
                    tick: frame.sim_time.tick,
                    location_meters: observation.location,
                    actors: vec![*agent, damaged_entity],
                    kind: WorldEventKind::NpcWitnessedCrime {
                        witness: *agent,
                        event: event.event_id,
                    },
                    physical_evidence: vec!["line_of_sight".to_string(), "loud_sound".to_string()],
                    narrative_tags: vec!["witness".to_string(), "consequence".to_string()],
                });
            }

            if should_escalate_story && self.story_events_suggested.insert(event.event_id) {
                let story = StoryEvent {
                    label: "player_identity_compromised".to_string(),
                    actors: event.actors.iter().copied().chain([*agent]).collect(),
                    location: event.location_meters,
                    tags: vec![
                        "story_consequence".to_string(),
                        "witness".to_string(),
                        "identity".to_string(),
                    ],
                };
                output.story_events.push(story.clone());
                out.command(WorldCommand::EmitStoryEvent(story));
                out.event(WorldEvent {
                    event_id: deterministic_event_id(frame.sim_time.tick, 52, *agent),
                    tick: frame.sim_time.tick,
                    location_meters: event.location_meters,
                    actors: event.actors.iter().copied().chain([*agent]).collect(),
                    kind: WorldEventKind::PlayerIdentityExposed,
                    physical_evidence: vec!["npc_witness".to_string()],
                    narrative_tags: vec!["story_consequence".to_string(), "identity".to_string()],
                });
            }
        }
    }

    fn handle_danger_observation(
        &mut self,
        frame: &FrameContext,
        out: &mut CommandSink,
        output: &mut AiFrameOutput,
        event: &WorldEvent,
    ) {
        for (agent, state) in frame.snapshot.agents.iter() {
            let key = (*agent, event.event_id);
            if self.danger_reactions.contains(&key) {
                continue;
            }

            let Some(observation) = observe_event_for_agent(frame, *agent, event, 18.0, 28.0)
            else {
                continue;
            };

            if observation.kind != ObservationKind::Danger {
                continue;
            }

            self.danger_reactions.insert(key);
            let persona = persona_for_agent(state.persona);
            let effective_quality = self.effective_quality_for_observation(
                *agent,
                state.ai_lod.min(frame.quality_tier),
                observation.kind,
                frame.sim_time.tick,
            );
            let decision =
                plan_agent_danger_response(*agent, &persona, &observation, effective_quality);
            self.record_decision_output(frame, out, output, &observation, decision);
        }
    }

    fn handle_sound_observation(
        &mut self,
        frame: &FrameContext,
        out: &mut CommandSink,
        output: &mut AiFrameOutput,
        event: &WorldEvent,
    ) {
        let WorldEventKind::SoundEmitted {
            source_entity,
            intensity,
            radius_meters,
            occlusion_hint,
            ..
        } = &event.kind
        else {
            return;
        };

        for (agent, state) in frame.snapshot.agents.iter() {
            if source_entity == &Some(*agent) {
                continue;
            }

            let key = (*agent, event.event_id);
            if self.sound_reactions.contains(&key) {
                continue;
            }

            let Some(observation) = observe_sound_for_agent(
                frame,
                *agent,
                event,
                *intensity,
                *radius_meters,
                *occlusion_hint,
            ) else {
                continue;
            };

            self.sound_reactions.insert(key);
            let persona = persona_for_agent(state.persona);
            let effective_quality = self.effective_quality_for_observation(
                *agent,
                state.ai_lod.min(frame.quality_tier),
                observation.kind,
                frame.sim_time.tick,
            );
            let decision =
                plan_agent_sound_response(*agent, &persona, &observation, effective_quality);
            let should_emit_sound_report = supports_world_report(decision.debug.selected_lod);
            self.record_decision_output(frame, out, output, &observation, decision);

            if should_emit_sound_report {
                out.event(WorldEvent {
                    event_id: deterministic_event_id(
                        frame.sim_time.tick,
                        53,
                        *agent ^ (event.event_id as u64).rotate_left(9),
                    ),
                    tick: frame.sim_time.tick,
                    location_meters: observation.location,
                    actors: source_entity.iter().copied().chain([*agent]).collect(),
                    kind: WorldEventKind::NpcHeardSound {
                        listener: *agent,
                        event: event.event_id,
                        source_entity: *source_entity,
                        confidence: observation.confidence,
                    },
                    physical_evidence: vec!["heard_sound".to_string(), "spatial_audio".to_string()],
                    narrative_tags: vec![
                        "audio".to_string(),
                        "hearing".to_string(),
                        "investigation".to_string(),
                    ],
                });
            }
        }
    }

    fn handle_navigation_observation(
        &mut self,
        frame: &FrameContext,
        out: &mut CommandSink,
        output: &mut AiFrameOutput,
        event: &WorldEvent,
        entity: EntityId,
    ) {
        let Some(state) = frame.snapshot.agents.find(entity) else {
            return;
        };

        let key = (entity, event.event_id);
        if self.navigation_reactions.contains(&key) {
            return;
        }

        let Some(observation) = observe_navigation_blocked_for_agent(frame, entity, event) else {
            return;
        };

        self.navigation_reactions.insert(key);
        let persona = persona_for_agent(state.persona);
        let effective_quality = self.effective_quality_for_observation(
            entity,
            state.ai_lod.min(frame.quality_tier),
            observation.kind,
            frame.sim_time.tick,
        );
        let decision = plan_agent_navigation_response_with_config(
            entity,
            &persona,
            &observation,
            effective_quality,
            &self.config,
        );
        self.record_decision_output(frame, out, output, &observation, decision);
    }

    fn record_decision_output(
        &mut self,
        frame: &FrameContext,
        out: &mut CommandSink,
        output: &mut AiFrameOutput,
        observation: &AgentObservation,
        decision: AgentDecision,
    ) {
        let validation = validate_agent_intents(
            &decision.intents,
            &frame.snapshot,
            &frame.recent_events,
            &decision.debug.available_actions,
        );
        let mut debug = decision.debug.clone();
        debug
            .rejected_actions
            .extend(validation.rejected.iter().map(intent_rejection_text));

        self.debug_decisions.push(debug.clone());
        output.debug_decisions.push(debug.clone());
        output.agent_intents.extend(validation.accepted.clone());
        emit_decision_explanation_event(frame, out, observation, &debug, &validation.accepted);
        emit_accepted_intent_events(frame, out, observation, &validation.accepted);
        if !validation.accepted.is_empty() {
            queue_agent_state_update(frame, out, observation, &decision);
            queue_accepted_intent_commands(frame, out, observation, &validation.accepted);
        }
        output.intent_validation.merge(validation);
        if let Some(dialogue) = &decision.dialogue {
            output.dialogue_requests.push(dialogue.clone());
        }
        if let Some(delta) = &decision.relationship_delta {
            self.relationship_graph.apply_delta(delta.clone());
            output.relationship_updates.push(delta.clone());
        }
        if let Some(command) = &decision.director_command {
            output.director_commands.push(command.clone());
        }

        if self
            .memories_recorded
            .insert((decision.agent, observation.event_id))
        {
            let memory_event_local = decision.agent ^ (observation.event_id as u64).rotate_left(17);
            self.memory_store.remember(
                decision.memory_update.clone(),
                observation.knowledge_source.clone(),
                frame.sim_time.tick,
                Some(observation.location),
            );
            output.memory_updates.push(decision.memory_update.clone());
            out.event(WorldEvent {
                event_id: deterministic_event_id(frame.sim_time.tick, 51, memory_event_local),
                tick: frame.sim_time.tick,
                location_meters: observation.location,
                actors: vec![decision.agent],
                kind: WorldEventKind::AgentMemoryUpdated {
                    agent: decision.memory_update.agent,
                    source_event: decision.memory_update.source_event,
                    memory_kind: memory_kind_label(&decision.memory_update.memory_kind).to_string(),
                    content: decision.memory_update.content.clone(),
                    importance: decision.memory_update.importance,
                    confidence: decision.memory_update.confidence,
                },
                physical_evidence: memory_evidence_tags(observation),
                narrative_tags: memory_narrative_tags(observation),
            });
        }

        if let Some(dialogue) = decision.dialogue {
            out.command(WorldCommand::EmitDialogue(DialogueEvent {
                speaker: dialogue.speaker,
                target: dialogue.target,
                text: dialogue
                    .text
                    .unwrap_or_else(|| fallback_dialogue_text(observation)),
                emotion: dialogue.emotion,
                voice_persona: Some(dialogue.voice_persona),
                location: observation.location,
            }));
        }
    }
}

impl EngineModule for AiCharactersModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            module_id: 50,
            name: "ai_characters_story",
            schema: SchemaVersion {
                name: "AiFrameOutput",
                version: 1,
            },
            default_quality: QualityTier::NormalRuntime,
        }
    }

    fn schema_requirements(&self) -> Vec<ashfall_core::schema::SchemaRequirement> {
        vec![ashfall_core::schema::SchemaRequirement::required(
            50,
            "PhysicsOutput",
            1,
            "AI characters consume physical evidence events from consequence physics",
        )]
    }

    fn tick(&mut self, frame: &FrameContext, out: &mut CommandSink) {
        let mut output = AiFrameOutput::default();

        for event in &frame.recent_events {
            match &event.kind {
                WorldEventKind::GlassWallFractured { entity } => {
                    self.handle_crime_observation(frame, out, &mut output, event, *entity);
                }
                WorldEventKind::ToxicGasReleased => {
                    self.handle_danger_observation(frame, out, &mut output, event);
                }
                WorldEventKind::StreetFlooded
                    if world_event_contains_any(
                        event,
                        &[
                            "biohazard",
                            "biological",
                            "biological_contamination",
                            "blood",
                            "fuel",
                            "grease",
                            "hydraulic",
                            "oil",
                            "slick_surface",
                        ],
                    ) =>
                {
                    self.handle_danger_observation(frame, out, &mut output, event);
                }
                WorldEventKind::SoundEmitted { .. } => {
                    self.handle_sound_observation(frame, out, &mut output, event);
                }
                WorldEventKind::NavigationMoveBlocked { entity } => {
                    self.handle_navigation_observation(frame, out, &mut output, event, *entity);
                }
                _ => {}
            }
        }

        if !output.is_empty() {
            let workload = ai_gpu_workload_from_frame_output(frame, &output);
            output.ai_timing = ai_performance_counters_for_workload(&workload);
            self.last_workload = Some(workload);
            self.last_output = Some(output);
        } else {
            self.last_output = None;
            self.last_workload = None;
        }
    }

    fn schedule_gpu(&mut self, graph: &mut GpuGraphBuilder) {
        let Some(workload) = self.last_workload.as_ref() else {
            return;
        };
        if workload.observation_count == 0 {
            return;
        }

        let agent_count = u64::try_from(workload.agent_count.max(1)).unwrap_or(u64::MAX);
        let recent_event_count =
            u64::try_from(workload.recent_event_count.max(1)).unwrap_or(u64::MAX);
        let observation_count =
            u64::try_from(workload.observation_count.max(1)).unwrap_or(u64::MAX);
        let memory_count = u64::try_from(workload.memory_update_count.max(1)).unwrap_or(u64::MAX);
        let relationship_count =
            u64::try_from(workload.relationship_update_count.max(1)).unwrap_or(u64::MAX);
        let intent_count = u64::try_from(workload.intent_count.max(1)).unwrap_or(u64::MAX);
        let dialogue_count =
            u64::try_from(workload.dialogue_request_count.max(1)).unwrap_or(u64::MAX);

        let event_ledger_window = ai_gpu_resource(
            graph,
            "ai event ledger window buffer",
            GpuResourceKind::Buffer,
            ai_resource_bytes(recent_event_count, 192),
            GpuResourceLifetime::Imported,
            false,
        );
        let agent_state_table = ai_gpu_resource(
            graph,
            "ai agent state table",
            GpuResourceKind::Buffer,
            ai_resource_bytes(agent_count, 256),
            GpuResourceLifetime::Imported,
            true,
        );
        let observation_candidates = ai_gpu_resource(
            graph,
            "ai observation candidate buffer",
            GpuResourceKind::Buffer,
            ai_resource_bytes(agent_count.saturating_mul(recent_event_count), 96),
            GpuResourceLifetime::Transient,
            false,
        );
        let visible_observations = ai_gpu_resource(
            graph,
            "ai visible observation buffer",
            GpuResourceKind::Buffer,
            ai_resource_bytes(observation_count, 160),
            GpuResourceLifetime::Transient,
            false,
        );

        let binning_pipeline = ai_compute_pipeline_with_permutation(
            graph,
            "ai_perception_query_binning",
            "ai/perception_query_binning.comp",
            QualityTier::BackgroundApproximation,
            ai_shader_permutation(["EVENT_LEDGER_WINDOW", "AI_LOD_BINS", "KNOWLEDGE_LIMITS"]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "ai_perception_query_binning",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(binning_pipeline),
                QualityTier::BackgroundApproximation,
            )
            .reads([event_ledger_window, agent_state_table])
            .writes([observation_candidates]),
        );

        let visibility_pipeline = ai_compute_pipeline_with_permutation(
            graph,
            "ai_observation_visibility_filter",
            "ai/observation_visibility_filter.comp",
            QualityTier::NormalRuntime,
            ai_shader_permutation([
                "SIGHT_HEARING_FILTER",
                "OCCLUSION_AWARE",
                "NO_GLOBAL_KNOWLEDGE",
            ]),
        );
        graph.add_pass(
            GpuPassDesc::new(
                "ai_observation_visibility_filter",
                GpuQueueKind::Compute,
                GpuDispatchKind::Compute(visibility_pipeline),
                QualityTier::NormalRuntime,
            )
            .reads([
                event_ledger_window,
                agent_state_table,
                observation_candidates,
            ])
            .writes([visible_observations]),
        );

        let mut current_memory_index = None;
        if workload.memory_update_count > 0 {
            let memory_record_buffer = ai_gpu_resource(
                graph,
                "ai memory record buffer",
                GpuResourceKind::Buffer,
                ai_resource_bytes(memory_count, 256),
                GpuResourceLifetime::Persistent,
                true,
            );
            let memory_relevance_index = ai_gpu_resource(
                graph,
                "ai memory relevance index",
                GpuResourceKind::Buffer,
                ai_resource_bytes(memory_count.saturating_add(observation_count), 128),
                GpuResourceLifetime::Transient,
                false,
            );
            let memory_pipeline = ai_compute_pipeline_with_permutation(
                graph,
                "ai_memory_relevance_index",
                "ai/memory_relevance_index.comp",
                QualityTier::NormalRuntime,
                ai_shader_permutation(["SHORT_TERM_MEMORY", "EVIDENCE_MEMORY", "GROUNDING_IDS"]),
            );
            graph.add_pass(
                GpuPassDesc::new(
                    "ai_memory_relevance_index",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(memory_pipeline),
                    QualityTier::NormalRuntime,
                )
                .reads([visible_observations, memory_record_buffer])
                .writes([memory_relevance_index]),
            );
            current_memory_index = Some(memory_relevance_index);
        }

        let mut intent_scores = None;
        if workload.intent_count > 0 {
            let available_actions = ai_gpu_resource(
                graph,
                "ai available action mask",
                GpuResourceKind::Buffer,
                ai_resource_bytes(intent_count, 64),
                GpuResourceLifetime::Imported,
                false,
            );
            let intent_score_buffer = ai_gpu_resource(
                graph,
                "ai intent score buffer",
                GpuResourceKind::Buffer,
                ai_resource_bytes(intent_count, 96),
                GpuResourceLifetime::Transient,
                false,
            );
            let intent_pipeline = ai_compute_pipeline_with_permutation(
                graph,
                "ai_intent_candidate_scoring",
                "ai/intent_candidate_scoring.comp",
                QualityTier::NormalRuntime,
                ai_shader_permutation(["INTENT_VALIDATION", "AVAILABLE_ACTIONS", "LOD_PLANNING"]),
            );
            let mut reads = vec![visible_observations, available_actions];
            if let Some(memory_index) = current_memory_index {
                reads.push(memory_index);
            }
            graph.add_pass(
                GpuPassDesc::new(
                    "ai_intent_candidate_scoring",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(intent_pipeline),
                    QualityTier::NormalRuntime,
                )
                .reads(reads)
                .writes([intent_score_buffer]),
            );
            intent_scores = Some(intent_score_buffer);
        }

        if workload.dialogue_request_count > 0 {
            let dialogue_context = ai_gpu_resource(
                graph,
                "ai dialogue context buffer",
                GpuResourceKind::Buffer,
                ai_resource_bytes(dialogue_count, 192),
                GpuResourceLifetime::Transient,
                false,
            );
            let dialogue_pipeline = ai_compute_pipeline_with_permutation(
                graph,
                "ai_dialogue_context_prepare",
                "ai/dialogue_context_prepare.comp",
                QualityTier::NormalRuntime,
                ai_shader_permutation(["SPEECH_ACTS", "VOICE_HANDOFF", "EMOTION_CURVES"]),
            );
            let mut reads = vec![visible_observations, agent_state_table];
            if let Some(memory_index) = current_memory_index {
                reads.push(memory_index);
            }
            graph.add_pass(
                GpuPassDesc::new(
                    "ai_dialogue_context_prepare",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(dialogue_pipeline),
                    QualityTier::NormalRuntime,
                )
                .reads(reads)
                .writes([dialogue_context]),
            );
        }

        if workload.debug_decision_count > 0 || workload.relationship_update_count > 0 {
            let debug_trace_buffer = ai_gpu_resource(
                graph,
                "ai debug decision trace buffer",
                GpuResourceKind::Buffer,
                ai_resource_bytes(observation_count.saturating_add(relationship_count), 224),
                GpuResourceLifetime::Persistent,
                false,
            );
            let debug_pipeline = ai_compute_pipeline_with_permutation(
                graph,
                "ai_debug_trace_compaction",
                "ai/debug_trace_compaction.comp",
                QualityTier::BackgroundApproximation,
                ai_shader_permutation(["WHY_TRACE", "DECISION_INSPECTOR", "RELATIONSHIP_DELTAS"]),
            );
            let mut reads = vec![visible_observations];
            if let Some(intent_scores) = intent_scores {
                reads.push(intent_scores);
            }
            if let Some(memory_index) = current_memory_index {
                reads.push(memory_index);
            }
            graph.add_pass(
                GpuPassDesc::new(
                    "ai_debug_trace_compaction",
                    GpuQueueKind::Compute,
                    GpuDispatchKind::Compute(debug_pipeline),
                    QualityTier::BackgroundApproximation,
                )
                .reads(reads)
                .writes([debug_trace_buffer]),
            );
        }
    }

    fn performance_counters(&self) -> PerformanceCounters {
        self.last_workload
            .as_ref()
            .map(ai_performance_counters_for_workload)
            .unwrap_or(PerformanceCounters {
                cpu_milliseconds: 0.45,
                gpu_milliseconds: 0.0,
                memory_bytes: 20 * 1024 * 1024,
            })
    }

    fn save_state(&self) -> Option<ModuleStateRecord> {
        let descriptor = self.descriptor();
        let mut entries = Vec::new();
        entries.extend(agent_event_entries("witnessed:", &self.witnessed));
        entries.extend(agent_event_entries("danger:", &self.danger_reactions));
        entries.extend(agent_event_entries(
            "navigation:",
            &self.navigation_reactions,
        ));
        entries.extend(agent_event_entries("sound:", &self.sound_reactions));
        entries.extend(agent_event_entries("memory:", &self.memories_recorded));
        entries.extend(agent_tick_entries(
            "deep_plan_tick:",
            &self.deep_planning_ticks,
        ));
        entries.extend(agent_tick_entries(
            "sound_reaction_tick:",
            &self.sound_reaction_ticks,
        ));
        entries.extend(
            self.story_events_suggested
                .iter()
                .map(|event_id| format!("story:{event_id}")),
        );
        entries.extend(self.memory_store.records.iter().map(encode_memory_record));
        entries.extend(
            self.relationship_graph
                .edges
                .values()
                .map(encode_relationship_edge),
        );
        Some(ModuleStateRecord::new(
            descriptor.module_id,
            descriptor.schema,
            AI_CHARACTERS_MODULE_STATE_VERSION,
            entries,
        ))
    }

    fn load_state(&mut self, state: &ModuleStateRecord) {
        if !(2..=AI_CHARACTERS_MODULE_STATE_VERSION).contains(&state.state_version) {
            return;
        }
        self.witnessed = state
            .entries_with_prefix("witnessed:")
            .filter_map(parse_agent_event_key)
            .collect();
        self.danger_reactions = state
            .entries_with_prefix("danger:")
            .filter_map(parse_agent_event_key)
            .collect();
        self.navigation_reactions = state
            .entries_with_prefix("navigation:")
            .filter_map(parse_agent_event_key)
            .collect();
        self.sound_reactions = state
            .entries_with_prefix("sound:")
            .filter_map(parse_agent_event_key)
            .collect();
        self.memories_recorded = state
            .entries_with_prefix("memory:")
            .filter_map(parse_agent_event_key)
            .collect();
        self.deep_planning_ticks = state
            .entries_with_prefix("deep_plan_tick:")
            .filter_map(parse_agent_tick_key)
            .collect();
        self.sound_reaction_ticks = state
            .entries_with_prefix("sound_reaction_tick:")
            .filter_map(parse_agent_tick_key)
            .collect();
        self.story_events_suggested = state
            .entries_with_prefix("story:")
            .filter_map(|event_id| event_id.parse::<WorldEventId>().ok())
            .collect();
        self.memory_store.records = state
            .entries_with_prefix("memory_record:")
            .filter_map(parse_memory_record)
            .collect();
        self.relationship_graph.edges = state
            .entries_with_prefix("relationship:")
            .filter_map(parse_relationship_edge)
            .map(|edge| ((edge.source, edge.target), edge))
            .collect();
        self.debug_decisions.clear();
        self.last_output = None;
        self.last_workload = None;
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct AiGpuWorkload {
    agent_count: usize,
    recent_event_count: usize,
    observation_count: usize,
    memory_update_count: usize,
    relationship_update_count: usize,
    intent_count: usize,
    dialogue_request_count: usize,
    debug_decision_count: usize,
    planning_decision_count: usize,
    hero_decision_count: usize,
    crime_observation_count: usize,
    danger_observation_count: usize,
    navigation_observation_count: usize,
    sound_observation_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct StoryGpuWorkload {
    recent_event_count: usize,
    active_thread_count: usize,
    active_faction_count: usize,
    unresolved_secret_count: usize,
    rumor_count: usize,
    rumor_grounding_count: usize,
    rumor_known_faction_count: usize,
    rumor_known_agent_count: usize,
    incomplete_rumor_count: usize,
    opportunity_count: usize,
    opportunity_faction_count: usize,
    opportunity_agent_count: usize,
    opportunity_world_validation_count: usize,
    command_count: usize,
    reputation_entry_count: usize,
    faction_tension_count: usize,
    grounded_event_count: usize,
    escalating_thread_count: usize,
    seeded_thread_count: usize,
}

fn ai_gpu_workload_from_frame_output(
    frame: &FrameContext,
    output: &AiFrameOutput,
) -> AiGpuWorkload {
    let mut workload = AiGpuWorkload {
        agent_count: frame.snapshot.agents.len(),
        recent_event_count: frame.recent_events.len(),
        observation_count: output
            .debug_decisions
            .len()
            .max(output.memory_updates.len())
            .max(output.dialogue_requests.len()),
        memory_update_count: output.memory_updates.len(),
        relationship_update_count: output.relationship_updates.len(),
        intent_count: output.agent_intents.len(),
        dialogue_request_count: output.dialogue_requests.len(),
        debug_decision_count: output.debug_decisions.len(),
        planning_decision_count: output
            .debug_decisions
            .iter()
            .filter(|decision| decision.selected_lod == AiLodTier::Planning)
            .count(),
        hero_decision_count: output
            .debug_decisions
            .iter()
            .filter(|decision| decision.selected_lod == AiLodTier::Hero)
            .count(),
        ..AiGpuWorkload::default()
    };

    for memory in &output.memory_updates {
        match memory.memory_kind {
            MemoryKind::Evidence => workload.crime_observation_count += 1,
            MemoryKind::Location => workload.navigation_observation_count += 1,
            MemoryKind::ShortTerm if memory.content.to_ascii_lowercase().contains("heard") => {
                workload.sound_observation_count += 1;
            }
            MemoryKind::ShortTerm => workload.danger_observation_count += 1,
            MemoryKind::LongTerm | MemoryKind::Social => {}
        }
    }

    workload
}

fn story_gpu_workload_from_state(
    frame: &FrameContext,
    state: &NarrativeDirectorState,
    command_count: usize,
) -> StoryGpuWorkload {
    StoryGpuWorkload {
        recent_event_count: frame.recent_events.len(),
        active_thread_count: state.active_threads.len(),
        active_faction_count: state.active_factions.len(),
        unresolved_secret_count: state.unresolved_secrets.len(),
        rumor_count: state.active_rumors.len(),
        rumor_grounding_count: state
            .active_rumors
            .iter()
            .map(|rumor| rumor.grounded_events.len())
            .sum(),
        rumor_known_faction_count: state
            .active_rumors
            .iter()
            .map(|rumor| rumor.known_to_factions.len())
            .sum(),
        rumor_known_agent_count: state
            .active_rumors
            .iter()
            .map(|rumor| rumor.known_to_agents.len())
            .sum(),
        incomplete_rumor_count: state
            .active_rumors
            .iter()
            .filter(|rumor| rumor.incomplete)
            .count(),
        opportunity_count: state.grounded_opportunities.len(),
        opportunity_faction_count: state
            .grounded_opportunities
            .iter()
            .map(|opportunity| opportunity.involved_factions.len())
            .sum(),
        opportunity_agent_count: state
            .grounded_opportunities
            .iter()
            .map(|opportunity| opportunity.involved_agents.len())
            .sum(),
        opportunity_world_validation_count: state
            .grounded_opportunities
            .iter()
            .filter(|opportunity| opportunity.requires_world_validation)
            .count(),
        command_count,
        reputation_entry_count: state.player_reputation.trust_by_faction.len()
            + state.player_reputation.fear_by_faction.len(),
        faction_tension_count: state.faction_tensions.tensions.len(),
        grounded_event_count: state
            .active_threads
            .iter()
            .map(|thread| thread.grounded_events.len())
            .sum(),
        escalating_thread_count: state
            .active_threads
            .iter()
            .filter(|thread| thread.status == StoryThreadStatus::Escalating)
            .count(),
        seeded_thread_count: state
            .active_threads
            .iter()
            .filter(|thread| thread.status == StoryThreadStatus::Seeded)
            .count(),
    }
}

fn ai_performance_counters_for_workload(workload: &AiGpuWorkload) -> PerformanceCounters {
    PerformanceCounters {
        cpu_milliseconds: 0.28
            + workload.observation_count as f32 * 0.035
            + workload.intent_count as f32 * 0.018
            + workload.dialogue_request_count as f32 * 0.025
            + workload.planning_decision_count as f32 * 0.04,
        gpu_milliseconds: estimate_ai_gpu_milliseconds(workload),
        memory_bytes: 20 * 1024 * 1024
            + workload.agent_count as u64 * 96 * 1024
            + workload.memory_update_count as u64 * 128 * 1024
            + workload.debug_decision_count as u64 * 64 * 1024,
    }
}

fn estimate_ai_gpu_milliseconds(workload: &AiGpuWorkload) -> f32 {
    if workload.observation_count == 0 {
        return 0.0;
    }
    (0.08
        + workload.agent_count as f32 * 0.004
        + workload.recent_event_count as f32 * 0.006
        + workload.observation_count as f32 * 0.028
        + workload.memory_update_count as f32 * 0.015
        + workload.relationship_update_count as f32 * 0.012
        + workload.intent_count as f32 * 0.018
        + workload.dialogue_request_count as f32 * 0.02
        + workload.debug_decision_count as f32 * 0.008
        + workload.planning_decision_count as f32 * 0.018
        + workload.hero_decision_count as f32 * 0.03
        + workload.crime_observation_count as f32 * 0.012
        + workload.danger_observation_count as f32 * 0.01
        + workload.navigation_observation_count as f32 * 0.004
        + workload.sound_observation_count as f32 * 0.006)
        .min(0.85)
}

fn estimate_story_gpu_milliseconds(workload: &StoryGpuWorkload) -> f32 {
    if workload.active_thread_count == 0
        && workload.active_faction_count == 0
        && workload.rumor_count == 0
        && workload.opportunity_count == 0
        && workload.command_count == 0
    {
        return 0.0;
    }
    (0.05
        + workload.recent_event_count as f32 * 0.004
        + workload.active_thread_count as f32 * 0.02
        + workload.active_faction_count as f32 * 0.012
        + workload.unresolved_secret_count as f32 * 0.008
        + workload.rumor_count as f32 * 0.014
        + workload.rumor_grounding_count as f32 * 0.004
        + workload.rumor_known_agent_count as f32 * 0.002
        + workload.incomplete_rumor_count as f32 * 0.006
        + workload.opportunity_count as f32 * 0.012
        + workload.opportunity_faction_count as f32 * 0.002
        + workload.opportunity_world_validation_count as f32 * 0.006
        + workload.command_count as f32 * 0.018
        + workload.reputation_entry_count as f32 * 0.006
        + workload.faction_tension_count as f32 * 0.008
        + workload.grounded_event_count as f32 * 0.005
        + workload.escalating_thread_count as f32 * 0.014
        + workload.seeded_thread_count as f32 * 0.008)
        .min(0.55)
}

fn ai_gpu_resource(
    graph: &mut GpuGraphBuilder,
    label: &'static str,
    kind: GpuResourceKind,
    byte_len: u64,
    lifetime: GpuResourceLifetime,
    bindless: bool,
) -> GpuResourceHandle {
    gpu_resource_for_owner(graph, 50, label, kind, byte_len, lifetime, bindless)
}

fn story_gpu_resource(
    graph: &mut GpuGraphBuilder,
    label: &'static str,
    kind: GpuResourceKind,
    byte_len: u64,
    lifetime: GpuResourceLifetime,
    bindless: bool,
) -> GpuResourceHandle {
    gpu_resource_for_owner(graph, 55, label, kind, byte_len, lifetime, bindless)
}

fn gpu_resource_for_owner(
    graph: &mut GpuGraphBuilder,
    owner: ModuleId,
    label: &'static str,
    kind: GpuResourceKind,
    byte_len: u64,
    lifetime: GpuResourceLifetime,
    bindless: bool,
) -> GpuResourceHandle {
    let desc = GpuResourceDesc::new(label, kind, byte_len)
        .owned_by(owner)
        .with_lifetime(lifetime);
    graph.declare_resource(if bindless { desc.bindless() } else { desc })
}

fn ai_compute_pipeline_with_permutation(
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

fn story_compute_pipeline_with_permutation(
    graph: &mut GpuGraphBuilder,
    label: &'static str,
    shader_key: &'static str,
    quality_tier: QualityTier,
    permutation: GpuShaderPermutation,
) -> ComputePipelineHandle {
    ai_compute_pipeline_with_permutation(graph, label, shader_key, quality_tier, permutation)
}

fn ai_shader_permutation(
    defines: impl IntoIterator<Item = impl Into<String>>,
) -> GpuShaderPermutation {
    GpuShaderPermutation::new(defines)
}

fn ai_resource_bytes(item_count: u64, bytes_per_item: u64) -> u64 {
    item_count
        .saturating_mul(bytes_per_item)
        .max(bytes_per_item)
}

fn throttle_quality_by_interval(
    ticks: &mut BTreeMap<EntityId, u64>,
    agent: EntityId,
    requested_quality: QualityTier,
    tick: u64,
    interval_ticks: u64,
) -> QualityTier {
    if requested_quality < QualityTier::NormalRuntime {
        return requested_quality;
    }

    if ticks
        .get(&agent)
        .is_some_and(|last_tick| tick.saturating_sub(*last_tick) < interval_ticks)
    {
        QualityTier::BackgroundApproximation
    } else {
        ticks.insert(agent, tick);
        requested_quality
    }
}

fn supports_world_report(lod: AiLodTier) -> bool {
    matches!(
        lod,
        AiLodTier::Reactive | AiLodTier::Planning | AiLodTier::Hero
    )
}

fn supports_story_escalation(lod: AiLodTier) -> bool {
    matches!(lod, AiLodTier::Planning | AiLodTier::Hero)
}

pub fn observe_event_for_agent(
    frame: &FrameContext,
    agent: EntityId,
    event: &WorldEvent,
    sight_radius_meters: f32,
    hearing_radius_meters: f32,
) -> Option<AgentObservation> {
    let transform = frame.snapshot.transforms.find(agent)?;
    let distance = transform.translation_meters.distance(event.location_meters);
    let kind = classify_event(event);
    let loud = event
        .narrative_tags
        .iter()
        .any(|tag| tag == "loud" || tag == "audio");
    let visible = distance <= sight_radius_meters;
    let audible = loud && distance <= hearing_radius_meters;
    if !visible && !audible {
        return None;
    }

    Some(AgentObservation {
        agent,
        event_id: event.event_id,
        kind,
        knowledge_source: if visible {
            KnowledgeSource::DirectObservation(event.event_id)
        } else {
            KnowledgeSource::Inference {
                based_on: vec![event.event_id],
            }
        },
        location: transform.translation_meters,
        distance_meters: distance,
        confidence: if visible { 0.86 } else { 0.58 },
        evidence: event.physical_evidence.clone(),
        tags: event.narrative_tags.clone(),
    })
}

pub fn observe_sound_for_agent(
    frame: &FrameContext,
    agent: EntityId,
    event: &WorldEvent,
    intensity: f32,
    radius_meters: f32,
    occlusion_hint: f32,
) -> Option<AgentObservation> {
    if !matches!(event.kind, WorldEventKind::SoundEmitted { .. }) {
        return None;
    }

    let transform = frame.snapshot.transforms.find(agent)?;
    let distance = transform.translation_meters.distance(event.location_meters);
    let intensity = intensity.clamp(0.0, 1.25);
    let occlusion = occlusion_hint.clamp(0.0, 0.95);
    let effective_hearing_radius =
        (radius_meters.max(0.0) * (0.45 + intensity * 0.65) * (1.0 - occlusion * 0.55)).max(0.0);
    if intensity < 0.12 || distance > effective_hearing_radius {
        return None;
    }

    let proximity = (1.0 - distance / effective_hearing_radius.max(0.1)).clamp(0.0, 1.0);
    let confidence =
        (0.32 + intensity * 0.36 + proximity * 0.28 - occlusion * 0.2).clamp(0.15, 0.92);
    let mut evidence = event.physical_evidence.clone();
    evidence.push("heard_sound".to_string());
    evidence.sort();
    evidence.dedup();
    let mut tags = event.narrative_tags.clone();
    tags.push("audio".to_string());
    tags.push("heard".to_string());
    tags.sort();
    tags.dedup();

    Some(AgentObservation {
        agent,
        event_id: event.event_id,
        kind: ObservationKind::Sound,
        knowledge_source: KnowledgeSource::Inference {
            based_on: vec![event.event_id],
        },
        location: transform.translation_meters,
        distance_meters: distance,
        confidence,
        evidence,
        tags,
    })
}

pub fn observe_navigation_blocked_for_agent(
    frame: &FrameContext,
    agent: EntityId,
    event: &WorldEvent,
) -> Option<AgentObservation> {
    let WorldEventKind::NavigationMoveBlocked { entity } = &event.kind else {
        return None;
    };
    if *entity != agent {
        return None;
    }

    let transform = frame.snapshot.transforms.find(agent)?;
    let mut evidence = event.physical_evidence.clone();
    evidence.push("navigation_blocked".to_string());
    evidence.sort();
    evidence.dedup();
    let mut tags = event.narrative_tags.clone();
    tags.push("navigation".to_string());
    tags.push("blocked".to_string());
    tags.sort();
    tags.dedup();

    Some(AgentObservation {
        agent,
        event_id: event.event_id,
        kind: ObservationKind::Navigation,
        knowledge_source: KnowledgeSource::DirectObservation(event.event_id),
        location: transform.translation_meters,
        distance_meters: transform.translation_meters.distance(event.location_meters),
        confidence: 0.94,
        evidence,
        tags,
    })
}

pub fn plan_agent_response(
    agent: EntityId,
    object: EntityId,
    persona: &AgentPersona,
    observation: &AgentObservation,
    ai_lod: QualityTier,
) -> AgentDecision {
    plan_agent_response_with_config(
        agent,
        object,
        persona,
        observation,
        ai_lod,
        &AiCharactersConfig::default(),
    )
}

pub fn plan_agent_response_with_config(
    agent: EntityId,
    object: EntityId,
    persona: &AgentPersona,
    observation: &AgentObservation,
    ai_lod: QualityTier,
    config: &AiCharactersConfig,
) -> AgentDecision {
    let selected_lod = ai_lod_from_quality(ai_lod, observation.kind);
    let player = 1;
    let authority = persona.faction.unwrap_or(config.default_authority_faction);
    let memory_update = MemoryUpdate {
        agent,
        memory_kind: MemoryKind::Evidence,
        source_event: Some(observation.event_id),
        content: "Saw the player break the alley glass wall".to_string(),
        importance: 0.92,
        confidence: observation.confidence,
        emotional_weight: 0.75,
        decay_policy: MemoryDecayPolicy::Never,
    };
    let dialogue = DialogueRequest {
        speaker: agent,
        target: Some(player),
        speech_act: SpeechAct::Report,
        intended_meaning: "warn player that witnessed vandalism will be reported".to_string(),
        text: Some("I saw that. Security will hear about this.".to_string()),
        emotion: EmotionState::alarmed(),
        urgency: 0.9,
        secrecy: 0.05,
        voice_persona: persona.voice_persona,
    };
    let intents = vec![
        AgentIntent::Investigate {
            agent,
            event: observation.event_id,
        },
        AgentIntent::ReportCrime {
            agent,
            event: observation.event_id,
            authority,
        },
        AgentIntent::SpeakTo {
            agent,
            target: player,
            act: SpeechAct::Report,
        },
    ];
    let plan = vec![
        PlanStep {
            label: "anchor memory to direct evidence".to_string(),
            intent: None,
            status: PlanStepStatus::Complete,
        },
        PlanStep {
            label: "investigate broken object".to_string(),
            intent: Some(AgentIntent::Investigate {
                agent,
                event: observation.event_id,
            }),
            status: PlanStepStatus::WaitingForValidation,
        },
        PlanStep {
            label: "report crime to authority".to_string(),
            intent: Some(AgentIntent::ReportCrime {
                agent,
                event: observation.event_id,
                authority,
            }),
            status: PlanStepStatus::WaitingForValidation,
        },
    ];
    let relationship_delta = RelationshipDelta {
        source: agent,
        target: player,
        trust_delta: -0.3,
        fear_delta: 0.08,
        suspicion_delta: 0.45,
        reason: format!("witnessed damage to entity {object}"),
    };
    let director_command = DirectorCommand::TriggerInvestigation(InvestigationSpec {
        source_event: observation.event_id,
        investigating_faction: authority,
        suspect: player,
        severity: 0.72,
    });
    let debug = AiDebugDecision {
        agent,
        event: Some(observation.event_id),
        goal: "preserve evidence and escalate to security".to_string(),
        selected_lod,
        available_actions: vec![
            "Investigate".to_string(),
            "ReportCrime".to_string(),
            "SpeakTo".to_string(),
        ],
        reasons: vec![
            "direct observation within sight radius".to_string(),
            "event carried crime and loud tags".to_string(),
            "persona values public safety".to_string(),
        ],
        rejected_actions: vec!["directly punish player: engine validation required".to_string()],
    };

    let mut decision = AgentDecision {
        agent,
        goal: debug.goal.clone(),
        plan,
        intents,
        dialogue: Some(dialogue),
        memory_update,
        relationship_delta: Some(relationship_delta),
        director_command: Some(director_command),
        debug,
    };
    apply_crime_lod_limits(&mut decision);
    decision
}

pub fn plan_agent_danger_response(
    agent: EntityId,
    persona: &AgentPersona,
    observation: &AgentObservation,
    ai_lod: QualityTier,
) -> AgentDecision {
    let selected_lod = ai_lod_from_quality(ai_lod, observation.kind);
    let memory_update = MemoryUpdate {
        agent,
        memory_kind: MemoryKind::ShortTerm,
        source_event: Some(observation.event_id),
        content: danger_memory_text(observation),
        importance: 0.82,
        confidence: observation.confidence,
        emotional_weight: 0.9,
        decay_policy: MemoryDecayPolicy::Gradual {
            half_life_seconds: 15.0,
        },
    };
    let dialogue = DialogueRequest {
        speaker: agent,
        target: None,
        speech_act: SpeechAct::Warn,
        intended_meaning: "warn nearby people to move away from a physical hazard".to_string(),
        text: Some(danger_dialogue_text(observation)),
        emotion: EmotionState::alarmed(),
        urgency: 0.95,
        secrecy: 0.0,
        voice_persona: persona.voice_persona,
    };
    let flee_intent = AgentIntent::Flee {
        agent,
        from: observation.event_id,
    };
    let plan = vec![
        PlanStep {
            label: "mark nearby hazard as immediate danger".to_string(),
            intent: None,
            status: PlanStepStatus::Complete,
        },
        PlanStep {
            label: "leave the dangerous area".to_string(),
            intent: Some(flee_intent.clone()),
            status: PlanStepStatus::WaitingForValidation,
        },
    ];
    let debug = AiDebugDecision {
        agent,
        event: Some(observation.event_id),
        goal: "preserve life by moving away from the hazard".to_string(),
        selected_lod,
        available_actions: vec!["Flee".to_string(), "SpeakTo".to_string()],
        reasons: vec![
            "event classified as local physical danger".to_string(),
            "hazard was within observation radius".to_string(),
            "fast danger reaction is allowed at low AI cost".to_string(),
        ],
        rejected_actions: vec![
            "invent antidote or clear hazard directly: physics/material systems own the hazard"
                .to_string(),
        ],
    };

    let mut decision = AgentDecision {
        agent,
        goal: debug.goal.clone(),
        plan,
        intents: vec![flee_intent],
        dialogue: Some(dialogue),
        memory_update,
        relationship_delta: None,
        director_command: None,
        debug,
    };
    apply_danger_lod_limits(&mut decision);
    decision
}

pub fn plan_agent_sound_response(
    agent: EntityId,
    _persona: &AgentPersona,
    observation: &AgentObservation,
    ai_lod: QualityTier,
) -> AgentDecision {
    let selected_lod = ai_lod_from_quality(ai_lod, observation.kind);
    let memory_update = MemoryUpdate {
        agent,
        memory_kind: MemoryKind::ShortTerm,
        source_event: Some(observation.event_id),
        content: sound_memory_text(observation),
        importance: sound_importance(observation),
        confidence: observation.confidence,
        emotional_weight: (0.25 + observation.confidence * 0.35).clamp(0.0, 0.75),
        decay_policy: MemoryDecayPolicy::Gradual {
            half_life_seconds: 25.0,
        },
    };
    let investigate_intent = AgentIntent::Investigate {
        agent,
        event: observation.event_id,
    };
    let plan = vec![
        PlanStep {
            label: "triangulate sound source from spatial audio".to_string(),
            intent: None,
            status: PlanStepStatus::Complete,
        },
        PlanStep {
            label: "investigate sound without assuming direct sight".to_string(),
            intent: Some(investigate_intent.clone()),
            status: PlanStepStatus::WaitingForValidation,
        },
    ];
    let debug = AiDebugDecision {
        agent,
        event: Some(observation.event_id),
        goal: "check the source of an unusual nearby sound".to_string(),
        selected_lod,
        available_actions: vec!["Investigate".to_string()],
        reasons: vec![
            "sound event was inside effective hearing radius".to_string(),
            "knowledge source is inference, not direct sight".to_string(),
            "investigation is a valid non-mutating intent".to_string(),
        ],
        rejected_actions: vec![
            "claim direct witness or report a crime from audio alone: insufficient evidence"
                .to_string(),
        ],
    };

    let mut decision = AgentDecision {
        agent,
        goal: debug.goal.clone(),
        plan,
        intents: vec![investigate_intent],
        dialogue: None,
        memory_update,
        relationship_delta: None,
        director_command: None,
        debug,
    };
    apply_sound_lod_limits(&mut decision);
    decision
}

pub fn plan_agent_navigation_response(
    agent: EntityId,
    persona: &AgentPersona,
    observation: &AgentObservation,
    ai_lod: QualityTier,
) -> AgentDecision {
    plan_agent_navigation_response_with_config(
        agent,
        persona,
        observation,
        ai_lod,
        &AiCharactersConfig::default(),
    )
}

pub fn plan_agent_navigation_response_with_config(
    agent: EntityId,
    _persona: &AgentPersona,
    observation: &AgentObservation,
    ai_lod: QualityTier,
    config: &AiCharactersConfig,
) -> AgentDecision {
    let selected_lod = ai_lod_from_quality(ai_lod, observation.kind);
    let recovery_intent = AgentIntent::MoveTo {
        agent,
        location: config.fallback_navigation_location,
    };
    let memory_update = MemoryUpdate {
        agent,
        memory_kind: MemoryKind::Location,
        source_event: Some(observation.event_id),
        content: navigation_memory_text(observation),
        importance: 0.48,
        confidence: observation.confidence,
        emotional_weight: 0.28,
        decay_policy: MemoryDecayPolicy::Gradual {
            half_life_seconds: 45.0,
        },
    };
    let plan = vec![
        PlanStep {
            label: "mark blocked route near current position".to_string(),
            intent: None,
            status: PlanStepStatus::Complete,
        },
        PlanStep {
            label: "choose an alternate route without repeating the blocked step".to_string(),
            intent: Some(recovery_intent.clone()),
            status: PlanStepStatus::WaitingForValidation,
        },
    ];
    let debug = AiDebugDecision {
        agent,
        event: Some(observation.event_id),
        goal: "recover from blocked movement".to_string(),
        selected_lod,
        available_actions: vec!["MoveTo".to_string()],
        reasons: vec![
            "navigation command reported no meaningful progress".to_string(),
            "blocked event belongs to this agent".to_string(),
            "MoveTo is used as a recovery intent and does not emit another movement step"
                .to_string(),
        ],
        rejected_actions: vec![
            "repeat blocked movement immediately: core navigation already reported obstruction"
                .to_string(),
        ],
    };

    AgentDecision {
        agent,
        goal: debug.goal.clone(),
        plan,
        intents: vec![recovery_intent],
        dialogue: None,
        memory_update,
        relationship_delta: None,
        director_command: None,
        debug,
    }
}

fn apply_crime_lod_limits(decision: &mut AgentDecision) {
    match decision.debug.selected_lod {
        AiLodTier::Dormant => {
            decision.goal = "retain direct evidence without waking full planner".to_string();
            decision.intents.clear();
            decision.dialogue = None;
            decision.relationship_delta = None;
            decision.director_command = None;
            decision.plan = vec![PlanStep {
                label: "store critical evidence for later review".to_string(),
                intent: None,
                status: PlanStepStatus::Complete,
            }];
            decision.debug.goal = decision.goal.clone();
            decision.debug.available_actions.clear();
            decision
                .debug
                .reasons
                .push("dormant tier records evidence without acting this frame".to_string());
            decision.debug.rejected_actions.push(
                "defer investigation, speech, and story escalation until AI budget returns"
                    .to_string(),
            );
        }
        AiLodTier::Schedule => {
            decision.goal = "record evidence and schedule lightweight investigation".to_string();
            decision
                .intents
                .retain(|intent| matches!(intent, AgentIntent::Investigate { agent: _, event: _ }));
            decision.dialogue = None;
            decision.relationship_delta = None;
            decision.director_command = None;
            decision.plan = vec![
                PlanStep {
                    label: "anchor memory to direct evidence".to_string(),
                    intent: None,
                    status: PlanStepStatus::Complete,
                },
                PlanStep {
                    label: "queue simple investigation without dialogue or faction escalation"
                        .to_string(),
                    intent: decision.intents.first().cloned(),
                    status: PlanStepStatus::WaitingForValidation,
                },
            ];
            decision.debug.goal = decision.goal.clone();
            decision.debug.available_actions = vec!["Investigate".to_string()];
            decision
                .debug
                .reasons
                .push("schedule tier avoids deep planning and voice generation".to_string());
            decision.debug.rejected_actions.push(
                "defer ReportCrime and SpeakTo until planning budget is available".to_string(),
            );
        }
        AiLodTier::Reactive => {
            decision.intents.retain(|intent| {
                matches!(
                    intent,
                    AgentIntent::Investigate { .. } | AgentIntent::SpeakTo { .. }
                )
            });
            decision.relationship_delta = None;
            decision.director_command = None;
            decision.debug.available_actions =
                vec!["Investigate".to_string(), "SpeakTo".to_string()];
            decision
                .debug
                .rejected_actions
                .push("defer faction report until planning tier is available".to_string());
        }
        AiLodTier::Planning | AiLodTier::Hero => {}
    }
}

fn apply_danger_lod_limits(decision: &mut AgentDecision) {
    match decision.debug.selected_lod {
        AiLodTier::Dormant => {
            decision.goal = "remember immediate danger without full simulation".to_string();
            decision.intents.clear();
            decision.dialogue = None;
            decision.plan = vec![PlanStep {
                label: "store danger memory for later update".to_string(),
                intent: None,
                status: PlanStepStatus::Complete,
            }];
            decision.debug.goal = decision.goal.clone();
            decision.debug.available_actions.clear();
            decision
                .debug
                .reasons
                .push("dormant tier keeps only critical safety memory".to_string());
        }
        AiLodTier::Schedule => {
            decision.dialogue = None;
            decision
                .debug
                .rejected_actions
                .push("skip generated warning speech at schedule tier".to_string());
        }
        AiLodTier::Reactive | AiLodTier::Planning | AiLodTier::Hero => {}
    }
}

fn apply_sound_lod_limits(decision: &mut AgentDecision) {
    match decision.debug.selected_lod {
        AiLodTier::Dormant | AiLodTier::Schedule => {
            decision.goal = "remember unusual sound without active investigation".to_string();
            decision.intents.clear();
            decision.dialogue = None;
            decision.plan = vec![PlanStep {
                label: "cache sound memory for low-frequency review".to_string(),
                intent: None,
                status: PlanStepStatus::Complete,
            }];
            decision.debug.goal = decision.goal.clone();
            decision.debug.available_actions.clear();
            decision
                .debug
                .reasons
                .push("low LOD hears the sound but does not claim a report or plan".to_string());
            decision
                .debug
                .rejected_actions
                .push("defer Investigate until reactive AI budget is available".to_string());
        }
        AiLodTier::Reactive | AiLodTier::Planning | AiLodTier::Hero => {}
    }
}

pub fn validate_agent_intent(
    intent: &AgentIntent,
    snapshot: &WorldSnapshot,
    recent_events: &[WorldEvent],
    available_actions: &[String],
) -> Result<(), AgentIntentValidationError> {
    let action = intent_action_name(intent);
    if !available_actions.is_empty()
        && !available_actions
            .iter()
            .any(|candidate| candidate == action)
    {
        return Err(AgentIntentValidationError::UnavailableAction(action));
    }

    match intent {
        AgentIntent::MoveTo { agent, .. } => require_agent(snapshot, *agent),
        AgentIntent::SpeakTo { agent, target, .. } => {
            require_agent(snapshot, *agent)?;
            require_entity(
                snapshot,
                *target,
                AgentIntentValidationError::MissingTarget(*target),
            )
        }
        AgentIntent::UseObject {
            agent,
            object,
            action,
        } => {
            require_agent(snapshot, *agent)?;
            if action.trim().is_empty() {
                return Err(AgentIntentValidationError::EmptyMethod("UseObject"));
            }
            require_entity(
                snapshot,
                *object,
                AgentIntentValidationError::MissingObject(*object),
            )
        }
        AgentIntent::Attack {
            agent,
            target,
            method,
        } => {
            require_agent(snapshot, *agent)?;
            if method.trim().is_empty() {
                return Err(AgentIntentValidationError::EmptyMethod("Attack"));
            }
            require_entity(
                snapshot,
                *target,
                AgentIntentValidationError::MissingTarget(*target),
            )
        }
        AgentIntent::Flee { agent, from } => {
            require_agent(snapshot, *agent)?;
            let event = require_event(recent_events, *from)?;
            if !can_flee_from(event) {
                return Err(AgentIntentValidationError::UnsupportedThreat(*from));
            }
            Ok(())
        }
        AgentIntent::Investigate { agent, event } => {
            require_agent(snapshot, *agent)?;
            require_event(recent_events, *event).map(|_| ())
        }
        AgentIntent::Trade {
            agent,
            target,
            offer,
        } => {
            require_agent(snapshot, *agent)?;
            if offer.trim().is_empty() {
                return Err(AgentIntentValidationError::EmptyMethod("Trade"));
            }
            require_entity(
                snapshot,
                *target,
                AgentIntentValidationError::MissingTarget(*target),
            )
        }
        AgentIntent::Hack {
            agent,
            target,
            method,
        } => {
            require_agent(snapshot, *agent)?;
            if method.trim().is_empty() {
                return Err(AgentIntentValidationError::EmptyMethod("Hack"));
            }
            require_entity(
                snapshot,
                *target,
                AgentIntentValidationError::MissingTarget(*target),
            )
        }
        AgentIntent::ReportCrime { agent, event, .. } => {
            require_agent(snapshot, *agent)?;
            require_event(recent_events, *event).map(|_| ())
        }
        AgentIntent::CallAlly {
            agent,
            ally,
            reason,
        } => {
            require_agent(snapshot, *agent)?;
            if reason.trim().is_empty() {
                return Err(AgentIntentValidationError::EmptyMethod("CallAlly"));
            }
            require_entity(
                snapshot,
                *ally,
                AgentIntentValidationError::MissingTarget(*ally),
            )
        }
    }
}

pub fn validate_agent_intents(
    intents: &[AgentIntent],
    snapshot: &WorldSnapshot,
    recent_events: &[WorldEvent],
    available_actions: &[String],
) -> AgentIntentValidationReport {
    let mut report = AgentIntentValidationReport::default();
    for intent in intents {
        match validate_agent_intent(intent, snapshot, recent_events, available_actions) {
            Ok(()) => report.accepted.push(intent.clone()),
            Err(error) => report.rejected.push(RejectedAgentIntent {
                intent: intent.clone(),
                error,
            }),
        }
    }
    report.passed = report.rejected.is_empty();
    report
}

fn emit_accepted_intent_events(
    frame: &FrameContext,
    out: &mut CommandSink,
    observation: &AgentObservation,
    accepted: &[AgentIntent],
) {
    for (index, intent) in accepted.iter().enumerate() {
        let agent = agent_for_intent(intent);
        out.event(WorldEvent {
            event_id: deterministic_event_id(
                frame.sim_time.tick,
                54,
                intent_event_local(agent, observation.event_id, index, intent),
            ),
            tick: frame.sim_time.tick,
            location_meters: observation.location,
            actors: intent_actor_refs(intent),
            kind: WorldEventKind::AgentIntentProposed {
                agent,
                action: intent_action_name(intent).to_string(),
                source_event: Some(observation.event_id),
                validation_passed: true,
            },
            physical_evidence: vec!["ai_intent_validation".to_string()],
            narrative_tags: vec![
                "ai".to_string(),
                "intent".to_string(),
                "validated".to_string(),
            ],
        });
    }
}

fn queue_accepted_intent_commands(
    frame: &FrameContext,
    out: &mut CommandSink,
    observation: &AgentObservation,
    accepted: &[AgentIntent],
) {
    let mut moved_agents = BTreeSet::new();

    for intent in accepted {
        match intent {
            AgentIntent::Flee { agent, from } => {
                if !moved_agents.insert(*agent) {
                    continue;
                }

                let threat_location = event_location(frame, *from).unwrap_or(observation.location);

                queue_agent_move_step(
                    out,
                    MoveEntityStepCommand {
                        entity: *agent,
                        mode: MoveEntityStepMode::AwayFrom,
                        reference_meters: threat_location,
                        max_step_meters: AI_FLEE_STEP_METERS,
                        stop_radius_meters: 0.0,
                    },
                );
            }
            AgentIntent::Investigate { agent, event } => {
                if !moved_agents.insert(*agent) {
                    continue;
                }

                let Some(target_location) = event_location(frame, *event) else {
                    continue;
                };

                queue_agent_move_step(
                    out,
                    MoveEntityStepCommand {
                        entity: *agent,
                        mode: MoveEntityStepMode::Toward,
                        reference_meters: target_location,
                        max_step_meters: AI_INVESTIGATE_STEP_METERS,
                        stop_radius_meters: AI_INVESTIGATE_STOP_RADIUS_METERS,
                    },
                );
            }
            AgentIntent::MoveTo { .. }
            | AgentIntent::SpeakTo { .. }
            | AgentIntent::UseObject { .. }
            | AgentIntent::Attack { .. }
            | AgentIntent::Trade { .. }
            | AgentIntent::Hack { .. }
            | AgentIntent::ReportCrime { .. }
            | AgentIntent::CallAlly { .. } => {}
        }
    }
}

fn queue_agent_move_step(out: &mut CommandSink, step: MoveEntityStepCommand) {
    if let Some(pending) = out.commands.iter_mut().rev().find_map(|command| {
        let WorldCommand::MoveEntityStep(existing) = command else {
            return None;
        };
        (existing.entity == step.entity).then_some(existing)
    }) {
        if move_step_priority(step.mode) >= move_step_priority(pending.mode) {
            *pending = step;
        }
    } else {
        out.command(WorldCommand::MoveEntityStep(step));
    }
}

fn move_step_priority(mode: MoveEntityStepMode) -> u8 {
    match mode {
        MoveEntityStepMode::Toward => 1,
        MoveEntityStepMode::AwayFrom => 2,
    }
}

fn event_location(frame: &FrameContext, event_id: WorldEventId) -> Option<Vec3> {
    frame
        .recent_events
        .iter()
        .find(|event| event.event_id == event_id)
        .map(|event| event.location_meters)
}

fn queue_agent_state_update(
    frame: &FrameContext,
    out: &mut CommandSink,
    observation: &AgentObservation,
    decision: &AgentDecision,
) {
    let Some(current) = frame.snapshot.agents.find(decision.agent) else {
        return;
    };

    let emotional_state = decision
        .dialogue
        .as_ref()
        .map(|dialogue| dialogue.emotion.clone())
        .unwrap_or_else(|| emotion_for_observation(observation.kind));

    if let Some(pending_state) = out.commands.iter_mut().rev().find_map(|command| {
        let WorldCommand::SetAgentState(agent, state) = command else {
            return None;
        };
        (*agent == decision.agent).then_some(state)
    }) {
        pending_state.emotional_state =
            merge_emotion_states(&pending_state.emotional_state, &emotional_state);
        return;
    }

    let emotional_state = merge_emotion_states(&current.emotional_state, &emotional_state);
    if current.emotional_state == emotional_state {
        return;
    }

    out.command(WorldCommand::SetAgentState(
        decision.agent,
        ashfall_core::world::AgentState {
            persona: current.persona,
            emotional_state,
            ai_lod: current.ai_lod,
        },
    ));
}

fn merge_emotion_states(current: &EmotionState, incoming: &EmotionState) -> EmotionState {
    EmotionState {
        fear: current.fear.max(incoming.fear),
        anger: current.anger.max(incoming.anger),
        urgency: current.urgency.max(incoming.urgency),
        trust: current.trust.min(incoming.trust),
    }
}

fn emotion_for_observation(kind: ObservationKind) -> EmotionState {
    match kind {
        ObservationKind::Crime | ObservationKind::Danger => EmotionState::alarmed(),
        ObservationKind::Navigation => EmotionState {
            fear: 0.05,
            anger: 0.12,
            urgency: 0.35,
            trust: 0.45,
        },
        ObservationKind::Sound => EmotionState {
            fear: 0.35,
            anger: 0.05,
            urgency: 0.55,
            trust: 0.25,
        },
        ObservationKind::Dialogue | ObservationKind::Story | ObservationKind::Unknown => {
            EmotionState::default()
        }
    }
}

fn emit_decision_explanation_event(
    frame: &FrameContext,
    out: &mut CommandSink,
    observation: &AgentObservation,
    debug: &AiDebugDecision,
    accepted: &[AgentIntent],
) {
    out.event(WorldEvent {
        event_id: deterministic_event_id(
            frame.sim_time.tick,
            58,
            debug.agent ^ (observation.event_id as u64).rotate_left(21),
        ),
        tick: frame.sim_time.tick,
        location_meters: observation.location,
        actors: vec![debug.agent],
        kind: WorldEventKind::AgentDecisionExplained {
            agent: debug.agent,
            source_event: debug.event,
            goal: debug.goal.clone(),
            selected_lod: format!("{:?}", debug.selected_lod),
            available_actions: debug.available_actions.clone(),
            accepted_actions: accepted
                .iter()
                .map(|intent| intent_action_name(intent).to_string())
                .collect(),
            rejected_actions: debug.rejected_actions.clone(),
            reasons: debug.reasons.clone(),
        },
        physical_evidence: vec!["ai_decision_trace".to_string()],
        narrative_tags: vec![
            "ai".to_string(),
            "decision".to_string(),
            "debug".to_string(),
        ],
    });
}

fn intent_rejection_text(rejection: &RejectedAgentIntent) -> String {
    format!(
        "rejected {}: {}",
        intent_action_name(&rejection.intent),
        intent_validation_error_text(&rejection.error)
    )
}

fn intent_validation_error_text(error: &AgentIntentValidationError) -> String {
    match error {
        AgentIntentValidationError::MissingAgent(agent) => {
            format!("agent {agent} is missing from the snapshot")
        }
        AgentIntentValidationError::MissingTarget(target) => {
            format!("target {target} is missing from the snapshot")
        }
        AgentIntentValidationError::MissingObject(object) => {
            format!("object {object} is missing from the snapshot")
        }
        AgentIntentValidationError::MissingEvent(event) => {
            format!("event {event} is not visible in recent events")
        }
        AgentIntentValidationError::UnavailableAction(action) => {
            format!("action {action} is not available to this planner")
        }
        AgentIntentValidationError::EmptyMethod(action) => {
            format!("action {action} requires a non-empty method")
        }
        AgentIntentValidationError::UnsupportedThreat(event) => {
            format!("event {event} is not a supported flee threat")
        }
    }
}

pub fn default_mara_persona() -> AgentPersona {
    AgentPersona {
        id: 300,
        name: "Mara".to_string(),
        background: "alley resident who knows security patrol patterns".to_string(),
        values: vec![
            "public safety".to_string(),
            "truthful testimony".to_string(),
        ],
        fears: vec!["retaliation".to_string(), "being ignored".to_string()],
        ambitions: vec!["keep the block livable".to_string()],
        faction: Some(ALLEY_SECURITY_FACTION_ID),
        secrets: vec![3_001],
        skills: vec!["street observation".to_string(), "first aid".to_string()],
        speech_style: "direct, alarmed, no embellishment".to_string(),
        voice_persona: 300,
        safety_profile: "ground claims in witnessed events".to_string(),
    }
}

pub fn persona_for_agent(persona: AgentPersonaId) -> AgentPersona {
    if persona == 300 {
        default_mara_persona()
    } else {
        AgentPersona {
            id: persona,
            name: format!("Agent {persona}"),
            background: "generated city resident".to_string(),
            values: vec!["self preservation".to_string()],
            fears: vec!["local danger".to_string()],
            ambitions: vec!["finish daily routine".to_string()],
            faction: None,
            secrets: Vec::new(),
            skills: vec!["local navigation".to_string()],
            speech_style: "brief".to_string(),
            voice_persona: persona,
            safety_profile: "do not invent unseen facts".to_string(),
        }
    }
}

fn classify_event(event: &WorldEvent) -> ObservationKind {
    match event.kind {
        WorldEventKind::GlassWallFractured { .. }
        | WorldEventKind::DamageApplied { .. }
        | WorldEventKind::NpcWitnessedCrime { .. } => ObservationKind::Crime,
        WorldEventKind::MetalBent { .. } => ObservationKind::Danger,
        WorldEventKind::StreetFlooded | WorldEventKind::ToxicGasReleased => ObservationKind::Danger,
        WorldEventKind::DialogueEmitted { .. } => ObservationKind::Dialogue,
        WorldEventKind::SoundEmitted { .. } | WorldEventKind::NpcHeardSound { .. } => {
            ObservationKind::Sound
        }
        WorldEventKind::NavigationMoveBlocked { .. } => ObservationKind::Navigation,
        WorldEventKind::StoryEventEmitted { .. }
        | WorldEventKind::PlayerIdentityExposed
        | WorldEventKind::SurveillanceIncreased { .. } => ObservationKind::Story,
        _ => ObservationKind::Unknown,
    }
}

fn ai_lod_from_quality(quality: QualityTier, observation: ObservationKind) -> AiLodTier {
    match quality {
        QualityTier::Disabled => AiLodTier::Dormant,
        QualityTier::BackgroundApproximation => AiLodTier::Schedule,
        QualityTier::NormalRuntime if matches!(observation, ObservationKind::Crime) => {
            AiLodTier::Planning
        }
        QualityTier::NormalRuntime => AiLodTier::Reactive,
        QualityTier::HeroHighFidelityRuntime | QualityTier::ReferenceOfflineValidation => {
            AiLodTier::Hero
        }
    }
}

fn memory_kind_label(kind: &MemoryKind) -> &'static str {
    match kind {
        MemoryKind::ShortTerm => "short_term",
        MemoryKind::LongTerm => "long_term",
        MemoryKind::Social => "social",
        MemoryKind::Location => "location",
        MemoryKind::Evidence => "evidence",
    }
}

fn memory_evidence_tags(observation: &AgentObservation) -> EvidenceRefs {
    let mut evidence = observation.evidence.clone();
    if evidence.is_empty() {
        evidence.push(match observation.kind {
            ObservationKind::Danger => "observed_hazard".to_string(),
            ObservationKind::Crime => "line_of_sight".to_string(),
            ObservationKind::Navigation => "navigation_blocked".to_string(),
            ObservationKind::Sound => "heard_sound".to_string(),
            _ => "personal_observation".to_string(),
        });
    }
    evidence.push("personal_memory".to_string());
    evidence.sort();
    evidence.dedup();
    evidence
}

fn memory_narrative_tags(observation: &AgentObservation) -> TagSet {
    let mut tags = vec!["memory".to_string()];
    match observation.kind {
        ObservationKind::Danger => tags.push("danger".to_string()),
        ObservationKind::Crime => {
            tags.push("evidence".to_string());
            tags.push("witness".to_string());
        }
        ObservationKind::Dialogue => tags.push("dialogue".to_string()),
        ObservationKind::Navigation => tags.push("navigation".to_string()),
        ObservationKind::Sound => tags.push("audio".to_string()),
        ObservationKind::Story => tags.push("story".to_string()),
        ObservationKind::Unknown => tags.push("observation".to_string()),
    }
    tags
}

fn danger_memory_text(observation: &AgentObservation) -> String {
    if observation_contains_any(
        observation,
        &[
            "biohazard",
            "biological",
            "biological_contamination",
            "blood",
        ],
    ) {
        "Noticed biological contamination nearby and needed to get clear".to_string()
    } else if observation_contains_any(
        observation,
        &["fuel", "grease", "hydraulic", "oil", "slick_surface"],
    ) {
        "Noticed an oil-slick hazard nearby and needed to move carefully".to_string()
    } else if observation_contains_any(observation, &["gas", "toxic"]) {
        "Noticed toxic gas nearby and needed to get clear".to_string()
    } else if observation_contains_any(observation, &["water", "flood"]) {
        "Noticed flooding nearby and needed to move to safety".to_string()
    } else {
        "Noticed a nearby physical hazard and needed to get clear".to_string()
    }
}

fn danger_dialogue_text(observation: &AgentObservation) -> String {
    if observation_contains_any(
        observation,
        &[
            "biohazard",
            "biological",
            "biological_contamination",
            "blood",
        ],
    ) {
        "Biohazard on the ground. Stay back!".to_string()
    } else if observation_contains_any(
        observation,
        &["fuel", "grease", "hydraulic", "oil", "slick_surface"],
    ) {
        "Oil on the ground. Watch your footing!".to_string()
    } else if observation_contains_any(observation, &["gas", "toxic"]) {
        "Gas! Get clear!".to_string()
    } else if observation_contains_any(observation, &["water", "flood"]) {
        "Water is rising. Move back!".to_string()
    } else {
        "Move! It's not safe here!".to_string()
    }
}

fn observation_contains_any(observation: &AgentObservation, needles: &[&str]) -> bool {
    observation
        .tags
        .iter()
        .chain(observation.evidence.iter())
        .any(|tag| {
            let normalized = tag.to_ascii_lowercase();
            needles.iter().any(|needle| normalized.contains(needle))
        })
}

fn navigation_memory_text(observation: &AgentObservation) -> String {
    if observation
        .tags
        .iter()
        .chain(observation.evidence.iter())
        .any(|tag| tag.contains("glass"))
    {
        "Movement was blocked near glass and needs an alternate route".to_string()
    } else {
        "Movement path was blocked and needs an alternate route".to_string()
    }
}

fn sound_memory_text(observation: &AgentObservation) -> String {
    let evidence = observation
        .tags
        .iter()
        .chain(observation.evidence.iter())
        .map(|tag| tag.to_ascii_lowercase())
        .collect::<Vec<_>>();

    if evidence.iter().any(|tag| tag.contains("glass")) {
        "Heard glass break nearby and marked the source for investigation".to_string()
    } else if evidence
        .iter()
        .any(|tag| tag.contains("gunshot") || tag.contains("explosion"))
    {
        "Heard a violent report nearby and marked the source for investigation".to_string()
    } else if evidence
        .iter()
        .any(|tag| tag.contains("water") || tag.contains("splash") || tag.contains("flood"))
    {
        "Heard water movement nearby and marked the source for investigation".to_string()
    } else {
        "Heard an unusual sound nearby and marked the source for investigation".to_string()
    }
}

fn sound_importance(observation: &AgentObservation) -> f32 {
    let important_tag = observation
        .tags
        .iter()
        .chain(observation.evidence.iter())
        .any(|tag| {
            let tag = tag.to_ascii_lowercase();
            tag.contains("glass")
                || tag.contains("gunshot")
                || tag.contains("explosion")
                || tag.contains("danger")
        });
    let base = if important_tag { 0.66 } else { 0.42 };
    (base + observation.confidence * 0.22).clamp(0.0, 0.9)
}

fn fallback_dialogue_text(observation: &AgentObservation) -> String {
    match observation.kind {
        ObservationKind::Danger => danger_dialogue_text(observation),
        ObservationKind::Crime => "I saw that. Security will hear about this.".to_string(),
        ObservationKind::Navigation => "I can't get through there.".to_string(),
        ObservationKind::Sound => "I heard something nearby.".to_string(),
        _ => "Stay alert.".to_string(),
    }
}

fn intent_action_name(intent: &AgentIntent) -> &'static str {
    match intent {
        AgentIntent::MoveTo { .. } => "MoveTo",
        AgentIntent::SpeakTo { .. } => "SpeakTo",
        AgentIntent::UseObject { .. } => "UseObject",
        AgentIntent::Attack { .. } => "Attack",
        AgentIntent::Flee { .. } => "Flee",
        AgentIntent::Investigate { .. } => "Investigate",
        AgentIntent::Trade { .. } => "Trade",
        AgentIntent::Hack { .. } => "Hack",
        AgentIntent::ReportCrime { .. } => "ReportCrime",
        AgentIntent::CallAlly { .. } => "CallAlly",
    }
}

fn agent_for_intent(intent: &AgentIntent) -> EntityId {
    match intent {
        AgentIntent::MoveTo { agent, .. }
        | AgentIntent::SpeakTo { agent, .. }
        | AgentIntent::UseObject { agent, .. }
        | AgentIntent::Attack { agent, .. }
        | AgentIntent::Flee { agent, .. }
        | AgentIntent::Investigate { agent, .. }
        | AgentIntent::Trade { agent, .. }
        | AgentIntent::Hack { agent, .. }
        | AgentIntent::ReportCrime { agent, .. }
        | AgentIntent::CallAlly { agent, .. } => *agent,
    }
}

fn intent_actor_refs(intent: &AgentIntent) -> Vec<EntityId> {
    let agent = agent_for_intent(intent);
    let mut actors = vec![agent];
    match intent {
        AgentIntent::SpeakTo { target, .. }
        | AgentIntent::Attack { target, .. }
        | AgentIntent::Trade { target, .. }
        | AgentIntent::Hack { target, .. } => actors.push(*target),
        AgentIntent::UseObject { object, .. } => actors.push(*object),
        AgentIntent::CallAlly { ally, .. } => actors.push(*ally),
        AgentIntent::MoveTo { .. }
        | AgentIntent::Flee { .. }
        | AgentIntent::Investigate { .. }
        | AgentIntent::ReportCrime { .. } => {}
    }
    actors.sort_unstable();
    actors.dedup();
    actors
}

fn intent_event_local(
    agent: EntityId,
    source_event: WorldEventId,
    index: usize,
    intent: &AgentIntent,
) -> EntityId {
    agent
        ^ (source_event as u64).rotate_left(11)
        ^ ((index as u64) << 48)
        ^ intent_action_salt(intent)
}

fn intent_action_salt(intent: &AgentIntent) -> u64 {
    match intent {
        AgentIntent::MoveTo { .. } => 0x01,
        AgentIntent::SpeakTo { .. } => 0x02,
        AgentIntent::UseObject { .. } => 0x03,
        AgentIntent::Attack { .. } => 0x04,
        AgentIntent::Flee { .. } => 0x05,
        AgentIntent::Investigate { .. } => 0x06,
        AgentIntent::Trade { .. } => 0x07,
        AgentIntent::Hack { .. } => 0x08,
        AgentIntent::ReportCrime { .. } => 0x09,
        AgentIntent::CallAlly { .. } => 0x0A,
    }
}

fn require_agent(
    snapshot: &WorldSnapshot,
    agent: EntityId,
) -> Result<(), AgentIntentValidationError> {
    if snapshot.agents.find(agent).is_some() {
        Ok(())
    } else {
        Err(AgentIntentValidationError::MissingAgent(agent))
    }
}

fn require_entity(
    snapshot: &WorldSnapshot,
    entity: EntityId,
    error: AgentIntentValidationError,
) -> Result<(), AgentIntentValidationError> {
    if snapshot.transforms.find(entity).is_some() {
        Ok(())
    } else {
        Err(error)
    }
}

fn require_event(
    recent_events: &[WorldEvent],
    event_id: WorldEventId,
) -> Result<&WorldEvent, AgentIntentValidationError> {
    recent_events
        .iter()
        .find(|event| event.event_id == event_id)
        .ok_or(AgentIntentValidationError::MissingEvent(event_id))
}

fn can_flee_from(event: &WorldEvent) -> bool {
    matches!(
        classify_event(event),
        ObservationKind::Danger | ObservationKind::Crime | ObservationKind::Sound
    )
}

fn sorted_pair(a: FactionId, b: FactionId) -> (FactionId, FactionId) {
    if a <= b { (a, b) } else { (b, a) }
}

fn deterministic_event_id(tick: u64, module: u64, local: EntityId) -> WorldEventId {
    ((tick as u128) << 80) | ((module as u128) << 64) | local as u128
}

fn deterministic_record_id(tick: u64, agent: EntityId, index: usize) -> u128 {
    ((tick as u128) << 80) | ((agent as u128) << 32) | index as u128
}

#[cfg(test)]
mod tests {
    use super::*;
    use ashfall_core::gpu::GpuServices;
    use ashfall_core::world::{
        AgentState as WorldAgentState, AudioEventKind, CommandSink, EntityTemplate, PhysicalBody,
        Renderable, WorldState,
    };

    fn make_frame(event: WorldEvent) -> FrameContext {
        let mut world = WorldState::default();
        world
            .spawn_entity_template(EntityTemplate {
                entity_id: Some(1),
                name: "player".to_string(),
                transform: Transform::at(Vec3::new(0.0, 0.0, 0.0)),
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
                physical_body: Some(PhysicalBody {
                    mass_kg: 120.0,
                    material_id: 1,
                    dynamic: false,
                    fragile: true,
                }),
                material_state: Some(MaterialState::default()),
                human: None,
                agent: None,
                tags: vec!["glass".to_string()],
            })
            .expect("glass should spawn");
        world
            .spawn_entity_template(EntityTemplate {
                entity_id: Some(3),
                name: "Mara".to_string(),
                transform: Transform::at(Vec3::new(5.5, 0.0, 0.0)),
                renderable: None,
                physical_body: None,
                material_state: None,
                human: None,
                agent: Some(WorldAgentState {
                    persona: 300,
                    emotional_state: EmotionState::default(),
                    ai_lod: QualityTier::NormalRuntime,
                }),
                tags: vec!["npc".to_string(), "witness".to_string()],
            })
            .expect("agent should spawn");

        FrameContext {
            frame_id: 1,
            sim_time: SimTime::new(1.0 / 60.0, 1),
            dt_seconds: 1.0 / 60.0,
            quality_tier: QualityTier::NormalRuntime,
            snapshot: world.snapshot(1, SimTime::new(1.0 / 60.0, 1)),
            recent_events: vec![event],
            forces: Vec::new(),
        }
    }

    fn make_frame_with_quality(event: WorldEvent, quality_tier: QualityTier) -> FrameContext {
        let mut frame = make_frame(event);
        frame.quality_tier = quality_tier;
        frame
    }

    fn glass_fracture_event() -> WorldEvent {
        WorldEvent {
            event_id: 7,
            tick: 1,
            location_meters: Vec3::new(2.0, 0.0, 0.0),
            actors: vec![1, 2],
            kind: WorldEventKind::GlassWallFractured { entity: 2 },
            physical_evidence: vec!["glass_shards".to_string()],
            narrative_tags: vec!["crime".to_string(), "loud".to_string()],
        }
    }

    fn toxic_gas_event() -> WorldEvent {
        WorldEvent {
            event_id: 9,
            tick: 1,
            location_meters: Vec3::new(4.0, 0.0, 0.0),
            actors: vec![5],
            kind: WorldEventKind::ToxicGasReleased,
            physical_evidence: vec!["toxic_gas_cloud".to_string()],
            narrative_tags: vec![
                "danger".to_string(),
                "toxic_gas".to_string(),
                "loud".to_string(),
            ],
        }
    }

    fn contamination_spill_event(
        event_id: WorldEventId,
        physical_evidence: &[&str],
        narrative_tags: &[&str],
    ) -> WorldEvent {
        WorldEvent {
            event_id,
            tick: 1,
            location_meters: Vec3::new(4.0, 0.0, 0.0),
            actors: vec![6],
            kind: WorldEventKind::StreetFlooded,
            physical_evidence: physical_evidence
                .iter()
                .map(|evidence| (*evidence).to_string())
                .collect(),
            narrative_tags: narrative_tags
                .iter()
                .map(|tag| (*tag).to_string())
                .collect(),
        }
    }

    fn glass_shatter_sound_event(intensity: f32, radius_meters: f32) -> WorldEvent {
        WorldEvent {
            event_id: 11,
            tick: 1,
            location_meters: Vec3::new(18.0, 0.0, 0.0),
            actors: vec![2],
            kind: WorldEventKind::SoundEmitted {
                kind: AudioEventKind::GlassShatter,
                source_entity: Some(2),
                material_id: Some(1),
                intensity,
                radius_meters,
                occlusion_hint: 0.08,
            },
            physical_evidence: vec!["glass_shards".to_string(), "heard_sound".to_string()],
            narrative_tags: vec!["audio".to_string(), "glass".to_string()],
        }
    }

    fn navigation_blocked_event(entity: EntityId) -> WorldEvent {
        WorldEvent {
            event_id: 13,
            tick: 1,
            location_meters: Vec3::new(5.5, 0.0, 0.0),
            actors: vec![entity],
            kind: WorldEventKind::NavigationMoveBlocked { entity },
            physical_evidence: vec!["navigation_blocked".to_string()],
            narrative_tags: vec!["navigation".to_string(), "blocked".to_string()],
        }
    }

    #[test]
    fn memory_store_records_grounded_evidence() {
        let mut store = MemoryStore::default();
        let memory_id = store.remember(
            MemoryUpdate {
                agent: 3,
                memory_kind: MemoryKind::Evidence,
                source_event: Some(7),
                content: "Saw the player break the alley glass wall".to_string(),
                importance: 0.9,
                confidence: 0.85,
                emotional_weight: 0.7,
                decay_policy: MemoryDecayPolicy::Never,
            },
            KnowledgeSource::DirectObservation(7),
            1,
            Some(Vec3::new(5.5, 0.0, 0.0)),
        );

        assert_eq!(store.records()[0].memory_id, memory_id);
        assert_eq!(store.recall_evidence_about(3, "glass").len(), 1);
        assert!(matches!(
            store.records()[0].source,
            KnowledgeSource::DirectObservation(7)
        ));
    }

    #[test]
    fn candle_action_scorer_ranks_valid_ai_actions() {
        let actions = vec![
            "Investigate".to_string(),
            "ReportCrime".to_string(),
            "SpeakTo".to_string(),
        ];

        let ranked = score_agent_actions_with_candle(&actions, &[0.61, 0.94, 0.72])
            .expect("candle tensor scorer should rank CPU logits");

        assert_eq!(ranked[0].action, "ReportCrime");
        assert_eq!(ranked[0].rank, 1);
        assert!(ranked[0].score > ranked[1].score);
    }

    #[test]
    fn ai_module_plans_reports_and_debugs_witnessed_crime() {
        let frame = make_frame(glass_fracture_event());
        let mut module = AiCharactersModule::default();
        let mut sink = CommandSink::default();

        module.tick(&frame, &mut sink);

        assert!(sink.events.iter().any(|event| {
            matches!(
                event.kind,
                WorldEventKind::NpcWitnessedCrime {
                    witness: 3,
                    event: 7
                }
            )
        }));
        assert!(sink.events.iter().any(|event| {
            matches!(
                event.kind,
                WorldEventKind::AgentMemoryUpdated {
                    agent: 3,
                    importance,
                    confidence,
                    ..
                } if importance > 0.8 && confidence > 0.8
            )
        }));
        assert_eq!(
            sink.events
                .iter()
                .filter(|event| {
                    matches!(
                        event.kind,
                        WorldEventKind::AgentIntentProposed {
                            agent: 3,
                            validation_passed: true,
                            ..
                        }
                    )
                })
                .count(),
            3
        );
        assert!(sink.events.iter().any(|event| {
            matches!(
                &event.kind,
                WorldEventKind::AgentDecisionExplained {
                    agent,
                    source_event,
                    goal,
                    selected_lod,
                    accepted_actions,
                    rejected_actions,
                    reasons,
                    ..
                } if *agent == 3
                    && *source_event == Some(7)
                    && goal.contains("preserve evidence")
                    && selected_lod == "Planning"
                    && accepted_actions.iter().any(|action| action == "ReportCrime")
                    && rejected_actions
                        .iter()
                        .any(|reason| reason.contains("directly punish"))
                    && reasons.iter().any(|reason| reason.contains("direct observation"))
            )
        }));
        assert!(sink.events.iter().any(|event| {
            matches!(
                &event.kind,
                WorldEventKind::AgentIntentProposed {
                    action,
                    source_event: Some(7),
                    ..
                } if action == "ReportCrime"
            )
        }));
        assert!(
            sink.events
                .iter()
                .any(|event| { matches!(event.kind, WorldEventKind::PlayerIdentityExposed) })
        );
        assert!(sink.commands.iter().any(|command| {
            matches!(
                command,
                WorldCommand::EmitDialogue(DialogueEvent {
                    speaker: 3,
                    text,
                    ..
                }) if text.contains("Security")
            )
        }));
        assert!(sink.commands.iter().any(|command| {
            matches!(
                command,
                WorldCommand::EmitStoryEvent(StoryEvent { label, .. })
                    if label == "player_identity_compromised"
            )
        }));

        let output = module.last_output().expect("AI output should be stored");
        assert!(output.intent_validation.passed);
        assert_eq!(output.intent_validation.accepted.len(), 3);
        assert!(output.intent_validation.rejected.is_empty());
        assert!(output.agent_intents.iter().any(|intent| {
            matches!(
                intent,
                AgentIntent::ReportCrime {
                    agent: 3,
                    event: 7,
                    authority: ALLEY_SECURITY_FACTION_ID
                }
            )
        }));
        assert!(
            output
                .debug_decisions
                .iter()
                .any(|decision| decision.selected_lod == AiLodTier::Planning)
        );
        assert_eq!(
            module
                .memory_store()
                .recall_evidence_about(3, "glass")
                .len(),
            1
        );
        assert!(
            module
                .relationship_graph()
                .get(3, 1)
                .is_some_and(|edge| edge.suspicion > 0.4)
        );
    }

    #[test]
    fn background_ai_records_crime_without_rich_story_or_dialogue() {
        let frame =
            make_frame_with_quality(glass_fracture_event(), QualityTier::BackgroundApproximation);
        let mut module = AiCharactersModule::default();
        let mut sink = CommandSink::default();

        module.tick(&frame, &mut sink);

        let output = module
            .last_output()
            .expect("background AI should still record important evidence");
        assert!(output.intent_validation.passed);
        assert_eq!(output.intent_validation.accepted.len(), 1);
        assert!(
            output.agent_intents.iter().any(|intent| {
                matches!(intent, AgentIntent::Investigate { agent: 3, event: 7 })
            })
        );
        assert!(output.dialogue_requests.is_empty());
        assert!(output.relationship_updates.is_empty());
        assert!(output.director_commands.is_empty());
        assert!(output.story_events.is_empty());
        assert!(output.debug_decisions.iter().any(|decision| {
            decision.selected_lod == AiLodTier::Schedule
                && decision
                    .reasons
                    .iter()
                    .any(|reason| reason.contains("avoids deep planning"))
        }));
        assert!(sink.commands.iter().any(|command| {
            matches!(
                command,
                WorldCommand::SetAgentState(
                    3,
                    ashfall_core::world::AgentState {
                        emotional_state,
                        ..
                    }
                ) if *emotional_state == EmotionState::alarmed()
            )
        }));
        assert!(sink.commands.iter().all(|command| {
            !matches!(
                command,
                WorldCommand::EmitDialogue(_) | WorldCommand::EmitStoryEvent(_)
            )
        }));
        assert!(sink.events.iter().any(|event| {
            matches!(
                &event.kind,
                WorldEventKind::AgentMemoryUpdated {
                    agent: 3,
                    memory_kind,
                    ..
                } if memory_kind == "evidence"
            )
        }));
        assert!(
            !sink
                .events
                .iter()
                .any(|event| matches!(event.kind, WorldEventKind::PlayerIdentityExposed))
        );
        assert!(
            !sink
                .events
                .iter()
                .any(|event| matches!(event.kind, WorldEventKind::NpcWitnessedCrime { .. }))
        );
    }

    #[test]
    fn deep_planning_frequency_limit_demotes_repeated_crimes_to_schedule_lod() {
        let first_frame = make_frame(glass_fracture_event());
        let mut second_event = glass_fracture_event();
        second_event.event_id = 8;
        second_event.tick = 2;
        let mut second_frame = make_frame(second_event);
        second_frame.frame_id = 2;
        second_frame.sim_time = SimTime::new(2.0 / 60.0, 2);
        let mut module = AiCharactersModule::default();
        let mut first_sink = CommandSink::default();

        module.tick(&first_frame, &mut first_sink);
        assert!(
            first_sink
                .events
                .iter()
                .any(|event| matches!(event.kind, WorldEventKind::PlayerIdentityExposed))
        );

        let mut second_sink = CommandSink::default();
        module.tick(&second_frame, &mut second_sink);

        let output = module
            .last_output()
            .expect("second crime should still produce cheap evidence work");
        assert!(output.debug_decisions.iter().any(|decision| {
            decision.event == Some(8)
                && decision.selected_lod == AiLodTier::Schedule
                && decision
                    .rejected_actions
                    .iter()
                    .any(|reason| reason.contains("defer ReportCrime"))
        }));
        assert_eq!(output.agent_intents.len(), 1);
        assert!(
            second_sink
                .commands
                .iter()
                .all(|command| !matches!(command, WorldCommand::EmitStoryEvent(_)))
        );
        assert!(
            !second_sink
                .events
                .iter()
                .any(|event| matches!(event.kind, WorldEventKind::PlayerIdentityExposed))
        );
    }

    #[test]
    fn sound_frequency_limit_keeps_repeated_sounds_as_memory_only() {
        let first_frame = make_frame(glass_shatter_sound_event(0.9, 28.0));
        let mut second_event = glass_shatter_sound_event(0.9, 28.0);
        second_event.event_id = 12;
        second_event.tick = 2;
        let mut second_frame = make_frame(second_event);
        second_frame.frame_id = 2;
        second_frame.sim_time = SimTime::new(2.0 / 60.0, 2);
        let mut module = AiCharactersModule::default();
        let mut first_sink = CommandSink::default();

        module.tick(&first_frame, &mut first_sink);
        assert!(
            first_sink
                .events
                .iter()
                .any(|event| matches!(event.kind, WorldEventKind::NpcHeardSound { .. }))
        );

        let mut second_sink = CommandSink::default();
        module.tick(&second_frame, &mut second_sink);

        let output = module
            .last_output()
            .expect("repeated sound should still be remembered");
        assert!(output.agent_intents.is_empty());
        assert!(output.debug_decisions.iter().any(|decision| {
            decision.event == Some(12)
                && decision.selected_lod == AiLodTier::Schedule
                && decision
                    .reasons
                    .iter()
                    .any(|reason| reason.contains("low LOD hears the sound"))
        }));
        assert!(second_sink.events.iter().any(|event| {
            matches!(
                &event.kind,
                WorldEventKind::AgentMemoryUpdated {
                    agent: 3,
                    source_event: Some(12),
                    memory_kind,
                    ..
                } if memory_kind == "short_term"
            )
        }));
        assert!(
            !second_sink
                .events
                .iter()
                .any(|event| matches!(event.kind, WorldEventKind::NpcHeardSound { .. }))
        );
    }

    #[test]
    fn ai_module_schedules_gpu_perception_memory_intent_and_debug_work() {
        let frame = make_frame(glass_fracture_event());
        let mut module = AiCharactersModule::default();
        let mut sink = CommandSink::default();

        module.tick(&frame, &mut sink);

        let output = module.last_output().expect("AI output should be stored");
        assert!(output.ai_timing.gpu_milliseconds > 0.0);
        assert!(module.performance_counters().gpu_milliseconds > 0.0);

        let services = GpuServices::new_vulkan();
        let mut graph = services.begin_frame(frame.frame_id);
        module.schedule_gpu(&mut graph);
        let report = services.submit(graph);

        assert!(report.validation.passed);
        assert!(
            report
                .passes
                .iter()
                .any(|pass| pass.name == "ai_perception_query_binning")
        );
        assert!(
            report
                .passes
                .iter()
                .any(|pass| pass.name == "ai_observation_visibility_filter")
        );
        assert!(
            report
                .passes
                .iter()
                .any(|pass| pass.name == "ai_memory_relevance_index")
        );
        assert!(
            report
                .passes
                .iter()
                .any(|pass| pass.name == "ai_intent_candidate_scoring")
        );
        assert!(
            report
                .passes
                .iter()
                .any(|pass| pass.name == "ai_dialogue_context_prepare")
        );
        assert!(
            report
                .passes
                .iter()
                .any(|pass| pass.name == "ai_debug_trace_compaction")
        );
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(50)
                && usage.label == "ai agent state table"
                && usage.bindless_index.is_some()
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "ai_perception_query_binning")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(50)
                && usage.label == "ai memory relevance index"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "ai_memory_relevance_index")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "ai_observation_visibility_filter"
                && pipeline.shader_key == "ai/observation_visibility_filter.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "NO_GLOBAL_KNOWLEDGE")
        }));

        let quiet_frame = make_frame(glass_shatter_sound_event(0.04, 28.0));
        let mut quiet_sink = CommandSink::default();
        module.tick(&quiet_frame, &mut quiet_sink);
        assert!(module.last_output().is_none());

        let mut quiet_graph = services.begin_frame(quiet_frame.frame_id + 1);
        module.schedule_gpu(&mut quiet_graph);
        let quiet_report = services.submit(quiet_graph);
        assert!(quiet_report.passes.is_empty());
    }

    #[test]
    fn ai_module_state_restores_memory_relationships_and_processed_events() {
        let frame = make_frame(glass_fracture_event());
        let mut module = AiCharactersModule::default();
        let mut sink = CommandSink::default();
        module.tick(&frame, &mut sink);

        let state = module.save_state().expect("AI module should save state");
        assert!(
            state
                .entries
                .iter()
                .any(|entry| entry.starts_with("memory_record:"))
        );
        assert!(
            state
                .entries
                .iter()
                .any(|entry| entry.starts_with("relationship:"))
        );

        let mut restored = AiCharactersModule::default();
        restored.load_state(&state);

        assert_eq!(
            restored
                .memory_store()
                .recall_evidence_about(3, "glass")
                .len(),
            1
        );
        assert!(restored.relationship_graph().get(3, 1).is_some_and(|edge| {
            edge.suspicion > 0.4
                && edge
                    .history
                    .iter()
                    .any(|reason| reason.contains("witnessed damage"))
        }));

        let mut replay_sink = CommandSink::default();
        restored.tick(&frame, &mut replay_sink);
        assert!(replay_sink.events.is_empty());
        assert!(replay_sink.commands.is_empty());
    }

    #[test]
    fn decision_inspector_rebuilds_why_from_ledger_events() {
        let frame = make_frame(glass_fracture_event());
        let mut module = AiCharactersModule::default();
        let mut sink = CommandSink::default();

        module.tick(&frame, &mut sink);

        let inspection = inspect_agent_decisions(&sink.events, 3);

        assert_eq!(inspection.agent, 3);
        assert_eq!(inspection.decisions.len(), 1);
        assert_eq!(inspection.memories.len(), 1);
        assert_eq!(inspection.intents.len(), 3);
        assert!(inspection.has_accepted_action("ReportCrime"));
        assert!(
            inspection
                .latest_goal
                .as_deref()
                .is_some_and(|goal| goal.contains("preserve evidence"))
        );
        assert!(inspection.latest_decision().is_some_and(|decision| {
            decision.source_event == Some(7)
                && decision.selected_lod == "Planning"
                && decision
                    .rejected_actions
                    .iter()
                    .any(|reason| reason.contains("directly punish"))
        }));
        assert!(
            inspection
                .why_lines()
                .iter()
                .any(|line| line.contains("direct observation within sight radius"))
        );

        let mut ledger = EventLedger::default();
        for event in sink.events {
            ledger.record(event);
        }

        assert_eq!(inspect_agent_decisions_from_ledger(&ledger, 3), inspection);
    }

    #[test]
    fn agent_debug_inspector_reports_memory_goals_relationships_and_why() {
        let frame = make_frame(glass_fracture_event());
        let mut module = AiCharactersModule::default();
        let mut sink = CommandSink::default();

        module.tick(&frame, &mut sink);

        let inspection = module.inspect_agent_debug_state(&sink.events, 3);

        assert_eq!(inspection.agent, 3);
        assert_eq!(inspection.decisions.len(), 1);
        assert_eq!(inspection.intents.len(), 3);
        assert!(inspection.latest_decision().is_some_and(|decision| {
            decision.source_event == Some(7) && decision.selected_lod == "Planning"
        }));
        assert!(inspection.goals.iter().any(|goal| {
            goal.goal.contains("preserve evidence")
                && goal.source_events.contains(&7)
                && goal
                    .accepted_actions
                    .iter()
                    .any(|action| action == "ReportCrime")
                && goal
                    .reasons
                    .iter()
                    .any(|reason| reason.contains("public safety"))
        }));
        assert!(inspection.memories.iter().any(|memory| {
            memory.agent == 3
                && matches!(&memory.kind, MemoryKind::Evidence)
                && matches!(&memory.source, KnowledgeSource::DirectObservation(event) if *event == 7)
                && memory.content.contains("glass")
        }));
        assert!(inspection.memory_events.iter().any(|memory| {
            memory.source_event == Some(7)
                && memory.memory_kind == "evidence"
                && memory.content.contains("glass")
        }));
        assert!(inspection.relationships_with(1).into_iter().any(|edge| {
            edge.source == 3
                && edge.target == 1
                && edge.suspicion > 0.4
                && edge
                    .history
                    .iter()
                    .any(|reason| reason.contains("witnessed damage"))
        }));
        assert!(inspection.why_lines.iter().any(|line| {
            line.contains("accepted Investigate, ReportCrime, SpeakTo")
                && line.contains("direct observation within sight radius")
        }));
        assert!(inspection.why_lines.iter().any(|line| {
            line.contains("relationship toward 1") && line.contains("witnessed damage")
        }));
        assert!(inspection.why_summary().contains("preserve evidence"));

        let mut ledger = EventLedger::default();
        for event in sink.events.iter().cloned() {
            ledger.record(event);
        }

        assert_eq!(
            module.inspect_agent_debug_state_from_ledger(&ledger, 3),
            inspection
        );
        assert_eq!(
            inspect_agent_debug_state_from_ledger(&module, &ledger, 3),
            inspection
        );
    }

    #[test]
    fn ai_module_flees_from_observed_toxic_gas() {
        let frame = make_frame(toxic_gas_event());
        let mut module = AiCharactersModule::default();
        let mut sink = CommandSink::default();

        module.tick(&frame, &mut sink);

        let output = module
            .last_output()
            .expect("danger reaction should produce AI output");
        assert!(output.intent_validation.passed);
        assert_eq!(output.intent_validation.accepted.len(), 1);
        assert!(
            output
                .agent_intents
                .iter()
                .any(|intent| { matches!(intent, AgentIntent::Flee { agent: 3, from: 9 }) })
        );
        assert!(sink.events.iter().any(|event| {
            matches!(
                &event.kind,
                WorldEventKind::AgentIntentProposed {
                    agent: 3,
                    action,
                    source_event: Some(9),
                    validation_passed: true,
                } if action == "Flee"
            )
        }));
        assert!(
            output
                .debug_decisions
                .iter()
                .any(|decision| decision.selected_lod == AiLodTier::Reactive)
        );
        assert!(output.memory_updates.iter().any(|memory| {
            memory.agent == 3
                && memory.memory_kind == MemoryKind::ShortTerm
                && memory.content.contains("toxic gas")
        }));
        assert!(sink.commands.iter().any(|command| {
            matches!(
                command,
                WorldCommand::EmitDialogue(DialogueEvent { speaker: 3, text, .. })
                    if text.contains("Gas")
            )
        }));
        let threat_location = frame.recent_events[0].location_meters;
        let flee_step = sink
            .commands
            .iter()
            .find_map(|command| match command {
                WorldCommand::MoveEntityStep(step) if step.entity == 3 => Some(step),
                _ => None,
            })
            .expect("accepted flee intent should request an engine movement step");
        assert_eq!(flee_step.mode, MoveEntityStepMode::AwayFrom);
        assert_eq!(flee_step.reference_meters, threat_location);
        assert_eq!(flee_step.max_step_meters, AI_FLEE_STEP_METERS);
        assert_eq!(flee_step.stop_radius_meters, 0.0);
        assert!(
            !sink
                .events
                .iter()
                .any(|event| matches!(event.kind, WorldEventKind::PlayerIdentityExposed))
        );

        let validation = validate_agent_intents(
            &output.agent_intents,
            &frame.snapshot,
            &frame.recent_events,
            &["Flee".to_string()],
        );
        assert!(validation.passed);
        assert_eq!(validation.accepted.len(), 1);
    }

    #[test]
    fn ai_module_names_oil_and_biological_spill_dangers() {
        for (event, expected_memory, expected_dialogue) in [
            (
                contamination_spill_event(
                    15,
                    &["oil_flow", "oil_contamination", "slick_surface"],
                    &["danger", "liquid"],
                ),
                "oil-slick",
                "Oil on the ground",
            ),
            (
                contamination_spill_event(
                    16,
                    &[
                        "biological_trace",
                        "biological_contamination",
                        "contaminated_surface",
                    ],
                    &["danger", "liquid"],
                ),
                "biological contamination",
                "Biohazard",
            ),
        ] {
            let frame = make_frame(event.clone());
            let mut module = AiCharactersModule::default();
            let mut sink = CommandSink::default();

            module.tick(&frame, &mut sink);

            let output = module
                .last_output()
                .expect("contamination danger should produce AI output");
            assert!(output.intent_validation.passed);
            assert!(
                output
                    .agent_intents
                    .iter()
                    .any(|intent| matches!(intent, AgentIntent::Flee { agent: 3, from } if *from == event.event_id))
            );
            assert!(output.memory_updates.iter().any(|memory| {
                memory.agent == 3
                    && memory.memory_kind == MemoryKind::ShortTerm
                    && memory.content.contains(expected_memory)
            }));
            assert!(sink.commands.iter().any(|command| {
                matches!(
                    command,
                    WorldCommand::EmitDialogue(DialogueEvent { speaker: 3, text, .. })
                        if text.contains(expected_dialogue)
                )
            }));
        }
    }

    #[test]
    fn ai_module_investigates_loud_sound_without_claiming_witness() {
        let frame = make_frame(glass_shatter_sound_event(0.9, 28.0));
        let mut module = AiCharactersModule::default();
        let mut sink = CommandSink::default();

        module.tick(&frame, &mut sink);

        assert!(sink.events.iter().any(|event| {
            matches!(
                event.kind,
                WorldEventKind::NpcHeardSound {
                    listener: 3,
                    event: 11,
                    source_entity: Some(2),
                    confidence,
                } if confidence > 0.4
            )
        }));
        assert!(sink.events.iter().any(|event| {
            matches!(
                &event.kind,
                WorldEventKind::AgentMemoryUpdated {
                    agent: 3,
                    memory_kind,
                    content,
                    ..
                } if memory_kind == "short_term" && content.contains("Heard glass break")
            )
        }));
        assert!(
            !sink
                .events
                .iter()
                .any(|event| matches!(event.kind, WorldEventKind::NpcWitnessedCrime { .. }))
        );
        assert!(
            !sink
                .events
                .iter()
                .any(|event| matches!(event.kind, WorldEventKind::PlayerIdentityExposed))
        );
        assert!(sink.commands.iter().any(|command| {
            matches!(
                command,
                WorldCommand::SetAgentState(
                    3,
                    ashfall_core::world::AgentState {
                        emotional_state,
                        ..
                    }
                ) if emotional_state.urgency > 0.5 && emotional_state.fear > 0.3
            )
        }));
        assert!(sink.commands.iter().all(|command| {
            !matches!(
                command,
                WorldCommand::EmitDialogue(_) | WorldCommand::EmitStoryEvent(_)
            )
        }));
        let sound_location = frame.recent_events[0].location_meters;
        let investigate_step = sink
            .commands
            .iter()
            .find_map(|command| match command {
                WorldCommand::MoveEntityStep(step) if step.entity == 3 => Some(step),
                _ => None,
            })
            .expect("accepted investigation should request an engine movement step");
        assert_eq!(investigate_step.mode, MoveEntityStepMode::Toward);
        assert_eq!(investigate_step.reference_meters, sound_location);
        assert_eq!(investigate_step.max_step_meters, AI_INVESTIGATE_STEP_METERS);
        assert_eq!(
            investigate_step.stop_radius_meters,
            AI_INVESTIGATE_STOP_RADIUS_METERS
        );

        let output = module
            .last_output()
            .expect("sound reaction should produce AI output");
        assert!(output.intent_validation.passed);
        assert_eq!(output.intent_validation.accepted.len(), 1);
        assert!(output.agent_intents.iter().any(|intent| {
            matches!(
                intent,
                AgentIntent::Investigate {
                    agent: 3,
                    event: 11
                }
            )
        }));
        assert!(sink.events.iter().any(|event| {
            matches!(
                &event.kind,
                WorldEventKind::AgentIntentProposed {
                    agent: 3,
                    action,
                    source_event: Some(11),
                    validation_passed: true,
                } if action == "Investigate"
            )
        }));
        assert!(sink.events.iter().any(|event| {
            matches!(
                &event.kind,
                WorldEventKind::AgentDecisionExplained {
                    agent,
                    source_event,
                    goal,
                    accepted_actions,
                    rejected_actions,
                    reasons,
                    ..
                } if *agent == 3
                    && *source_event == Some(11)
                    && goal.contains("unusual nearby sound")
                    && accepted_actions.iter().any(|action| action == "Investigate")
                    && rejected_actions
                        .iter()
                        .any(|reason| reason.contains("direct witness"))
                    && reasons.iter().any(|reason| reason.contains("hearing radius"))
            )
        }));
        assert!(output.debug_decisions.iter().any(|decision| {
            decision
                .rejected_actions
                .iter()
                .any(|reason| reason.contains("direct witness"))
        }));

        let validation = validate_agent_intents(
            &output.agent_intents,
            &frame.snapshot,
            &frame.recent_events,
            &["Investigate".to_string()],
        );
        assert!(validation.passed);
    }

    #[test]
    fn same_frame_investigations_coalesce_agent_transform_commands() {
        let mut frame = make_frame(glass_fracture_event());
        let mut sound = glass_shatter_sound_event(0.9, 28.0);
        sound.location_meters = frame.recent_events[0].location_meters;
        frame.recent_events.push(sound);
        let mut module = AiCharactersModule::default();
        let mut sink = CommandSink::default();

        module.tick(&frame, &mut sink);

        assert_eq!(
            sink.commands
                .iter()
                .filter(|command| {
                    matches!(command, WorldCommand::MoveEntityStep(step) if step.entity == 3)
                })
                .count(),
            1
        );
        let evidence_location = frame.recent_events[0].location_meters;
        let coalesced_step = sink
            .commands
            .iter()
            .find_map(|command| match command {
                WorldCommand::MoveEntityStep(step) if step.entity == 3 => Some(step),
                _ => None,
            })
            .expect("investigation should still request one movement command");
        assert_eq!(coalesced_step.mode, MoveEntityStepMode::Toward);
        assert_eq!(coalesced_step.reference_meters, evidence_location);
    }

    #[test]
    fn ai_module_records_own_blocked_navigation_without_repath_loop() {
        let frame = make_frame(navigation_blocked_event(3));
        let mut module = AiCharactersModule::default();
        let mut sink = CommandSink::default();

        module.tick(&frame, &mut sink);

        let output = module
            .last_output()
            .expect("own blocked movement should produce AI recovery output");
        assert!(output.intent_validation.passed);
        assert_eq!(output.intent_validation.accepted.len(), 1);
        assert!(output.agent_intents.iter().any(|intent| {
            matches!(
                intent,
                AgentIntent::MoveTo {
                    agent: 3,
                    location: ALLEY_STORY_LOCATION_ID
                }
            )
        }));
        assert!(sink.events.iter().any(|event| {
            matches!(
                &event.kind,
                WorldEventKind::AgentMemoryUpdated {
                    agent: 3,
                    source_event: Some(13),
                    memory_kind,
                    content,
                    ..
                } if memory_kind == "location" && content.contains("blocked")
            )
        }));
        assert!(sink.events.iter().any(|event| {
            matches!(
                &event.kind,
                WorldEventKind::AgentIntentProposed {
                    agent: 3,
                    action,
                    source_event: Some(13),
                    validation_passed: true,
                } if action == "MoveTo"
            )
        }));
        assert!(sink.events.iter().any(|event| {
            matches!(
                &event.kind,
                WorldEventKind::AgentDecisionExplained {
                    agent,
                    source_event,
                    goal,
                    accepted_actions,
                    rejected_actions,
                    reasons,
                    ..
                } if *agent == 3
                    && *source_event == Some(13)
                    && goal.contains("blocked movement")
                    && accepted_actions.iter().any(|action| action == "MoveTo")
                    && rejected_actions
                        .iter()
                        .any(|reason| reason.contains("repeat blocked movement"))
                    && reasons
                        .iter()
                        .any(|reason| reason.contains("does not emit another movement step"))
            )
        }));
        assert!(sink.commands.iter().any(|command| {
            matches!(
                command,
                WorldCommand::SetAgentState(
                    3,
                    ashfall_core::world::AgentState {
                        emotional_state,
                        ..
                    }
                ) if emotional_state.urgency >= 0.35 && emotional_state.anger >= 0.12
            )
        }));
        assert!(sink.commands.iter().all(|command| {
            !matches!(
                command,
                WorldCommand::MoveEntityStep(_)
                    | WorldCommand::EmitDialogue(_)
                    | WorldCommand::EmitStoryEvent(_)
            )
        }));

        let state = module.save_state().expect("AI module should save state");
        assert!(state.entries.iter().any(|entry| entry == "navigation:3:13"));
        let mut restored = AiCharactersModule::default();
        restored.load_state(&state);
        let mut replay_sink = CommandSink::default();
        restored.tick(&frame, &mut replay_sink);
        assert!(replay_sink.events.is_empty());
        assert!(replay_sink.commands.is_empty());
    }

    #[test]
    fn ai_planning_uses_configured_authority_and_navigation_location() {
        let config = AiCharactersConfig {
            default_authority_faction: 902,
            fallback_navigation_location: 778,
        };
        let persona = persona_for_agent(301);
        let crime_observation = AgentObservation {
            agent: 3,
            event_id: 21,
            kind: ObservationKind::Crime,
            knowledge_source: KnowledgeSource::DirectObservation(21),
            location: Vec3::new(2.0, 0.0, 0.0),
            distance_meters: 3.5,
            confidence: 0.91,
            evidence: vec!["line_of_sight".to_string()],
            tags: vec!["crime".to_string()],
        };

        let crime_decision = plan_agent_response_with_config(
            3,
            2,
            &persona,
            &crime_observation,
            QualityTier::HeroHighFidelityRuntime,
            &config,
        );

        assert!(crime_decision.intents.iter().any(|intent| {
            matches!(
                intent,
                AgentIntent::ReportCrime {
                    agent: 3,
                    event: 21,
                    authority: 902
                }
            )
        }));
        assert!(matches!(
            crime_decision.director_command,
            Some(DirectorCommand::TriggerInvestigation(InvestigationSpec {
                investigating_faction: 902,
                source_event: 21,
                ..
            }))
        ));

        let navigation_observation = AgentObservation {
            agent: 3,
            event_id: 22,
            kind: ObservationKind::Navigation,
            knowledge_source: KnowledgeSource::DirectObservation(22),
            location: Vec3::new(5.5, 0.0, 0.0),
            distance_meters: 0.0,
            confidence: 0.94,
            evidence: vec!["navigation_blocked".to_string()],
            tags: vec!["navigation".to_string(), "blocked".to_string()],
        };
        let navigation_decision = plan_agent_navigation_response_with_config(
            3,
            &persona,
            &navigation_observation,
            QualityTier::NormalRuntime,
            &config,
        );

        assert!(navigation_decision.intents.iter().any(|intent| {
            matches!(
                intent,
                AgentIntent::MoveTo {
                    agent: 3,
                    location: 778
                }
            )
        }));
    }

    #[test]
    fn ai_module_ignores_blocked_navigation_for_non_agent_entity() {
        let frame = make_frame(navigation_blocked_event(1));
        let mut module = AiCharactersModule::default();
        let mut sink = CommandSink::default();

        module.tick(&frame, &mut sink);

        assert!(sink.events.is_empty());
        assert!(sink.commands.is_empty());
        assert!(module.last_output().is_none());
    }

    #[test]
    fn quiet_sounds_do_not_create_ai_reactions() {
        let frame = make_frame(glass_shatter_sound_event(0.04, 28.0));
        let mut module = AiCharactersModule::default();
        let mut sink = CommandSink::default();

        module.tick(&frame, &mut sink);

        assert!(sink.events.is_empty());
        assert!(sink.commands.is_empty());
        assert!(module.last_output().is_none());
    }

    #[test]
    fn intent_validation_report_filters_and_describes_rejections() {
        let frame = make_frame(glass_fracture_event());
        let report = validate_agent_intents(
            &[
                AgentIntent::Investigate { agent: 3, event: 7 },
                AgentIntent::SpeakTo {
                    agent: 3,
                    target: 404,
                    act: SpeechAct::Warn,
                },
            ],
            &frame.snapshot,
            &frame.recent_events,
            &["Investigate".to_string(), "SpeakTo".to_string()],
        );

        assert!(!report.passed);
        assert_eq!(report.accepted.len(), 1);
        assert_eq!(report.rejected.len(), 1);
        assert!(intent_rejection_text(&report.rejected[0]).contains("target 404"));

        let mut output = AiFrameOutput::default();
        output.agent_intents.extend(report.accepted.clone());
        output.intent_validation.merge(report);

        assert!(!output.intent_validation.passed);
        assert_eq!(output.agent_intents.len(), 1);
        assert_eq!(output.intent_validation.rejected.len(), 1);
    }

    #[test]
    fn intent_validation_rejects_impossible_actions() {
        let frame = make_frame(glass_fracture_event());

        let missing_event = validate_agent_intent(
            &AgentIntent::Investigate {
                agent: 3,
                event: 404,
            },
            &frame.snapshot,
            &frame.recent_events,
            &[],
        );
        assert_eq!(
            missing_event,
            Err(AgentIntentValidationError::MissingEvent(404))
        );

        let missing_target = validate_agent_intent(
            &AgentIntent::SpeakTo {
                agent: 3,
                target: 404,
                act: SpeechAct::Warn,
            },
            &frame.snapshot,
            &frame.recent_events,
            &[],
        );
        assert_eq!(
            missing_target,
            Err(AgentIntentValidationError::MissingTarget(404))
        );

        let unavailable = validate_agent_intent(
            &AgentIntent::Flee { agent: 3, from: 7 },
            &frame.snapshot,
            &frame.recent_events,
            &["MoveTo".to_string()],
        );
        assert_eq!(
            unavailable,
            Err(AgentIntentValidationError::UnavailableAction("Flee"))
        );

        let story_frame = make_frame(WorldEvent {
            event_id: 10,
            tick: 1,
            location_meters: Vec3::new(2.0, 0.0, 0.0),
            actors: vec![3],
            kind: WorldEventKind::StoryEventEmitted {
                label: "quiet_rumor".to_string(),
            },
            physical_evidence: Vec::new(),
            narrative_tags: vec!["story".to_string()],
        });
        let unsupported_threat = validate_agent_intent(
            &AgentIntent::Flee { agent: 3, from: 10 },
            &story_frame.snapshot,
            &story_frame.recent_events,
            &[],
        );
        assert_eq!(
            unsupported_threat,
            Err(AgentIntentValidationError::UnsupportedThreat(10))
        );
    }

    #[test]
    fn story_director_tracks_grounded_investigation_state() {
        let frame = make_frame(WorldEvent {
            event_id: 8,
            tick: 1,
            location_meters: Vec3::new(2.0, 0.0, 0.0),
            actors: vec![1, 3],
            kind: WorldEventKind::PlayerIdentityExposed,
            physical_evidence: vec!["npc_witness".to_string()],
            narrative_tags: vec!["identity".to_string(), "story_consequence".to_string()],
        });
        let mut director = StoryDirectorModule::default();
        let mut sink = CommandSink::default();

        director.tick(&frame, &mut sink);

        assert!(sink.events.iter().any(|event| {
            matches!(
                event.kind,
                WorldEventKind::SecurityAlertRaised {
                    faction: ALLEY_SECURITY_FACTION_ID,
                    threat: 1,
                    severity,
                    ..
                } if severity > 0.7
            )
        }));
        assert!(director.state().tension > 0.2);
        assert!(director.state().city_alertness.security > 0.3);
        assert!(
            director
                .state()
                .active_threads
                .iter()
                .any(|thread| thread.grounded_events.contains(&8))
        );
        assert!(director.commands().iter().any(|command| {
            matches!(
                command,
                DirectorCommand::TriggerInvestigation(InvestigationSpec {
                    source_event: 8,
                    suspect: 1,
                    ..
                })
            )
        }));
        assert!(director.state().active_rumors.iter().any(|rumor| {
            rumor.source_event == 8
                && rumor.origin_faction == ALLEY_SECURITY_FACTION_ID
                && rumor.label == "a witness identified the glass-breaker"
                && rumor.credibility > 0.8
                && !rumor.incomplete
                && rumor.known_to_agents == vec![1, 3]
                && rumor.known_to_factions == vec![ALLEY_SECURITY_FACTION_ID]
                && rumor.grounded_events.contains(&8)
        }));
        assert!(
            director
                .state()
                .grounded_opportunities
                .iter()
                .any(|opportunity| {
                    opportunity.grounded_event == 8
                        && opportunity.kind == StoryOpportunityKind::MoralDilemma
                        && opportunity.requires_world_validation
                        && opportunity.source_rumor.is_some()
                })
        );
        assert!(
            director
                .state()
                .grounded_opportunities
                .iter()
                .any(|opportunity| {
                    opportunity.grounded_event == 8
                        && opportunity.kind == StoryOpportunityKind::Secret
                        && opportunity
                            .involved_factions
                            .contains(&ALLEY_SECURITY_FACTION_ID)
                })
        );
        assert!(director.commands().iter().any(|command| {
            matches!(
                command,
                DirectorCommand::CreateMoralDilemma(DilemmaSpec {
                    grounded_event: 8,
                    ..
                })
            )
        }));
        assert!(
            director
                .factions()
                .get(ALLEY_SECURITY_FACTION_ID)
                .is_some_and(|faction| faction.alertness > 0.3)
        );
    }

    #[test]
    fn story_director_uses_configured_security_context() {
        let frame = make_frame(WorldEvent {
            event_id: 8,
            tick: 1,
            location_meters: Vec3::new(2.0, 0.0, 0.0),
            actors: vec![1, 3],
            kind: WorldEventKind::PlayerIdentityExposed,
            physical_evidence: vec!["npc_witness".to_string()],
            narrative_tags: vec!["identity".to_string(), "story_consequence".to_string()],
        });
        let config = StoryDirectorConfig {
            security_faction: 900,
            opposition_faction: 901,
            story_location: 777,
        };
        let mut director = StoryDirectorModule::new(config);
        let mut sink = CommandSink::default();

        director.tick(&frame, &mut sink);

        assert!(sink.events.iter().any(|event| {
            matches!(
                event.kind,
                WorldEventKind::SecurityAlertRaised {
                    faction: 900,
                    threat: 1,
                    severity,
                    ..
                } if severity > 0.7
            )
        }));
        assert!(
            director
                .factions()
                .get(900)
                .is_some_and(|faction| faction.territory == vec![777]
                    && faction.enemies == vec![901]
                    && faction.alertness > 0.3)
        );
        assert!(director.factions().get(ALLEY_SECURITY_FACTION_ID).is_none());
        assert_eq!(director.state().faction_tensions.tension(900, 901), 0.66);
        assert!(director.commands().iter().any(|command| {
            matches!(
                command,
                DirectorCommand::TriggerInvestigation(InvestigationSpec {
                    investigating_faction: 900,
                    suspect: 1,
                    ..
                })
            )
        }));
    }

    #[test]
    fn story_director_schedules_gpu_pressure_opportunity_and_command_validation() {
        let frame = make_frame(WorldEvent {
            event_id: 8,
            tick: 1,
            location_meters: Vec3::new(2.0, 0.0, 0.0),
            actors: vec![1, 3],
            kind: WorldEventKind::PlayerIdentityExposed,
            physical_evidence: vec!["npc_witness".to_string()],
            narrative_tags: vec!["identity".to_string(), "story_consequence".to_string()],
        });
        let mut director = StoryDirectorModule::default();
        let mut sink = CommandSink::default();

        director.tick(&frame, &mut sink);

        assert!(director.performance_counters().gpu_milliseconds > 0.0);
        let services = GpuServices::new_vulkan();
        let mut graph = services.begin_frame(frame.frame_id);
        director.schedule_gpu(&mut graph);
        let report = services.submit(graph);

        assert!(report.validation.passed);
        assert!(
            report
                .passes
                .iter()
                .any(|pass| pass.name == "story_thread_pressure_update")
        );
        assert!(
            report
                .passes
                .iter()
                .any(|pass| pass.name == "story_faction_reputation_update")
        );
        assert!(
            report
                .passes
                .iter()
                .any(|pass| pass.name == "story_grounded_opportunity_index")
        );
        assert!(
            report
                .passes
                .iter()
                .any(|pass| pass.name == "story_rumor_propagation")
        );
        assert!(
            report
                .passes
                .iter()
                .any(|pass| pass.name == "story_director_command_validation")
        );
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(55)
                && usage.label == "story active thread table"
                && usage.bindless_index.is_some()
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "story_thread_pressure_update")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(55)
                && usage.label == "story grounded rumor table"
                && usage.bindless_index.is_some()
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "story_rumor_propagation")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(55)
                && usage.label == "story director opportunity table"
                && usage.bindless_index.is_some()
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "story_grounded_opportunity_index")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(55)
                && usage.label == "story rumor propagation buffer"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "story_rumor_propagation")
                && usage
                    .readers
                    .iter()
                    .any(|reader| reader == "story_grounded_opportunity_index")
        }));
        assert!(report.registered_resource_usage.iter().any(|usage| {
            usage.owner == Some(55)
                && usage.label == "story grounded command validation buffer"
                && usage
                    .writers
                    .iter()
                    .any(|writer| writer == "story_director_command_validation")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "story_director_command_validation"
                && pipeline.shader_key == "story/director_command_validation.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "NO_FORCED_SCENES")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "story_rumor_propagation"
                && pipeline.shader_key == "story/rumor_propagation.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "NO_GLOBAL_KNOWLEDGE")
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "KNOWLEDGE_GROUNDING")
        }));
        assert!(report.pipeline_report.pipelines.iter().any(|pipeline| {
            pipeline.label == "story_grounded_opportunity_index"
                && pipeline.shader_key == "story/grounded_opportunity_index.comp"
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "DIRECTOR_OPPORTUNITIES")
                && pipeline
                    .permutation_defines
                    .iter()
                    .any(|define| define == "WORLD_VALIDATED_OFFERS")
        }));

        let mut replay_sink = CommandSink::default();
        director.tick(&frame, &mut replay_sink);
        assert!(replay_sink.events.is_empty());
        assert!(director.commands().is_empty());
    }

    #[test]
    fn story_director_state_restores_threads_reputation_and_factions() {
        let frame = make_frame(WorldEvent {
            event_id: 8,
            tick: 1,
            location_meters: Vec3::new(2.0, 0.0, 0.0),
            actors: vec![1, 3],
            kind: WorldEventKind::PlayerIdentityExposed,
            physical_evidence: vec!["npc_witness".to_string()],
            narrative_tags: vec!["identity".to_string(), "story_consequence".to_string()],
        });
        let mut director = StoryDirectorModule::default();
        let mut sink = CommandSink::default();
        director.tick(&frame, &mut sink);

        let state = director
            .save_state()
            .expect("story director should save state");
        assert!(
            state
                .entries
                .iter()
                .any(|entry| entry.starts_with("story_thread:"))
        );
        assert!(
            state
                .entries
                .iter()
                .any(|entry| entry.starts_with("faction_state:"))
        );
        assert!(
            state
                .entries
                .iter()
                .any(|entry| entry.starts_with("rumor:"))
        );
        assert!(
            state
                .entries
                .iter()
                .any(|entry| entry.starts_with("opportunity:"))
        );

        let mut restored = StoryDirectorModule::default();
        restored.load_state(&state);

        assert!(restored.state().tension > 0.2);
        assert!(restored.state().city_alertness.security > 0.3);
        assert!(
            restored
                .state()
                .active_threads
                .iter()
                .any(|thread| thread.grounded_events.contains(&8)
                    && thread.status == StoryThreadStatus::Escalating)
        );
        assert!(restored.state().active_rumors.iter().any(|rumor| {
            rumor.source_event == 8
                && rumor.origin_faction == ALLEY_SECURITY_FACTION_ID
                && rumor.known_to_agents == vec![1, 3]
                && rumor.known_to_factions == vec![ALLEY_SECURITY_FACTION_ID]
        }));
        assert!(
            restored
                .state()
                .grounded_opportunities
                .iter()
                .any(|opportunity| opportunity.grounded_event == 8
                    && opportunity.kind == StoryOpportunityKind::MoralDilemma
                    && opportunity.requires_world_validation)
        );
        assert_eq!(
            restored
                .state()
                .player_reputation
                .trust_by_faction
                .get(&ALLEY_SECURITY_FACTION_ID)
                .copied(),
            Some(-0.25)
        );
        assert!(
            restored
                .factions()
                .get(ALLEY_SECURITY_FACTION_ID)
                .is_some_and(|faction| {
                    faction.alertness > 0.3 && faction.reputation_with_player < 0.0
                })
        );

        let mut replay_sink = CommandSink::default();
        restored.tick(&frame, &mut replay_sink);
        assert!(replay_sink.events.is_empty());
        assert!(restored.commands().is_empty());
    }

    #[test]
    fn story_director_increases_surveillance_from_heard_sound_report() {
        let frame = make_frame(WorldEvent {
            event_id: 12,
            tick: 1,
            location_meters: Vec3::new(5.5, 0.0, 0.0),
            actors: vec![2, 3],
            kind: WorldEventKind::NpcHeardSound {
                listener: 3,
                event: 11,
                source_entity: Some(2),
                confidence: 0.72,
            },
            physical_evidence: vec!["heard_sound".to_string(), "npc_audio_report".to_string()],
            narrative_tags: vec!["audio".to_string(), "hearing".to_string()],
        });
        let mut director = StoryDirectorModule::default();
        let mut sink = CommandSink::default();

        director.tick(&frame, &mut sink);

        assert!(sink.events.iter().any(|event| {
            matches!(
                event.kind,
                WorldEventKind::SurveillanceIncreased {
                    location: ALLEY_STORY_LOCATION_ID,
                    source_event: 11,
                    amount,
                } if amount > 0.1
            )
        }));
        assert!(director.state().city_alertness.security > 0.1);
        assert!(director.state().active_threads.iter().any(|thread| {
            thread.label == "unidentified sound investigation"
                && thread.grounded_events.contains(&11)
                && thread.grounded_events.contains(&12)
        }));
        assert!(director.commands().iter().any(|command| {
            matches!(
                command,
                DirectorCommand::IncreaseSurveillance(ALLEY_STORY_LOCATION_ID)
            )
        }));
        assert!(director.commands().iter().any(|command| {
            matches!(
                command,
                DirectorCommand::IncreasePressure {
                    location: ALLEY_STORY_LOCATION_ID,
                    amount,
                } if *amount > 0.1
            )
        }));
        assert!(director.commands().iter().any(|command| {
            matches!(
                command,
                DirectorCommand::IntroduceRumor(RumorSpec {
                    source_event: 11,
                    faction: ALLEY_SECURITY_FACTION_ID,
                    label,
                    credibility,
                }) if label == "an unidentified impact drew a security sweep"
                    && *credibility > 0.6
                    && *credibility < 0.75
            )
        }));
        assert!(director.state().active_rumors.iter().any(|rumor| {
            rumor.source_event == 11
                && rumor.incomplete
                && rumor.known_to_agents == vec![2, 3]
                && rumor.grounded_events == vec![11, 12]
        }));
        assert!(
            director
                .state()
                .grounded_opportunities
                .iter()
                .any(|opportunity| {
                    opportunity.grounded_event == 11
                        && opportunity.kind == StoryOpportunityKind::Job
                        && opportunity.source_rumor.is_some()
                        && opportunity.requires_world_validation
                })
        );
        assert!(director.commands().iter().any(|command| {
            matches!(
                command,
                DirectorCommand::OfferJob(JobOpportunitySpec {
                    grounded_event: 11,
                    ..
                })
            )
        }));
    }

    #[test]
    fn story_director_seeds_grounded_opportunities_from_environmental_hazards() {
        let frame = make_frame(toxic_gas_event());
        let mut director = StoryDirectorModule::default();
        let mut sink = CommandSink::default();

        director.tick(&frame, &mut sink);

        assert!(sink.events.is_empty());
        assert!(
            director
                .state()
                .grounded_opportunities
                .iter()
                .any(|opportunity| {
                    opportunity.grounded_event == 9
                        && opportunity.kind == StoryOpportunityKind::MoralDilemma
                        && opportunity.label.contains("evacuation")
                        && opportunity.requires_world_validation
                })
        );
        assert!(director.commands().iter().any(|command| {
            matches!(
                command,
                DirectorCommand::CreateMoralDilemma(DilemmaSpec {
                    grounded_event: 9,
                    ..
                })
            )
        }));

        let mut replay_sink = CommandSink::default();
        director.tick(&frame, &mut replay_sink);
        assert!(replay_sink.events.is_empty());
        assert!(director.commands().is_empty());
    }

    #[test]
    fn story_director_names_contamination_cleanup_jobs_from_evidence() {
        for (event, expected_thread, expected_job) in [
            (
                contamination_spill_event(
                    17,
                    &["oil_flow", "oil_contamination", "slick_surface"],
                    &["danger", "liquid"],
                ),
                "oil slick containment",
                "contain the oil slick",
            ),
            (
                contamination_spill_event(
                    18,
                    &[
                        "biological_trace",
                        "biological_contamination",
                        "contaminated_surface",
                    ],
                    &["danger", "liquid"],
                ),
                "biohazard containment",
                "contain the biological spill",
            ),
        ] {
            let frame = make_frame(event.clone());
            let mut director = StoryDirectorModule::default();
            let mut sink = CommandSink::default();

            director.tick(&frame, &mut sink);

            assert!(sink.events.is_empty());
            assert!(director.state().active_threads.iter().any(|thread| {
                thread.label == expected_thread && thread.grounded_events.contains(&event.event_id)
            }));
            assert!(
                director
                    .state()
                    .grounded_opportunities
                    .iter()
                    .any(|opportunity| {
                        opportunity.grounded_event == event.event_id
                            && opportunity.kind == StoryOpportunityKind::Job
                            && opportunity.label.contains(expected_job)
                            && opportunity.requires_world_validation
                    })
            );
            assert!(director.commands().iter().any(|command| {
                matches!(
                    command,
                    DirectorCommand::OfferJob(JobOpportunitySpec {
                        grounded_event,
                        label,
                        ..
                    }) if *grounded_event == event.event_id && label.contains(expected_job)
                )
            }));
        }
    }
}
