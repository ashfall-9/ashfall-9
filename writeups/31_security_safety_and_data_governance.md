# 31 — Security, Safety, and Data Governance

## Purpose

The engine uses AI-assisted evaluation, optional AI characters, optional voice personas, and possible reference photography. These systems create safety, privacy, identity, licensing, and security risks. This document defines project-level controls.

## Core policy

```text
No external provider, AI model, voice, dashboard, or dataset may become a hidden dependency of the renderer, physics engine, or core simulation.
```

All provider interaction must happen through adapters and be recorded in artifacts when it affects an eval or milestone decision.

## Secrets and credentials

Rules:

```text
- never commit provider API keys
- never write secrets into eval artifacts
- never include full request headers in logs
- never expose provider keys to dev_web clients
- store credentials through environment variables or platform secret storage
- redact tokens from panic/error reports
```

Recommended environment names:

```text
OPENAI_API_KEY
AI_EVAL_PROVIDER
VOICE_PROVIDER
DEV_WEB_BIND_ADDR
```

## AI eval data handling

AI visual evaluation may send rendered images to an external provider. Treat captures as project data.

Controls:

```text
- keep provider adapters in eval_lab/provider crates
- store exactly which images were sent
- store prompt, model, and provider metadata
- store provider response JSON
- allow local-only eval mode for confidential scenes
- do not send human reference photography unless licensing/consent permits it
```

For early sky work, captures contain no personal data, but the same policy should exist before humans/voices are added.

## Reference photography policy

For material, sky, human, or texture reference datasets:

```text
- track source, license, date acquired, and usage permission
- separate public-domain/owned/licensed/restricted datasets
- do not train or generate identifiable humans from unlicensed personal photos
- do not mix test-reference images with generation training data unless explicitly allowed
- keep dataset manifests under version control, but not necessarily the data itself
```

Dataset record example:

```json
{
  "dataset_id": "sky_refs_public_001",
  "source": "owned_photography_or_licensed_collection",
  "license": "project-owned",
  "allowed_uses": ["visual_reference", "eval_comparison"],
  "disallowed_uses": ["model_training"],
  "contains_people": false,
  "contains_voice": false
}
```

## Human generation and consent

Human generation is a high-risk area.

Rules:

```text
- do not generate a recognizable real person without explicit consent metadata
- do not use scraped identity photos as generation targets
- do not store real-person biometric references without access controls
- store consent/provenance metadata with any human asset derived from a person
- support deletion of person-derived references and generated assets when required
```

Consent metadata shape:

```rust
pub struct ConsentMetadata {
    pub subject_id: Option<String>,
    pub consent_scope: Vec<ConsentScope>,
    pub expires_at_utc: Option<String>,
    pub source_asset_ids: Vec<AssetId>,
    pub audit_note: String,
}
```

## Voice persona safety

Voice personas must not be real-person impersonation features.

Rules:

```text
- no voice target such as "sounds like Actor X" unless explicit rights and consent exist
- store voice as generated persona parameters, not as celebrity/real-person reference labels
- disclose AI-generated voices to end users
- store voice provider/model metadata
- cache generated audio with provenance and usage scope
- allow disabling voice provider calls in local/offline mode
```

Voice persona record:

```rust
pub struct VoicePersonaSpec {
    pub voice_id: VoiceId,
    pub synthetic_voice_label: String,
    pub disclosure_required: bool,
    pub consent_metadata: Option<ConsentMetadata>,
    pub disallowed_imitation_targets: Vec<String>,
    pub cache_policy: VoiceCachePolicy,
}
```

## AI character safety boundaries

Agentic characters should be constrained by the game world, not allowed to operate as unrestricted tools.

Rules:

```text
- agents act through AiIntent, not arbitrary code execution
- agents only perceive what the game exposes in PerceptionFrame
- agents do not receive secrets, API keys, or local filesystem access
- external LLM outputs must be converted into bounded game actions
- unsafe or impossible actions should return ActionFeedback::Rejected
```

## Dev dashboard security

The dev dashboard is a local development aid, not a trusted public service.

Controls:

```text
- bind to localhost by default
- require explicit config to bind externally
- do not expose provider keys
- do not allow arbitrary shell command execution
- restrict file browsing to artifact directories
- log dashboard actions that mutate settings
```

## Codex/coding-agent governance

Coding agents can edit source code, so treat them as privileged development tools.

Rules:

```text
- use AGENTS.md and task-specific path allowlists
- never ask agents to paste secrets into code or docs
- reject changes that lower test/rubric/performance thresholds
- require human review for interface changes and milestone promotion
- do not allow generated code to introduce unreviewed unsafe blocks
```

## Supply-chain controls

```text
- use Cargo.lock for applications/tools
- audit new dependencies before adding them
- prefer well-maintained crates for security-sensitive areas
- keep provider SDKs behind adapters so they can be replaced
- run dependency update tasks separately from feature tasks
```

## Logging policy

Do log:

```text
- dependency versions
- provider/model names
- prompt version IDs
- hardware/driver info
- timing and memory metrics
- pass/fail reasons
```

Do not log:

```text
- API keys
- auth headers
- private user prompts outside eval consent scope
- raw personal photos unless explicitly part of a protected artifact store
- raw voice identity samples without consent metadata
```

## First milestone safety status

Sky milestone is relatively low risk because it contains no humans, voices, personal data, or player communication. Still apply the artifact, provider, and secret-management policies now so the process is ready before higher-risk components appear.
