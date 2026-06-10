# 32 — Component Acceptance Checklists

## Purpose

Each component should have a clear definition of done. These checklists prevent vague completion claims and help assign bounded Codex/ChatGPT tasks.

## Global component checklist

A component can be milestone-accepted only if:

```text
- public interfaces are documented
- interface version is declared
- unit tests exist for pure logic
- integration tests exist where applicable
- telemetry exists for runtime cost
- failure modes are explicit
- artifacts are reproducible
- dependency boundaries are respected
- performance budget is named
- regression gate exists
```

## Renderer checklist

```text
Architecture:
- renderer consumes FrameSnapshot and RenderTask inputs
- no winit types in renderer core
- no AI/voice/provider dependencies
- raw Vulkan isolated and reviewed
- render graph owns synchronization declarations

Quality:
- HDR pipeline exists
- physical-camera metadata exists
- debug AOVs exist for active features
- reference/oracle path exists or is planned for the feature

Performance:
- per-pass GPU timings
- CPU frame timings
- memory telemetry
- shader/pipeline counts
- p95/p99 frame times

Correctness:
- Vulkan validation clean in eval profile
- render graph resource hazards tested
- shader reflection/resource mismatch tested
- swapchain resize/suspend behavior tested for desktop
```

## Sky/cloud feature checklist

```text
- deterministic seed-driven cloud fields
- clear sky, sparse, broken, overcast, cirrus, cumulonimbus cases
- atmosphere transmittance or equivalent
- HDR sun and exposure path
- cloud transmittance affects sun visibility
- reduced-res raymarch and upscale path
- temporal reprojection with history validity output
- debug AOVs: density, transmittance, step count, history validity
- AI visual judge schema passes
- temporal metrics pass
- hardware profile performance passes
```

## Material generator checklist

```text
Architecture:
- generated material has runtime material, authoring graph, physical hints, provenance
- runtime renderer consumes compact RuntimeMaterial, not full generation graph
- material generation can be rerun from manifest/seed/input references

Quality:
- preview scenes under multiple lighting conditions
- tiling/repetition check
- BRDF parameter bounds check
- normal/displacement plausibility check
- energy-conservation sanity check

Performance:
- texture/procedural cache memory budget
- shader variant budget
- bake time and runtime cost telemetry
```

## Physics consequence checklist

```text
Architecture:
- physics emits consequence events, not renderer-specific commands
- solver support is declared per material/object class
- simulation LOD emits compatible event types
- renderer can consume fracture/fluid/gas events through scene updates

Correctness:
- material properties have units
- timestep policy documented
- deterministic replay policy documented where required
- collision/contact/fracture event schemas tested

Performance:
- solver timing
- object/particle/constraint counts
- memory usage
- LOD transitions measured
```

## Human generator checklist

```text
Architecture:
- HumanSpec input and HumanAssetBundle output documented
- consent/provenance metadata supported
- render avatar, physics avatar, animation rig, AI anchor, voice anchor separated

Milestone staging:
- skin patch before full body
- eye close-up before full face
- hair lock before full hair system
- mouth/teeth/tongue before speaking face
- face expression loop before full AI-driven human

Quality:
- skin subsurface/roughness variation
- eye moisture/cornea/iris depth checks
- hair lighting and LOD checks
- expression temporal stability
- uncanny/artifact review

Performance:
- triangle/meshlet budget
- material/texture/procedural cache budget
- skinning/deformation cost
- hair rendering cost
- LOD transition quality
```

## AI character checklist

```text
Architecture:
- character observes PerceptionFrame only
- decisions are AiIntent values, not direct world mutation
- action feedback closes the loop
- memory model is bounded and inspectable
- external LLM provider types do not leak

Behavior:
- no omniscience
- reactions are grounded in observed events
- agent goals and constraints are explicit
- replay/debug log exists for decisions

Performance:
- per-agent decision budget
- async provider timeouts
- local fallback behavior
- cache/memory budget
```

## Voice persona checklist

```text
Architecture:
- provider abstraction exists
- speech requests return stream/cache handles
- lip-sync output is separate from audio stream
- voice metadata stores disclosure/provenance

Safety:
- no real-person imitation without consent
- AI voice disclosure is represented in UI/game metadata
- provider/model/version recorded for generated audio
- cache policy is explicit

Performance:
- first-audio latency measured
- streaming latency measured
- cache hit rate measured
- fallback behavior exists when provider fails
```

## Eval lab checklist

```text
- deterministic scene runner
- artifact manifest writer
- artifact validator
- capture exporter
- GPU/CPU telemetry ingestion
- AI judge adapter with schema validation
- temporal metrics
- pass/fail decision engine
- human review artifact
- dashboard/report output optional
```

## Asset pipeline checklist

```text
- content-addressed asset IDs
- build manifest
- dependency graph
- glTF subset import/export
- generated cache invalidation
- texture compression policy
- no runtime synchronous load on render thread
- later MaterialX/OpenUSD bridges isolated from first milestone
```

## Codex task acceptance checklist

Every coding-agent task should answer:

```text
- What component is being edited?
- What paths are allowed?
- What paths are forbidden?
- What interface version changes are expected?
- What tests must pass?
- What eval command must run?
- What performance budget must not regress?
- What artifacts prove the change?
```

Task output is not accepted if:

```text
- it relaxes thresholds
- it edits unrelated milestone content
- it introduces unreviewed unsafe code
- it hides provider errors
- it removes telemetry
- it makes eval artifacts incomplete
```
