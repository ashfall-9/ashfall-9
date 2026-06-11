#![deny(unsafe_op_in_unsafe_fn)]

use std::{num::NonZeroU32, sync::Arc, time::Instant};

use engine_core::{
    CameraState, EngineRuntime, GpuCapabilities, PresentationState, PresentationSurfaceId,
    RenderQualityProfile, SwapchainId,
};
use eval_lab::BurnCloudPhotorealismJudge;
use gfx_vk::{VulkanBootstrapConfig, try_collect_gpu_capabilities};
use renderer_realtime::{SkyImageMetrics, SkyRenderSettings, render_sky_capture};
use scene_schema::SkySceneRequest;
use vulkano::{
    VulkanError,
    buffer::BufferContents,
    command_buffer::{
        AutoCommandBufferBuilder, CommandBufferUsage, RenderPassBeginInfo,
        allocator::StandardCommandBufferAllocator,
    },
    format::Format,
    image::view::ImageView,
    pipeline::{
        GraphicsPipeline, Pipeline, PipelineLayout, PipelineShaderStageCreateInfo,
        graphics::{
            GraphicsPipelineCreateInfo,
            color_blend::{ColorBlendAttachmentState, ColorBlendState},
            input_assembly::InputAssemblyState,
            multisample::MultisampleState,
            rasterization::RasterizationState,
            vertex_input::VertexInputState,
            viewport::{Viewport, ViewportState},
        },
        layout::PipelineDescriptorSetLayoutCreateInfo,
    },
    render_pass::{Framebuffer, FramebufferCreateInfo, RenderPass},
    swapchain::PresentMode,
    sync::GpuFuture,
};
use vulkano_util::{
    context::{VulkanoConfig, VulkanoContext},
    window::{VulkanoWindows, WindowDescriptor},
};
#[cfg(target_os = "windows")]
use winit::platform::windows::EventLoopBuilderExtWindows;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition},
    event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowId},
};

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct WindowHandleId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq)]
pub struct DesktopAppConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub seed: u64,
    pub print_device_name: bool,
}

impl Default for DesktopAppConfig {
    fn default() -> Self {
        Self {
            title: "Ashfall 2 - Sky Smoke".to_string(),
            width: 1280,
            height: 720,
            seed: 1,
            print_device_name: true,
        }
    }
}

pub fn run_desktop_app(config: DesktopAppConfig) -> anyhow::Result<()> {
    let event_loop = create_event_loop()?;
    let mut app = GpuSkyGameApp::new(config);
    event_loop.run_app(&mut app)?;
    Ok(())
}

fn create_event_loop() -> anyhow::Result<EventLoop<()>> {
    let mut builder = EventLoop::builder();
    #[cfg(target_os = "windows")]
    builder.with_any_thread(true);
    Ok(builder.build()?)
}

pub struct DesktopApp {
    pub state: DesktopAppState,
    runtime: EngineRuntime,
    config: DesktopAppConfig,
    window: Option<Arc<Window>>,
    presenter: Option<WindowPresenter>,
    window_id: Option<WindowId>,
    gpu: Option<GpuCapabilities>,
    sky: SkySceneRequest,
    sky_settings: SkyRenderSettings,
    judge: BurnCloudPhotorealismJudge,
    input: MovementInput,
    camera_position_m: [f32; 3],
    yaw_radians: f32,
    pitch_radians: f32,
    mouse_look_active: bool,
    last_cursor_position: Option<PhysicalPosition<f64>>,
    render_scale: f32,
    last_frame_fps: f32,
    last_redraw: Instant,
    frame_count: u64,
}

struct WindowPresenter {
    _context: softbuffer::Context<Arc<Window>>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    size: Option<[NonZeroU32; 2]>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DesktopCameraBasis {
    forward: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    fov_y_radians: f32,
    tan_x: f32,
    tan_y: f32,
}

#[derive(Clone, Debug, PartialEq)]
struct DesktopFrameMetrics {
    sky: SkyImageMetrics,
    frame_fps: f32,
    render_width: u32,
    render_height: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct MovementInput {
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    look_left: bool,
    look_right: bool,
    look_up: bool,
    look_down: bool,
    fast: bool,
    mouse_delta: [f32; 2],
}

struct GpuSkyGameApp {
    config: DesktopAppConfig,
    context: VulkanoContext,
    windows: VulkanoWindows,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    render_pass: Option<Arc<RenderPass>>,
    pipeline: Option<Arc<GraphicsPipeline>>,
    framebuffers: Vec<Arc<Framebuffer>>,
    input: MovementInput,
    sky: SkySceneRequest,
    camera_position_m: [f32; 3],
    yaw_radians: f32,
    pitch_radians: f32,
    mouse_look_active: bool,
    frame_index: u64,
    last_frame: Instant,
    last_fps: f32,
}

#[derive(BufferContents, Clone, Copy, Debug, PartialEq)]
#[repr(C)]
struct SkyPushConstants {
    camera_forward_tan_x: [f32; 4],
    camera_right_tan_y: [f32; 4],
    camera_up_time: [f32; 4],
    camera_position_seed: [f32; 4],
    sun_direction_radius: [f32; 4],
    resolution_weather: [f32; 4],
}

impl GpuSkyGameApp {
    fn new(config: DesktopAppConfig) -> Self {
        let vulkano_config = VulkanoConfig {
            print_device_name: config.print_device_name,
            ..Default::default()
        };
        let context = VulkanoContext::new(vulkano_config);
        let command_buffer_allocator = Arc::new(StandardCommandBufferAllocator::new(
            context.device().clone(),
            Default::default(),
        ));
        let sky = desktop_sky_scene(config.seed);
        let (initial_yaw, initial_pitch) = initial_gpu_camera_angles(config.seed);

        Self {
            config,
            context,
            windows: VulkanoWindows::default(),
            command_buffer_allocator,
            render_pass: None,
            pipeline: None,
            framebuffers: Vec::new(),
            input: MovementInput::default(),
            sky,
            camera_position_m: [0.0, 0.0, 0.0],
            yaw_radians: initial_yaw,
            pitch_radians: initial_pitch,
            mouse_look_active: true,
            frame_index: 0,
            last_frame: Instant::now(),
            last_fps: 0.0,
        }
    }

    fn initialize_window(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        if self.windows.primary_window_id().is_some() {
            return Ok(());
        }

        let descriptor = WindowDescriptor {
            title: self.config.title.clone(),
            width: self.config.width as f32,
            height: self.config.height as f32,
            present_mode: PresentMode::Immediate,
            ..Default::default()
        };
        self.windows
            .create_window(event_loop, &self.context, &descriptor, |_| {});
        if let Some(window) = self.windows.get_primary_window() {
            configure_game_cursor(window, true);
        }

        let renderer = self
            .windows
            .get_primary_renderer()
            .ok_or_else(|| anyhow::anyhow!("primary Vulkan renderer was not created"))?;
        let render_pass =
            create_sky_render_pass(self.context.device().clone(), renderer.swapchain_format())?;
        self.framebuffers =
            build_sky_framebuffers(render_pass.clone(), renderer.swapchain_image_views())?;
        self.pipeline = Some(create_sky_pipeline(
            self.context.device().clone(),
            render_pass.clone(),
            renderer.swapchain_image_size(),
        )?);
        self.render_pass = Some(render_pass);
        Ok(())
    }

    fn render(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let now = Instant::now();
        let dt_s = (now - self.last_frame).as_secs_f32().clamp(0.0, 0.1);
        self.last_frame = now;
        self.update_camera(dt_s);

        let Some(window) = self.windows.get_primary_window() else {
            event_loop.exit();
            return Ok(());
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            self.input.mouse_delta = [0.0, 0.0];
            return Ok(());
        }

        let render_pass = self
            .render_pass
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("render pass missing before redraw"))?
            .clone();
        let (before_future, image_index, graphics_queue, swapchain_recreated) = {
            let renderer = self
                .windows
                .get_primary_renderer_mut()
                .ok_or_else(|| anyhow::anyhow!("primary Vulkan renderer missing before redraw"))?;
            let mut swapchain_recreated = false;
            let before_future = match renderer.acquire(None, |_| {
                swapchain_recreated = true;
            }) {
                Ok(future) => future,
                Err(VulkanError::OutOfDate) => return Ok(()),
                Err(error) => return Err(anyhow::anyhow!(error)),
            };
            (
                before_future,
                renderer.image_index() as usize,
                renderer.graphics_queue(),
                swapchain_recreated,
            )
        };

        if swapchain_recreated {
            let renderer = self
                .windows
                .get_primary_renderer()
                .ok_or_else(|| anyhow::anyhow!("primary Vulkan renderer missing after resize"))?;
            self.framebuffers =
                build_sky_framebuffers(render_pass.clone(), renderer.swapchain_image_views())?;
            self.pipeline = Some(create_sky_pipeline(
                self.context.device().clone(),
                render_pass.clone(),
                renderer.swapchain_image_size(),
            )?);
        }

        let Some(framebuffer) = self.framebuffers.get(image_index).cloned() else {
            return Ok(());
        };
        let pipeline = self
            .pipeline
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("sky pipeline missing before redraw"))?
            .clone();
        let push_constants = self.sky_push_constants(size.width, size.height);
        let mut builder = AutoCommandBufferBuilder::primary(
            self.command_buffer_allocator.clone(),
            graphics_queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )?;
        builder
            .begin_render_pass(
                RenderPassBeginInfo {
                    clear_values: vec![Some([0.50, 0.68, 0.92, 1.0].into())],
                    ..RenderPassBeginInfo::framebuffer(framebuffer)
                },
                Default::default(),
            )?
            .bind_pipeline_graphics(pipeline.clone())?
            .push_constants(pipeline.layout().clone(), 0, push_constants)?;
        unsafe {
            builder.draw(3, 1, 0, 0)?;
        }
        builder.end_render_pass(Default::default())?;

        let command_buffer = builder.build()?;
        let future = before_future
            .then_execute(graphics_queue, command_buffer)?
            .boxed();
        let renderer = self
            .windows
            .get_primary_renderer_mut()
            .ok_or_else(|| anyhow::anyhow!("primary Vulkan renderer missing before present"))?;
        renderer.present(future, true);

        self.last_fps = fps_from_ms(dt_s * 1000.0);
        self.frame_index += 1;
        if self.frame_index.is_multiple_of(20)
            && let Some(window) = self.windows.get_primary_window()
        {
            let cloud_proximity = cloud_proximity_label(
                self.camera_position_m,
                gpu_cloud_center_m(self.config.seed, self.sky.time_seconds),
                gpu_cloud_radii_m(),
            );
            window.set_title(&format!(
                "{} | frame {} | {:.1} FPS | {} | GPU sky/clouds | WASD Shift move R/F height mouse/QE/TG look",
                self.config.title, self.frame_index, self.last_fps, cloud_proximity
            ));
        }

        Ok(())
    }

    fn update_camera(&mut self, dt_s: f32) {
        self.yaw_radians += axis(self.input.look_right, self.input.look_left) * 1.7 * dt_s;
        self.pitch_radians = (self.pitch_radians
            + axis(self.input.look_up, self.input.look_down) * 1.7 * dt_s)
            .clamp(-1.2, 1.2);
        self.yaw_radians += self.input.mouse_delta[0] * 0.0025;
        self.pitch_radians =
            (self.pitch_radians - self.input.mouse_delta[1] * 0.0025).clamp(-1.2, 1.2);
        self.input.mouse_delta = [0.0, 0.0];

        let forward = [self.yaw_radians.sin(), 0.0, self.yaw_radians.cos()];
        let right = [self.yaw_radians.cos(), 0.0, -self.yaw_radians.sin()];
        let move_speed = if self.input.fast { 6_500.0 } else { 1_800.0 };
        let forward_axis = axis(self.input.forward, self.input.backward);
        let right_axis = axis(self.input.right, self.input.left);
        let up_axis = axis(self.input.up, self.input.down);
        self.camera_position_m[0] +=
            (forward[0] * forward_axis + right[0] * right_axis) * move_speed * dt_s;
        self.camera_position_m[1] += up_axis * move_speed * dt_s;
        self.camera_position_m[2] +=
            (forward[2] * forward_axis + right[2] * right_axis) * move_speed * dt_s;

        let pitch_cos = self.pitch_radians.cos();
        let camera_forward = [
            self.yaw_radians.sin() * pitch_cos,
            self.pitch_radians.sin(),
            self.yaw_radians.cos() * pitch_cos,
        ];
        let camera_right = [self.yaw_radians.cos(), 0.0, -self.yaw_radians.sin()];
        self.sky.time_seconds += dt_s;
        self.sky.camera.position_m = self.camera_position_m;
        self.sky.camera.forward = normalized_or_default(camera_forward, [0.0, 0.0, 1.0]);
        self.sky.camera.up = normalized_or_default(
            cross3(self.sky.camera.forward, camera_right),
            [0.0, 1.0, 0.0],
        );
    }

    fn sky_push_constants(&self, width: u32, height: u32) -> SkyPushConstants {
        let aspect = width.max(1) as f32 / height.max(1) as f32;
        let basis = desktop_camera_basis(&self.sky.camera, aspect);
        let cloud = self.sky.clouds.first();
        let coverage = cloud.map(|cloud| cloud.coverage).unwrap_or(0.46);
        let density = cloud.map(|cloud| cloud.density).unwrap_or(0.48);

        SkyPushConstants {
            camera_forward_tan_x: [
                basis.forward[0],
                basis.forward[1],
                basis.forward[2],
                basis.tan_x,
            ],
            camera_right_tan_y: [basis.right[0], basis.right[1], basis.right[2], basis.tan_y],
            camera_up_time: [basis.up[0], basis.up[1], basis.up[2], self.sky.time_seconds],
            camera_position_seed: [
                self.camera_position_m[0],
                self.camera_position_m[1],
                self.camera_position_m[2],
                self.config.seed as f32,
            ],
            sun_direction_radius: [
                self.sky.sun.direction[0],
                self.sky.sun.direction[1],
                self.sky.sun.direction[2],
                self.sky.sun.angular_radius_degrees.to_radians(),
            ],
            resolution_weather: [width as f32, height as f32, coverage, density],
        }
    }

    fn set_key_state(&mut self, key: KeyCode, state: ElementState) {
        let pressed = state == ElementState::Pressed;
        match key {
            KeyCode::KeyW => self.input.forward = pressed,
            KeyCode::KeyS => self.input.backward = pressed,
            KeyCode::KeyA => self.input.left = pressed,
            KeyCode::KeyD => self.input.right = pressed,
            KeyCode::KeyR | KeyCode::Space => self.input.up = pressed,
            KeyCode::KeyF | KeyCode::ControlLeft | KeyCode::ControlRight => {
                self.input.down = pressed;
            }
            KeyCode::KeyQ | KeyCode::ArrowLeft => self.input.look_left = pressed,
            KeyCode::KeyE | KeyCode::ArrowRight => self.input.look_right = pressed,
            KeyCode::KeyT | KeyCode::ArrowUp => self.input.look_up = pressed,
            KeyCode::KeyG | KeyCode::ArrowDown => self.input.look_down = pressed,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => self.input.fast = pressed,
            _ => {}
        }
    }
}

impl ApplicationHandler for GpuSkyGameApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.initialize_window(event_loop) {
            eprintln!("failed to initialize Ashfall GPU sky game app: {error:?}");
            event_loop.exit();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if Some(window_id) != self.windows.primary_window_id() {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Focused(focused) => {
                if let Some(window) = self.windows.get_primary_window() {
                    configure_game_cursor(window, focused && self.mouse_look_active);
                }
            }
            WindowEvent::Resized(_) => {
                if let Some(renderer) = self.windows.get_primary_renderer_mut() {
                    renderer.resize();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(key) = event.physical_key {
                    if key == KeyCode::Escape && event.state == ElementState::Pressed {
                        if let Some(window) = self.windows.get_primary_window() {
                            configure_game_cursor(window, false);
                        }
                        event_loop.exit();
                    } else {
                        self.set_key_state(key, event.state);
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                self.mouse_look_active = true;
                if let Some(window) = self.windows.get_primary_window() {
                    configure_game_cursor(window, true);
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.render(event_loop) {
                    eprintln!("Ashfall GPU sky render error: {error}");
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event
            && self.mouse_look_active
        {
            self.input.mouse_delta[0] += delta.0 as f32;
            self.input.mouse_delta[1] += delta.1 as f32;
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.windows.get_primary_window() {
            window.request_redraw();
        }
    }
}

fn create_sky_render_pass(
    device: Arc<vulkano::device::Device>,
    format: Format,
) -> anyhow::Result<Arc<RenderPass>> {
    Ok(vulkano::single_pass_renderpass!(
        device,
        attachments: {
            color: {
                format: format,
                samples: 1,
                load_op: Clear,
                store_op: Store,
            },
        },
        pass: {
            color: [color],
            depth_stencil: {},
        },
    )?)
}

fn create_sky_pipeline(
    device: Arc<vulkano::device::Device>,
    render_pass: Arc<RenderPass>,
    image_extent: [u32; 2],
) -> anyhow::Result<Arc<GraphicsPipeline>> {
    let vs = sky_vs::load(device.clone())?
        .entry_point("main")
        .ok_or_else(|| anyhow::anyhow!("missing sky vertex shader entry point"))?;
    let fs = sky_fs::load(device.clone())?
        .entry_point("main")
        .ok_or_else(|| anyhow::anyhow!("missing sky fragment shader entry point"))?;
    let stages = [
        PipelineShaderStageCreateInfo::new(vs.clone()),
        PipelineShaderStageCreateInfo::new(fs),
    ];
    let layout_create_info = PipelineDescriptorSetLayoutCreateInfo::from_stages(&stages)
        .into_pipeline_layout_create_info(device.clone())?;
    let layout = PipelineLayout::new(device.clone(), layout_create_info)?;
    let subpass = vulkano::render_pass::Subpass::from(render_pass, 0)
        .ok_or_else(|| anyhow::anyhow!("missing sky render subpass 0"))?;
    let width = image_extent[0].max(1) as f32;
    let height = image_extent[1].max(1) as f32;
    let mut viewport_state = ViewportState::default();
    viewport_state.viewports[0] = Viewport {
        offset: [0.0, 0.0],
        extent: [width, height],
        depth_range: 0.0..=1.0,
    };

    Ok(GraphicsPipeline::new(
        device,
        None,
        GraphicsPipelineCreateInfo {
            stages: stages.into_iter().collect(),
            vertex_input_state: Some(VertexInputState::default()),
            input_assembly_state: Some(InputAssemblyState::default()),
            viewport_state: Some(viewport_state),
            rasterization_state: Some(RasterizationState::default()),
            multisample_state: Some(MultisampleState::default()),
            color_blend_state: Some(ColorBlendState {
                attachments: vec![ColorBlendAttachmentState {
                    // Full-screen sky/cloud output is opaque; blending can expose stale
                    // swapchain contents as view-dependent transparent tunnels.
                    blend: None,
                    ..Default::default()
                }],
                ..Default::default()
            }),
            subpass: Some(subpass.into()),
            ..GraphicsPipelineCreateInfo::layout(layout)
        },
    )?)
}

fn build_sky_framebuffers(
    render_pass: Arc<RenderPass>,
    image_views: &[Arc<ImageView>],
) -> anyhow::Result<Vec<Arc<Framebuffer>>> {
    image_views
        .iter()
        .map(|image_view| {
            Ok(Framebuffer::new(
                render_pass.clone(),
                FramebufferCreateInfo {
                    attachments: vec![image_view.clone()],
                    ..Default::default()
                },
            )?)
        })
        .collect()
}

mod sky_vs {
    vulkano_shaders::shader! {
        ty: "vertex",
        src: r#"
            #version 450

            layout(location = 0) out vec2 out_uv;

            void main() {
                vec2 positions[3] = vec2[](
                    vec2(-1.0, -1.0),
                    vec2( 3.0, -1.0),
                    vec2(-1.0,  3.0)
                );
                vec2 position = positions[gl_VertexIndex];
                out_uv = position * 0.5 + 0.5;
                gl_Position = vec4(position, 0.0, 1.0);
            }
        "#,
    }
}

mod sky_fs {
    vulkano_shaders::shader! {
        ty: "fragment",
        src: r#"
#version 450

layout(location = 0) in vec2 in_uv;
layout(location = 0) out vec4 out_color;

layout(push_constant) uniform SkyPushConstants {
    vec4 camera_forward_tan_x;
    vec4 camera_right_tan_y;
    vec4 camera_up_time;
    vec4 camera_position_seed;
    vec4 sun_direction_radius;
    vec4 resolution_weather;
} pc;

const float PI = 3.14159265358979323846;
const int PRIMARY_STEPS = 64;
const int SHADOW_STEPS = 5;
const float CLOUD_BASE_M = 1150.0;
const float CLOUD_TOP_M = 4550.0;
const float CLOUD_LAYER_THICKNESS_M = CLOUD_TOP_M - CLOUD_BASE_M;
const float CLOUD_MAX_DISTANCE_M = 82000.0;
const float MIN_TRANSMITTANCE = 0.012;

float saturate(float value) {
    return clamp(value, 0.0, 1.0);
}

vec3 saturate3(vec3 value) {
    return clamp(value, vec3(0.0), vec3(1.0));
}

float remap01(float value, float low, float high) {
    return saturate((value - low) / max(high - low, 0.00001));
}

float hash11(float value) {
    return fract(sin(value * 127.1 + 311.7) * 43758.5453123);
}

float hash12(vec2 value) {
    vec3 p3 = fract(vec3(value.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

float hash13(vec3 value) {
    return fract(sin(dot(value, vec3(127.1, 311.7, 74.7))) * 43758.5453123);
}

vec2 hash22(vec2 value) {
    vec3 p3 = fract(vec3(value.xyx) * vec3(0.1031, 0.11369, 0.13787));
    p3 += dot(p3, p3.yzx + 19.19);
    return fract(vec2((p3.x + p3.y) * p3.z, (p3.x + p3.z) * p3.y));
}

vec3 hash33(vec3 value) {
    return fract(sin(vec3(
        dot(value, vec3(127.1, 311.7,  74.7)),
        dot(value, vec3(269.5, 183.3, 246.1)),
        dot(value, vec3(113.5, 271.9, 124.6))
    )) * 43758.5453123);
}

float value_noise2(vec2 value) {
    vec2 cell = floor(value);
    vec2 local = fract(value);
    vec2 smooth_local = local * local * (3.0 - 2.0 * local);

    float a = hash12(cell + vec2(0.0, 0.0));
    float b = hash12(cell + vec2(1.0, 0.0));
    float c = hash12(cell + vec2(0.0, 1.0));
    float d = hash12(cell + vec2(1.0, 1.0));
    return mix(mix(a, b, smooth_local.x), mix(c, d, smooth_local.x), smooth_local.y);
}

float value_noise3(vec3 value) {
    vec3 cell = floor(value);
    vec3 local = fract(value);
    vec3 smooth_local = local * local * (3.0 - 2.0 * local);

    float c000 = hash13(cell + vec3(0.0, 0.0, 0.0));
    float c100 = hash13(cell + vec3(1.0, 0.0, 0.0));
    float c010 = hash13(cell + vec3(0.0, 1.0, 0.0));
    float c110 = hash13(cell + vec3(1.0, 1.0, 0.0));
    float c001 = hash13(cell + vec3(0.0, 0.0, 1.0));
    float c101 = hash13(cell + vec3(1.0, 0.0, 1.0));
    float c011 = hash13(cell + vec3(0.0, 1.0, 1.0));
    float c111 = hash13(cell + vec3(1.0, 1.0, 1.0));

    float x00 = mix(c000, c100, smooth_local.x);
    float x10 = mix(c010, c110, smooth_local.x);
    float x01 = mix(c001, c101, smooth_local.x);
    float x11 = mix(c011, c111, smooth_local.x);
    float y0 = mix(x00, x10, smooth_local.y);
    float y1 = mix(x01, x11, smooth_local.y);
    return mix(y0, y1, smooth_local.z);
}

float fbm2(vec2 value) {
    float sum = 0.0;
    float amplitude = 0.5;
    float norm = 0.0;
    mat2 octave = mat2(1.61, 1.08, -1.08, 1.61);
    for (int i = 0; i < 5; i++) {
        sum += value_noise2(value) * amplitude;
        norm += amplitude;
        value = octave * value + vec2(13.17, 7.31);
        amplitude *= 0.54;
    }
    return sum / max(norm, 0.00001);
}

float fbm3(vec3 value) {
    float sum = 0.0;
    float amplitude = 0.5;
    float norm = 0.0;
    for (int i = 0; i < 5; i++) {
        sum += value_noise3(value) * amplitude;
        norm += amplitude;
        value = value * 2.03 + vec3(17.13, 7.71, 31.41);
        amplitude *= 0.53;
    }
    return sum / max(norm, 0.00001);
}

float worley2(vec2 value) {
    vec2 cell = floor(value);
    vec2 local = fract(value);
    float closest = 10.0;

    for (int y = -1; y <= 1; y++) {
        for (int x = -1; x <= 1; x++) {
            vec2 offset = vec2(float(x), float(y));
            vec2 feature = offset + hash22(cell + offset) - local;
            closest = min(closest, dot(feature, feature));
        }
    }

    return sqrt(closest);
}

float worley3(vec3 value) {
    vec3 cell = floor(value);
    vec3 local = fract(value);
    float closest = 10.0;

    for (int z = -1; z <= 1; z++) {
        for (int y = -1; y <= 1; y++) {
            for (int x = -1; x <= 1; x++) {
                vec3 offset = vec3(float(x), float(y), float(z));
                vec3 feature = offset + hash33(cell + offset) - local;
                closest = min(closest, dot(feature, feature));
            }
        }
    }

    return sqrt(closest);
}

vec3 aces(vec3 color) {
    const float a = 2.51;
    const float b = 0.03;
    const float c = 2.43;
    const float d = 0.59;
    const float e = 0.14;
    return saturate3((color * (a * color + b)) / (color * (c * color + d) + e));
}

float henyey_greenstein(float cos_theta, float g) {
    float g2 = g * g;
    float denom = pow(max(1.0 + g2 - 2.0 * g * cos_theta, 0.0001), 1.5);
    return (1.0 - g2) / (4.0 * PI * denom);
}

float interleaved_gradient_noise(vec2 pixel, float frameish) {
    return fract(52.9829189 * fract(0.06711056 * pixel.x + 0.00583715 * pixel.y + frameish * 0.071));
}

bool ray_cloud_layer_interval(vec3 origin, vec3 direction, out float t0, out float t1) {
    if (abs(direction.y) < 0.0001) {
        if (origin.y < CLOUD_BASE_M || origin.y > CLOUD_TOP_M) {
            t0 = 0.0;
            t1 = 0.0;
            return false;
        }
        t0 = 0.0;
        t1 = CLOUD_MAX_DISTANCE_M;
        return true;
    }

    float t_base = (CLOUD_BASE_M - origin.y) / direction.y;
    float t_top = (CLOUD_TOP_M - origin.y) / direction.y;
    float near_t = min(t_base, t_top);
    float far_t = max(t_base, t_top);

    t0 = max(near_t, 0.0);
    t1 = min(far_t, CLOUD_MAX_DISTANCE_M);
    return t1 > t0;
}

vec2 wind_offset_m(float time_seconds, float seed) {
    float seed_angle = mix(-0.45, 0.45, hash11(seed + 2.7));
    vec2 direction = normalize(vec2(cos(seed_angle), sin(seed_angle)) + vec2(0.85, 0.28));
    return direction * time_seconds * 28.0;
}

float cloud_height01(vec3 world_position) {
    return saturate((world_position.y - CLOUD_BASE_M) / CLOUD_LAYER_THICKNESS_M);
}

float vertical_density_profile(float h) {
    // Real cumulus clouds tend to have a comparatively flat condensation base and a
    // softer, boiling top.  This profile gives weight to the body while keeping the
    // base planar instead of spherical.
    float flat_base = smoothstep(0.015, 0.085, h);
    float soft_top = 1.0 - smoothstep(0.72, 1.00, h);
    float rising_tower = smoothstep(0.18, 0.58, h) * (1.0 - smoothstep(0.78, 1.00, h));
    float base_body = mix(0.70, 1.20, rising_tower);
    return flat_base * soft_top * base_body;
}

float cloud_coverage_field(vec2 world_xz, float time_seconds, float seed) {
    vec2 wind = wind_offset_m(time_seconds, seed);
    vec2 seed_offset = vec2(hash11(seed + 10.0), hash11(seed + 37.0)) * 10000.0;
    vec2 p = (world_xz + wind + seed_offset) * 0.000085;

    float large_weather = fbm2(p * 0.72 + vec2(seed * 0.011, -seed * 0.017));
    float cell_breakup = 1.0 - saturate(worley2(p * 1.38 + vec2(seed * 0.021, 4.3)) * 1.06);
    float cloud_streets = fbm2(vec2(p.x * 2.20 + p.y * 0.42, p.y * 0.66 - p.x * 0.19));
    float local_variation = fbm2(p * 3.40 + vec2(11.0, seed * 0.013));

    float field = large_weather * 0.56 + cell_breakup * 0.25 + cloud_streets * 0.12 + local_variation * 0.07;
    float requested_coverage = saturate(pc.resolution_weather.z);
    float threshold = mix(0.62, 0.37, requested_coverage);
    float mask = remap01(field, threshold, 0.96);
    return smoothstep(0.0, 1.0, mask);
}

float raw_cloud_density(vec3 world_position) {
    float h = cloud_height01(world_position);
    if (h <= 0.0 || h >= 1.0) {
        return 0.0;
    }

    float time_seconds = pc.camera_up_time.w;
    float seed = pc.camera_position_seed.w;
    float density_setting = saturate(pc.resolution_weather.w);

    vec2 wind = wind_offset_m(time_seconds, seed);
    float coverage_mask = cloud_coverage_field(world_position.xz, time_seconds, seed);
    if (coverage_mask <= 0.001) {
        return 0.0;
    }

    float profile = vertical_density_profile(h);
    vec2 low_warp = vec2(
        fbm2(world_position.xz * 0.00019 + vec2(seed * 0.017, time_seconds * 0.012)),
        fbm2(world_position.zx * 0.00022 + vec2(-time_seconds * 0.014, seed * 0.023))
    ) - 0.5;

    vec3 q = vec3(
        (world_position.x + wind.x + low_warp.x * 1800.0) * 0.00038,
        (world_position.y - CLOUD_BASE_M) * 0.00062,
        (world_position.z + wind.y + low_warp.y * 1800.0) * 0.00038
    );
    q += vec3(seed * 0.013, 0.0, seed * 0.019);
    q.x += h * 0.38;
    q.z += h * 0.19;

    float billow_low = fbm3(q * 2.10 + vec3(3.7, time_seconds * 0.018, seed * 0.011));
    float billow_mid = fbm3(q * 4.60 + vec3(seed * 0.029, -time_seconds * 0.026, 5.1));
    float fine_noise = fbm3(q * 9.80 + vec3(-time_seconds * 0.044, 8.2, seed * 0.031));

    float primary_shape = coverage_mask * profile;
    primary_shape += (billow_low - 0.46) * 0.42 * profile;
    primary_shape += (billow_mid - 0.50) * 0.19 * profile;

    float density = remap01(primary_shape, 0.12, 0.88);

    // Cell-like erosion removes the old shiny/spherical look.  The erosion is stronger
    // near boundaries and near the top, producing cauliflower towers and wispy breakup.
    float edge = 1.0 - smoothstep(0.24, 0.82, density);
    float top_evaporation = smoothstep(0.56, 0.98, h);
    float low_cells = worley3(q * 3.55 + vec3(seed * 0.031, 2.0, time_seconds * 0.013));
    float high_cells = worley3(q * 8.20 + vec3(8.0, seed * 0.043, -time_seconds * 0.031));
    float cellular_erosion = smoothstep(0.18, 0.72, low_cells) * 0.34;
    cellular_erosion += smoothstep(0.12, 0.64, high_cells) * mix(0.16, 0.31, top_evaporation);
    density -= cellular_erosion * edge;

    // Flat, heavy underside plus soft high-detail rim texture.
    float underside = 1.0 - smoothstep(0.035, 0.20, h);
    density += underside * coverage_mask * 0.12;
    density += (fine_noise - 0.52) * 0.075 * edge * (1.0 - underside * 0.35);

    // Let the requested density control thickness, not specular brightness.
    density *= mix(0.72, 1.46, density_setting);
    return saturate(density);
}

float cloud_shadow_transmittance(vec3 world_position, vec3 sun_direction) {
    float t0;
    float t1;
    if (!ray_cloud_layer_interval(world_position + sun_direction * 35.0, sun_direction, t0, t1)) {
        return 1.0;
    }

    float max_distance = min(t1, 12500.0);
    float step_length = max_distance / float(SHADOW_STEPS);
    float t = step_length * 0.62;
    float optical_depth = 0.0;

    for (int i = 0; i < SHADOW_STEPS; i++) {
        vec3 p = world_position + sun_direction * t;
        float shadow_density = raw_cloud_density(p);
        optical_depth += shadow_density * step_length;
        t += step_length;
    }

    float density_setting = saturate(pc.resolution_weather.w);
    float extinction = mix(0.00042, 0.00105, density_setting);
    return exp(-optical_depth * extinction * 1.30);
}

vec3 sky_radiance(vec3 direction, vec3 sun_direction) {
    float horizon_amount = pow(1.0 - saturate(direction.y * 0.5 + 0.5), 2.2);
    float sky_up = saturate(direction.y * 0.5 + 0.5);
    vec3 zenith = vec3(0.18, 0.39, 0.78);
    vec3 horizon = vec3(0.66, 0.75, 0.88);
    vec3 lower_haze = vec3(0.84, 0.82, 0.75);

    vec3 sky = mix(horizon, zenith, pow(sky_up, 0.74));
    sky = mix(sky, lower_haze, horizon_amount * 0.34);

    float mu = saturate(dot(direction, sun_direction));
    float sun_radius = max(pc.sun_direction_radius.w, 0.0042);
    float sun_disk = smoothstep(cos(sun_radius * 1.85), cos(sun_radius * 0.42), mu);
    float mie_haze = pow(mu, 22.0);
    float wide_glow = pow(mu, 3.2);

    vec3 sun_color = vec3(1.00, 0.83, 0.55);
    sky += sun_color * sun_disk * 9.0;
    sky += sun_color * mie_haze * 0.32;
    sky += vec3(1.00, 0.68, 0.34) * wide_glow * 0.075;

    if (direction.y < 0.0) {
        float ground = saturate(-direction.y * 3.5);
        vec3 ground_haze = mix(vec3(0.54, 0.60, 0.62), vec3(0.20, 0.23, 0.22), ground);
        sky = mix(sky, ground_haze, saturate(ground * 0.80));
    }

    float sun_height = saturate(sun_direction.y * 0.5 + 0.5);
    sky *= mix(0.78, 1.06, sun_height);
    return sky;
}

vec3 cloud_lighting(vec3 world_position, vec3 view_to_camera, vec3 sun_direction, float density, float distance_m) {
    float h = cloud_height01(world_position);
    float shadow = cloud_shadow_transmittance(world_position, sun_direction);
    float cos_theta = dot(-view_to_camera, sun_direction);

    // A matte cloud is dominated by volumetric scattering, not glossy reflection.
    // Use broad forward scatter, a small backward term, blue skylight, and a powder
    // response at soft edges to create white bodies and warm silver linings.
    float forward_phase = henyey_greenstein(cos_theta, 0.58) * 1.55;
    float wide_phase = henyey_greenstein(cos_theta, 0.18) * 0.80;
    float backward_phase = henyey_greenstein(cos_theta, -0.26) * 0.34;
    float phase = 0.50 + forward_phase + wide_phase + backward_phase;

    float powder = 1.0 - exp(-density * 5.2);
    float soft_edge = pow(saturate(1.0 - density), 2.4);
    float silver_lining = soft_edge * pow(saturate(cos_theta), 2.6) * shadow;

    vec3 sun_color = vec3(1.00, 0.90, 0.74);
    vec3 zenith_ambient = vec3(0.54, 0.66, 0.86);
    vec3 horizon_ambient = vec3(0.74, 0.77, 0.78);
    vec3 underside_ambient = vec3(0.32, 0.36, 0.42);

    vec3 ambient = mix(underside_ambient, mix(horizon_ambient, zenith_ambient, h), smoothstep(0.10, 0.70, h));
    float ambient_occlusion = mix(0.54, 1.0, shadow) * mix(0.78, 1.10, h);
    vec3 lighting = ambient * ambient_occlusion * 0.48;
    lighting += sun_color * shadow * phase * (0.28 + powder * 0.58);
    lighting += sun_color * silver_lining * 0.48;

    float aerial = saturate(1.0 - exp(-distance_m * 0.000035));
    vec3 air_color = sky_radiance(normalize(world_position - pc.camera_position_seed.xyz), sun_direction);
    lighting = mix(lighting, air_color * 0.92, aerial * 0.42);

    return lighting * vec3(0.96, 0.97, 0.95);
}

void main() {
    vec2 ndc = vec2(in_uv.x * 2.0 - 1.0, (1.0 - in_uv.y) * 2.0 - 1.0);

    vec3 camera_forward = normalize(pc.camera_forward_tan_x.xyz);
    vec3 camera_right = normalize(pc.camera_right_tan_y.xyz);
    vec3 camera_up = normalize(pc.camera_up_time.xyz);
    float tan_x = pc.camera_forward_tan_x.w;
    float tan_y = pc.camera_right_tan_y.w;
    vec3 origin = pc.camera_position_seed.xyz;
    vec3 sun_direction = normalize(pc.sun_direction_radius.xyz);

    vec3 ray_direction = normalize(camera_forward + camera_right * ndc.x * tan_x + camera_up * ndc.y * tan_y);
    vec3 background = sky_radiance(ray_direction, sun_direction);
    vec3 color = background;

    float t0;
    float t1;
    if (ray_cloud_layer_interval(origin, ray_direction, t0, t1)) {
        float segment = max(t1 - t0, 1.0);
        float step_length = segment / float(PRIMARY_STEPS);
        float jitter = interleaved_gradient_noise(gl_FragCoord.xy, pc.camera_up_time.w);
        float t = t0 + step_length * jitter;

        vec3 accumulated = vec3(0.0);
        float transmittance = 1.0;
        float density_setting = saturate(pc.resolution_weather.w);
        float extinction = mix(0.00042, 0.00105, density_setting);

        for (int i = 0; i < PRIMARY_STEPS; i++) {
            if (t > t1) {
                break;
            }

            vec3 p = origin + ray_direction * t;
            float density = raw_cloud_density(p);

            if (density > 0.0015) {
                float optical_depth = density * extinction * step_length;
                float alpha = 1.0 - exp(-optical_depth);
                vec3 lit_cloud = cloud_lighting(p, -ray_direction, sun_direction, density, t);
                accumulated += transmittance * alpha * lit_cloud;
                transmittance *= exp(-optical_depth);

                if (transmittance < MIN_TRANSMITTANCE) {
                    break;
                }
            }

            t += step_length;
        }

        color = accumulated + background * transmittance;
    }

    color = aces(color);
    color = pow(color, vec3(1.0 / 2.2));
    out_color = vec4(saturate3(color), 1.0);
}
"#,
    }
}

impl DesktopApp {
    pub fn new(config: DesktopAppConfig) -> Self {
        let sky = desktop_sky_scene(config.seed);
        Self {
            state: DesktopAppState::Uninitialized,
            runtime: EngineRuntime::new(RenderQualityProfile::Smoke),
            config,
            window: None,
            presenter: None,
            window_id: None,
            gpu: None,
            sky,
            sky_settings: SkyRenderSettings::balanced(),
            judge: BurnCloudPhotorealismJudge::default(),
            input: MovementInput::default(),
            camera_position_m: [0.0, 0.0, 0.0],
            yaw_radians: 0.0,
            pitch_radians: 0.0,
            mouse_look_active: false,
            last_cursor_position: None,
            render_scale: 0.20,
            last_frame_fps: 0.0,
            last_redraw: Instant::now(),
            frame_count: 0,
        }
    }

    pub fn presentation_state(&self) -> PresentationState {
        match self.state {
            DesktopAppState::WindowReady(_)
            | DesktopAppState::SurfaceReady(_)
            | DesktopAppState::SwapchainReady(_) => PresentationState::Available,
            DesktopAppState::ResizePending { .. } => PresentationState::ResizePending,
            DesktopAppState::Minimized => PresentationState::Minimized,
            DesktopAppState::SurfaceLost => PresentationState::SurfaceLost,
            DesktopAppState::DeviceLost => PresentationState::DeviceLost,
            DesktopAppState::Uninitialized
            | DesktopAppState::Suspended
            | DesktopAppState::ShuttingDown => PresentationState::Unavailable,
        }
    }

    fn initialize_window(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        if self.window.is_some() {
            return Ok(());
        }

        let attributes = Window::default_attributes()
            .with_title(self.config.title.clone())
            .with_inner_size(LogicalSize::new(
                f64::from(self.config.width),
                f64::from(self.config.height),
            ));
        let window = Arc::new(event_loop.create_window(attributes)?);
        self.window_id = Some(window.id());
        self.state = DesktopAppState::SurfaceReady(PresentationSurfaceId(1));
        self.presenter = Some(WindowPresenter::new(window.clone())?);
        self.gpu = Some(
            try_collect_gpu_capabilities(VulkanBootstrapConfig {
                print_device_name: self.config.print_device_name,
                ..VulkanBootstrapConfig::default()
            })
            .unwrap_or_else(|error| GpuCapabilities::unavailable(error.to_string())),
        );
        window.request_redraw();
        self.window = Some(window);
        Ok(())
    }

    fn redraw(&mut self) {
        let now = Instant::now();
        let dt_s = now
            .duration_since(self.last_redraw)
            .as_secs_f32()
            .clamp(1.0 / 240.0, 1.0 / 20.0);
        self.last_redraw = now;
        let snapshot = self.runtime.advance_frame(dt_s);
        self.update_camera(dt_s, snapshot.sim_time_s as f32);
        self.frame_count += 1;
        let metrics = self.draw_sky().unwrap_or_else(|error| {
            eprintln!("failed to draw Ashfall sky preview: {error}");
            self.state = DesktopAppState::SurfaceLost;
            None
        });
        if let Some(metrics) = &metrics {
            self.last_frame_fps = metrics.frame_fps;
            self.tune_render_scale(metrics.frame_fps);
        }

        if let Some(window) = &self.window {
            let gpu_name = self
                .gpu
                .as_ref()
                .map(|gpu| gpu.device_name.as_str())
                .unwrap_or("gpu unknown");
            if let Some(metrics) = metrics {
                let report = self.judge.judge(&metrics.sky);
                window.set_title(&format!(
                    "{} | frame {} | {:.1} FPS | Burn {:.3} {} | {}x{} | {}",
                    self.config.title,
                    snapshot.frame_index,
                    metrics.frame_fps,
                    report.score,
                    if report.pass { "pass" } else { "fail" },
                    metrics.render_width,
                    metrics.render_height,
                    gpu_name
                ));
            } else {
                window.set_title(&format!(
                    "{} | frame {} | waiting for surface | {}",
                    self.config.title, snapshot.frame_index, gpu_name
                ));
            }
        }
    }

    fn update_camera(&mut self, dt_s: f32, time_seconds: f32) {
        let look_speed = 1.35;
        self.yaw_radians += axis(self.input.look_right, self.input.look_left) * look_speed * dt_s;
        self.pitch_radians = (self.pitch_radians
            + axis(self.input.look_up, self.input.look_down) * look_speed * dt_s)
            .clamp(-1.2, 1.2);
        self.yaw_radians += self.input.mouse_delta[0] * 0.0025;
        self.pitch_radians =
            (self.pitch_radians - self.input.mouse_delta[1] * 0.0025).clamp(-1.2, 1.2);
        self.input.mouse_delta = [0.0, 0.0];

        let forward = [self.yaw_radians.sin(), 0.0, self.yaw_radians.cos()];
        let right = [self.yaw_radians.cos(), 0.0, -self.yaw_radians.sin()];
        let move_speed = if self.input.fast { 6_500.0 } else { 1_800.0 };
        let forward_axis = axis(self.input.forward, self.input.backward);
        let right_axis = axis(self.input.right, self.input.left);
        let up_axis = axis(self.input.up, self.input.down);
        self.camera_position_m[0] +=
            (forward[0] * forward_axis + right[0] * right_axis) * move_speed * dt_s;
        self.camera_position_m[1] += up_axis * move_speed * dt_s;
        self.camera_position_m[2] +=
            (forward[2] * forward_axis + right[2] * right_axis) * move_speed * dt_s;

        let pitch_cos = self.pitch_radians.cos();
        let camera_forward = [
            self.yaw_radians.sin() * pitch_cos,
            self.pitch_radians.sin(),
            self.yaw_radians.cos() * pitch_cos,
        ];
        let camera_right = [self.yaw_radians.cos(), 0.0, -self.yaw_radians.sin()];
        self.sky.time_seconds = time_seconds;
        self.sky.camera.position_m = self.camera_position_m;
        self.sky.camera.forward = normalized_or_default(camera_forward, [0.0, 0.0, 1.0]);
        self.sky.camera.up = normalized_or_default(
            cross3(self.sky.camera.forward, camera_right),
            [0.0, 1.0, 0.0],
        );
    }

    fn draw_sky(&mut self) -> anyhow::Result<Option<DesktopFrameMetrics>> {
        let Some(window) = &self.window else {
            return Ok(None);
        };
        let Some(presenter) = &mut self.presenter else {
            return Ok(None);
        };
        let size = window.inner_size();
        let Some(width) = NonZeroU32::new(size.width) else {
            return Ok(None);
        };
        let Some(height) = NonZeroU32::new(size.height) else {
            return Ok(None);
        };

        let frame_start = Instant::now();
        presenter.ensure_size(width, height)?;
        let mut settings = self.sky_settings;
        let [render_width, render_height] =
            desktop_render_extent(size.width, size.height, self.render_scale);
        settings.width = render_width;
        settings.height = render_height;
        settings.noise_octaves = 4;
        settings.raymarch_steps = 3;
        settings.detail_strength = 0.82;
        let capture = render_sky_capture(&self.sky, settings);
        let mut buffer = presenter
            .surface
            .buffer_mut()
            .map_err(|error| anyhow::anyhow!("failed to acquire window pixel buffer: {error}"))?;
        blit_scaled_rgba_bilinear(
            &capture.rgba,
            capture.metrics.width,
            capture.metrics.height,
            size.width,
            size.height,
            &mut buffer,
        );
        draw_crisp_sun(
            &mut buffer,
            size.width,
            size.height,
            self.sky.sun.direction,
            &self.sky.camera,
            self.sky.sun.angular_radius_degrees,
        );
        buffer
            .present()
            .map_err(|error| anyhow::anyhow!("failed to present window pixel buffer: {error}"))?;
        let frame_ms = frame_start.elapsed().as_secs_f32() * 1000.0;
        let frame_fps = fps_from_ms(frame_ms);

        Ok(Some(DesktopFrameMetrics {
            sky: capture.metrics,
            frame_fps,
            render_width,
            render_height,
        }))
    }

    fn tune_render_scale(&mut self, frame_fps: f32) {
        if frame_fps < 80.0 {
            self.render_scale = (self.render_scale * 0.82).max(0.12);
        } else if frame_fps > 112.0 {
            self.render_scale = (self.render_scale * 1.03).min(0.50);
        }
    }

    fn handle_resize(&mut self, width: u32, height: u32) {
        self.state = if width == 0 || height == 0 {
            DesktopAppState::Minimized
        } else {
            DesktopAppState::ResizePending { width, height }
        };
    }

    fn handle_key(&mut self, physical_key: PhysicalKey, state: ElementState) {
        let pressed = state == ElementState::Pressed;
        let PhysicalKey::Code(code) = physical_key else {
            return;
        };

        match code {
            KeyCode::KeyW => self.input.forward = pressed,
            KeyCode::KeyS => self.input.backward = pressed,
            KeyCode::KeyA => self.input.left = pressed,
            KeyCode::KeyD => self.input.right = pressed,
            KeyCode::KeyR | KeyCode::Space => self.input.up = pressed,
            KeyCode::KeyF | KeyCode::ControlLeft | KeyCode::ControlRight => {
                self.input.down = pressed;
            }
            KeyCode::KeyQ | KeyCode::ArrowLeft => self.input.look_left = pressed,
            KeyCode::KeyE | KeyCode::ArrowRight => self.input.look_right = pressed,
            KeyCode::KeyT | KeyCode::ArrowUp => self.input.look_up = pressed,
            KeyCode::KeyG | KeyCode::ArrowDown => self.input.look_down = pressed,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => self.input.fast = pressed,
            _ => {}
        }
    }

    fn handle_mouse_button(&mut self, button: MouseButton, state: ElementState) {
        if button == MouseButton::Left {
            if let Some(window) = &self.window {
                configure_game_cursor(window, state == ElementState::Pressed);
            }
            self.mouse_look_active = state == ElementState::Pressed;
            self.last_cursor_position = None;
        }
    }

    fn handle_cursor_move(&mut self, position: PhysicalPosition<f64>) {
        if self.mouse_look_active
            && let Some(previous) = self.last_cursor_position
        {
            let dx = (position.x - previous.x) as f32;
            let dy = (position.y - previous.y) as f32;
            let sensitivity = 0.004;
            self.yaw_radians += dx * sensitivity;
            self.pitch_radians = (self.pitch_radians - dy * sensitivity).clamp(-1.2, 1.2);
        }
        self.last_cursor_position = Some(position);
    }

    fn handle_mouse_motion(&mut self, delta: (f64, f64)) {
        if self.mouse_look_active {
            self.input.mouse_delta[0] += delta.0 as f32;
            self.input.mouse_delta[1] += delta.1 as f32;
        }
    }
}

impl WindowPresenter {
    fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let context = softbuffer::Context::new(window.clone())
            .map_err(|error| anyhow::anyhow!("failed to create window pixel context: {error}"))?;
        let surface = softbuffer::Surface::new(&context, window)
            .map_err(|error| anyhow::anyhow!("failed to create window pixel surface: {error}"))?;

        Ok(Self {
            _context: context,
            surface,
            size: None,
        })
    }

    fn ensure_size(&mut self, width: NonZeroU32, height: NonZeroU32) -> anyhow::Result<()> {
        let new_size = [width, height];
        if self.size == Some(new_size) {
            return Ok(());
        }

        self.surface
            .resize(width, height)
            .map_err(|error| anyhow::anyhow!("failed to resize window pixel surface: {error}"))?;
        self.size = Some(new_size);
        Ok(())
    }
}

fn configure_game_cursor(window: &Window, enabled: bool) {
    if enabled {
        let _ = window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
        window.set_cursor_visible(false);
    } else {
        let _ = window.set_cursor_grab(CursorGrabMode::None);
        window.set_cursor_visible(true);
    }
}

fn desktop_sky_scene(seed: u64) -> SkySceneRequest {
    let mut sky = SkySceneRequest::cloudy_smoke(seed);
    sky.camera.fov_y_degrees = 72.0;
    sky.sun.direction = [0.46, 0.36, 0.82];
    sky.sun.angular_radius_degrees = 0.62;
    if let Some(cloud) = sky.clouds.first_mut() {
        cloud.coverage = 0.46;
        cloud.density = 0.48;
        cloud.thickness_m = 1_250.0;
    }
    sky
}

fn desktop_render_extent(window_width: u32, window_height: u32, scale: f32) -> [u32; 2] {
    let aspect = window_width.max(1) as f32 / window_height.max(1) as f32;
    let max_height = 288.0;
    let scaled_height = (window_height as f32 * scale.clamp(0.12, 0.50)).min(max_height);
    let height = scaled_height.round().max(120.0) as u32;
    let width = (height as f32 * aspect).round().max(320.0) as u32;

    [width.max(1), height.max(1)]
}

fn fps_from_ms(frame_ms: f32) -> f32 {
    if frame_ms > f32::EPSILON {
        1000.0 / frame_ms
    } else {
        0.0
    }
}

fn blit_scaled_rgba_bilinear(
    source_rgba: &[u8],
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
    target: &mut [u32],
) {
    if source_width == 0 || source_height == 0 || target_width == 0 || target_height == 0 {
        return;
    }

    let source_width = source_width as usize;
    let source_height = source_height as usize;
    let target_width = target_width as usize;
    let target_height = target_height as usize;
    let copied_len = target.len().min(target_width * target_height);
    for (index, pixel) in target.iter_mut().enumerate().take(copied_len) {
        let x = index % target_width;
        let y = index / target_width;
        let source_x = if target_width > 1 {
            x as f32 * (source_width - 1) as f32 / (target_width - 1) as f32
        } else {
            0.0
        };
        let source_y = if target_height > 1 {
            y as f32 * (source_height - 1) as f32 / (target_height - 1) as f32
        } else {
            0.0
        };
        *pixel = sample_rgba_bilinear(source_rgba, source_width, source_height, source_x, source_y);
    }
}

fn sample_rgba_bilinear(
    source_rgba: &[u8],
    source_width: usize,
    source_height: usize,
    x: f32,
    y: f32,
) -> u32 {
    let x0 = x.floor().clamp(0.0, (source_width - 1) as f32) as usize;
    let y0 = y.floor().clamp(0.0, (source_height - 1) as f32) as usize;
    let x1 = (x0 + 1).min(source_width - 1);
    let y1 = (y0 + 1).min(source_height - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let c00 = read_rgb(source_rgba, source_width, x0, y0);
    let c10 = read_rgb(source_rgba, source_width, x1, y0);
    let c01 = read_rgb(source_rgba, source_width, x0, y1);
    let c11 = read_rgb(source_rgba, source_width, x1, y1);
    let top = [
        lerp(c00[0], c10[0], tx),
        lerp(c00[1], c10[1], tx),
        lerp(c00[2], c10[2], tx),
    ];
    let bottom = [
        lerp(c01[0], c11[0], tx),
        lerp(c01[1], c11[1], tx),
        lerp(c01[2], c11[2], tx),
    ];
    let red = lerp(top[0], bottom[0], ty).round().clamp(0.0, 255.0) as u32;
    let green = lerp(top[1], bottom[1], ty).round().clamp(0.0, 255.0) as u32;
    let blue = lerp(top[2], bottom[2], ty).round().clamp(0.0, 255.0) as u32;

    (red << 16) | (green << 8) | blue
}

fn read_rgb(source_rgba: &[u8], source_width: usize, x: usize, y: usize) -> [f32; 3] {
    let index = (y * source_width + x) * 4;
    if index + 2 >= source_rgba.len() {
        return [0.0, 0.0, 0.0];
    }

    [
        f32::from(source_rgba[index]),
        f32::from(source_rgba[index + 1]),
        f32::from(source_rgba[index + 2]),
    ]
}

fn draw_crisp_sun(
    target: &mut [u32],
    width: u32,
    height: u32,
    sun_direction: [f32; 3],
    camera: &CameraState,
    angular_radius_degrees: f32,
) {
    if width == 0 || height == 0 {
        return;
    }
    let aspect = width as f32 / height as f32;
    let basis = desktop_camera_basis(camera, aspect);
    let sun = normalized_or_default(sun_direction, [0.16, 0.34, 0.93]);
    let view_z = dot3(sun, basis.forward);
    if view_z <= 0.01 {
        return;
    }
    let screen = [
        0.5 + dot3(sun, basis.right) / (view_z * basis.tan_x * 2.0),
        0.5 - dot3(sun, basis.up) / (view_z * basis.tan_y * 2.0),
    ];
    if screen[0] < -0.18 || screen[0] > 1.18 || screen[1] < -0.18 || screen[1] > 1.18 {
        return;
    }

    let center_x = screen[0] * width as f32;
    let center_y = screen[1] * height as f32;
    let disc_radius = (angular_radius_degrees.to_radians() / basis.fov_y_radians * height as f32)
        .clamp(5.0, 16.0);
    let glow_radius = disc_radius * 12.0;
    let min_x = (center_x - glow_radius).floor().max(0.0) as u32;
    let max_x = (center_x + glow_radius)
        .ceil()
        .min(width.saturating_sub(1) as f32) as u32;
    let min_y = (center_y - glow_radius).floor().max(0.0) as u32;
    let max_y = (center_y + glow_radius)
        .ceil()
        .min(height.saturating_sub(1) as f32) as u32;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as f32 + 0.5 - center_x;
            let dy = y as f32 + 0.5 - center_y;
            let distance = (dx * dx + dy * dy).sqrt();
            let disc = 1.0 - smoothstep(disc_radius * 0.78, disc_radius * 1.15, distance);
            let glow = (1.0 - smoothstep(disc_radius * 1.2, glow_radius, distance)).powf(2.4);
            let strength = (disc * 0.95 + glow * 0.16).clamp(0.0, 1.0);
            if strength <= 0.001 {
                continue;
            }
            let index = (y * width + x) as usize;
            if index >= target.len() {
                continue;
            }
            target[index] = blend_rgb(target[index], [255.0, 245.0, 218.0], strength);
        }
    }
}

fn blend_rgb(pixel: u32, color: [f32; 3], alpha: f32) -> u32 {
    let alpha = alpha.clamp(0.0, 1.0);
    let inv = 1.0 - alpha;
    let red = (((pixel >> 16) & 0xff) as f32 * inv + color[0] * alpha).round() as u32;
    let green = (((pixel >> 8) & 0xff) as f32 * inv + color[1] * alpha).round() as u32;
    let blue = ((pixel & 0xff) as f32 * inv + color[2] * alpha).round() as u32;

    (red.min(255) << 16) | (green.min(255) << 8) | blue.min(255)
}

fn axis(positive: bool, negative: bool) -> f32 {
    match (positive, negative) {
        (true, false) => 1.0,
        (false, true) => -1.0,
        _ => 0.0,
    }
}

fn initial_gpu_camera_angles(seed: u64) -> (f32, f32) {
    let cloud_center = gpu_cloud_center_m(seed, 0.0);
    let yaw = cloud_center[0].atan2(cloud_center[2]);
    let horizontal_distance =
        (cloud_center[0] * cloud_center[0] + cloud_center[2] * cloud_center[2]).sqrt();
    let pitch = cloud_center[1].atan2(horizontal_distance);
    (yaw, pitch)
}

fn gpu_cloud_center_m(seed: u64, time_seconds: f32) -> [f32; 3] {
    const CLOUD_LAYER_TARGET_HEIGHT_M: f32 = 2_650.0;
    const CLOUD_LAYER_TARGET_DISTANCE_M: f32 = 7_400.0;
    [
        (seed as f32 * 12.9898).sin() * 1_250.0 + (time_seconds * 0.025).sin() * 260.0,
        CLOUD_LAYER_TARGET_HEIGHT_M,
        CLOUD_LAYER_TARGET_DISTANCE_M,
    ]
}

fn gpu_cloud_radii_m() -> [f32; 3] {
    [8_000.0, 1_700.0, 8_000.0]
}

fn cloud_proximity_label(camera: [f32; 3], _center: [f32; 3], _radii: [f32; 3]) -> String {
    const CLOUD_LAYER_BASE_M: f32 = 1_150.0;
    const CLOUD_LAYER_TOP_M: f32 = 4_550.0;
    if camera[1] < CLOUD_LAYER_BASE_M {
        format!("{:.0}m below cloud layer", CLOUD_LAYER_BASE_M - camera[1])
    } else if camera[1] > CLOUD_LAYER_TOP_M {
        format!("{:.0}m above cloud layer", camera[1] - CLOUD_LAYER_TOP_M)
    } else {
        "inside cloud layer".to_string()
    }
}

fn normalized_or_default(value: [f32; 3], default: [f32; 3]) -> [f32; 3] {
    let length = dot3(value, value).sqrt();
    if length > f32::EPSILON {
        [value[0] / length, value[1] / length, value[2] / length]
    } else {
        default
    }
}

fn desktop_camera_basis(camera: &CameraState, aspect: f32) -> DesktopCameraBasis {
    let forward = normalized_or_default(camera.forward, [0.0, 0.0, 1.0]);
    let requested_up = normalized_or_default(camera.up, [0.0, 1.0, 0.0]);
    let mut right = cross3(requested_up, forward);
    if dot3(right, right) <= 0.0001 {
        right = [1.0, 0.0, 0.0];
    } else {
        right = normalized_or_default(right, [1.0, 0.0, 0.0]);
    }
    let up = normalized_or_default(cross3(forward, right), [0.0, 1.0, 0.0]);
    let fov_y_radians = camera.fov_y_degrees.clamp(35.0, 95.0).to_radians();
    let tan_y = (fov_y_radians * 0.5).tan();

    DesktopCameraBasis {
        forward,
        right,
        up,
        fov_y_radians,
        tan_x: tan_y * aspect.max(0.1),
        tan_y,
    }
}

fn dot3(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn cross3(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn lerp(left: f32, right: f32, t: f32) -> f32 {
    left + (right - left) * t.clamp(0.0, 1.0)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if (edge1 - edge0).abs() <= f32::EPSILON {
        return if x >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl ApplicationHandler for DesktopApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.initialize_window(event_loop) {
            eprintln!("failed to initialize Ashfall desktop app: {error}");
            self.state = DesktopAppState::DeviceLost;
            event_loop.exit();
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.state = DesktopAppState::Suspended;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if Some(window_id) != self.window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                self.state = DesktopAppState::ShuttingDown;
                if let Some(window) = &self.window {
                    configure_game_cursor(window, false);
                }
                event_loop.exit();
            }
            WindowEvent::Focused(focused) => {
                if let Some(window) = &self.window {
                    configure_game_cursor(window, focused && self.mouse_look_active);
                }
            }
            WindowEvent::Resized(size) => {
                self.handle_resize(size.width, size.height);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed
                    && matches!(event.physical_key, PhysicalKey::Code(KeyCode::Escape))
                {
                    self.state = DesktopAppState::ShuttingDown;
                    if let Some(window) = &self.window {
                        configure_game_cursor(window, false);
                    }
                    event_loop.exit();
                    return;
                }
                self.handle_key(event.physical_key, event.state);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.handle_mouse_button(button, state);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.handle_cursor_move(position);
            }
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            self.handle_mouse_motion(delta);
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_to_zero_marks_minimized() {
        let mut app = DesktopApp::new(DesktopAppConfig::default());

        app.handle_resize(0, 720);

        assert_eq!(app.presentation_state(), PresentationState::Minimized);
    }

    #[test]
    fn nonzero_resize_is_pending() {
        let mut app = DesktopApp::new(DesktopAppConfig::default());

        app.handle_resize(800, 600);

        assert_eq!(app.presentation_state(), PresentationState::ResizePending);
    }

    #[test]
    fn camera_basis_keeps_right_handed_screen_axis() {
        let camera = CameraState {
            forward: [0.0, 0.0, 1.0],
            up: [0.0, 1.0, 0.0],
            ..Default::default()
        };

        let basis = desktop_camera_basis(&camera, 16.0 / 9.0);

        assert!(basis.right[0] > 0.99);
        assert!(basis.up[1] > 0.99);
    }

    #[test]
    fn initial_gpu_camera_angles_face_cloud_center() {
        let seed = DesktopAppConfig::default().seed;
        let (yaw, pitch) = initial_gpu_camera_angles(seed);
        let pitch_cos = pitch.cos();
        let forward = [yaw.sin() * pitch_cos, pitch.sin(), yaw.cos() * pitch_cos];
        let cloud_direction = normalized_or_default(gpu_cloud_center_m(seed, 0.0), [0.0, 0.0, 1.0]);

        assert!(dot3(forward, cloud_direction) > 0.999);
    }

    #[test]
    fn d_and_a_strafe_right_and_left_when_facing_forward() {
        let mut app = DesktopApp::new(DesktopAppConfig::default());
        app.input.right = true;

        app.update_camera(1.0, 0.0);

        assert!(app.camera_position_m[0] > 0.0);

        let mut app = DesktopApp::new(DesktopAppConfig::default());
        app.input.left = true;

        app.update_camera(1.0, 0.0);

        assert!(app.camera_position_m[0] < 0.0);
    }
}
