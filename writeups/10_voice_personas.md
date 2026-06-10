# 10 — Voice Personas

## Goal

Each AI character should have a voice persona that is consistent, comfortable to hear repeatedly, low-latency enough for interaction, and safe with respect to real-person identity.

## Architecture

Use two interchangeable voice paths:

```text
Realtime speech-to-speech path:
  best for natural low-latency conversation and interruption

Chained speech path:
  speech-to-text -> AI reasoning -> text-to-speech
  best for explicit control, logging, moderation, caching, and deterministic gameplay hooks
```

Both paths must feed the same game-level speech/lip-sync contracts. Synthesis must be job-based or streaming so voice generation never blocks renderer or simulation hot paths.

## Voice persona spec

```rust
pub struct VoicePersonaSpec {
    pub voice_id: VoiceId,
    pub persona_name: String,
    pub base_voice_provider_key: Option<String>,
    pub speaking_rate: f32,
    pub pitch_style: PitchStyle,
    pub emotional_range: EmotionalRange,
    pub accent_hint: Option<String>,
    pub disallowed_imitation_targets: Vec<String>,
    pub disclosure_required: bool,
    pub cache_policy: VoiceCachePolicy,
    pub provenance: VoiceProvenance,
}
```

## Voice runtime API

```rust
pub trait VoiceRuntime {
    fn submit_synthesis(
        &mut self,
        request: SpeechSynthesisRequest,
    ) -> anyhow::Result<SpeechJobId>;

    fn poll_synthesis(
        &mut self,
        job: SpeechJobId,
    ) -> anyhow::Result<SpeechJobStatus>;

    fn start_realtime_session(
        &mut self,
        session: VoiceSessionRequest,
    ) -> anyhow::Result<RealtimeVoiceSession>;

    fn lipsync(
        &mut self,
        audio: AudioStreamHandle,
    ) -> anyhow::Result<LipSyncTrack>;
}

pub enum SpeechJobStatus {
    Queued,
    Streaming(AudioStreamHandle),
    Complete { audio: AudioStreamHandle, lipsync: Option<LipSyncTrack> },
    Failed(VoiceError),
    Cancelled,
}
```

## Identity and disclosure rules

```text
- do not imitate real people without explicit rights
- do not describe a generated voice as "sounds like [real person]"
- store provenance for any voice sample or provider voice
- disclose AI-generated voice where required
- keep generated personas distinct from living individuals
```

## Comfort criteria

Voice quality should be evaluated for:

```text
- intelligibility
- repeated-listening comfort
- emotional appropriateness
- lack of harsh artifacts
- stable persona identity
- natural pauses
- good interruption behavior
- lip-sync alignment
```

## Runtime performance

```text
- cache common barks and reactions
- stream longer dialogue
- pre-generate likely lines when planning permits
- store phoneme/viseme timings
- degrade gracefully to lower-quality or text-only output
- never block render thread on voice generation
```

## Lip sync

Lip sync should combine:

```text
- phoneme/viseme timing from audio or provider
- dialogue intent emotion
- face rig constraints
- eye/gaze behavior
- interruption/cutoff handling
```

Audio-driven animation alone is insufficient for believable speaking humans.

## First voice milestone

Do not begin with fully interactive speech. Start with one generated fictional voice persona reading a fixed set of lines.

Acceptance:

```text
- audio is clear and comfortable
- persona is consistent across lines
- disclosure metadata exists
- generated audio has cached artifact IDs
- lip-sync track aligns to audio within tolerance
- provider can be swapped through trait interface
```

## V4 provider-path clarification

Voice support should be specified by outcome, not by a fixed provider implementation:

```text
low-latency interactive conversation:
  use realtime session adapter when enabled

bounded generated speech / narration / cached barks:
  use request/job-based speech generation adapter

speech-to-text for logs or chained agents:
  use transcription adapter
```

All provider paths must return project-owned outputs:

```text
- audio stream or artifact handle
- transcript if available
- word/phoneme/viseme timing if available
- provider/model metadata
- disclosure/provenance metadata
- cache key and usage rights
```

Provider-specific request and event types must not enter `engine_core`, `renderer_realtime`, or `ai_world` public interfaces.
