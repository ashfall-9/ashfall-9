# 23 — Vulkan, winit, Surface, and Swapchain Lifecycle

## Ownership rule

```text
app_desktop owns the OS event loop and window lifecycle.
gfx_vk owns Vulkan instance/device/surface/swapchain objects.
renderer owns render targets, render graph resources, scene proxies, and presentation requests.
```

No renderer feature should depend on winit types. No gameplay, AI, physics, or material component should depend on winit or raw window handles.

## Desktop application lifecycle

The desktop app should be structured around the current winit application model.

```rust
pub struct DesktopApp {
    pub state: DesktopAppState,
    pub engine: EngineRuntime,
}

pub enum DesktopAppState {
    Uninitialized,
    WindowReady(WindowHandleId),
    SurfaceReady(PresentationSurfaceId),
    SwapchainReady(SwapchainId),
    Suspended,
    Minimized,
    ResizePending { width: u32, height: u32 },
    SurfaceLost,
    DeviceLost,
    ShuttingDown,
}
```

The actual implementation should create the window at the correct point in the winit lifecycle and then ask `gfx_vk` to create or recreate the presentation surface.


## V4 winit redraw rule

Current winit guidance centers event handling on `ApplicationHandler` and `EventLoop::run_app`. The desktop app should request rendering through redraw events rather than treating `AboutToWait` as the render loop.

Recommended policy:

```text
- create the window during the appropriate app lifecycle callback
- request redraw when simulation/render state needs a new frame
- render in response to WindowEvent::RedrawRequested
- handle AboutToWait only for scheduling/wakeup decisions, not as the main draw call site
- treat suspend/resume and zero-sized surfaces as normal states
```

Sketch:

```rust
impl ApplicationHandler for DesktopApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // create window if needed, then initialize/reinitialize presentation
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::RedrawRequested => {
                // tick engine and submit one presented frame if the surface is available
            }
            WindowEvent::Resized(size) => {
                // mark resize pending; gfx_vk recreates swapchain at a safe point
            }
            WindowEvent::CloseRequested => event_loop.exit(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // request_redraw if continuous rendering is enabled or a timed update is due
    }
}
```

## Presentation host interface

The renderer should not directly own the app event loop. Use an interface like this:

```rust
pub trait PresentationHost {
    fn presentation_state(&self) -> PresentationState;
    fn surface_id(&self) -> Option<PresentationSurfaceId>;
    fn current_extent(&self) -> Option<[u32; 2]>;
    fn request_redraw(&self);
}

pub enum PresentationState {
    Unavailable,
    Available,
    Minimized,
    ResizePending,
    SurfaceLost,
    DeviceLost,
}
```

The renderer can render an offscreen capture without a presentation surface, but it cannot present without `PresentationState::Available`.

## Swapchain recreation contract

Swapchain recreation must be an expected path, not an error path.

Triggers:

```text
- window resize
- scale-factor change
- surface out-of-date
- surface lost
- present mode or HDR mode change
- minimized/restored transition
- format/color-space change
```

Rules:

```text
- old swapchain images remain alive until GPU work using them has completed
- new swapchain creation is serialized through gfx_vk
- renderer features must not cache swapchain image handles directly
- graph resources imported from swapchain images are valid for one frame graph only
- capture/offscreen targets are independent from swapchain images
```

## Headless/offscreen evaluation path

`app_headless` should not create a winit window or swapchain.

It should create:

```text
- Vulkan instance/device/queues
- offscreen color/HDR/depth/debug images
- command submission path
- capture/export path
- telemetry path
```

It should not require:

```text
- OS window handle
- swapchain
- present mode
- display refresh rate
```

Certification caveat:

```text
Headless smoke tests prove code correctness and artifact generation.
They do not prove final performance unless run on the named target hardware
with the intended driver and GPU settings.
```

## Raw-window-handle compatibility

The project must pin compatible versions of winit, Vulkano, and raw-window-handle-related crates.

Rules:

```text
- keep window-handle conversion code only in app_desktop/gfx_vk boundary code
- document exact crate versions in dependency decision records
- never expose raw platform handles through engine_core interfaces
- add a smoke test that creates and destroys a surface repeatedly without leaks
```

## Device-lost handling

Device loss is rare but must have a controlled failure mode.

Minimum behavior:

```text
- stop submitting new GPU work
- flush telemetry and logs
- mark current eval run as failed due to device loss
- release resources in dependency order if possible
- require full renderer/device reinitialization before continuing
```

Do not attempt silent continuation after device loss during certification runs.
