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
            color_blend::{AttachmentBlend, ColorBlendAttachmentState, ColorBlendState},
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
                    blend: Some(AttachmentBlend::alpha()),
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

            float hash12(vec2 value) {
                vec3 p3 = fract(vec3(value.xyx) * 0.1031);
                p3 += dot(p3, p3.yzx + 33.33);
                return fract((p3.x + p3.y) * p3.z);
            }

            float hash13(float value) {
                return hash12(vec2(value, value * 1.61803398875 + 7.13));
            }

            vec3 hash31(float value) {
                return vec3(
                    hash13(value + 0.11),
                    hash13(value + 17.31),
                    hash13(value + 41.17)
                );
            }

            float value_noise(vec2 value) {
                vec2 cell = floor(value);
                vec2 local = fract(value);
                vec2 smooth_local = local * local * (3.0 - 2.0 * local);
                float seed = pc.camera_position_seed.w * 0.017;
                float a = hash12(cell + seed);
                float b = hash12(cell + vec2(1.0, 0.0) + seed);
                float c = hash12(cell + vec2(0.0, 1.0) + seed);
                float d = hash12(cell + vec2(1.0, 1.0) + seed);
                return mix(mix(a, b, smooth_local.x), mix(c, d, smooth_local.x), smooth_local.y);
            }

            float fbm(vec2 value) {
                float sum = 0.0;
                float amplitude = 0.5;
                float norm = 0.0;
                for (int i = 0; i < 5; i++) {
                    sum += value_noise(value) * amplitude;
                    norm += amplitude;
                    value = mat2(1.62, 1.03, -1.03, 1.62) * value + vec2(7.1, 3.4);
                    amplitude *= 0.52;
                }
                return sum / norm;
            }

            float fbm_fast(vec2 value) {
                float sum = 0.0;
                float amplitude = 0.5;
                float norm = 0.0;
                for (int i = 0; i < 2; i++) {
                    sum += value_noise(value) * amplitude;
                    norm += amplitude;
                    value = mat2(1.58, 0.96, -0.96, 1.58) * value + vec2(4.8, 2.9);
                    amplitude *= 0.54;
                }
                return sum / norm;
            }

            float hash33(vec3 value) {
                vec3 p3 = fract(value * 0.1031);
                p3 += dot(p3, p3.yzx + 33.33);
                return fract((p3.x + p3.y) * p3.z);
            }

            float value_noise3(vec3 value) {
                vec3 cell = floor(value);
                vec3 local = fract(value);
                vec3 smooth_local = local * local * (3.0 - 2.0 * local);
                float seed = pc.camera_position_seed.w * 0.017;
                float c000 = hash33(cell + vec3(seed));
                float c100 = hash33(cell + vec3(1.0, 0.0, 0.0) + vec3(seed));
                float c010 = hash33(cell + vec3(0.0, 1.0, 0.0) + vec3(seed));
                float c110 = hash33(cell + vec3(1.0, 1.0, 0.0) + vec3(seed));
                float c001 = hash33(cell + vec3(0.0, 0.0, 1.0) + vec3(seed));
                float c101 = hash33(cell + vec3(1.0, 0.0, 1.0) + vec3(seed));
                float c011 = hash33(cell + vec3(0.0, 1.0, 1.0) + vec3(seed));
                float c111 = hash33(cell + vec3(1.0, 1.0, 1.0) + vec3(seed));
                float x00 = mix(c000, c100, smooth_local.x);
                float x10 = mix(c010, c110, smooth_local.x);
                float x01 = mix(c001, c101, smooth_local.x);
                float x11 = mix(c011, c111, smooth_local.x);
                float y0 = mix(x00, x10, smooth_local.y);
                float y1 = mix(x01, x11, smooth_local.y);
                return mix(y0, y1, smooth_local.z);
            }

            float fbm3(vec3 value) {
                float sum = 0.0;
                float amplitude = 0.5;
                float norm = 0.0;
                for (int i = 0; i < 3; i++) {
                    sum += value_noise3(value) * amplitude;
                    norm += amplitude;
                    value = value * 2.04 + vec3(11.7, 5.2, 8.3);
                    amplitude *= 0.52;
                }
                return sum / norm;
            }

            vec3 aces(vec3 value) {
                const float a = 2.51;
                const float b = 0.03;
                const float c = 2.43;
                const float d = 0.59;
                const float e = 0.14;
                return clamp((value * (a * value + b)) / (value * (c * value + d) + e), 0.0, 1.0);
            }

            float lobe(vec2 p, vec2 center, float radius) {
                return 1.0 - smoothstep(radius * 0.58, radius, length(p - center));
            }

            float volume_lobe(vec3 p, vec3 center, vec3 radius) {
                vec3 q = (p - center) / radius;
                return 1.0 - smoothstep(0.49, 1.0816, dot(q, q));
            }

            float crisp_volume_lobe(vec3 p, vec3 center, vec3 radius) {
                vec3 q = (p - center) / radius;
                return 1.0 - smoothstep(0.56, 1.018, dot(q, q));
            }

            float soft_shape_union(float a, float b) {
                a = clamp(a, 0.0, 1.0);
                b = clamp(b, 0.0, 1.0);
                return clamp(a + b - a * b * 0.72, 0.0, 1.0);
            }

            float moving_volume_lobe(vec3 p, vec3 center, vec3 radius, float phase, float amp) {
                vec3 offset = vec3(
                    sin(phase + center.z * 7.3),
                    sin(phase * 0.73 + center.x * 8.1) * 0.45,
                    cos(phase * 0.91 + center.y * 6.7)
                ) * amp;
                return volume_lobe(p, center + offset, radius);
            }

            float animated_puff_field(vec3 p, float time, float seed) {
                float field = 0.0;
                for (int i = 0; i < 11; i++) {
                    float fi = float(i);
                    vec3 rnd = hash31(seed * 0.071 + fi * 23.73);
                    float split = smoothstep(
                        0.16,
                        0.90,
                        0.5 + 0.5 * sin(time * (0.105 + rnd.y * 0.055) + seed * 0.019 + fi * 2.31)
                    );
                    vec3 center = vec3(
                        mix(-0.76, 0.76, rnd.x),
                        mix(-0.34, 0.42, rnd.y),
                        mix(-0.58, 0.58, rnd.z)
                    );
                    center.x += sin(time * (0.14 + rnd.z * 0.07) + fi * 1.91) * 0.060 + (rnd.x - 0.5) * split * 0.15;
                    center.y += sin(time * (0.12 + rnd.x * 0.05) + fi * 1.37) * 0.042;
                    center.z += cos(time * (0.13 + rnd.y * 0.06) + fi * 1.67) * 0.070 + (rnd.z - 0.5) * split * 0.12;
                    vec3 radius = vec3(
                        mix(0.16, 0.33, rnd.z),
                        mix(0.13, 0.29, rnd.x),
                        mix(0.17, 0.36, rnd.y)
                    ) * (1.0 - split * 0.13);
                    field = soft_shape_union(field, crisp_volume_lobe(p, center, radius) * 0.92);
                }
                return field;
            }

            float soft_edge_puff_field(vec3 p, float time, float seed) {
                float field = 0.0;
                for (int i = 0; i < 14; i++) {
                    float fi = float(i);
                    vec3 rnd = hash31(seed * 0.093 + fi * 31.19);
                    vec3 side = normalize(vec3(rnd.x - 0.5, (rnd.y - 0.5) * 0.55, rnd.z - 0.5));
                    float drift = sin(time * (0.08 + rnd.z * 0.05) + fi * 1.73 + seed * 0.013);
                    vec3 center = side * vec3(0.72, 0.42, 0.62);
                    center += vec3(drift * 0.075, sin(time * 0.10 + fi) * 0.045, cos(time * 0.09 + fi * 1.7) * 0.070);
                    vec3 radius = vec3(
                        mix(0.11, 0.28, rnd.z),
                        mix(0.09, 0.23, rnd.x),
                        mix(0.12, 0.30, rnd.y)
                    );
                    field = soft_shape_union(field, volume_lobe(p, center, radius) * 0.82);
                }
                return field;
            }

            float body_billow_field(vec3 p, float time, float seed) {
                float field = 0.0;
                for (int i = 0; i < 15; i++) {
                    float fi = float(i);
                    vec3 rnd = hash31(seed * 0.127 + fi * 19.91);
                    vec3 center = vec3(
                        mix(-0.78, 0.78, rnd.x),
                        mix(-0.30, 0.38, rnd.y),
                        mix(-0.58, 0.58, rnd.z)
                    );
                    center.x += sin(time * (0.060 + rnd.y * 0.040) + fi * 1.47) * 0.055;
                    center.y += cos(time * (0.070 + rnd.z * 0.035) + fi * 1.21) * 0.040;
                    center.z += sin(time * (0.065 + rnd.x * 0.045) + fi * 1.83) * 0.060;
                    vec3 radius = vec3(
                        mix(0.20, 0.42, rnd.z),
                        mix(0.16, 0.34, rnd.x),
                        mix(0.22, 0.44, rnd.y)
                    );
                    field = soft_shape_union(field, volume_lobe(p, center, radius) * 0.78);
                }
                return field;
            }

            vec3 volume_cloud_flow(vec3 local, float time, float seed) {
                vec2 wind = vec2(time * 0.072 + seed * 0.013, -time * 0.038);
                float roll = fbm_fast(local.xz * 1.28 + wind);
                float shear = fbm_fast(local.zy * 1.46 - wind.yx + vec2(3.7, 1.9));
                float lift = value_noise(local.xy * 1.06 + vec2(seed * 0.021, time * 0.046));
                float gust = sin(time * 0.18 + local.z * 5.4 + seed * 0.011) * (1.0 - smoothstep(0.18, 0.95, abs(local.y)));
                vec3 flow = vec3(roll - 0.5, (lift - 0.5) * 0.48, shear - 0.5) * 0.155;
                flow.x += local.y * 0.078 * sin(time * 0.052 + seed * 0.017) + gust * 0.045;
                flow.z += local.x * 0.054 * cos(time * 0.043 + seed * 0.011);
                return local + flow;
            }

            float volume_cloud_shape(vec3 local, float time, float seed) {
                float phase = time * 0.46 + seed * 0.009;
                vec3 p = local;
                p.x += local.y * 0.16 * sin(time * 0.15 + seed * 0.013);
                p.z += local.x * local.y * 0.11 * cos(time * 0.12 + seed * 0.017);
                float radius2 = dot(local, local);
                float envelope = 1.0 - smoothstep(0.64, 1.1449, radius2);
                float bottom_lift = smoothstep(-1.10, -0.76, local.y);
                float anvil_cap = 1.0 - smoothstep(0.76, 1.04, local.y);
                float lobes = 0.0;
                lobes = soft_shape_union(lobes, moving_volume_lobe(p, vec3(-0.62, -0.05, 0.02), vec3(0.30, 0.40, 0.44), phase + 0.2, 0.076) * 0.92);
                lobes = soft_shape_union(lobes, moving_volume_lobe(p, vec3(-0.38, 0.13, -0.25), vec3(0.31, 0.35, 0.34), phase + 1.1, 0.068) * 0.92);
                lobes = soft_shape_union(lobes, moving_volume_lobe(p, vec3(-0.16, -0.18, 0.22), vec3(0.39, 0.39, 0.43), phase + 2.2, 0.060) * 0.94);
                lobes = soft_shape_union(lobes, moving_volume_lobe(p, vec3(0.10, -0.17, -0.12), vec3(0.43, 0.40, 0.43), phase + 3.0, 0.066) * 0.94);
                lobes = soft_shape_union(lobes, moving_volume_lobe(p, vec3(0.36, -0.04, 0.20), vec3(0.36, 0.38, 0.38), phase + 4.1, 0.070) * 0.92);
                lobes = soft_shape_union(lobes, moving_volume_lobe(p, vec3(0.60, 0.08, -0.08), vec3(0.27, 0.33, 0.35), phase + 5.4, 0.078) * 0.88);
                lobes = soft_shape_union(lobes, moving_volume_lobe(p, vec3(0.02, 0.34, 0.08), vec3(0.34, 0.25, 0.36), phase + 6.5, 0.058) * 0.88);
                lobes = soft_shape_union(lobes, moving_volume_lobe(p, vec3(-0.25, 0.30, 0.27), vec3(0.25, 0.22, 0.28), phase + 7.0, 0.060) * 0.84);
                lobes = soft_shape_union(lobes, moving_volume_lobe(p, vec3(0.31, 0.26, -0.30), vec3(0.25, 0.23, 0.27), phase + 8.4, 0.066) * 0.84);
                lobes = soft_shape_union(lobes, moving_volume_lobe(p, vec3(-0.32, -0.38, 0.02), vec3(0.23, 0.18, 0.28), phase + 9.2, 0.052) * 0.80);
                float puffs = animated_puff_field(p, time, seed);
                float soft_puffs = soft_edge_puff_field(p, time, seed);
                float billows = body_billow_field(p, time, seed);
                lobes = soft_shape_union(lobes, puffs * 0.90);
                lobes = soft_shape_union(lobes, billows * 0.76);

                float body = max(envelope * 0.022, soft_shape_union(lobes, soft_puffs * 0.66));
                float edge = smoothstep(0.12, 0.42, lobes) * (1.0 - smoothstep(0.54, 0.88, lobes));
                float wind_cut = value_noise(p.xz * 9.5 + vec2(time * 0.55 + p.y * 2.6, seed * 0.037));
                float filament = value_noise(p.xy * 13.5 + vec2(-time * 0.54 + p.z * 4.2, 9.1));
                float fracture = value_noise(p.xz * 17.0 + vec2(time * 0.70, seed * 0.059));
                float shear_gap = fbm_fast(p.xz * 6.4 + vec2(time * 0.34 + p.y * 2.7, seed * 0.061));
                float vein_gap = value_noise(p.yz * 9.5 + vec2(time * 0.46 + p.x * 3.0, seed * 0.083));
                float ruffle = fbm_fast(p.xz * 5.2 + vec2(time * 0.22 + p.y * 1.7, seed * 0.037));
                float vertical_ruffle = fbm_fast(p.xy * 6.1 + vec2(-time * 0.18 + p.z * 1.5, seed * 0.049));
                body += edge * ((wind_cut - 0.48) * 0.30 + (filament - 0.50) * 0.15);
                body += edge * ((ruffle - 0.48) * 0.18 + (vertical_ruffle - 0.50) * 0.11);
                body -= edge * max(0.0, 0.54 - filament) * 0.15;
                body -= edge * max(0.0, 0.44 - fracture) * 0.05;
                body -= edge * max(0.0, 0.43 - ruffle) * 0.08;
                body -= smoothstep(0.20, 0.64, lobes) * max(0.0, 0.42 - shear_gap) * 0.09;
                body -= edge * max(0.0, 0.40 - vein_gap) * 0.05;
                body += billows * 0.105;
                body += soft_puffs * smoothstep(0.26, 0.98, radius2) * 0.170;
                return clamp(body, 0.0, 1.0) * bottom_lift * anvil_cap;
            }

            float volume_cloud_shadow_shape(vec3 local, float time, float seed) {
                float phase = time * 0.46 + seed * 0.009;
                vec3 p = local;
                p.x += local.y * 0.13 * sin(time * 0.15 + seed * 0.013);
                p.z += local.x * local.y * 0.08 * cos(time * 0.12 + seed * 0.017);
                float radius2 = dot(local, local);
                float envelope = 1.0 - smoothstep(0.64, 1.1449, radius2);
                float bottom_lift = smoothstep(-1.10, -0.76, local.y);
                float anvil_cap = 1.0 - smoothstep(0.76, 1.04, local.y);
                float lobes = 0.0;
                lobes = soft_shape_union(lobes, moving_volume_lobe(p, vec3(-0.54, -0.04, -0.02), vec3(0.42, 0.46, 0.50), phase + 0.4, 0.060) * 0.90);
                lobes = soft_shape_union(lobes, moving_volume_lobe(p, vec3(-0.10, -0.17, 0.15), vec3(0.50, 0.45, 0.48), phase + 2.6, 0.050) * 0.94);
                lobes = soft_shape_union(lobes, moving_volume_lobe(p, vec3(0.34, -0.05, 0.10), vec3(0.44, 0.43, 0.44), phase + 4.7, 0.058) * 0.92);
                lobes = soft_shape_union(lobes, moving_volume_lobe(p, vec3(0.48, 0.12, -0.20), vec3(0.34, 0.36, 0.38), phase + 6.2, 0.060) * 0.86);
                lobes = soft_shape_union(lobes, moving_volume_lobe(p, vec3(0.00, 0.30, 0.04), vec3(0.40, 0.28, 0.42), phase + 7.3, 0.045) * 0.86);
                return clamp(max(envelope * 0.08, lobes), 0.0, 1.0) * bottom_lift * anvil_cap;
            }

            bool ray_ellipsoid_interval(vec3 origin, vec3 direction, vec3 center, vec3 radii, out float t0, out float t1) {
                vec3 o = (origin - center) / radii;
                vec3 d = direction / radii;
                float a = dot(d, d);
                float b = 2.0 * dot(o, d);
                float c = dot(o, o) - 1.0;
                float disc = b * b - 4.0 * a * c;
                if (a <= 0.0000001 || disc < 0.0) {
                    return false;
                }
                float root = sqrt(disc);
                t0 = (-b - root) / (2.0 * a);
                t1 = (-b + root) / (2.0 * a);
                return t1 > 0.0 && t1 > t0;
            }

            float volume_cloud_density_from_shape(vec3 local, float shape, float time, float seed) {
                vec2 drift = vec2(time * 0.064, -time * 0.026 + seed * 0.019);
                float wind = value_noise(local.xz * 1.20 + drift);
                vec2 shear = vec2(wind - 0.5, value_noise(local.zy * 1.52 - drift.yx) - 0.5) * 0.42;
                float coarse = fbm_fast(local.xz * 2.15 + drift + shear);
                float billow = fbm_fast(local.xy * 4.25 + vec2(seed * 0.027, time * 0.050) + shear * 0.55);
                float pocket = value_noise(local.zy * 7.40 + vec2(5.1 + seed * 0.011, time * 0.058) - shear);
                float fiber = value_noise(local.xz * 10.0 + drift * 3.0 + shear * 1.7);
                float vein = value_noise(local.xy * 7.0 + vec2(-time * 0.20 + seed * 0.017, local.z * 2.2));
                float foam = value_noise(local.yz * 18.0 + vec2(time * 0.20 + seed * 0.029, local.x * 5.4));
                float grain = value_noise(local.xz * 26.0 + vec2(-time * 0.24 + seed * 0.041, local.y * 4.7));
                float scallop = fbm_fast(local.xy * 11.0 + vec2(time * 0.13 + local.z * 1.9, seed * 0.037));
                float sharp_shape = smoothstep(0.14, 0.34, shape);
                float density_edge = smoothstep(0.12, 0.42, shape) * (1.0 - smoothstep(0.58, 0.90, shape));
                float foam_edge = smoothstep(0.10, 0.48, shape) * (1.0 - smoothstep(0.64, 0.94, shape));
                float body_texture = 0.72 + coarse * 0.24 + billow * 0.26 + (fiber - 0.5) * 0.10 + (scallop - 0.5) * foam_edge * 0.16;
                float body = shape * sharp_shape * body_texture;
                float detail = (fiber - 0.5) * shape * sharp_shape * 0.060
                    + (pocket - 0.5) * shape * 0.040
                    + (vein - 0.5) * density_edge * 0.034
                    + (foam - 0.46) * foam_edge * 0.085
                    + (grain - 0.52) * foam_edge * 0.045;
                float density = body + detail;
                density -= max(0.0, 0.42 - pocket) * shape * density_edge * 0.072;
                density -= max(0.0, 0.44 - foam) * foam_edge * 0.070;
                density -= max(0.0, 0.40 - scallop) * density_edge * 0.046;
                float soft_shell = smoothstep(0.08, 0.28, shape) * (1.0 - smoothstep(0.54, 0.86, shape));
                density = max(density, smoothstep(0.20, 0.52, shape) * 0.22 + soft_shell * 0.10);
                return clamp(density, 0.0, 1.0);
            }

            float volume_cloud_density(vec3 local, float time, float seed) {
                vec3 flowed = volume_cloud_flow(local, time, seed);
                return volume_cloud_density_from_shape(flowed, volume_cloud_shape(flowed, time, seed), time, seed);
            }

            void render_volume_cloud_instance(
                vec3 cloud_center,
                vec3 cloud_radii,
                float cloud_seed,
                float cloud_strength,
                vec3 camera_position,
                vec3 ray,
                vec3 sun,
                float time,
                out vec3 volume_color,
                out float volume_alpha,
                out float inside_mist,
                out float sun_occluder
            ) {
                volume_alpha = 0.0;
                volume_color = vec3(0.0);
                inside_mist = 0.0;
                sun_occluder = 0.0;

                vec3 camera_in_cloud = (camera_position - cloud_center) / cloud_radii;
                vec3 camera_flowed = volume_cloud_flow(camera_in_cloud, time, cloud_seed);
                float camera_shape = volume_cloud_shape(camera_flowed, time, cloud_seed);
                float camera_density = volume_cloud_density_from_shape(camera_flowed, camera_shape, time, cloud_seed);
                float inside_cloud = 1.0 - smoothstep(0.58, 0.82, length(camera_in_cloud));
                inside_mist = clamp(smoothstep(0.30, 0.56, camera_density) * smoothstep(0.24, 0.54, camera_shape) * inside_cloud * 0.96 * cloud_strength, 0.0, 0.96);

                float volume_t0 = 0.0;
                float volume_t1 = 0.0;
                if (inside_mist < 0.82 && ray_ellipsoid_interval(camera_position, ray, cloud_center, cloud_radii, volume_t0, volume_t1)) {
                    float start_t = max(volume_t0, 0.0);
                    float end_t = volume_t1;
                    float span = max(end_t - start_t, 0.0);
                    float step_len = span / 6.0;
                    float march_jitter = 0.0;
                    vec3 accum = vec3(0.0);
                    float max_shape = 0.0;
                    float max_density = 0.0;
                    float detail_mix = 0.0;
                    float surface_score = 0.0;
                    float surface_crisp = 0.0;
                    float optical_path = 0.0;
                    float density_path = 0.0;
                    float core_path = 0.0;
                    float shell_path = 0.0;
                    float max_shell = 0.0;
                    vec3 surface_color = vec3(0.0);
                    vec3 sun_local = normalize(vec3(sun.x / cloud_radii.x, sun.y / cloud_radii.y, sun.z / cloud_radii.z));

                    for (int i = 0; i < 6; i++) {
                        float t = start_t + (float(i) + 0.5 + march_jitter) * step_len;
                        vec3 sample_pos = camera_position + ray * t;
                        vec3 local = (sample_pos - cloud_center) / cloud_radii;
                        vec3 flowed = volume_cloud_flow(local, time, cloud_seed);
                        float shape = volume_cloud_shape(flowed, time, cloud_seed);
                        float density = volume_cloud_density_from_shape(flowed, shape, time, cloud_seed);
                        max_shape = max(max_shape, shape);
                        max_density = max(max_density, density);
                        float shell = smoothstep(0.08, 0.30, shape) * (1.0 - smoothstep(0.48, 0.82, shape));
                        max_shell = max(max_shell, shell);

                        float core_alpha = smoothstep(0.34, 0.74, shape);
                        density_path += density * step_len * 0.0010;
                        core_path += core_alpha * step_len * 0.0010;
                        shell_path += shell * step_len * 0.0010;
                        optical_path += (density * 1.82 + core_alpha * 0.68 + shell * 0.34) * step_len * 0.00118;
                        float body_alpha = 1.0 - exp(-(density * 2.35 + core_alpha * 0.78 + shell * 0.36) * step_len * 0.00124 * cloud_strength);
                        float sample_alpha = body_alpha * (1.0 - volume_alpha);
                        float height_light = smoothstep(-0.72, 0.62, flowed.y);

                        float shadow_near = volume_cloud_shadow_shape(flowed + sun_local * 0.24, time, cloud_seed);
                        float shadow_mid = volume_cloud_shadow_shape(flowed + sun_local * 0.54, time, cloud_seed);
                        float shadow_far = volume_cloud_shadow_shape(flowed + sun_local * 0.92, time, cloud_seed);
                        float shadow_body = 1.0 - smoothstep(0.5184, 1.1664, dot(flowed + sun_local * 0.34, flowed + sun_local * 0.34));
                        vec3 base_normal_world = normalize(vec3(flowed.x / cloud_radii.x, flowed.y / cloud_radii.y, flowed.z / cloud_radii.z) + vec3(0.0, -0.00010, 0.0));
                        vec3 detail_normal_world = normalize(vec3(
                            value_noise(flowed.yz * 9.0 + vec2(time * 0.14, cloud_seed * 0.017)) - 0.5,
                            value_noise(flowed.xz * 8.5 + vec2(-time * 0.12, cloud_seed * 0.023)) - 0.5,
                            value_noise(flowed.xy * 9.5 + vec2(time * 0.15, cloud_seed * 0.031)) - 0.5
                        ));
                        vec3 surface_normal_world = normalize(base_normal_world * 0.94 + detail_normal_world * 0.06);
                        float sun_dot = dot(surface_normal_world, sun);
                        float light_face = smoothstep(-0.08, 0.74, sun_dot);
                        float dark_face = smoothstep(0.08, -0.58, sun_dot);
                        float underside_shadow = smoothstep(0.20, -0.56, flowed.y) * (1.0 - clamp(sun.y, 0.0, 1.0) * 0.45);
                        float optical_depth = shadow_near * 0.64 + shadow_mid * 0.48 + shadow_far * 0.32 + shadow_body * 0.18 + density * 0.38 + underside_shadow * 0.34;
                        float light_transmittance = clamp(exp(-optical_depth * 1.04), 0.18, 1.0);

                        float local_detail = value_noise(flowed.xz * 6.4 + vec2(time * 0.10, -time * 0.04) + cloud_seed * 0.013);
                        float billow_detail = fbm_fast(flowed.xy * 5.4 + vec2(-time * 0.09, time * 0.045) + cloud_seed * 0.021);
                        float fiber_detail = value_noise(flowed.xz * 10.5 + vec2(time * 0.18, cloud_seed * 0.041));
                        detail_mix += (local_detail * 0.45 + billow_detail * 0.55) * sample_alpha;
                        float rim_light = smoothstep(0.49, 1.0404, dot(flowed, flowed));
                        float crisp = smoothstep(0.30, 0.80, local_detail * 0.38 + billow_detail * 0.56 + fiber_detail * 0.06);
                        float direct_light = light_face * light_transmittance;
                        float soft_bounce = smoothstep(0.20, 0.68, shape + density * 0.55) * (0.24 + light_transmittance * 0.20);
                        float sky_ambient = 0.44 + height_light * 0.25 + crisp * 0.04;
                        vec3 cool_shadow = mix(vec3(0.48, 0.53, 0.62), vec3(0.74, 0.77, 0.80), height_light);
                        vec3 ambient_color = cool_shadow * (sky_ambient + soft_bounce * 0.46);
                        vec3 direct_color = vec3(1.10, 1.07, 0.96) * (direct_light * 0.72);
                        vec3 scatter_color = vec3(1.0, 0.88, 0.66) * (rim_light * 0.012 * direct_light);
                        vec3 sample_color = ambient_color + direct_color + scatter_color;
                        sample_color = mix(sample_color, vec3(0.995, 0.988, 0.94), soft_bounce * 0.24 + direct_light * 0.075);
                        float broad_shadow = underside_shadow * 0.20 + dark_face * (1.0 - light_transmittance) * 0.22;
                        sample_color *= 1.0 - broad_shadow * smoothstep(0.18, 0.74, shape);
                        sample_color -= vec3(0.08, 0.09, 0.12) * dark_face * (1.0 - light_transmittance) * smoothstep(0.22, 0.72, shape) * 0.22;
                        sample_color *= 0.92 + crisp * 0.15;
                        sample_color = mix(sample_color, vec3(0.68, 0.84, 1.0), inside_cloud * 0.20);

                        float current_surface_score = density * 0.82 + shape * 0.58 + crisp * 0.12;
                        if (current_surface_score > surface_score) {
                            surface_score = current_surface_score;
                            surface_crisp = crisp;
                            surface_color = sample_color;
                        }
                        accum += sample_color * sample_alpha;
                        volume_alpha += sample_alpha;
                    }

                    vec3 averaged_color = accum / max(volume_alpha, 0.001);
                    float surface_weight = smoothstep(0.42, 1.05, surface_score) * 0.65;
                    volume_color = mix(averaged_color, surface_color, surface_weight);
                    float edge_width = clamp(max(abs(dFdx(max_shape)), abs(dFdy(max_shape))) * 1.1, 0.010, 0.046);
                    float path_body = density_path * 2.05 + core_path * 1.85 + shell_path * 0.72;
                    float body_presence = clamp(max_density * 1.55 + max_shape * 0.88 + path_body * 0.52, 0.0, 2.6);
                    float core_layer = smoothstep(0.64, 1.16, body_presence) * smoothstep(0.30, 0.60, max_density);
                    float billow_layer = smoothstep(0.26, 0.78, body_presence) * smoothstep(0.08, 0.36, max_density) * (1.0 - core_layer * 0.08);
                    float fluff_layer = max_shell * smoothstep(0.025, 0.18, max_density) * (1.0 - core_layer * 0.24);
                    float veil_layer = smoothstep(0.035, 0.15, max_shape) * (1.0 - smoothstep(0.44, 0.76, max_shape));
                    float coverage_floor = smoothstep(0.18 - edge_width, 0.56 + edge_width, max_shape) * mix(0.00, 0.70, smoothstep(0.05, 0.30, max_density));
                    float surface_alpha = smoothstep(0.32, 0.86, surface_score) * mix(0.28, 0.88, core_layer);
                    float optical_alpha = 1.0 - exp(-(optical_path * 1.72 + path_body * 0.96) * cloud_strength);
                    float core_alpha_resolved = core_layer * 0.982 + smoothstep(0.78, 1.0, core_layer) * 0.012;
                    float billow_alpha = billow_layer * mix(0.62, 0.90, smoothstep(0.20, 0.68, surface_score));
                    float fluff_alpha = fluff_layer * 0.68;
                    float veil_alpha = veil_layer * 0.14;
                    float opaque_body_floor = smoothstep(0.055, 0.20, max_density) * smoothstep(0.08, 0.30, max_shape) * (0.82 + 0.15 * smoothstep(0.42, 1.10, body_presence));
                    float resolved_detail = detail_mix / max(volume_alpha, 0.001);
                    volume_color = mix(volume_color, surface_color * (0.88 + surface_crisp * 0.22), surface_weight * 0.28);
                    volume_color = mix(volume_color, volume_color * (0.90 + resolved_detail * 0.20), smoothstep(0.30, 0.76, max_shape));
                    float layer_alpha = 1.0
                        - (1.0 - core_alpha_resolved)
                        * (1.0 - billow_alpha)
                        * (1.0 - fluff_alpha)
                        * (1.0 - veil_alpha);
                    layer_alpha = clamp(layer_alpha, 0.0, 0.992);
                    float resolved_alpha = max(max(volume_alpha * 0.88, optical_alpha), max(opaque_body_floor, max(coverage_floor, max(surface_alpha, layer_alpha))));
                    float strength_alpha = smoothstep(0.02, 0.86, cloud_strength);
                    volume_alpha = clamp(resolved_alpha * strength_alpha, 0.0, 0.992);
                }

                sun_occluder = max(volume_alpha * 1.05, inside_mist * 0.96);
            }

            void merge_cloud_layer(
                inout vec3 cloud_color,
                inout float cloud_alpha,
                vec3 layer_color,
                float layer_alpha
            ) {
                layer_alpha = clamp(layer_alpha, 0.0, 0.995);
                if (layer_alpha <= 0.0005) {
                    return;
                }
                if (cloud_alpha <= 0.0005) {
                    cloud_color = layer_color;
                    cloud_alpha = layer_alpha;
                    return;
                }

                float overlap = clamp(cloud_alpha * layer_alpha, 0.0, 1.0);
                float union_alpha = 1.0 - (1.0 - cloud_alpha) * (1.0 - layer_alpha);
                float shared_body = overlap * smoothstep(0.18, 0.82, max(cloud_alpha, layer_alpha));
                union_alpha = max(union_alpha, clamp(max(cloud_alpha, layer_alpha) + shared_body * 0.18, 0.0, 0.995));
                vec3 shared_color = mix((cloud_color + layer_color) * 0.5, vec3(0.94, 0.95, 0.92), shared_body * 0.20);
                vec3 merged_layer_color = mix(layer_color, shared_color, overlap * 0.62);
                float old_weight = cloud_alpha;
                float new_weight = layer_alpha * (1.0 - cloud_alpha * 0.30);
                cloud_color = (cloud_color * old_weight + merged_layer_color * new_weight) / max(old_weight + new_weight, 0.001);
                cloud_alpha = clamp(union_alpha, 0.0, 0.995);
            }

            float add_cloud_mass(float cloud_mass, float layer_mass) {
                layer_mass = clamp(layer_mass, 0.0, 2.0);
                return min(cloud_mass + layer_mass * (1.0 - cloud_mass * 0.10), 2.60);
            }

            float cloud_instance_density_at(vec3 sample_pos, vec3 center, vec3 radii, float seed, float strength, float time) {
                if (strength <= 0.010) {
                    return 0.0;
                }

                vec3 local = (sample_pos - center) / radii;
                float radius2 = dot(local, local);
                if (radius2 > 2.08) {
                    return 0.0;
                }

                vec3 flowed = volume_cloud_flow(local, time, seed);
                float vertical = smoothstep(-1.10, -0.78, local.y) * (1.0 - smoothstep(0.82, 1.12, local.y));
                float envelope = (1.0 - smoothstep(0.58, 1.72, radius2)) * vertical;
                if (envelope <= 0.006) {
                    return 0.0;
                }

                vec3 winded = flowed + vec3(time * 0.030, seed * 0.011, -time * 0.024);
                float macro = fbm_fast(winded.xz * 1.70 + vec2(seed * 0.019, time * 0.050 + winded.y * 1.15));
                float lift = fbm_fast(winded.xy * 3.10 + vec2(-time * 0.075 + winded.z * 0.7, seed * 0.027));
                float billow = value_noise3(winded * 4.70 + vec3(2.1, time * 0.10, seed * 0.021));
                float foam = value_noise3(winded * 9.60 + vec3(8.4, time * 0.17, seed * 0.023));
                float lace = value_noise3(winded * 19.0 + vec3(seed * 0.017, time * 0.18, 2.3));
                float edge = smoothstep(0.44, 1.58, radius2);
                float clump = smoothstep(0.34, 0.82, macro * 0.42 + billow * 0.34 + foam * 0.24 + envelope * 0.18);
                float cloud_weather = envelope * (0.30 + macro * 0.26 + billow * 0.18 + clump * 0.38);
                float density = cloud_weather
                    + (lift - 0.47) * envelope * 0.22
                    + (foam - 0.45) * envelope * 0.22
                    + (lace - 0.50) * envelope * 0.055
                    + max(0.0, clump - 0.42) * envelope * 0.34;
                density -= edge * envelope * max(0.0, 0.74 - billow) * 0.92;
                density -= edge * envelope * max(0.0, 0.58 - foam) * 0.56;
                density -= edge * envelope * max(0.0, 0.62 - clump) * 0.42;
                density -= edge * envelope * max(0.0, 0.50 - lace) * 0.18;
                density = smoothstep(0.02, 0.76, density) * vertical * (0.48 + envelope * 0.52);
                density *= 0.74 + foam * 0.22 + lace * 0.08;
                return clamp(density * strength * 1.55, 0.0, 1.58);
            }

            float cloud_instance_shadow_mass_at(vec3 sample_pos, vec3 center, vec3 radii, float seed, float strength, float time) {
                if (strength <= 0.010) {
                    return 0.0;
                }

                vec3 local = (sample_pos - center) / radii;
                float radius2 = dot(local, local);
                if (radius2 > 1.82) {
                    return 0.0;
                }

                vec3 flowed = volume_cloud_flow(local, time, seed);
                float shape = volume_cloud_shadow_shape(flowed, time, seed);
                float radius_fade = 1.0 - smoothstep(1.28, 1.82, radius2);
                float body = smoothstep(0.16, 0.50, shape);
                return clamp((shape * 0.95 + body * 0.55) * strength * radius_fade, 0.0, 1.65);
            }

            float merged_cloud_density_at(
                vec3 sample_pos,
                float time,
                float base_seed,
                vec3 cloud_center,
                vec3 main_radii,
                float main_strength,
                vec3 left_center,
                vec3 left_radii,
                float left_strength,
                vec3 left_bridge_center,
                vec3 left_bridge_radii,
                float left_bridge_strength,
                vec3 right_center,
                vec3 right_radii,
                float right_strength,
                vec3 bridge_center,
                vec3 bridge_radii,
                float bridge_strength,
                vec3 high_center,
                vec3 high_radii,
                float high_strength,
                vec3 newborn_center,
                vec3 newborn_radii,
                float newborn_strength,
                vec3 seedling_center,
                vec3 seedling_radii,
                float seedling_strength,
                vec3 wake_center,
                vec3 wake_radii,
                float wake_strength
            ) {
                float mass = 0.0;
                mass = add_cloud_mass(mass, cloud_instance_density_at(sample_pos, cloud_center, main_radii, base_seed, main_strength, time));
                mass = add_cloud_mass(mass, cloud_instance_density_at(sample_pos, left_center, left_radii, base_seed + 19.7, left_strength, time));
                mass = add_cloud_mass(mass, cloud_instance_density_at(sample_pos, left_bridge_center, left_bridge_radii, base_seed + 27.9, left_bridge_strength, time));
                mass = add_cloud_mass(mass, cloud_instance_density_at(sample_pos, right_center, right_radii, base_seed + 37.3, right_strength, time));
                mass = add_cloud_mass(mass, cloud_instance_density_at(sample_pos, bridge_center, bridge_radii, base_seed + 43.5, bridge_strength, time));
                mass = add_cloud_mass(mass, cloud_instance_density_at(sample_pos, high_center, high_radii, base_seed + 61.1, high_strength, time));
                mass = add_cloud_mass(mass, cloud_instance_density_at(sample_pos, newborn_center, newborn_radii, base_seed + 91.9, newborn_strength, time));
                mass = add_cloud_mass(mass, cloud_instance_density_at(sample_pos, seedling_center, seedling_radii, base_seed + 123.7, seedling_strength, time));
                mass = add_cloud_mass(mass, cloud_instance_density_at(sample_pos, wake_center, wake_radii, base_seed + 151.2, wake_strength, time));
                return mass;
            }

            float merged_cloud_shadow_mass_at(
                vec3 sample_pos,
                float time,
                float base_seed,
                vec3 cloud_center,
                vec3 main_radii,
                float main_strength,
                vec3 left_center,
                vec3 left_radii,
                float left_strength,
                vec3 left_bridge_center,
                vec3 left_bridge_radii,
                float left_bridge_strength,
                vec3 right_center,
                vec3 right_radii,
                float right_strength,
                vec3 bridge_center,
                vec3 bridge_radii,
                float bridge_strength,
                vec3 high_center,
                vec3 high_radii,
                float high_strength,
                vec3 newborn_center,
                vec3 newborn_radii,
                float newborn_strength,
                vec3 seedling_center,
                vec3 seedling_radii,
                float seedling_strength,
                vec3 wake_center,
                vec3 wake_radii,
                float wake_strength
            ) {
                float mass = 0.0;
                mass = add_cloud_mass(mass, cloud_instance_shadow_mass_at(sample_pos, cloud_center, main_radii, base_seed, main_strength, time));
                mass = add_cloud_mass(mass, cloud_instance_shadow_mass_at(sample_pos, left_center, left_radii, base_seed + 19.7, left_strength, time));
                mass = add_cloud_mass(mass, cloud_instance_shadow_mass_at(sample_pos, left_bridge_center, left_bridge_radii, base_seed + 27.9, left_bridge_strength, time));
                mass = add_cloud_mass(mass, cloud_instance_shadow_mass_at(sample_pos, right_center, right_radii, base_seed + 37.3, right_strength, time));
                mass = add_cloud_mass(mass, cloud_instance_shadow_mass_at(sample_pos, bridge_center, bridge_radii, base_seed + 43.5, bridge_strength, time));
                mass = add_cloud_mass(mass, cloud_instance_shadow_mass_at(sample_pos, high_center, high_radii, base_seed + 61.1, high_strength, time));
                mass = add_cloud_mass(mass, cloud_instance_shadow_mass_at(sample_pos, newborn_center, newborn_radii, base_seed + 91.9, newborn_strength, time));
                mass = add_cloud_mass(mass, cloud_instance_shadow_mass_at(sample_pos, seedling_center, seedling_radii, base_seed + 123.7, seedling_strength, time));
                mass = add_cloud_mass(mass, cloud_instance_shadow_mass_at(sample_pos, wake_center, wake_radii, base_seed + 151.2, wake_strength, time));
                return mass;
            }

            vec3 merged_cloud_surface_normal(vec3 sample_pos, vec3 cluster_center, vec3 cluster_radii, float time, float base_seed) {
                vec3 local = (sample_pos - cluster_center) / cluster_radii;
                vec3 broad = normalize(vec3(local.x / cluster_radii.x, local.y / cluster_radii.y, local.z / cluster_radii.z) + vec3(0.0, 0.00008, 0.0));
                vec3 detail = normalize(vec3(
                    value_noise(sample_pos.yz * 0.010 + vec2(time * 0.09, base_seed * 0.013)) - 0.5,
                    value_noise(sample_pos.xz * 0.009 + vec2(-time * 0.08, base_seed * 0.017)) - 0.5,
                    value_noise(sample_pos.xy * 0.010 + vec2(time * 0.10, base_seed * 0.019)) - 0.5
                ));
                return normalize(broad * 0.66 + detail * 0.34);
            }

            void render_merged_cloud_field(
                vec3 cluster_center,
                vec3 cluster_radii,
                vec3 cloud_center,
                vec3 main_radii,
                float main_strength,
                vec3 left_center,
                vec3 left_radii,
                float left_strength,
                vec3 left_bridge_center,
                vec3 left_bridge_radii,
                float left_bridge_strength,
                vec3 right_center,
                vec3 right_radii,
                float right_strength,
                vec3 bridge_center,
                vec3 bridge_radii,
                float bridge_strength,
                vec3 high_center,
                vec3 high_radii,
                float high_strength,
                vec3 newborn_center,
                vec3 newborn_radii,
                float newborn_strength,
                vec3 seedling_center,
                vec3 seedling_radii,
                float seedling_strength,
                vec3 wake_center,
                vec3 wake_radii,
                float wake_strength,
                float base_seed,
                vec3 camera_position,
                vec3 ray,
                vec3 sun,
                float time,
                out vec3 volume_color,
                out float volume_alpha,
                out float inside_mist,
                out float sun_occluder
            ) {
                volume_color = vec3(0.0);
                volume_alpha = 0.0;
                inside_mist = 0.0;
                sun_occluder = 0.0;

                float camera_mass = merged_cloud_density_at(camera_position, time, base_seed, cloud_center, main_radii, main_strength, left_center, left_radii, left_strength, left_bridge_center, left_bridge_radii, left_bridge_strength, right_center, right_radii, right_strength, bridge_center, bridge_radii, bridge_strength, high_center, high_radii, high_strength, newborn_center, newborn_radii, newborn_strength, seedling_center, seedling_radii, seedling_strength, wake_center, wake_radii, wake_strength);
                inside_mist = clamp(smoothstep(0.16, 0.70, camera_mass) * 0.97, 0.0, 0.97);

                float volume_t0 = 0.0;
                float volume_t1 = 0.0;
                if (!ray_ellipsoid_interval(camera_position, ray, cluster_center, cluster_radii, volume_t0, volume_t1)) {
                    sun_occluder = inside_mist * 0.96;
                    return;
                }

                float start_t = max(volume_t0, 0.0);
                float end_t = volume_t1;
                float span = max(end_t - start_t, 0.0);
                float step_len = span / 12.0;
                float march_jitter = 0.0;
                vec3 accum = vec3(0.0);
                float max_density = 0.0;
                float optical_mass = 0.0;

                for (int i = 0; i < 12; i++) {
                    float t = start_t + (float(i) + 0.5 + march_jitter) * step_len;
                    vec3 sample_pos = camera_position + ray * t;
                    float density = merged_cloud_density_at(sample_pos, time, base_seed, cloud_center, main_radii, main_strength, left_center, left_radii, left_strength, left_bridge_center, left_bridge_radii, left_bridge_strength, right_center, right_radii, right_strength, bridge_center, bridge_radii, bridge_strength, high_center, high_radii, high_strength, newborn_center, newborn_radii, newborn_strength, seedling_center, seedling_radii, seedling_strength, wake_center, wake_radii, wake_strength);
                    float shell = smoothstep(0.05, 0.28, density) * (1.0 - smoothstep(0.84, 1.65, density));
                    vec3 noise_pos = sample_pos * 0.0010 + vec3(time * 0.018, base_seed * 0.013, -time * 0.012);
                    float surface_noise = fbm_fast(sample_pos.xz * 0.0078 + vec2(time * 0.16 + base_seed * 0.017, sample_pos.y * 0.0045));
                    float fibre_noise = fbm_fast(sample_pos.xy * 0.0110 + vec2(-time * 0.13, base_seed * 0.031 + sample_pos.z * 0.0035));
                    float pearl_noise = value_noise3(noise_pos * 18.0 + vec3(base_seed * 0.031, time * 0.21, 2.7));
                    float pin_noise = value_noise(sample_pos.xz * 0.030 + vec2(-time * 0.30, base_seed * 0.047 + sample_pos.y * 0.005));
                    float foam_detail = clamp(surface_noise * 0.38 + fibre_noise * 0.34 + pearl_noise * 0.28, 0.0, 1.0);
                    density = max(0.0, density
                        + (surface_noise - 0.50) * shell * 0.085
                        + (fibre_noise - 0.50) * shell * 0.065
                        + (pearl_noise - 0.48) * shell * 0.060
                        - max(0.0, 0.42 - pin_noise) * shell * 0.070);
                    if (density <= 0.010) {
                        continue;
                    }

                    vec3 local_cluster = (sample_pos - cluster_center) / cluster_radii;
                    float height_light = smoothstep(-0.62, 0.74, local_cluster.y);
                    float core = smoothstep(0.42, 1.08, density);
                    max_density = max(max_density, density);
                    optical_mass += density * step_len * 0.0018;

                    vec3 shadow_near_local = (sample_pos + sun * 300.0 - cluster_center) / cluster_radii;
                    vec3 shadow_mid_local = (sample_pos + sun * 720.0 - cluster_center) / cluster_radii;
                    vec3 shadow_far_local = (sample_pos + sun * 1260.0 - cluster_center) / cluster_radii;
                    float shadow_near = 1.0 - smoothstep(0.60, 1.22, dot(shadow_near_local, shadow_near_local));
                    float shadow_mid = 1.0 - smoothstep(0.54, 1.14, dot(shadow_mid_local, shadow_mid_local));
                    float shadow_far = 1.0 - smoothstep(0.48, 1.06, dot(shadow_far_local, shadow_far_local));
                    float shadow_texture = fbm_fast(sample_pos.xz * 0.0028 + vec2(time * 0.055 + base_seed * 0.013, sample_pos.y * 0.0019));
                    float light_depth = (shadow_near * 0.68 + shadow_mid * 0.42 + shadow_far * 0.22) * (0.64 + shadow_texture * 0.38) + density * 0.38;
                    float light_transmittance = clamp(exp(-light_depth * 1.10), 0.12, 1.0);

                    float broad_sun = clamp(0.52 + sun.y * 0.30 + dot(normalize(cluster_center - sample_pos), sun) * 0.12, 0.18, 0.86);
                    float light_face = clamp(broad_sun + (foam_detail - 0.5) * shell * 0.055, 0.14, 0.88);
                    float dark_face = smoothstep(0.04, -0.58, local_cluster.y) * (1.0 - light_transmittance);
                    float underside = smoothstep(-0.02, -0.66, local_cluster.y) * (1.0 - clamp(sun.y, 0.0, 1.0) * 0.35);

                    vec3 cool_shadow = mix(vec3(0.55, 0.60, 0.68), vec3(0.82, 0.85, 0.88), height_light);
                    vec3 ambient_color = cool_shadow * (0.70 + height_light * 0.20 + shell * 0.035);
                    vec3 direct_color = vec3(1.04, 1.03, 0.99) * (light_face * light_transmittance * 0.18);
                    vec3 bounce_color = vec3(0.85, 0.92, 1.0) * (0.30 + height_light * 0.14 + foam_detail * shell * 0.025) * (1.0 - underside * 0.24);
                    vec3 scatter_color = vec3(0.96, 0.93, 0.84) * (shell * light_face * 0.0008);
                    vec3 sample_color = ambient_color + direct_color + bounce_color + scatter_color;
                    sample_color = mix(sample_color, vec3(0.93, 0.94, 0.92), core * 0.020 + light_face * 0.010);
                    sample_color *= 1.0 - (1.0 - light_transmittance) * (0.46 + dark_face * 0.20) * smoothstep(0.12, 1.02, density);
                    sample_color *= 1.0 - underside * core * 0.22;
                    sample_color *= 0.86 + foam_detail * 0.12 + fibre_noise * shell * 0.060 + pin_noise * shell * 0.030;
                    sample_color = max(sample_color, vec3(0.20, 0.21, 0.23));

                    float extinction = density * 7.75 + core * 1.18 + shell * 0.56;
                    float sample_alpha = (1.0 - exp(-extinction * step_len * 0.00162)) * (1.0 - volume_alpha);
                    accum += sample_color * sample_alpha;
                    volume_alpha += sample_alpha;
                    if (volume_alpha > 0.991) {
                        break;
                    }
                }

                if (volume_alpha > 0.0005) {
                    volume_color = accum / max(volume_alpha, 0.001);
                }

                float optical_alpha = 1.0 - exp(-optical_mass * 2.05);
                float mid_body_floor = smoothstep(0.16, 0.48, max_density) * 0.25;
                float core_body_floor = smoothstep(0.50, 1.10, max_density) * 0.82;
                volume_alpha = clamp(max(volume_alpha, max(optical_alpha, max(mid_body_floor, core_body_floor))), 0.0, 0.995);
                volume_color = mix(volume_color, vec3(0.94, 0.95, 0.92), smoothstep(0.90, 2.10, optical_mass) * 0.08);
                sun_occluder = max(volume_alpha * 1.04, inside_mist * 0.96);
            }

            void main() {
                vec2 resolution = max(pc.resolution_weather.xy, vec2(1.0));
                vec2 uv = gl_FragCoord.xy / resolution;
                float aspect = resolution.x / resolution.y;
                vec3 forward = normalize(pc.camera_forward_tan_x.xyz);
                vec3 right = normalize(pc.camera_right_tan_y.xyz);
                vec3 up = normalize(pc.camera_up_time.xyz);
                float tan_x = pc.camera_forward_tan_x.w;
                float tan_y = pc.camera_right_tan_y.w;
                float time = pc.camera_up_time.w;
                vec3 camera_position = pc.camera_position_seed.xyz;
                vec3 sun = normalize(pc.sun_direction_radius.xyz);

                vec2 ndc = vec2(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
                vec3 ray = normalize(forward + right * ndc.x * tan_x + up * ndc.y * tan_y);
                float horizon = smoothstep(-0.10, 0.42, ray.y);
                float zenith = smoothstep(-0.02, 0.88, ray.y);
                vec3 horizon_blue = vec3(0.30, 0.58, 1.0);
                vec3 upper_blue = vec3(0.018, 0.18, 0.86);
                vec3 color = mix(horizon_blue, upper_blue, pow(zenith, 0.62));
                color = mix(color, vec3(0.96, 0.70, 0.42), (1.0 - horizon) * 0.018);

                float sun_view_z = max(dot(sun, forward), 0.001);
                vec2 sun_screen = vec2(
                    0.5 + dot(sun, right) / (sun_view_z * tan_x * 2.0),
                    0.5 - dot(sun, up) / (sun_view_z * tan_y * 2.0)
                );
                vec2 sun_delta = vec2((uv.x - sun_screen.x) * aspect, uv.y - sun_screen.y);
                float sun_distance = length(sun_delta);
                float sun_radius = clamp(pc.sun_direction_radius.w * 2.15 / max(2.0 * atan(tan_y), 0.01), 0.013, 0.044);
                float sun_disc = 1.0 - smoothstep(sun_radius * 0.78, sun_radius * 1.22, sun_distance);
                float sun_glow = exp(-sun_distance * 8.5);
                color += vec3(1.0, 0.78, 0.45) * exp(-sun_distance * 3.2) * 0.14;

                vec3 cloud_center = vec3(
                    sin(pc.camera_position_seed.w * 12.9898) * 420.0 + sin(time * 0.025) * 80.0,
                    760.0,
                    3800.0
                );
                float split_cycle = 0.5 + 0.5 * sin(time * 0.11 + pc.camera_position_seed.w * 0.071);
                float split_open = smoothstep(0.18, 0.86, split_cycle);
                float merge_open = 1.0 - smoothstep(0.70, 1.0, abs(split_cycle * 2.0 - 1.0));
                float detach_open = smoothstep(0.48, 0.92, split_cycle);

                vec3 main_radii = vec3(920.0, 560.0, 680.0);
                vec3 left_center = cloud_center + vec3(-960.0 - split_open * 620.0 + sin(time * 0.047) * 130.0, 40.0 + sin(time * 0.033) * 100.0, 300.0 + cos(time * 0.041) * 190.0);
                vec3 right_center = cloud_center + vec3(980.0 + split_open * 650.0 + cos(time * 0.039) * 140.0, -35.0 + cos(time * 0.044) * 95.0, 620.0 + sin(time * 0.050) * 210.0);
                vec3 high_center = cloud_center + vec3(110.0 + sin(time * 0.061) * 300.0, 455.0 + split_open * 210.0, -310.0 + cos(time * 0.052) * 240.0);
                vec3 newborn_center = mix(cloud_center + vec3(360.0, 80.0, 120.0), right_center + vec3(430.0, 110.0, -180.0), split_open);
                vec3 bridge_center = mix(cloud_center, right_center, 0.48) + vec3(0.0, 45.0 + sin(time * 0.08) * 35.0, -90.0);
                vec3 left_bridge_center = mix(cloud_center, left_center, 0.45) + vec3(0.0, 30.0 + cos(time * 0.075) * 35.0, 40.0);
                vec3 seedling_center = newborn_center + vec3(520.0 + detach_open * 520.0 + sin(time * 0.16) * 90.0, 140.0 + sin(time * 0.11) * 80.0, -310.0 + cos(time * 0.13) * 130.0);
                vec3 wake_center = left_center + vec3(-520.0 - detach_open * 360.0, 130.0 + cos(time * 0.12) * 70.0, -260.0 + sin(time * 0.10) * 150.0);

                vec3 left_radii = vec3(560.0, 350.0, 460.0) * (0.88 + merge_open * 0.14);
                vec3 right_radii = vec3(590.0, 365.0, 480.0) * (0.86 + split_open * 0.13);
                vec3 high_radii = vec3(600.0, 260.0, 390.0) * (0.84 + split_open * 0.16);
                vec3 newborn_radii = vec3(370.0, 210.0, 285.0) * (0.72 + split_open * 0.30);
                vec3 bridge_radii = vec3(430.0, 170.0, 250.0) * (0.48 + merge_open * 0.34);
                vec3 left_bridge_radii = vec3(380.0, 165.0, 245.0) * (0.46 + merge_open * 0.32);
                vec3 seedling_radii = vec3(310.0, 170.0, 235.0) * (0.62 + detach_open * 0.38);
                vec3 wake_radii = vec3(360.0, 175.0, 270.0) * (0.58 + detach_open * 0.32);

                float main_strength = 0.76 + merge_open * 0.08;
                float left_strength = 0.82 + merge_open * 0.12;
                float left_bridge_strength = merge_open * 0.38;
                float right_strength = 0.84 + split_open * 0.12;
                float bridge_strength = merge_open * 0.42;
                float high_strength = 0.56 + split_open * 0.30;
                float newborn_strength = split_open * 0.82;
                float seedling_strength = detach_open * 0.72;
                float wake_strength = detach_open * 0.56;
                vec3 cluster_center = cloud_center + vec3(220.0, 120.0, 300.0);
                vec3 cluster_radii = vec3(4380.0 + detach_open * 340.0, 1620.0, 2660.0);

                vec3 cloud_union_color = vec3(0.0);
                float cloud_union_alpha = 0.0;
                float inside_mist = 0.0;
                float sun_occluder = 0.0;
                vec3 inside_color = vec3(0.91, 0.92, 0.88) + vec3(0.04, 0.045, 0.055) * smoothstep(-0.20, 0.55, ray.y);

                render_merged_cloud_field(
                    cluster_center,
                    cluster_radii,
                    cloud_center,
                    main_radii,
                    main_strength,
                    left_center,
                    left_radii,
                    left_strength,
                    left_bridge_center,
                    left_bridge_radii,
                    left_bridge_strength,
                    right_center,
                    right_radii,
                    right_strength,
                    bridge_center,
                    bridge_radii,
                    bridge_strength,
                    high_center,
                    high_radii,
                    high_strength,
                    newborn_center,
                    newborn_radii,
                    newborn_strength,
                    seedling_center,
                    seedling_radii,
                    seedling_strength,
                    wake_center,
                    wake_radii,
                    wake_strength,
                    pc.camera_position_seed.w,
                    camera_position,
                    ray,
                    sun,
                    time,
                    cloud_union_color,
                    cloud_union_alpha,
                    inside_mist,
                    sun_occluder
                );

                color = mix(color, cloud_union_color, cloud_union_alpha);
                color = mix(color, inside_color, inside_mist);
                float sun_occlusion = 1.0 - sun_occluder * smoothstep(0.38, 0.0, sun_distance) * 0.98;
                color += vec3(1.0, 0.92, 0.74) * sun_disc * sun_occlusion * 3.6;
                color += vec3(1.0, 0.80, 0.48) * sun_glow * sun_occlusion * 0.18;
                color = pow(aces(color), vec3(1.0 / 2.2));
                out_color = vec4(color, 1.0);
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
    [
        (seed as f32 * 12.9898).sin() * 420.0 + (time_seconds * 0.025).sin() * 80.0,
        760.0,
        3800.0,
    ]
}

fn gpu_cloud_radii_m() -> [f32; 3] {
    [1550.0, 760.0, 880.0]
}

fn cloud_proximity_label(camera: [f32; 3], center: [f32; 3], radii: [f32; 3]) -> String {
    let delta = [
        camera[0] - center[0],
        camera[1] - center[1],
        camera[2] - center[2],
    ];
    let center_distance = dot3(delta, delta).sqrt();
    let local = [
        delta[0] / radii[0].max(f32::EPSILON),
        delta[1] / radii[1].max(f32::EPSILON),
        delta[2] / radii[2].max(f32::EPSILON),
    ];
    let local_distance = dot3(local, local).sqrt();
    if local_distance <= 1.0 {
        "inside cloud".to_string()
    } else {
        let surface_distance = center_distance * (1.0 - 1.0 / local_distance);
        format!("{:.0}m to cloud", surface_distance.max(0.0))
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
