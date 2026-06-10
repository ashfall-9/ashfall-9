# 18 — References and Terms

## Current dependency notes

### Vulkano

Vulkano is a safe and rich Rust wrapper around Vulkan. It is appropriate for safer startup and development ergonomics, but the engine should keep a controlled raw Vulkan escape hatch for unsupported features or critical optimization paths.

Source: https://docs.rs/vulkano/latest/vulkano/

### winit

Winit is the cross-platform window creation and event-loop library. Current winit uses an `ApplicationHandler` / `run_app` event model and supports raw-window-handle integration needed for Vulkan surface creation.

Source: https://docs.rs/winit/latest/winit/

### Burn

Burn is a Rust deep-learning framework with multiple swappable backends. Its docs list WGPU, Candle, LibTorch, Flex, CUDA, ROCm, and related backend features; the `vulkan` feature exposes the WGPU backend with an alternative SPIR-V compiler path. Treat it as AI/model tooling behind workers, not as direct Vulkan renderer interop.

Source: https://docs.rs/burn/latest/burn/

### gweb / dev web layer

The exact `gweb` dependency should be treated as unresolved until a crate is selected. Use an internal `DevUiServer` trait and choose a concrete web framework later.

## Graphics references

### Vulkan 1.4

Khronos announced Vulkan 1.4 as a release that consolidates previously optional features and increases minimum hardware limits. Treat Vulkan 1.4 as a preferred capability tier, not the first baseline until hardware support is confirmed.

Source: https://www.khronos.org/news/press/khronos-streamlines-development-and-deployment-of-gpu-accelerated-applications-with-vulkan-1.4

### Dynamic rendering

Dynamic rendering removes the need to create traditional render pass objects for many cases and lets developers reference rendering attachments directly.

Source: https://docs.vulkan.org/samples/latest/samples/extensions/dynamic_rendering/README.html

### Descriptor indexing and descriptor buffer

Descriptor indexing enables large indexed resource arrays, useful for bindless-style designs. Descriptor buffer is a more advanced extension that stores descriptors in buffers and can reduce descriptor-management overhead, but it should not be a first-milestone requirement.

Sources:
- https://docs.vulkan.org/samples/latest/samples/extensions/descriptor_indexing/README.html
- https://www.khronos.org/blog/vk-ext-descriptor-buffer

### Vulkan ray tracing

Vulkan ray tracing is exposed through KHR extensions including acceleration structures, ray tracing pipelines, and ray queries. Ray tracing should be optional and feature-detected.

Sources:
- https://www.khronos.org/blog/vulkan-ray-tracing-final-specification-release
- https://docs.vulkan.org/guide/latest/extensions/ray_tracing.html

## Rendering references

### Atmospheric scattering

Precomputed atmospheric scattering is a foundational real-time sky/atmosphere approach. Bruneton and Neyret present real-time atmosphere rendering from ground to space, accounting for Rayleigh and Mie scattering.

Sources:
- https://diglib.eg.org/items/0af15b7c-795a-4cca-97d3-4cc9b99126e4
- https://ebruneton.github.io/precomputed_atmospheric_scattering/

### Volumetric clouds

Guerrilla’s Horizon/Nubis presentations are important production references for real-time volumetric cloud modeling, animation, lighting, weather maps, and performance targets.

Sources:
- https://www.guerrilla-games.com/read/the-real-time-volumetric-cloudscapes-of-horizon-zero-dawn
- https://www.guerrilla-games.com/read/nubis-authoring-real-time-volumetric-cloudscapes-with-the-decima-engine

## Scene/material standards

### glTF 2.0

glTF 2.0 includes a PBR metallic-roughness material model and is a good initial runtime interchange target.

Source: https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html

### MaterialX

MaterialX defines standard material/shading graph concepts useful for look development and interchange. Use a subset after the internal material graph exists.

Source: https://github.com/AcademySoftwareFoundation/MaterialX/blob/main/documents/Specification/MaterialX.PBRSpec.md

### OpenUSD

OpenUSD is an extensible framework and ecosystem for describing, composing, simulating, and collaboratively constructing 3D scenes. Use it as a later interchange bridge, not the first runtime scene graph.

Sources:
- https://aousd.org/
- https://openusd.org/dev/user_guides/schemas/usdRender/overview.html

## AI and voice references

### OpenAI Codex

Codex is OpenAI’s coding agent and can read, edit, run, review, and debug code in development workflows. Use it through component-scoped tasks and project instructions.

Sources:
- https://developers.openai.com/codex
- https://developers.openai.com/codex/guides/agents-md

### OpenAI image/vision

OpenAI’s image/vision docs describe image inputs and image analysis use cases. Use image models as one evaluation signal, not as the sole milestone authority.

Source: https://developers.openai.com/api/docs/guides/images-vision

### OpenAI voice

OpenAI’s voice docs distinguish realtime audio/voice-agent flows and text-to-speech workflows. OpenAI’s TTS docs also require clear disclosure to end users that a TTS voice is AI-generated.

Sources:
- https://developers.openai.com/api/docs/guides/realtime
- https://developers.openai.com/api/docs/guides/voice-agents
- https://developers.openai.com/api/docs/guides/text-to-speech

## Profiling and debugging references

### Vulkan validation layers

Khronos validation layers help developers verify correct Vulkan API use during development.

Source: https://github.com/KhronosGroup/Vulkan-ValidationLayers

### RenderDoc

RenderDoc is a frame-capture graphics debugger available for Vulkan and other graphics APIs.

Source: https://github.com/baldurk/renderdoc

### NVIDIA Nsight Graphics

Nsight Graphics supports debugging, profiling, frame export, and ray-tracing analysis for Vulkan and other APIs.

Source: https://developer.nvidia.com/nsight-graphics

### AMD Radeon GPU Profiler

Radeon GPU Profiler provides hardware-level profiling for Vulkan, DirectX 12, OpenCL, and HIP workloads on AMD hardware.

Source: https://gpuopen.com/rgp/

## Terms

### Milestone-certified

A component is accepted for a milestone under named test scenes, hardware profiles, visual rubrics, temporal checks, and performance budgets. It does not mean the feature is literally perfect.

### Object class

A category of renderable or simulatable content developed in isolation. Examples: clouds, ceramic sphere, glass sphere, liquid droplet, skin patch, eye, hair lock, face, full human.

### Frame snapshot

Read-only world view for one frame. Components consume snapshots instead of reading each other’s private state.

### Consequence event

A physical event that changes the world and can be consumed by renderer, AI, audio, or gameplay: impact, fracture, liquid emission, gas emission, material damage.

### Artifact bundle

Immutable directory containing captures, AOVs, telemetry, settings, evaluator reports, and pass/fail decisions for one evaluation run.

### Renderer oracle

Slower high-quality renderer used for reference captures and validation. It does not need to meet realtime budgets.

## V2 added references

### Vulkan synchronization

Vulkan execution and memory synchronization is explicit; command batches and command buffers may overlap or execute out of order except where implicit guarantees or explicit synchronization apply. The render graph must therefore own resource-hazard validation and barrier generation.

Sources:
- https://docs.vulkan.org/spec/latest/chapters/synchronization.html
- https://docs.vulkan.org/spec/latest/chapters/cmdbuffers.html

### Descriptor indexing lifetime caution

Descriptor indexing enables update-after-bind and non-uniform indexing patterns, but it shifts responsibility to application-side synchronization, descriptor lifetime, and resource validity rules.

Sources:
- https://docs.vulkan.org/samples/latest/samples/extensions/descriptor_indexing/README.html
- https://docs.vulkan.org/tutorial/latest/Building_a_Simple_Engine/Advanced_Topics/Descriptor_Indexing_UpdateAfterBind.html

### OpenAI Evals deprecation

OpenAI has announced deprecation of the Evals platform, with existing evals becoming read-only on October 31, 2026 and shutdown scheduled for November 30, 2026. This project should therefore own its `eval_lab` artifact and rubric infrastructure.

Sources:
- https://developers.openai.com/api/docs/deprecations
- https://developers.openai.com/api/docs/guides/evals

### Codex operational guidance

Codex supports repository instructions through `AGENTS.md`, and OpenAI’s Codex documentation recommends using project guidance and validation-oriented workflows for coding-agent tasks.

Sources:
- https://developers.openai.com/codex/guides/agents-md
- https://developers.openai.com/codex/learn/best-practices


## V3 verified-source notes

### Vulkano current assumption

Current docs describe Vulkano as a safe and rich Rust wrapper around Vulkan. Keep using it as the safer default path, but retain the raw Vulkan escape hatch because wrapper coverage and performance needs may not perfectly match the latest Vulkan feature set.

Source: https://docs.rs/vulkano

### winit current assumption

Current winit docs identify the crate as cross-platform window creation and event-loop management, with the 0.30 application module centered around `ApplicationHandler`. Keep winit isolated in `app_desktop`; renderer internals should not depend on the event-loop model.

Sources:
- https://docs.rs/winit/latest/winit/
- https://docs.rs/winit/latest/winit/application/trait.ApplicationHandler.html

### Burn current assumption

Current Burn docs list a `vulkan` feature as making the WGPU backend available with an alternative SPIR-V compiler path. Therefore Burn remains useful for AI/model tooling, but should not be designed as direct renderer Vulkan interop.

Source: https://docs.rs/burn

### Vulkan 1.4 and baseline policy

Khronos announced Vulkan 1.4, and the current Vulkan specification line is Vulkan 1.4. Treat Vulkan 1.4 as a preferred capability tier while keeping Vulkan 1.3 as the practical first baseline until the hardware matrix proves otherwise.

Sources:
- https://www.vulkan.org/news/auto-23155-a676f167a3982c6a4f6d36a46284cad8
- https://registry.khronos.org/vulkan/specs/latest/html/vkspec.html

### Vulkan descriptor indexing and dynamic rendering

Descriptor indexing enables patterns such as update-after-bind and non-uniform indexing, but it shifts complexity into synchronization and resource lifetime. Dynamic rendering improves render-pass flexibility, but it does not remove the need for explicit synchronization.

Sources:
- https://docs.vulkan.org/samples/latest/samples/extensions/descriptor_indexing/README.html
- https://docs.vulkan.org/samples/latest/samples/extensions/dynamic_rendering/README.html

### Vulkan ray tracing and validation

Vulkan ray tracing is exposed through KHR acceleration-structure, ray-tracing-pipeline, and ray-query functionality; it should remain optional and feature-detected. Khronos validation layers help verify correct Vulkan API use, and synchronization validation is important for render-graph development.

Sources:
- https://docs.vulkan.org/guide/latest/extensions/ray_tracing.html
- https://github.com/KhronosGroup/Vulkan-ValidationLayers
- https://vulkan.lunarg.com/doc/view/latest/windows/synchronization_usage.html

### OpenAI Evals deprecation

OpenAI's deprecation docs state that the Evals platform was announced for deprecation on June 3, 2026, becomes read-only on October 31, 2026, and is scheduled to shut down on November 30, 2026. Keep `eval_lab` project-owned and use provider adapters only as replaceable components.

Source: https://developers.openai.com/api/docs/deprecations

### Codex and AGENTS.md

OpenAI's Codex documentation says Codex reads `AGENTS.md` files before doing work. Keep repository-specific coding-agent constraints in `AGENTS.md`, and keep task-specific path allowlists in the individual task prompt.

Source: https://developers.openai.com/codex/guides/agents-md


## V3 added references and checked claims

### Vulkano current crate docs

Current docs describe Vulkano as a safe and rich Rust wrapper around Vulkan. The docs also note raw-window-handle compatibility matters when creating surfaces from windowing libraries.

Sources:
- https://docs.rs/vulkano
- https://docs.rs/crate/vulkano/latest

### winit current crate docs

Current docs describe winit as a cross-platform window creation and event-loop library. They also note winit itself does not draw on a window; it exposes window/display handles for graphics APIs.

Source: https://docs.rs/winit/latest/winit/

### raw-window-handle

`raw-window-handle` provides standard types and traits for exposing platform-specific display/window handles to graphics libraries.

Source: https://docs.rs/raw-window-handle

### Burn Vulkan feature

Burn's docs list `vulkan` as making the WGPU backend available with an alternative SPIR-V compiler. This supports the rule that Burn should be an AI/eval/tooling backend, not a direct renderer Vulkan interop layer.

Source: https://docs.rs/burn

### Vulkan 1.4 and descriptor indexing

Khronos documents Vulkan 1.4 as consolidating important features such as dynamic rendering local read and scalar block layout. Khronos descriptor indexing samples document update-after-bind as flexible but dependent on correct application-side usage and synchronization.

Sources:
- https://www.vulkan.org/news/auto-23155-a676f167a3982c6a4f6d36a46284cad8
- https://docs.vulkan.org/samples/latest/samples/extensions/descriptor_indexing/README.html
- https://docs.vulkan.org/guide/latest/extensions/VK_EXT_descriptor_indexing.html

### OpenAI voice/eval APIs

OpenAI voice docs distinguish speech-to-speech sessions from chained voice pipelines. OpenAI TTS docs require clear disclosure that generated speech is AI-generated. OpenAI's evaluation best-practices docs state that the Evals platform is being deprecated with read-only and shutdown dates in 2026.

Sources:
- https://developers.openai.com/api/docs/guides/voice-agents
- https://developers.openai.com/api/docs/guides/text-to-speech
- https://developers.openai.com/api/docs/guides/evaluation-best-practices

### MaterialX and OpenUSD current docs

MaterialX lists current specification material including physically based shading nodes. OpenUSD render schema docs describe render settings/products/vars/AOV-style outputs; AOUSD announced OpenUSD v26.03 in 2026 with 3D Gaussian Splat schema and WebAssembly build support.

Sources:
- https://materialx.org/Specification.html
- https://openusd.org/dev/user_guides/schemas/usdRender/overview.html
- https://aousd.org/blog/openusd-v26-03/

## Revision 4 source-verification notes

### winit event loop and redraw

Current winit docs show the `ApplicationHandler` / `EventLoop::run_app` model and note that `AboutToWait` is not the ideal event to drive rendering from; applications should render in response to `WindowEvent::RedrawRequested`.

Sources:
- https://docs.rs/winit/latest/winit/
- https://docs.rs/winit/latest/winit/application/trait.ApplicationHandler.html

### Vulkano and raw-window-handle compatibility

Current Vulkano docs describe it as a safe and rich Rust wrapper around Vulkan and note that surface creation depends on raw-window-handle compatibility, currently mentioning `rwh_06` compatibility.

Source: https://docs.rs/vulkano/latest/vulkano/

### Burn Vulkan feature

Current Burn docs list `vulkan` as making the WGPU backend available with an alternative SPIR-V compiler. This supports the existing policy that Burn should be AI/model/tooling infrastructure, not direct renderer Vulkan interop.

Source: https://docs.rs/burn/latest/burn/

### Vulkan 1.4 preferred tier

Khronos documents Vulkan 1.4 as consolidating previously optional features and increasing minimum capabilities. V4 keeps Vulkan 1.3 as the practical first baseline and treats Vulkan 1.4 as a preferred capability tier after hardware verification.

Source: https://www.vulkan.org/news/auto-23155-a676f167a3982c6a4f6d36a46284cad8

### Descriptor indexing and descriptor buffer

Khronos descriptor-indexing samples describe update-after-bind as flexible, but descriptors must not be updated while they are actually accessed by the GPU. Descriptor buffer simplifies aspects of descriptor management, but does not remove application-side lifetime and synchronization obligations.

Sources:
- https://docs.vulkan.org/samples/latest/samples/extensions/descriptor_indexing/README.html
- https://docs.vulkan.org/samples/latest/samples/extensions/descriptor_buffer_basic/README.html

### OpenAI API, voice/audio, and Evals deprecation

Current OpenAI API docs distinguish the Responses API for model requests with tool use, audio, image, and text inputs from the Realtime API for low-latency voice/audio sessions. OpenAI's deprecation page states that the Evals platform is becoming read-only on October 31, 2026 and is scheduled to shut down on November 30, 2026.

Sources:
- https://developers.openai.com/api/reference/overview
- https://developers.openai.com/api/docs/guides/realtime
- https://developers.openai.com/api/docs/deprecations
