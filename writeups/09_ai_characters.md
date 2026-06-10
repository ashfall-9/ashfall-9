# 09 — AI-Based Characters

## Goal

Characters should be agentic rather than fixed dialogue-tree actors. They should perceive the world, maintain memory, choose goals, plan actions, speak, react to physical consequences, and remain bounded by game rules.

## Sanity constraint

AI must not directly mutate world state. AI emits intents; simulation validates and applies actions.

```text
AI observes -> AI decides intent -> simulation validates -> animation/action system executes -> AI receives feedback
```

## Agent architecture

```text
Perception:
  visible entities, sounds, impacts, speech, social context, danger

Memory:
  episodic records, semantic facts, relationships, promises, emotional tags

Needs/goals:
  safety, curiosity, task goals, social state, fatigue, fear, hunger if modeled

Planner:
  chooses actions constrained by known world state and abilities

Dialogue:
  decides what to say, when to interrupt, when to stay silent

Motor/action layer:
  maps intent into locomotion, gaze, gesture, manipulation, animation

World consistency:
  prevents omniscience; agent knows only what it perceived or was told
```

## AI controller API

```rust
pub trait AiCharacterController {
    fn observe(
        &mut self,
        character: CharacterId,
        perception: PerceptionFrame,
    ) -> anyhow::Result<()>;

    fn decide(
        &mut self,
        character: CharacterId,
        dt_s: f32,
        world: &AiWorldView,
    ) -> anyhow::Result<Vec<AiIntent>>;

    fn receive_feedback(
        &mut self,
        character: CharacterId,
        feedback: ActionFeedback,
    ) -> anyhow::Result<()>;
}
```

## Intent contract

```rust
pub enum AiIntent {
    MoveTo { target: Vec3, urgency: f32 },
    LookAt { target: LookTarget, priority: f32 },
    Speak { utterance: DialogueIntent },
    Manipulate { object: EntityId, action: ManipulationKind },
    ReactToImpact { event_id: EventId },
    Flee { threat: EntityId },
    Investigate { target: EntityId },
    Idle { style: IdleStyle },
}
```

Intent rules:

```text
- intents can be rejected by simulation
- intents must be serializable for replay/debugging
- intents include priority and expiry where needed
- direct world edits are forbidden
```

## Local vs external AI split

Local systems:

```text
- perception filtering
- behavior fallback
- path/action validation
- memory retrieval ranking
- emotion/state estimates
- simple dialogue routing
- animation helper models
```

External models:

```text
- rich dialogue
- high-level planning
- development-time evaluation
- test-case generation
- narrative improvisation
```

External calls must run on AI worker threads, not on the render thread.

## Memory model

```rust
pub struct MemoryRecord {
    pub id: MemoryId,
    pub character: CharacterId,
    pub timestamp_s: f64,
    pub source: MemorySource,
    pub content: MemoryContent,
    pub confidence: f32,
    pub emotional_weight: f32,
    pub expires_at_s: Option<f64>,
}
```

Memory should be queryable, summarizeable, and inspectable during debugging.

## Physical consequence perception

AI should perceive physics through summarized events:

```text
- loud impact nearby
- object broke
- liquid spilled
- smoke/gas appeared
- path blocked
- character injured or threatened
```

Do not feed raw solver state directly into agent prompts. Use perception summaries with visibility/audibility constraints.

## AI safety and control

```text
- provider prompts are versioned
- tool/action calls are whitelisted
- no arbitrary file/network access from game agents
- dialogue can be moderated or filtered
- NPCs cannot execute code
- fallback behavior exists when provider unavailable
```

## AI evaluation

Test agents with replayable scenarios:

```text
- sees object fall and reacts
- hears fracture off-screen and investigates
- remembers a promise
- does not know hidden information
- handles rejected action intent
- speaks with consistent persona
```

Metrics:

```text
- action validity rate
- latency
- memory consistency
- hallucinated knowledge rate
- player comfort/readability
- provider cost
- fallback quality
```
