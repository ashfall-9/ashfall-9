# 14 — Component R&D Roadmap

## Roadmap principle

Parallelize tooling and infrastructure, not object-class certification. Only one visible object class should be under milestone certification at a time.

## Foundation work packages

```text
F-001 workspace bootstrap
F-002 AGENTS.md and coding-agent rules
F-003 engine_core handles/events/modules
F-004 telemetry crate
F-005 app_desktop winit shell
F-006 app_headless shell
F-007 gfx_vk device/swapchain/offscreen target
F-008 render_graph skeleton
F-009 shader_lab compile/cache/check
F-010 eval_lab artifact format
```

Acceptance:

```text
- cargo check/test passes
- window clears
- headless image capture works
- telemetry output exists
- validation clean for smoke frame
```

## Renderer work packages

```text
R-001 Vulkan bootstrap through Vulkano
R-002 raw Vulkan escape hatch policy
R-003 render graph resource/barrier validation
R-004 GPU timestamps and debug labels
R-005 shader compile/cache/hot reload
R-006 HDR color pipeline and tone mapping
R-007 atmosphere LUTs
R-008 sun disc and exposure
R-009 volumetric cloud density/raymarch
R-010 cloud lighting/shadow/transmittance
R-011 temporal reprojection/upscale
R-012 capture/AOV export
R-013 renderer oracle prototype
R-014 descriptor indexing experiments
R-015 optional ray tracing backend
```

## Sky milestone packages

```text
S-001 sky scene schema
S-002 sky seed-set manager
S-003 atmosphere feature
S-004 sun feature
S-005 cloud feature
S-006 sky debug AOVs
S-007 sky eval rubric
S-008 temporal camera paths
S-009 performance budget profiles
S-010 certification report generator
```

## Material packages

```text
M-001 runtime material schema
M-002 glTF PBR subset
M-003 procedural field library
M-004 material preview scenes
M-005 generated texture cache
M-006 material eval rubric
M-007 photo-to-material prototype
M-008 MaterialX subset mapper
M-009 skin material patch
M-010 fracture interior material generator
```

## Physics packages

```text
P-001 physical material schema
P-002 consequence event bus
P-003 rigid-body baseline
P-004 deterministic replay scene
P-005 ceramic fracture prototype
P-006 fragment renderer coupling
P-007 liquid particle prototype
P-008 gas voxel prototype
P-009 simulation LOD
P-010 AI perception of physical events
```

## Human packages

```text
H-001 human identity/provenance schema
H-002 skin patch material milestone
H-003 eye close-up milestone
H-004 hair lock milestone
H-005 mouth/teeth/tongue milestone
H-006 face neutral pose milestone
H-007 expression loop milestone
H-008 rig/deformation tiers
H-009 full-head milestone
H-010 full-human milestone
```

## AI packages

```text
A-001 perception frame schema
A-002 action intent schema
A-003 memory store
A-004 local fallback controller
A-005 external LLM adapter
A-006 dialogue intent contract
A-007 physics event perception
A-008 agent replay tests
A-009 provider cost/latency telemetry
A-010 safety and prompt versioning
```

## Voice packages

```text
V-001 voice persona schema
V-002 TTS provider trait
V-003 realtime voice trait
V-004 audio cache
V-005 lip-sync track schema
V-006 disclosure/provenance metadata
V-007 comfort evaluation set
V-008 latency telemetry
V-009 provider fallback
V-010 speaking-character integration
```

## Eval packages

```text
E-001 artifact directory format
E-002 capture exporter
E-003 GPU/CPU telemetry store
E-004 AI image judge adapter
E-005 reference image manager
E-006 temporal stability checker
E-007 rubric/scoring engine
E-008 dashboard/report generator
E-009 Codex task generator
E-010 milestone gatekeeper
```

## Dependency order

```text
Foundation -> Renderer smoke -> Capture/telemetry -> Sky -> Material object -> Physics object -> Human parts -> AI/voice integration
```

Do not start full humans before skin, eye, hair, mouth, face, animation, and evaluation foundations are certified separately.
