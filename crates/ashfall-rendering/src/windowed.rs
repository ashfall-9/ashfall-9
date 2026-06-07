use std::{error::Error, sync::Arc, time::Instant};

use ashfall_core::{
    core::{
        EmotionState, EntityId, MaterialDescriptor, MaterialState, MeshAssetHandle, QualityTier,
        Transform,
    },
    world::{HumanState, Renderable, WorldEvent, WorldEventKind, WorldSnapshot},
};
use ashfall_human::{
    HumanProxyBox, HumanProxyFaceFeature, HumanProxyFaceFeatureKind, HumanProxyGeometry,
    HumanProxyPart, HumanSurfaceState, evaluate_human_surface_response,
    human_proxy_geometry_for_quality, human_proxy_geometry_for_state,
    human_surface_response_from_snapshot, surface_response_profile_for_human_state,
};
use vulkano::{
    VulkanError,
    buffer::{Buffer, BufferContents, BufferCreateInfo, BufferUsage, Subbuffer},
    command_buffer::{
        AutoCommandBufferBuilder, CommandBufferUsage, RenderPassBeginInfo,
        allocator::StandardCommandBufferAllocator,
    },
    format::Format,
    image::{Image, ImageCreateInfo, ImageType, ImageUsage, view::ImageView},
    memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator},
    pipeline::{
        GraphicsPipeline, Pipeline, PipelineLayout, PipelineShaderStageCreateInfo,
        graphics::{
            GraphicsPipelineCreateInfo,
            color_blend::{AttachmentBlend, ColorBlendAttachmentState, ColorBlendState},
            depth_stencil::{CompareOp, DepthState, DepthStencilState},
            input_assembly::InputAssemblyState,
            multisample::MultisampleState,
            rasterization::RasterizationState,
            vertex_input::{Vertex, VertexDefinition},
            viewport::{Viewport, ViewportState},
        },
        layout::PipelineDescriptorSetLayoutCreateInfo,
    },
    render_pass::{Framebuffer, FramebufferCreateInfo, RenderPass},
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
    error::EventLoopError,
    event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowId},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WindowRenderMode {
    #[default]
    Beauty,
    Debug,
    Mixed,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowDebugOverlayFlags {
    pub city_chunks: bool,
    pub streaming_cells: bool,
    pub navigation_graph: bool,
    pub material_placements: bool,
    pub event_markers: bool,
    pub gas_volumes: bool,
    pub route_consequences: bool,
    pub performance_hud: bool,
}

impl WindowDebugOverlayFlags {
    pub const fn none() -> Self {
        Self {
            city_chunks: false,
            streaming_cells: false,
            navigation_graph: false,
            material_placements: false,
            event_markers: false,
            gas_volumes: false,
            route_consequences: false,
            performance_hud: false,
        }
    }

    pub const fn all() -> Self {
        Self {
            city_chunks: true,
            streaming_cells: true,
            navigation_graph: true,
            material_placements: true,
            event_markers: true,
            gas_volumes: true,
            route_consequences: true,
            performance_hud: true,
        }
    }

    pub fn any_enabled(self) -> bool {
        self.city_chunks
            || self.streaming_cells
            || self.navigation_graph
            || self.material_placements
            || self.event_markers
            || self.gas_volumes
            || self.route_consequences
            || self.performance_hud
    }
}

impl Default for WindowDebugOverlayFlags {
    fn default() -> Self {
        Self::none()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowBeautyFrameBudget {
    pub target_frame_ms: f32,
    pub target_cpu_ms: f32,
    pub target_gpu_ms: f32,
    pub max_geometry_upload_mb_per_frame: f32,
    pub max_material_pages_generated_per_frame: u32,
    pub max_shadow_pages_updated_per_frame: u32,
    pub minimum_foreground_lod_bias: f32,
}

impl Default for WindowBeautyFrameBudget {
    fn default() -> Self {
        Self {
            target_frame_ms: 16.67,
            target_cpu_ms: 6.0,
            target_gpu_ms: 9.5,
            max_geometry_upload_mb_per_frame: 8.0,
            max_material_pages_generated_per_frame: 12,
            max_shadow_pages_updated_per_frame: 16,
            minimum_foreground_lod_bias: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowNaturalEnvironmentConfig {
    pub enable_sky: bool,
    pub enable_sun: bool,
    pub enable_moon: bool,
    pub enable_clouds: bool,
    pub enable_fog: bool,
    pub enable_rain: bool,
    pub neon_is_accent_only: bool,
    pub sun_direction_world: [f32; 3],
    pub moon_direction_world: [f32; 3],
    pub cloud_coverage: f32,
    pub fog_density: f32,
    pub rain_intensity: f32,
    pub exposure_value: f32,
    pub white_balance_kelvin: f32,
}

impl WindowNaturalEnvironmentConfig {
    pub const fn rainy_alley_default() -> Self {
        Self {
            enable_sky: true,
            enable_sun: true,
            enable_moon: true,
            enable_clouds: true,
            enable_fog: true,
            enable_rain: true,
            neon_is_accent_only: true,
            sun_direction_world: [0.24, -0.78, 0.58],
            moon_direction_world: [-0.36, 0.54, 0.76],
            cloud_coverage: 0.62,
            fog_density: 0.012,
            rain_intensity: 0.38,
            exposure_value: 1.18,
            white_balance_kelvin: 6200.0,
        }
    }
}

impl Default for WindowNaturalEnvironmentConfig {
    fn default() -> Self {
        Self::rainy_alley_default()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowedRendererConfig {
    pub title: String,
    pub width: f32,
    pub height: f32,
    pub print_device_name: bool,
    pub render_mode: WindowRenderMode,
    pub debug_overlays: WindowDebugOverlayFlags,
    pub beauty_frame_budget: WindowBeautyFrameBudget,
    pub natural_environment: WindowNaturalEnvironmentConfig,
}

impl Default for WindowedRendererConfig {
    fn default() -> Self {
        Self {
            title: "Ashfall".to_string(),
            width: 1280.0,
            height: 720.0,
            print_device_name: true,
            render_mode: WindowRenderMode::Beauty,
            debug_overlays: WindowDebugOverlayFlags::none(),
            beauty_frame_budget: WindowBeautyFrameBudget::default(),
            natural_environment: WindowNaturalEnvironmentConfig::default(),
        }
    }
}

impl WindowedRendererConfig {
    pub fn beauty_default(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }

    pub fn debug_default(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            render_mode: WindowRenderMode::Debug,
            debug_overlays: WindowDebugOverlayFlags::all(),
            ..Self::default()
        }
    }

    pub fn mixed_default(
        title: impl Into<String>,
        debug_overlays: WindowDebugOverlayFlags,
    ) -> Self {
        Self {
            title: title.into(),
            render_mode: WindowRenderMode::Mixed,
            debug_overlays,
            ..Self::default()
        }
    }

    pub fn is_beauty_mode(&self) -> bool {
        matches!(self.render_mode, WindowRenderMode::Beauty)
    }

    pub fn allows_debug_geometry(&self) -> bool {
        matches!(self.render_mode, WindowRenderMode::Debug)
            || (matches!(self.render_mode, WindowRenderMode::Mixed)
                && self.debug_overlays.any_enabled())
    }
}

pub const WINDOW_CAMERA_DEFAULT_FOV_Y_RADIANS: f32 = std::f32::consts::FRAC_PI_3;
pub const WINDOW_CAMERA_DEFAULT_ASPECT_RATIO: f32 = 16.0 / 9.0;
pub const WINDOW_CAMERA_DEFAULT_NEAR_METERS: f32 = 0.08;
pub const WINDOW_CAMERA_DEFAULT_FAR_METERS: f32 = 80.0;
pub const WINDOW_DEFAULT_VIEWPORT_SIZE_PIXELS: [f32; 2] = [1280.0, 720.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowInputState {
    pub move_forward: bool,
    pub move_backward: bool,
    pub move_left: bool,
    pub move_right: bool,
    pub move_up: bool,
    pub move_down: bool,
    pub turn_left: bool,
    pub turn_right: bool,
    pub turn_up: bool,
    pub turn_down: bool,
    pub fast_modifier: bool,
    pub action_primary: bool,
    pub mouse_delta: [f32; 2],
    pub viewport_size_pixels: [f32; 2],
}

impl Default for WindowInputState {
    fn default() -> Self {
        Self {
            move_forward: false,
            move_backward: false,
            move_left: false,
            move_right: false,
            move_up: false,
            move_down: false,
            turn_left: false,
            turn_right: false,
            turn_up: false,
            turn_down: false,
            fast_modifier: false,
            action_primary: false,
            mouse_delta: [0.0, 0.0],
            viewport_size_pixels: WINDOW_DEFAULT_VIEWPORT_SIZE_PIXELS,
        }
    }
}

impl WindowInputState {
    fn reset_frame_delta(&mut self) {
        self.action_primary = false;
        self.mouse_delta = [0.0, 0.0];
    }

    pub fn viewport_aspect_ratio(&self) -> f32 {
        window_viewport_aspect_ratio(self.viewport_size_pixels)
            .unwrap_or(WINDOW_CAMERA_DEFAULT_ASPECT_RATIO)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowCameraBasis {
    pub forward: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowCameraControlSettings {
    pub base_vertical_speed_meters_per_second: f32,
    pub fast_vertical_speed_meters_per_second: f32,
    pub key_turn_radians_per_second: f32,
    pub mouse_sensitivity_radians_per_pixel: f32,
    pub min_pitch_radians: f32,
    pub max_pitch_radians: f32,
    pub min_height_meters: f32,
}

impl Default for WindowCameraControlSettings {
    fn default() -> Self {
        Self {
            base_vertical_speed_meters_per_second: 3.5,
            fast_vertical_speed_meters_per_second: 9.0,
            key_turn_radians_per_second: 1.7,
            mouse_sensitivity_radians_per_pixel: 0.0025,
            min_pitch_radians: -1.2,
            max_pitch_radians: 1.2,
            min_height_meters: 0.25,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowPerspectiveCamera {
    pub position: [f32; 3],
    pub yaw_radians: f32,
    pub pitch_radians: f32,
    pub fov_y_radians: f32,
    pub aspect_ratio: f32,
    pub near_meters: f32,
    pub far_meters: f32,
}

impl WindowPerspectiveCamera {
    pub fn new(position: [f32; 3], yaw_radians: f32, pitch_radians: f32) -> Self {
        Self {
            position,
            yaw_radians,
            pitch_radians,
            fov_y_radians: WINDOW_CAMERA_DEFAULT_FOV_Y_RADIANS,
            aspect_ratio: WINDOW_CAMERA_DEFAULT_ASPECT_RATIO,
            near_meters: WINDOW_CAMERA_DEFAULT_NEAR_METERS,
            far_meters: WINDOW_CAMERA_DEFAULT_FAR_METERS,
        }
    }

    pub fn basis(&self) -> WindowCameraBasis {
        window_camera_basis(self.yaw_radians, self.pitch_radians)
    }

    pub fn world_to_clip_matrix(&self) -> [[f32; 4]; 4] {
        window_world_to_clip_matrix(self)
    }

    pub fn projected_ndc(&self, point: [f32; 3]) -> Option<[f32; 3]> {
        window_projected_ndc(self.world_to_clip_matrix(), point, self.near_meters)
    }

    pub fn set_aspect_ratio(&mut self, aspect_ratio: f32) {
        if aspect_ratio.is_finite() && aspect_ratio > f32::EPSILON {
            self.aspect_ratio = aspect_ratio;
        }
    }

    pub fn set_viewport_size_pixels(&mut self, viewport_size_pixels: [f32; 2]) {
        if let Some(aspect_ratio) = window_viewport_aspect_ratio(viewport_size_pixels) {
            self.aspect_ratio = aspect_ratio;
        }
    }

    pub fn apply_controls(
        &mut self,
        input: &WindowInputState,
        dt_seconds: f32,
        settings: &WindowCameraControlSettings,
    ) {
        if input.turn_left {
            self.yaw_radians -= settings.key_turn_radians_per_second * dt_seconds;
        }
        if input.turn_right {
            self.yaw_radians += settings.key_turn_radians_per_second * dt_seconds;
        }
        if input.turn_up {
            self.pitch_radians += settings.key_turn_radians_per_second * dt_seconds;
        }
        if input.turn_down {
            self.pitch_radians -= settings.key_turn_radians_per_second * dt_seconds;
        }

        self.yaw_radians += input.mouse_delta[0] * settings.mouse_sensitivity_radians_per_pixel;
        self.pitch_radians = (self.pitch_radians
            - input.mouse_delta[1] * settings.mouse_sensitivity_radians_per_pixel)
            .clamp(settings.min_pitch_radians, settings.max_pitch_radians);

        let speed = if input.fast_modifier {
            settings.fast_vertical_speed_meters_per_second
        } else {
            settings.base_vertical_speed_meters_per_second
        };
        let step = speed * dt_seconds;
        if input.move_up {
            self.position[2] += step;
        }
        if input.move_down {
            self.position[2] = (self.position[2] - step).max(settings.min_height_meters);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowSnapshotSceneOptions<'a> {
    pub snapshot: &'a WorldSnapshot,
    pub frame_index: u64,
    pub camera: WindowPerspectiveCamera,
    pub player_entity: Option<EntityId>,
}

impl<'a> WindowSnapshotSceneOptions<'a> {
    pub fn new(
        snapshot: &'a WorldSnapshot,
        frame_index: u64,
        camera: WindowPerspectiveCamera,
    ) -> Self {
        Self {
            snapshot,
            frame_index,
            camera,
            player_entity: None,
        }
    }

    pub fn with_player_entity(mut self, player_entity: EntityId) -> Self {
        self.player_entity = Some(player_entity);
        self
    }
}

pub fn window_viewport_aspect_ratio(viewport_size_pixels: [f32; 2]) -> Option<f32> {
    let [width, height] = viewport_size_pixels;
    if width.is_finite() && height.is_finite() && width > f32::EPSILON && height > f32::EPSILON {
        Some(width / height)
    } else {
        None
    }
}

pub fn window_camera_basis(yaw_radians: f32, pitch_radians: f32) -> WindowCameraBasis {
    let yaw_sin = yaw_radians.sin();
    let yaw_cos = yaw_radians.cos();
    let pitch_sin = pitch_radians.sin();
    let pitch_cos = pitch_radians.cos();
    let forward = [yaw_sin * pitch_cos, yaw_cos * pitch_cos, pitch_sin];
    let right = [yaw_cos, -yaw_sin, 0.0];
    let up = cross3(right, forward);
    WindowCameraBasis { forward, right, up }
}

pub fn window_world_to_clip_matrix(camera: &WindowPerspectiveCamera) -> [[f32; 4]; 4] {
    let basis = camera.basis();
    let focal = 1.0 / (camera.fov_y_radians * 0.5).tan();
    let x_scale = focal / camera.aspect_ratio;
    let y_scale = -focal;
    let z_scale = camera.far_meters / (camera.far_meters - camera.near_meters);
    let z_bias =
        -(camera.far_meters * camera.near_meters) / (camera.far_meters - camera.near_meters);
    let eye = camera.position;
    let rows = [
        [
            basis.right[0] * x_scale,
            basis.right[1] * x_scale,
            basis.right[2] * x_scale,
            -dot3(basis.right, eye) * x_scale,
        ],
        [
            basis.up[0] * y_scale,
            basis.up[1] * y_scale,
            basis.up[2] * y_scale,
            -dot3(basis.up, eye) * y_scale,
        ],
        [
            basis.forward[0] * z_scale,
            basis.forward[1] * z_scale,
            basis.forward[2] * z_scale,
            z_bias - dot3(basis.forward, eye) * z_scale,
        ],
        [
            basis.forward[0],
            basis.forward[1],
            basis.forward[2],
            -dot3(basis.forward, eye),
        ],
    ];
    transpose_matrix4(rows)
}

pub fn window_projected_ndc(
    matrix: [[f32; 4]; 4],
    point: [f32; 3],
    near_meters: f32,
) -> Option<[f32; 3]> {
    let clip = window_transform_point(matrix, point);
    if clip[3] <= near_meters {
        return None;
    }

    Some([clip[0] / clip[3], clip[1] / clip[3], clip[2] / clip[3]])
}

pub fn window_transform_point(matrix: [[f32; 4]; 4], point: [f32; 3]) -> [f32; 4] {
    let vector = [point[0], point[1], point[2], 1.0];
    [
        matrix[0][0] * vector[0]
            + matrix[1][0] * vector[1]
            + matrix[2][0] * vector[2]
            + matrix[3][0] * vector[3],
        matrix[0][1] * vector[0]
            + matrix[1][1] * vector[1]
            + matrix[2][1] * vector[2]
            + matrix[3][1] * vector[3],
        matrix[0][2] * vector[0]
            + matrix[1][2] * vector[1]
            + matrix[2][2] * vector[2]
            + matrix[3][2] * vector[3],
        matrix[0][3] * vector[0]
            + matrix[1][3] * vector[1]
            + matrix[2][3] * vector[2]
            + matrix[3][3] * vector[3],
    ]
}

fn transpose_matrix4(rows: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    [
        [rows[0][0], rows[1][0], rows[2][0], rows[3][0]],
        [rows[0][1], rows[1][1], rows[2][1], rows[3][1]],
        [rows[0][2], rows[1][2], rows[2][2], rows[3][2]],
        [rows[0][3], rows[1][3], rows[2][3], rows[3][3]],
    ]
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowFrameState {
    pub clear_color: [f32; 4],
    pub vertices: Vec<WindowSceneVertex>,
    pub indices: Vec<u32>,
    pub world_to_clip: [[f32; 4]; 4],
    pub dynamic_light: WindowDynamicLight,
    pub atmosphere: WindowAtmosphere,
    pub title: Option<String>,
}

impl Default for WindowFrameState {
    fn default() -> Self {
        Self {
            clear_color: [0.36, 0.44, 0.54, 1.0],
            vertices: Vec::new(),
            indices: Vec::new(),
            world_to_clip: identity_matrix4(),
            dynamic_light: WindowDynamicLight::disabled(),
            atmosphere: WindowAtmosphere::wet_alley(),
            title: None,
        }
    }
}

fn identity_matrix4() -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

#[derive(BufferContents, Vertex, Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct WindowSceneVertex {
    #[format(R32G32B32_SFLOAT)]
    pub position: [f32; 3],
    #[format(R32G32B32_SFLOAT)]
    pub normal: [f32; 3],
    #[format(R32G32B32A32_SFLOAT)]
    pub color: [f32; 4],
    #[format(R32_UINT)]
    pub coordinate_space: u32,
    #[format(R32G32B32A32_SFLOAT)]
    pub surface_response: [f32; 4],
    #[format(R32G32B32A32_SFLOAT)]
    pub material_detail: [f32; 4],
    #[format(R32G32B32A32_SFLOAT)]
    pub generated_cache: [f32; 4],
}

impl Default for WindowSceneVertex {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            normal: [0.0, 0.0, 1.0],
            color: [0.0; 4],
            coordinate_space: Self::WORLD_SPACE,
            surface_response: WINDOW_SURFACE_RESPONSE_DEFAULT,
            material_detail: WINDOW_SURFACE_DETAIL_DEFAULT,
            generated_cache: WINDOW_SURFACE_CACHE_DEFAULT,
        }
    }
}

impl WindowSceneVertex {
    pub const WORLD_SPACE: u32 = 0;
    pub const SCREEN_SPACE: u32 = 1;

    pub fn world(position: [f32; 3], normal: [f32; 3], color: [f32; 4]) -> Self {
        Self {
            position,
            normal,
            color,
            coordinate_space: Self::WORLD_SPACE,
            surface_response: WINDOW_SURFACE_RESPONSE_DEFAULT,
            material_detail: WINDOW_SURFACE_DETAIL_DEFAULT,
            generated_cache: WINDOW_SURFACE_CACHE_DEFAULT,
        }
    }

    pub fn world_with_surface_response(
        position: [f32; 3],
        normal: [f32; 3],
        color: [f32; 4],
        surface_response: [f32; 4],
    ) -> Self {
        Self {
            position,
            normal,
            color,
            coordinate_space: Self::WORLD_SPACE,
            surface_response: window_clamp_surface_response(surface_response),
            material_detail: WINDOW_SURFACE_DETAIL_DEFAULT,
            generated_cache: WINDOW_SURFACE_CACHE_DEFAULT,
        }
    }

    pub fn world_with_surface_detail(
        position: [f32; 3],
        normal: [f32; 3],
        color: [f32; 4],
        surface_response: [f32; 4],
        material_detail: [f32; 4],
    ) -> Self {
        Self {
            position,
            normal,
            color,
            coordinate_space: Self::WORLD_SPACE,
            surface_response: window_clamp_surface_response(surface_response),
            material_detail: window_clamp_surface_detail(material_detail),
            generated_cache: WINDOW_SURFACE_CACHE_DEFAULT,
        }
    }

    pub fn world_with_surface_detail_and_cache(
        position: [f32; 3],
        normal: [f32; 3],
        color: [f32; 4],
        surface_response: [f32; 4],
        material_detail: [f32; 4],
        generated_cache: [f32; 4],
    ) -> Self {
        Self {
            position,
            normal,
            color,
            coordinate_space: Self::WORLD_SPACE,
            surface_response: window_clamp_surface_response(surface_response),
            material_detail: window_clamp_surface_detail(material_detail),
            generated_cache: window_clamp_surface_cache(generated_cache),
        }
    }

    pub fn screen(position: [f32; 3], color: [f32; 4]) -> Self {
        Self {
            position,
            normal: [0.0, 0.0, 1.0],
            color,
            coordinate_space: Self::SCREEN_SPACE,
            surface_response: WINDOW_SURFACE_RESPONSE_DEFAULT,
            material_detail: WINDOW_SURFACE_DETAIL_DEFAULT,
            generated_cache: WINDOW_SURFACE_CACHE_DEFAULT,
        }
    }
}

pub const WINDOW_MESH_ALLEY_GLASS_INTACT: u128 = 2_000;
pub const WINDOW_MESH_ALLEY_GLASS_FRACTURED: u128 = 2_001;
pub const WINDOW_MESH_MARA_HUMAN_BUNDLE: u128 = 3_000;
pub const WINDOW_MESH_NEON_SIGN: u128 = 4_000;
pub const WINDOW_MESH_SERVICE_PIPE: u128 = 5_000;
pub const WINDOW_MESH_WET_ASPHALT: u128 = 6_000;
pub const WINDOW_MESH_SECURITY_CAMERA: u128 = 7_000;
pub const WINDOW_MESH_SERVICE_DOOR: u128 = 7_100;
pub const WINDOW_MESH_MARKET_STALL: u128 = 7_200;
pub const WINDOW_SCREEN_SPACE_DEPTH: f32 = 0.0;
pub const WINDOW_HUD_EVENT_PULSE_SECONDS: f32 = 3.25;
pub const WINDOW_HUD_EVENT_STRIP_MAX_PULSES: usize = 14;
pub const WINDOW_WORLD_EVENT_MARKER_MAX_COUNT: usize = 48;
const WINDOW_FRACTURED_CRACK_DENSITY_THRESHOLD: f32 = 0.5;
pub const WINDOW_SURFACE_DETAIL_DEFAULT: [f32; 4] = [1.0, 0.0, 0.0, 0.0];
pub const WINDOW_SURFACE_CACHE_DEFAULT: [f32; 4] = [0.0, 0.0, 0.0, 0.0];
pub const WINDOW_SURFACE_RESPONSE_DEFAULT: [f32; 4] = [0.78, 0.0, 0.0, 0.0];
pub const WINDOW_SURFACE_RESPONSE_ROUGH_DIRT: [f32; 4] = [0.96, 0.0, 0.0, 0.0];
pub const WINDOW_SURFACE_RESPONSE_GLASS: [f32; 4] = [0.08, 0.0, 0.36, 0.02];
pub const WINDOW_SURFACE_RESPONSE_METAL: [f32; 4] = [0.34, 0.75, 0.02, 0.0];
pub const WINDOW_SURFACE_RESPONSE_WET_ROAD: [f32; 4] = [0.18, 0.0, 0.92, 0.0];
pub const WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON: [f32; 4] = [0.12, 0.0, 0.18, 1.0];
pub const WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE: [f32; 4] = [0.28, 0.0, 0.05, 0.52];
pub const WINDOW_SURFACE_RESPONSE_HUMAN_SKIN: [f32; 4] = [0.52, 0.0, 0.16, 0.0];
pub const WINDOW_SURFACE_RESPONSE_HUMAN_EYE: [f32; 4] = [0.06, 0.0, 0.88, 0.04];
pub const WINDOW_SURFACE_RESPONSE_HUMAN_HAIR: [f32; 4] = [0.62, 0.0, 0.18, 0.0];
pub const WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH: [f32; 4] = [0.82, 0.0, 0.08, 0.0];
pub const WINDOW_SURFACE_RESPONSE_HUMAN_CYBERNETIC: [f32; 4] = [0.24, 0.82, 0.04, 0.18];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct WindowMaterialStateDetail {
    scale_boost: f32,
    state_intensity: f32,
    relief: f32,
    crack_density: f32,
    moisture: f32,
    soot: f32,
    corrosion: f32,
    heat: f32,
    electrical_charge: f32,
    plastic_strain: f32,
    biological_contamination: f32,
    oil_contamination: f32,
    seed_salt: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct WindowMaterialStateChannels {
    crack_density: f32,
    moisture: f32,
    soot: f32,
    corrosion: f32,
    heat: f32,
    electrical_charge: f32,
    plastic_strain: f32,
    biological_contamination: f32,
    oil_contamination: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct WindowOrientedSurfaceRect {
    center: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    half_extents: [f32; 2],
    color: [f32; 4],
    surface_response: [f32; 4],
    state_detail: WindowMaterialStateDetail,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct WindowMicroDetailRecipe {
    seed: u64,
    density: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct WindowGeneratedCityBuildingSlice {
    x0: f32,
    x1: f32,
    y0: f32,
    y1: f32,
    face_y: f32,
    side: f32,
    height: f32,
    segment: usize,
    jitter: f32,
}

impl WindowMaterialStateDetail {
    fn is_neutral(self) -> bool {
        self.scale_boost <= 1.001
            && self.state_intensity <= 0.001
            && self.relief <= 0.001
            && self.crack_density <= 0.001
            && self.moisture <= 0.001
            && self.soot <= 0.001
            && self.corrosion <= 0.001
            && self.heat <= 0.001
            && self.electrical_charge <= 0.001
            && self.plastic_strain <= 0.001
            && self.biological_contamination <= 0.001
            && self.oil_contamination <= 0.001
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowDynamicLight {
    pub position_meters: [f32; 3],
    pub radius_meters: f32,
    pub color: [f32; 3],
    pub intensity: f32,
}

impl WindowDynamicLight {
    pub fn new(
        position_meters: [f32; 3],
        radius_meters: f32,
        color: [f32; 3],
        intensity: f32,
    ) -> Self {
        Self {
            position_meters,
            radius_meters: radius_meters.max(0.0),
            color: [
                color[0].clamp(0.0, 8.0),
                color[1].clamp(0.0, 8.0),
                color[2].clamp(0.0, 8.0),
            ],
            intensity: intensity.max(0.0),
        }
    }

    pub fn disabled() -> Self {
        Self::new([0.0; 3], 0.0, [0.0; 3], 0.0)
    }

    pub fn is_enabled(&self) -> bool {
        self.radius_meters > f32::EPSILON && self.intensity > f32::EPSILON
    }

    fn position_radius(self) -> [f32; 4] {
        [
            self.position_meters[0],
            self.position_meters[1],
            self.position_meters[2],
            self.radius_meters,
        ]
    }

    fn color_intensity(self) -> [f32; 4] {
        [self.color[0], self.color[1], self.color[2], self.intensity]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowAtmosphere {
    pub fog_color: [f32; 3],
    pub fog_density_per_meter: f32,
    pub exposure: f32,
    pub bloom_strength: f32,
}

impl WindowAtmosphere {
    pub fn new(
        fog_color: [f32; 3],
        fog_density_per_meter: f32,
        exposure: f32,
        bloom_strength: f32,
    ) -> Self {
        Self {
            fog_color: [
                fog_color[0].clamp(0.0, 2.0),
                fog_color[1].clamp(0.0, 2.0),
                fog_color[2].clamp(0.0, 2.0),
            ],
            fog_density_per_meter: fog_density_per_meter.clamp(0.0, 0.18),
            exposure: exposure.clamp(0.2, 3.0),
            bloom_strength: bloom_strength.clamp(0.0, 1.5),
        }
    }

    pub fn wet_alley() -> Self {
        Self::new([0.43, 0.5, 0.58], 0.013, 1.34, 0.16)
    }

    fn color_density(self) -> [f32; 4] {
        [
            self.fog_color[0],
            self.fog_color[1],
            self.fog_color[2],
            self.fog_density_per_meter,
        ]
    }

    fn exposure_bloom(self) -> [f32; 4] {
        [self.exposure, self.bloom_strength, 0.0, 0.0]
    }
}

pub fn window_atmosphere_for_alley(
    frame_index: u64,
    event_count: usize,
    dynamic_light: WindowDynamicLight,
) -> WindowAtmosphere {
    let pulse = (frame_index as f32 * 0.023).sin().mul_add(0.5, 0.5);
    let event_pressure = (event_count as f32 / 42.0).clamp(0.0, 1.0);
    let light_energy = if dynamic_light.is_enabled() {
        (dynamic_light.intensity * dynamic_light.radius_meters / 16.0).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let light_tint = if dynamic_light.is_enabled() {
        dynamic_light.color
    } else {
        [0.86, 0.88, 0.9]
    };
    let daylight = [0.56, 0.62, 0.68];
    let neon_influence = light_energy * 0.026;
    let fog_color = [
        daylight[0] + light_tint[0] * neon_influence + event_pressure * 0.035,
        daylight[1] + light_tint[1] * (neon_influence * 0.62 + pulse * 0.006),
        daylight[2] + light_tint[2] * (neon_influence * 0.72),
    ];
    WindowAtmosphere::new(
        fog_color,
        0.009 + pulse * 0.003 + event_pressure * 0.004,
        1.18 + light_energy * 0.026 + event_pressure * 0.026,
        0.028 + light_energy * 0.030 + event_pressure * 0.025,
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowInfrastructureVisualState {
    pub surveillance_coverage: f32,
    pub power_instability: f32,
    pub drainage_overflow: f32,
    pub data_activity: f32,
}

impl Default for WindowInfrastructureVisualState {
    fn default() -> Self {
        Self {
            surveillance_coverage: 0.28,
            power_instability: 0.08,
            drainage_overflow: 0.16,
            data_activity: 0.3,
        }
    }
}

impl WindowInfrastructureVisualState {
    pub fn new(
        surveillance_coverage: f32,
        power_instability: f32,
        drainage_overflow: f32,
        data_activity: f32,
    ) -> Self {
        Self {
            surveillance_coverage: surveillance_coverage.clamp(0.0, 1.0),
            power_instability: power_instability.clamp(0.0, 1.0),
            drainage_overflow: drainage_overflow.clamp(0.0, 1.0),
            data_activity: data_activity.clamp(0.0, 1.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WindowCityStreamingCellState {
    Unloaded,
    SummaryLoaded,
    GameplayLoaded,
    RenderHighDetail,
    HeroLoaded,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowCityStreamingCellVisual {
    pub center_meters: [f32; 2],
    pub half_extents_meters: [f32; 2],
    pub state: WindowCityStreamingCellState,
    pub priority: f32,
    pub camera_alignment: f32,
    pub dependency_count: u32,
    pub requested_streaming_megabytes: f32,
}

impl WindowCityStreamingCellVisual {
    pub fn new(
        center_meters: [f32; 2],
        half_extents_meters: [f32; 2],
        state: WindowCityStreamingCellState,
    ) -> Self {
        Self {
            center_meters,
            half_extents_meters: [
                half_extents_meters[0].abs().max(0.5),
                half_extents_meters[1].abs().max(0.5),
            ],
            state,
            priority: 0.0,
            camera_alignment: 0.0,
            dependency_count: 0,
            requested_streaming_megabytes: 0.0,
        }
    }

    pub fn with_streaming_metrics(
        mut self,
        priority: f32,
        camera_alignment: f32,
        dependency_count: usize,
        requested_streaming_bytes: u64,
    ) -> Self {
        self.priority = priority.clamp(0.0, 4.0);
        self.camera_alignment = camera_alignment.clamp(-1.0, 1.0);
        self.dependency_count = dependency_count.min(u32::MAX as usize) as u32;
        self.requested_streaming_megabytes =
            (requested_streaming_bytes as f32 / (1024.0 * 1024.0)).clamp(0.0, 2048.0);
        self
    }

    pub fn is_loaded(&self) -> bool {
        matches!(
            self.state,
            WindowCityStreamingCellState::GameplayLoaded
                | WindowCityStreamingCellState::RenderHighDetail
                | WindowCityStreamingCellState::HeroLoaded
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowCityDistrictVisualKind {
    CorporateCore,
    RainAlleySlum,
    IndustrialDock,
    BlackMarket,
    ClinicDistrict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowCityTraversalVisualKind {
    Walk,
    CoverDash,
    StealthPath,
    ServiceLadder,
    Transit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowCityDangerVisualKind {
    PhysicalDamage,
    Flooding,
    SlipperyContamination,
    BiohazardContamination,
    ToxicGas,
    Surveillance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowCityRouteConsequenceStatus {
    Blocked,
    Dangerous,
    Restricted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowCityMaterialPlacementKind {
    WetRoad,
    Glass,
    Neon,
    Water,
    HumanSkin,
    Metal,
    Generic,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowCityMaterialPlacementVisual {
    pub center_meters: [f32; 3],
    pub half_extents_meters: [f32; 2],
    pub kind: WindowCityMaterialPlacementKind,
    pub base_color: [f32; 4],
    pub surface_response: [f32; 4],
    pub wetness: f32,
    pub damage: f32,
    pub pollution: f32,
    pub traffic_wear: f32,
    pub faction_accent_color: [f32; 4],
    pub faction_influence: f32,
    pub importance: f32,
    pub detail_seed: u64,
}

impl WindowCityMaterialPlacementVisual {
    pub fn new(
        center_meters: [f32; 3],
        half_extents_meters: [f32; 2],
        kind: WindowCityMaterialPlacementKind,
    ) -> Self {
        Self {
            center_meters,
            half_extents_meters: [
                half_extents_meters[0].abs().max(0.08),
                half_extents_meters[1].abs().max(0.08),
            ],
            kind,
            base_color: window_city_material_kind_color(kind),
            surface_response: window_city_material_kind_surface_response(kind),
            wetness: 0.0,
            damage: 0.0,
            pollution: 0.0,
            traffic_wear: 0.0,
            faction_accent_color: [0.34, 0.78, 1.0, 1.0],
            faction_influence: 0.0,
            importance: 0.0,
            detail_seed: 0,
        }
    }

    pub fn with_base_color(mut self, base_color: [f32; 4]) -> Self {
        self.base_color = window_clamp_color(base_color);
        self
    }

    pub fn with_surface_response(mut self, surface_response: [f32; 4]) -> Self {
        self.surface_response = window_clamp_surface_response(surface_response);
        self
    }

    pub fn with_surface_state(
        mut self,
        wetness: f32,
        damage: f32,
        pollution: f32,
        traffic_wear: f32,
    ) -> Self {
        self.wetness = wetness.clamp(0.0, 1.0);
        self.damage = damage.clamp(0.0, 1.0);
        self.pollution = pollution.clamp(0.0, 1.0);
        self.traffic_wear = traffic_wear.clamp(0.0, 1.0);
        self
    }

    pub fn with_faction_accent(
        mut self,
        faction_accent_color: [f32; 3],
        faction_influence: f32,
    ) -> Self {
        self.faction_accent_color = window_clamp_color([
            faction_accent_color[0],
            faction_accent_color[1],
            faction_accent_color[2],
            1.0,
        ]);
        self.faction_influence = faction_influence.clamp(0.0, 1.0);
        self
    }

    pub fn with_importance(mut self, importance: f32) -> Self {
        self.importance = importance.clamp(0.0, 1.0);
        self
    }

    pub fn with_detail_seed(mut self, detail_seed: u64) -> Self {
        self.detail_seed = detail_seed;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowSurfaceDecalKind {
    GrimeStain,
    CrackField,
    WaterPuddle,
    RoadMarking,
    Poster,
    Graffiti,
    Scorch,
    Corrosion,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowSurfaceDecalVisual {
    pub center_meters: [f32; 3],
    pub right_axis: [f32; 3],
    pub up_axis: [f32; 3],
    pub half_extents_meters: [f32; 2],
    pub kind: WindowSurfaceDecalKind,
    pub color: [f32; 4],
    pub surface_response: [f32; 4],
    pub scale_boost: f32,
    pub state_intensity: f32,
    pub relief: f32,
    pub detail_seed: u64,
}

impl WindowSurfaceDecalVisual {
    pub fn new(
        center_meters: [f32; 3],
        half_extents_meters: [f32; 2],
        kind: WindowSurfaceDecalKind,
    ) -> Self {
        Self {
            center_meters,
            right_axis: [1.0, 0.0, 0.0],
            up_axis: [0.0, 1.0, 0.0],
            half_extents_meters: [
                half_extents_meters[0].abs().max(0.015),
                half_extents_meters[1].abs().max(0.015),
            ],
            kind,
            color: window_surface_decal_kind_color(kind),
            surface_response: window_surface_decal_kind_surface_response(kind),
            scale_boost: 1.35,
            state_intensity: 0.42,
            relief: 0.28,
            detail_seed: 0,
        }
    }

    pub fn with_axes(mut self, right_axis: [f32; 3], up_axis: [f32; 3]) -> Self {
        self.right_axis = right_axis;
        self.up_axis = up_axis;
        self
    }

    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = window_clamp_color(color);
        self
    }

    pub fn with_surface_response(mut self, surface_response: [f32; 4]) -> Self {
        self.surface_response = window_clamp_surface_response(surface_response);
        self
    }

    pub fn with_state_detail(
        mut self,
        scale_boost: f32,
        state_intensity: f32,
        relief: f32,
    ) -> Self {
        self.scale_boost = scale_boost.clamp(1.0, 4.4);
        self.state_intensity = state_intensity.clamp(0.0, 1.0);
        self.relief = relief.clamp(0.0, 1.0);
        self
    }

    pub fn with_detail_seed(mut self, detail_seed: u64) -> Self {
        self.detail_seed = detail_seed;
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WindowDecalPacketVisual {
    pub decals: Vec<WindowSurfaceDecalVisual>,
}

impl WindowDecalPacketVisual {
    pub fn new(decals: Vec<WindowSurfaceDecalVisual>) -> Self {
        Self { decals }
    }

    pub fn push(&mut self, decal: WindowSurfaceDecalVisual) {
        self.decals.push(decal);
    }

    pub fn is_empty(&self) -> bool {
        self.decals.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowGeneratedCityChunkVisual {
    pub chunk_id: u64,
    pub center_meters: [f32; 2],
    pub half_extents_meters: [f32; 2],
    pub district_kind: WindowCityDistrictVisualKind,
    pub streaming_state: WindowCityStreamingCellState,
    pub district_color: [f32; 4],
    pub faction_color: [f32; 4],
    pub faction_strength: f32,
    pub surveillance_level: f32,
    pub crime_pressure: f32,
    pub pollution: f32,
    pub npc_count: u32,
    pub story_hook_count: u32,
    pub destructible_count: u32,
    pub light_count: u32,
    pub hazard_count: u32,
    pub streaming_dependency_count: u32,
    pub crowd_spawn_rule_count: u32,
    pub traffic_rule_count: u32,
    pub expected_background_crowd_count: u32,
    pub expected_vehicle_or_transit_count: u32,
    pub population_validation_passed: bool,
    pub detail_seed: u64,
}

impl WindowGeneratedCityChunkVisual {
    pub fn new(
        chunk_id: u64,
        center_meters: [f32; 2],
        half_extents_meters: [f32; 2],
        district_kind: WindowCityDistrictVisualKind,
        streaming_state: WindowCityStreamingCellState,
    ) -> Self {
        Self {
            chunk_id,
            center_meters,
            half_extents_meters: [
                half_extents_meters[0].abs().max(1.0),
                half_extents_meters[1].abs().max(1.0),
            ],
            district_kind,
            streaming_state,
            district_color: window_generated_city_district_color(district_kind),
            faction_color: [0.34, 0.78, 1.0, 1.0],
            faction_strength: 0.0,
            surveillance_level: 0.0,
            crime_pressure: 0.0,
            pollution: 0.0,
            npc_count: 0,
            story_hook_count: 0,
            destructible_count: 0,
            light_count: 0,
            hazard_count: 0,
            streaming_dependency_count: 0,
            crowd_spawn_rule_count: 0,
            traffic_rule_count: 0,
            expected_background_crowd_count: 0,
            expected_vehicle_or_transit_count: 0,
            population_validation_passed: true,
            detail_seed: chunk_id,
        }
    }

    pub fn with_district_pressure(
        mut self,
        surveillance_level: f32,
        crime_pressure: f32,
        pollution: f32,
    ) -> Self {
        self.surveillance_level = surveillance_level.clamp(0.0, 1.0);
        self.crime_pressure = crime_pressure.clamp(0.0, 1.0);
        self.pollution = pollution.clamp(0.0, 1.0);
        self
    }

    pub fn with_faction(mut self, faction_color: [f32; 3], faction_strength: f32) -> Self {
        self.faction_color =
            window_clamp_color([faction_color[0], faction_color[1], faction_color[2], 1.0]);
        self.faction_strength = faction_strength.clamp(0.0, 1.0);
        self
    }

    pub fn with_activity_counts(
        mut self,
        npc_count: usize,
        story_hook_count: usize,
        destructible_count: usize,
        light_count: usize,
        hazard_count: usize,
        streaming_dependency_count: usize,
    ) -> Self {
        self.npc_count = npc_count.min(u32::MAX as usize) as u32;
        self.story_hook_count = story_hook_count.min(u32::MAX as usize) as u32;
        self.destructible_count = destructible_count.min(u32::MAX as usize) as u32;
        self.light_count = light_count.min(u32::MAX as usize) as u32;
        self.hazard_count = hazard_count.min(u32::MAX as usize) as u32;
        self.streaming_dependency_count = streaming_dependency_count.min(u32::MAX as usize) as u32;
        self
    }

    pub fn with_population_counts(
        mut self,
        crowd_spawn_rule_count: usize,
        traffic_rule_count: usize,
        expected_background_crowd_count: usize,
        expected_vehicle_or_transit_count: usize,
        population_validation_passed: bool,
    ) -> Self {
        self.crowd_spawn_rule_count = crowd_spawn_rule_count.min(u32::MAX as usize) as u32;
        self.traffic_rule_count = traffic_rule_count.min(u32::MAX as usize) as u32;
        self.expected_background_crowd_count =
            expected_background_crowd_count.min(u32::MAX as usize) as u32;
        self.expected_vehicle_or_transit_count =
            expected_vehicle_or_transit_count.min(u32::MAX as usize) as u32;
        self.population_validation_passed = population_validation_passed;
        self
    }

    pub fn with_detail_seed(mut self, detail_seed: u64) -> Self {
        self.detail_seed = detail_seed;
        self
    }

    pub fn detail_loaded(&self) -> bool {
        self.streaming_state >= WindowCityStreamingCellState::GameplayLoaded
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowCityPersistentCellVisual {
    pub cell_id: u64,
    pub center_meters: [f32; 2],
    pub half_extents_meters: [f32; 2],
    pub severity: f32,
    pub damaged_count: u32,
    pub material_override_count: u32,
    pub infrastructure_delta_count: u32,
    pub faction_delta_count: u32,
    pub story_thread_count: u32,
    pub ai_memory_count: u32,
    pub resident_asset_count: u32,
    pub danger_field_count: u32,
    pub restricted_zone_count: u32,
    pub blocked_route_count: u32,
    pub dangerous_route_count: u32,
    pub infrastructure_system_mask: u32,
    pub save_required: bool,
    pub detail_seed: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowCityPersistenceCounts {
    pub damaged_count: usize,
    pub material_override_count: usize,
    pub infrastructure_delta_count: usize,
    pub faction_delta_count: usize,
    pub story_thread_count: usize,
    pub ai_memory_count: usize,
    pub resident_asset_count: usize,
}

pub const WINDOW_CITY_INFRASTRUCTURE_POWER: u32 = 1 << 0;
pub const WINDOW_CITY_INFRASTRUCTURE_WATER: u32 = 1 << 1;
pub const WINDOW_CITY_INFRASTRUCTURE_DATA: u32 = 1 << 2;
pub const WINDOW_CITY_INFRASTRUCTURE_SURVEILLANCE: u32 = 1 << 3;
pub const WINDOW_CITY_INFRASTRUCTURE_TRANSIT: u32 = 1 << 4;
pub const WINDOW_CITY_INFRASTRUCTURE_DRAINAGE: u32 = 1 << 5;
pub const WINDOW_CITY_INFRASTRUCTURE_ALL: u32 = WINDOW_CITY_INFRASTRUCTURE_POWER
    | WINDOW_CITY_INFRASTRUCTURE_WATER
    | WINDOW_CITY_INFRASTRUCTURE_DATA
    | WINDOW_CITY_INFRASTRUCTURE_SURVEILLANCE
    | WINDOW_CITY_INFRASTRUCTURE_TRANSIT
    | WINDOW_CITY_INFRASTRUCTURE_DRAINAGE;

impl WindowCityPersistentCellVisual {
    pub fn new(cell_id: u64, center_meters: [f32; 2], half_extents_meters: [f32; 2]) -> Self {
        Self {
            cell_id,
            center_meters,
            half_extents_meters: [
                half_extents_meters[0].abs().max(1.0),
                half_extents_meters[1].abs().max(1.0),
            ],
            severity: 0.0,
            damaged_count: 0,
            material_override_count: 0,
            infrastructure_delta_count: 0,
            faction_delta_count: 0,
            story_thread_count: 0,
            ai_memory_count: 0,
            resident_asset_count: 0,
            danger_field_count: 0,
            restricted_zone_count: 0,
            blocked_route_count: 0,
            dangerous_route_count: 0,
            infrastructure_system_mask: 0,
            save_required: false,
            detail_seed: cell_id,
        }
    }

    pub fn with_severity(mut self, severity: f32) -> Self {
        self.severity = severity.clamp(0.0, 1.0);
        self
    }

    pub fn with_persistence_counts(mut self, counts: WindowCityPersistenceCounts) -> Self {
        self.damaged_count = counts.damaged_count.min(u32::MAX as usize) as u32;
        self.material_override_count = counts.material_override_count.min(u32::MAX as usize) as u32;
        self.infrastructure_delta_count =
            counts.infrastructure_delta_count.min(u32::MAX as usize) as u32;
        self.faction_delta_count = counts.faction_delta_count.min(u32::MAX as usize) as u32;
        self.story_thread_count = counts.story_thread_count.min(u32::MAX as usize) as u32;
        self.ai_memory_count = counts.ai_memory_count.min(u32::MAX as usize) as u32;
        self.resident_asset_count = counts.resident_asset_count.min(u32::MAX as usize) as u32;
        self
    }

    pub fn with_navigation_counts(
        mut self,
        danger_field_count: usize,
        restricted_zone_count: usize,
        blocked_route_count: usize,
        dangerous_route_count: usize,
    ) -> Self {
        self.danger_field_count = danger_field_count.min(u32::MAX as usize) as u32;
        self.restricted_zone_count = restricted_zone_count.min(u32::MAX as usize) as u32;
        self.blocked_route_count = blocked_route_count.min(u32::MAX as usize) as u32;
        self.dangerous_route_count = dangerous_route_count.min(u32::MAX as usize) as u32;
        self
    }

    pub fn with_infrastructure_system_mask(mut self, infrastructure_system_mask: u32) -> Self {
        self.infrastructure_system_mask =
            infrastructure_system_mask & WINDOW_CITY_INFRASTRUCTURE_ALL;
        self
    }

    pub fn with_save_required(mut self, save_required: bool) -> Self {
        self.save_required = save_required;
        self
    }

    pub fn with_detail_seed(mut self, detail_seed: u64) -> Self {
        self.detail_seed = detail_seed;
        self
    }

    pub fn has_visible_consequence(&self) -> bool {
        self.severity > 0.01
            || self.damaged_count > 0
            || self.material_override_count > 0
            || self.infrastructure_delta_count > 0
            || self.infrastructure_system_mask != 0
            || self.faction_delta_count > 0
            || self.story_thread_count > 0
            || self.ai_memory_count > 0
            || self.resident_asset_count > 0
            || self.danger_field_count > 0
            || self.restricted_zone_count > 0
            || self.blocked_route_count > 0
            || self.dangerous_route_count > 0
    }

    pub fn has_infrastructure_system(&self, infrastructure_system_mask: u32) -> bool {
        self.infrastructure_system_mask & infrastructure_system_mask != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowCityDangerFieldVisual {
    pub center_meters: [f32; 3],
    pub radius_meters: f32,
    pub kind: WindowCityDangerVisualKind,
    pub severity: f32,
}

impl WindowCityDangerFieldVisual {
    pub fn new(center_meters: [f32; 3], kind: WindowCityDangerVisualKind) -> Self {
        Self {
            center_meters,
            radius_meters: 1.0,
            kind,
            severity: 0.0,
        }
    }

    pub fn with_radius(mut self, radius_meters: f32) -> Self {
        self.radius_meters = radius_meters.max(0.1);
        self
    }

    pub fn with_severity(mut self, severity: f32) -> Self {
        self.severity = severity.clamp(0.0, 1.0);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowCityRouteConsequenceVisual {
    pub start_meters: [f32; 3],
    pub end_meters: [f32; 3],
    pub status: WindowCityRouteConsequenceStatus,
    pub severity: f32,
}

impl WindowCityRouteConsequenceVisual {
    pub fn new(
        start_meters: [f32; 3],
        end_meters: [f32; 3],
        status: WindowCityRouteConsequenceStatus,
    ) -> Self {
        Self {
            start_meters,
            end_meters,
            status,
            severity: 0.0,
        }
    }

    pub fn with_severity(mut self, severity: f32) -> Self {
        self.severity = severity.clamp(0.0, 1.0);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowGeneratedCityNavigationNodeVisual {
    pub position_meters: [f32; 3],
    pub important: bool,
    pub story_focus: bool,
    pub surveillance_level: f32,
    pub danger_level: f32,
}

impl WindowGeneratedCityNavigationNodeVisual {
    pub fn new(position_meters: [f32; 3]) -> Self {
        Self {
            position_meters,
            important: false,
            story_focus: false,
            surveillance_level: 0.0,
            danger_level: 0.0,
        }
    }

    pub fn with_importance(mut self, important: bool, story_focus: bool) -> Self {
        self.important = important;
        self.story_focus = story_focus;
        self
    }

    pub fn with_pressure(mut self, surveillance_level: f32, danger_level: f32) -> Self {
        self.surveillance_level = surveillance_level.clamp(0.0, 1.0);
        self.danger_level = danger_level.clamp(0.0, 1.0);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowGeneratedCityNavigationEdgeVisual {
    pub start_meters: [f32; 3],
    pub end_meters: [f32; 3],
    pub traversal: WindowCityTraversalVisualKind,
    pub route_priority: f32,
}

impl WindowGeneratedCityNavigationEdgeVisual {
    pub fn new(
        start_meters: [f32; 3],
        end_meters: [f32; 3],
        traversal: WindowCityTraversalVisualKind,
    ) -> Self {
        Self {
            start_meters,
            end_meters,
            traversal,
            route_priority: 0.0,
        }
    }

    pub fn with_route_priority(mut self, route_priority: f32) -> Self {
        self.route_priority = route_priority.clamp(0.0, 1.0);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowProceduralMeshInstance {
    pub mesh: MeshAssetHandle,
    pub center_meters: [f32; 2],
    pub z_meters: f32,
    pub color: [f32; 4],
    pub crack_density: f32,
    pub moisture: f32,
    pub soot: f32,
    pub corrosion: f32,
    pub heat: f32,
    pub electrical_charge: f32,
    pub plastic_strain: f32,
    pub biological_contamination: f32,
    pub oil_contamination: f32,
    pub billboard_right: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowHudPulseVisual {
    pub color: [f32; 4],
    pub weight: f32,
}

impl WindowHudPulseVisual {
    pub fn new(color: [f32; 4], weight: f32) -> Self {
        Self {
            color,
            weight: weight.clamp(0.0, 1.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowWorldMarkerVisual {
    pub position: [f32; 2],
    pub radius: f32,
    pub color: [f32; 4],
    pub axis_half_extent_scale: f32,
    pub spoke_half_width_scale: f32,
}

impl WindowWorldMarkerVisual {
    pub fn new(position: [f32; 2], radius: f32, color: [f32; 4]) -> Self {
        Self {
            position,
            radius,
            color,
            axis_half_extent_scale: 0.72,
            spoke_half_width_scale: 0.08,
        }
    }

    pub fn with_axis_half_extent_scale(mut self, axis_half_extent_scale: f32) -> Self {
        self.axis_half_extent_scale = axis_half_extent_scale.max(0.0);
        self
    }

    pub fn with_spoke_half_width_scale(mut self, spoke_half_width_scale: f32) -> Self {
        self.spoke_half_width_scale = spoke_half_width_scale.max(0.0);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowWorldEventMarker {
    pub position: [f32; 2],
    pub base_radius: f32,
    pub color: [f32; 4],
    pub remaining_seconds: f32,
    pub total_seconds: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowGasVolumeVisual {
    pub center: [f32; 3],
    pub radius_meters: f32,
    pub height_meters: f32,
    pub visibility_blocking: f32,
    pub hazard_level: f32,
    pub color: [f32; 4],
    pub phase_seed: f32,
}

impl WindowGasVolumeVisual {
    pub fn new(center: [f32; 3], color: [f32; 4]) -> Self {
        Self {
            center,
            radius_meters: 1.6,
            height_meters: 2.6,
            visibility_blocking: 0.45,
            hazard_level: 0.25,
            color,
            phase_seed: 0.0,
        }
    }

    pub fn with_radius(mut self, radius_meters: f32) -> Self {
        self.radius_meters = radius_meters.max(0.05);
        self
    }

    pub fn with_height(mut self, height_meters: f32) -> Self {
        self.height_meters = height_meters.max(0.05);
        self
    }

    pub fn with_visibility_blocking(mut self, visibility_blocking: f32) -> Self {
        self.visibility_blocking = visibility_blocking.clamp(0.0, 1.0);
        self
    }

    pub fn with_hazard_level(mut self, hazard_level: f32) -> Self {
        self.hazard_level = hazard_level.clamp(0.0, 1.0);
        self
    }

    pub fn with_phase_seed(mut self, phase_seed: f32) -> Self {
        self.phase_seed = phase_seed;
        self
    }

    pub fn with_alpha_scale(mut self, alpha_scale: f32) -> Self {
        self.color[3] = (self.color[3] * alpha_scale.clamp(0.0, 1.0)).clamp(0.0, 1.0);
        self
    }
}

impl WindowWorldEventMarker {
    pub fn new(position: [f32; 2], base_radius: f32, color: [f32; 4], total_seconds: f32) -> Self {
        Self {
            position,
            base_radius,
            color,
            remaining_seconds: total_seconds,
            total_seconds,
        }
    }

    pub fn normalized_lifetime(&self) -> f32 {
        if self.total_seconds <= f32::EPSILON {
            return 0.0;
        }

        (self.remaining_seconds / self.total_seconds).clamp(0.0, 1.0)
    }

    pub fn visible_radius(&self) -> f32 {
        let elapsed = 1.0 - self.normalized_lifetime();
        self.base_radius * (1.0 + elapsed * 0.9)
    }

    pub fn visible_color(&self) -> [f32; 4] {
        let fade = self.normalized_lifetime();
        [
            self.color[0] * fade,
            self.color[1] * fade,
            self.color[2] * fade,
            self.color[3],
        ]
    }

    pub fn visible_marker(&self) -> WindowWorldMarkerVisual {
        WindowWorldMarkerVisual::new(self.position, self.visible_radius(), self.visible_color())
    }

    pub fn tick(&mut self, dt_seconds: f32) -> bool {
        self.remaining_seconds = (self.remaining_seconds - dt_seconds.max(0.0)).max(0.0);
        self.remaining_seconds > 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowHudEventPulse {
    pub color: [f32; 4],
    pub remaining_seconds: f32,
    pub total_seconds: f32,
    pub weight: f32,
}

impl WindowHudEventPulse {
    pub fn new(color: [f32; 4], weight: f32) -> Self {
        Self {
            color,
            remaining_seconds: WINDOW_HUD_EVENT_PULSE_SECONDS,
            total_seconds: WINDOW_HUD_EVENT_PULSE_SECONDS,
            weight: weight.clamp(0.0, 1.0),
        }
    }

    pub fn normalized_lifetime(&self) -> f32 {
        if self.total_seconds <= f32::EPSILON {
            return 0.0;
        }

        (self.remaining_seconds / self.total_seconds).clamp(0.0, 1.0)
    }

    pub fn visible_color(&self) -> [f32; 4] {
        let fade = self.normalized_lifetime();
        [
            self.color[0] * (0.35 + fade * 0.65),
            self.color[1] * (0.35 + fade * 0.65),
            self.color[2] * (0.35 + fade * 0.65),
            (self.color[3] * fade).clamp(0.0, 1.0),
        ]
    }

    pub fn visible_pulse(&self) -> WindowHudPulseVisual {
        WindowHudPulseVisual::new(self.visible_color(), self.weight)
    }

    pub fn tick(&mut self, dt_seconds: f32) -> bool {
        self.remaining_seconds = (self.remaining_seconds - dt_seconds.max(0.0)).max(0.0);
        self.remaining_seconds > 0.0
    }
}

pub fn window_hud_event_pulse_from_event(event: &WorldEvent) -> WindowHudEventPulse {
    let (color, weight) = match &event.kind {
        WorldEventKind::GlassWallFractured { .. } => ([0.72, 0.96, 1.0, 0.92], 1.0),
        WorldEventKind::DamageApplied { .. }
        | WorldEventKind::MetalBent { .. }
        | WorldEventKind::FlexibleConstraintResolved { .. } => ([0.98, 0.42, 0.18, 0.88], 0.8),
        WorldEventKind::StreetFlooded => ([0.12, 0.48, 1.0, 0.84], 0.86),
        WorldEventKind::ToxicGasReleased => ([0.32, 0.95, 0.28, 0.84], 0.9),
        WorldEventKind::NpcHeardSound { .. } => ([1.0, 0.74, 0.24, 0.78], 0.64),
        WorldEventKind::NavigationMoveBlocked { .. } => ([1.0, 0.62, 0.22, 0.78], 0.58),
        WorldEventKind::NpcWitnessedCrime { .. }
        | WorldEventKind::PlayerIdentityExposed
        | WorldEventKind::SecurityAlertRaised { .. }
        | WorldEventKind::FactionLostTerritory { .. }
        | WorldEventKind::FactionReputationChanged { .. }
        | WorldEventKind::SurveillanceIncreased { .. } => ([1.0, 0.18, 0.28, 0.92], 0.94),
        WorldEventKind::AgentDecisionExplained { .. }
        | WorldEventKind::AgentIntentProposed { .. }
        | WorldEventKind::AgentMemoryUpdated { .. }
        | WorldEventKind::AgentStateChanged { .. }
        | WorldEventKind::StoryEventEmitted { .. } => ([0.78, 0.58, 1.0, 0.86], 0.7),
        WorldEventKind::DialogueEmitted { .. }
        | WorldEventKind::VoiceLineSpoken { .. }
        | WorldEventKind::SpeechSynthesized { .. }
        | WorldEventKind::FacialAnimationApplied { .. } => ([0.38, 1.0, 0.72, 0.82], 0.72),
        WorldEventKind::SoundEmitted {
            intensity,
            radius_meters,
            ..
        } => (
            [0.46, 0.82, 1.0, 0.74],
            (0.34
                + intensity.clamp(0.0, 1.0) * 0.38
                + radius_meters.clamp(0.0, 30.0) / 30.0 * 0.28)
                .clamp(0.0, 1.0),
        ),
        WorldEventKind::AudioFrameMixed {
            peak_intensity,
            active_sound_count,
            ..
        } => (
            [0.28, 0.62, 0.96, 0.54],
            (0.18 + peak_intensity.clamp(0.0, 1.0) * 0.42 + *active_sound_count as f32 * 0.04)
                .clamp(0.0, 1.0),
        ),
        WorldEventKind::AssetStreamingRequested { .. } => ([0.3, 0.74, 1.0, 0.62], 0.46),
        WorldEventKind::AssetBecameResident { .. } => ([0.34, 1.0, 0.68, 0.68], 0.54),
        WorldEventKind::MaterialStateChanged { .. } | WorldEventKind::MeshReplaced { .. } => {
            ([0.38, 0.9, 0.95, 0.68], 0.58)
        }
        WorldEventKind::ForceApplied { .. }
        | WorldEventKind::TransformChanged { .. }
        | WorldEventKind::EntitySpawned { .. }
        | WorldEventKind::EntityDespawned { .. }
        | WorldEventKind::HumanAppearanceUpdated { .. }
        | WorldEventKind::PowerTransformerOverheated { .. }
        | WorldEventKind::Custom(_) => ([0.56, 0.62, 0.72, 0.5], 0.34),
    };

    WindowHudEventPulse::new(color, weight)
}

pub fn window_world_marker_from_event(event: &WorldEvent) -> Option<WindowWorldEventMarker> {
    let position = [event.location_meters.x, event.location_meters.y];
    match &event.kind {
        WorldEventKind::ForceApplied { .. } => Some(WindowWorldEventMarker::new(
            position,
            0.32,
            [0.32, 0.62, 1.0, 1.0],
            0.45,
        )),
        WorldEventKind::DamageApplied { .. }
        | WorldEventKind::MetalBent { .. }
        | WorldEventKind::FlexibleConstraintResolved { .. } => Some(WindowWorldEventMarker::new(
            position,
            0.42,
            [0.98, 0.42, 0.18, 1.0],
            0.9,
        )),
        WorldEventKind::GlassWallFractured { .. } => Some(WindowWorldEventMarker::new(
            position,
            0.78,
            [0.72, 0.96, 1.0, 1.0],
            1.35,
        )),
        WorldEventKind::MeshReplaced { .. } | WorldEventKind::MaterialStateChanged { .. } => Some(
            WindowWorldEventMarker::new(position, 0.24, [0.38, 0.9, 0.95, 1.0], 0.65),
        ),
        WorldEventKind::StreetFlooded => Some(WindowWorldEventMarker::new(
            position,
            1.05,
            [0.12, 0.48, 1.0, 1.0],
            1.0,
        )),
        WorldEventKind::ToxicGasReleased => Some(WindowWorldEventMarker::new(
            position,
            1.1,
            [0.32, 0.95, 0.28, 1.0],
            1.1,
        )),
        WorldEventKind::NpcWitnessedCrime { .. } | WorldEventKind::PlayerIdentityExposed => Some(
            WindowWorldEventMarker::new(position, 0.72, [1.0, 0.28, 0.18, 1.0], 1.5),
        ),
        WorldEventKind::NpcHeardSound { .. } => Some(WindowWorldEventMarker::new(
            position,
            0.48,
            [1.0, 0.74, 0.24, 1.0],
            1.1,
        )),
        WorldEventKind::NavigationMoveBlocked { .. } => Some(WindowWorldEventMarker::new(
            position,
            0.34,
            [1.0, 0.62, 0.22, 1.0],
            0.9,
        )),
        WorldEventKind::AgentMemoryUpdated { .. }
        | WorldEventKind::AgentStateChanged { .. }
        | WorldEventKind::AgentIntentProposed { .. }
        | WorldEventKind::AgentDecisionExplained { .. } => Some(WindowWorldEventMarker::new(
            position,
            0.42,
            [0.78, 0.58, 1.0, 1.0],
            1.2,
        )),
        WorldEventKind::FactionLostTerritory { .. }
        | WorldEventKind::FactionReputationChanged { .. }
        | WorldEventKind::SecurityAlertRaised { .. }
        | WorldEventKind::SurveillanceIncreased { .. } => Some(WindowWorldEventMarker::new(
            position,
            0.62,
            [1.0, 0.18, 0.28, 1.0],
            1.35,
        )),
        WorldEventKind::SoundEmitted {
            intensity,
            radius_meters,
            ..
        } => {
            let radius = 0.26 + radius_meters.clamp(0.0, 30.0) / 30.0 * intensity.clamp(0.25, 1.0);
            Some(WindowWorldEventMarker::new(
                position,
                radius,
                [0.46, 0.82, 1.0, 1.0],
                1.0,
            ))
        }
        WorldEventKind::DialogueEmitted { .. }
        | WorldEventKind::VoiceLineSpoken { .. }
        | WorldEventKind::SpeechSynthesized { .. }
        | WorldEventKind::FacialAnimationApplied { .. } => Some(WindowWorldEventMarker::new(
            position,
            0.46,
            [0.38, 1.0, 0.72, 1.0],
            1.25,
        )),
        WorldEventKind::HumanAppearanceUpdated { .. } => Some(WindowWorldEventMarker::new(
            position,
            0.32,
            [1.0, 0.56, 0.78, 1.0],
            0.9,
        )),
        WorldEventKind::StoryEventEmitted { .. } => Some(WindowWorldEventMarker::new(
            position,
            0.58,
            [0.94, 0.78, 0.34, 1.0],
            1.2,
        )),
        WorldEventKind::PowerTransformerOverheated { .. } => Some(WindowWorldEventMarker::new(
            position,
            0.6,
            [1.0, 0.82, 0.18, 1.0],
            1.0,
        )),
        WorldEventKind::AssetStreamingRequested { .. }
        | WorldEventKind::AssetBecameResident { .. } => Some(WindowWorldEventMarker::new(
            position,
            0.2,
            [0.3, 0.74, 1.0, 1.0],
            0.7,
        )),
        WorldEventKind::AudioFrameMixed { .. } => None,
        WorldEventKind::EntitySpawned { .. }
        | WorldEventKind::EntityDespawned { .. }
        | WorldEventKind::TransformChanged { .. }
        | WorldEventKind::Custom(_) => Some(WindowWorldEventMarker::new(
            position,
            0.28,
            [0.56, 0.62, 0.72, 1.0],
            0.75,
        )),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowEntityProxyKind {
    GenericRenderable,
    GlassWall,
    LightPanel,
    LowHazardSurface,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowEntityProxyVisual {
    pub kind: WindowEntityProxyKind,
    pub center_meters: [f32; 2],
    pub z_meters: f32,
    pub color: [f32; 4],
    pub surface_response: [f32; 4],
    pub crack_density: f32,
    pub moisture: f32,
    pub soot: f32,
    pub corrosion: f32,
    pub heat: f32,
    pub electrical_charge: f32,
    pub plastic_strain: f32,
    pub biological_contamination: f32,
    pub oil_contamination: f32,
}

impl WindowEntityProxyVisual {
    pub fn new(
        kind: WindowEntityProxyKind,
        center_meters: [f32; 2],
        z_meters: f32,
        color: [f32; 4],
    ) -> Self {
        Self {
            kind,
            center_meters,
            z_meters,
            color,
            surface_response: window_proxy_kind_surface_response(kind),
            crack_density: 0.0,
            moisture: 0.0,
            soot: 0.0,
            corrosion: 0.0,
            heat: 0.0,
            electrical_charge: 0.0,
            plastic_strain: 0.0,
            biological_contamination: 0.0,
            oil_contamination: 0.0,
        }
    }

    pub fn with_surface_response(mut self, surface_response: [f32; 4]) -> Self {
        self.surface_response = window_clamp_surface_response(surface_response);
        self
    }

    pub fn with_material_state(mut self, material_state: Option<&MaterialState>) -> Self {
        if let Some(material_state) = material_state {
            self.crack_density = material_state.crack_density.clamp(0.0, 1.0);
            self.moisture = material_state.moisture.clamp(0.0, 1.0);
            self.soot = material_state.soot.clamp(0.0, 1.0);
            self.corrosion = material_state.corrosion.clamp(0.0, 1.0);
            self.heat = ((material_state.temperature - 293.15) / 800.0).clamp(0.0, 1.0);
            self.electrical_charge = (material_state.electrical_charge / 240.0).clamp(0.0, 1.0);
            self.plastic_strain = material_state.plastic_strain.clamp(0.0, 1.0);
            self.biological_contamination = material_state.biological_contamination.clamp(0.0, 1.0);
            self.oil_contamination = material_state.oil_contamination.clamp(0.0, 1.0);
        }
        self
    }
}

pub fn window_entity_color(
    tags: Option<&[String]>,
    material: Option<&MaterialDescriptor>,
    material_state: Option<&MaterialState>,
    frame_index: u64,
) -> [f32; 4] {
    let mut color = material
        .map(|material| {
            let visual = &material.visual;
            [
                visual.base_color_linear[0],
                visual.base_color_linear[1],
                visual.base_color_linear[2],
                (1.0 - visual.transparency).clamp(0.42, 1.0),
            ]
        })
        .unwrap_or([0.58, 0.62, 0.68, 1.0]);

    if let Some(material) = material {
        let emission = material.visual.emission_linear;
        color[0] += emission[0] * 0.35;
        color[1] += emission[1] * 0.35;
        color[2] += emission[2] * 0.35;
    }

    if window_tags_have(tags, "player") {
        color = [0.34, 0.82, 1.0, 1.0];
    } else if window_tags_have(tags, "npc") {
        color = [0.96, 0.72, 0.28, 1.0];
    } else if window_tags_have(tags, "glass") {
        color = [0.62, 0.88, 1.0, 0.86];
    } else if window_tags_have(tags, "light") || window_tags_have(tags, "neon") {
        let pulse = (frame_index as f32 * 0.11).sin().mul_add(0.5, 0.5);
        color = [
            0.9 + pulse * 0.1,
            0.12 + pulse * 0.1,
            0.52 + pulse * 0.35,
            1.0,
        ];
    } else if window_tags_have(tags, "water_leak") {
        color = [0.15, 0.52, 0.9, 0.88];
    } else if window_tags_have(tags, "ground") {
        color = [0.05, 0.065, 0.075, 0.94];
    }

    if let Some(state) = material_state {
        color[0] += state.crack_density * 0.28 + state.corrosion * 0.12;
        color[1] += state.electrical_charge.clamp(0.0, 160.0) / 160.0 * 0.18;
        color[2] += state.moisture * 0.22;
        color[3] = (color[3] + state.crack_density * 0.12).clamp(0.42, 1.0);
    }

    window_clamp_color(color)
}

pub fn window_entity_surface_response(
    tags: Option<&[String]>,
    material: Option<&MaterialDescriptor>,
    material_state: Option<&MaterialState>,
) -> [f32; 4] {
    let mut response = material
        .map(|material| {
            let visual = &material.visual;
            let emission = visual.emission_linear;
            let emissive_strength =
                ((emission[0] + emission[1] + emission[2]) / 8.0).clamp(0.0, 1.0);
            [
                visual.roughness.clamp(0.04, 1.0),
                visual.metallic.clamp(0.0, 1.0),
                visual.transparency.clamp(0.0, 1.0) * 0.55,
                emissive_strength,
            ]
        })
        .unwrap_or(WINDOW_SURFACE_RESPONSE_DEFAULT);

    if window_tags_have(tags, "glass") {
        response = WINDOW_SURFACE_RESPONSE_GLASS;
    } else if window_tags_have(tags, "light") || window_tags_have(tags, "neon") {
        response = WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON;
    } else if window_tags_have(tags, "water_leak")
        || window_tags_have(tags, "hazard")
        || window_tags_have(tags, "ground")
    {
        response = WINDOW_SURFACE_RESPONSE_WET_ROAD;
    }

    if let Some(state) = material_state {
        let moisture = state.moisture.clamp(0.0, 1.0);
        let hot = ((state.temperature - 293.15) / 800.0).clamp(0.0, 1.0);
        let charge = (state.electrical_charge / 240.0).clamp(0.0, 1.0);
        response[0] =
            (response[0] + state.soot * 0.26 + state.corrosion * 0.2 + state.crack_density * 0.12
                - moisture * 0.42)
                .clamp(0.04, 1.0);
        response[2] = response[2].max(moisture);
        response[3] = response[3].max(charge).max(hot * 0.58);
    }

    window_clamp_surface_response(response)
}

pub fn window_dynamic_light_from_snapshot(
    snapshot: &WorldSnapshot,
    frame_index: u64,
) -> WindowDynamicLight {
    let mut best: Option<(f32, WindowDynamicLight)> = None;

    for (entity, transform) in snapshot.transforms.iter() {
        let tags = snapshot.tags.find(*entity).map(Vec::as_slice);
        if window_tags_have(tags, "audio_zone:alley") || window_tags_have(tags, "player") {
            continue;
        }

        let renderable = snapshot.renderables.find(*entity);
        if renderable.is_some_and(|renderable| !renderable.visible) {
            continue;
        }

        let material =
            renderable.and_then(|renderable| snapshot.materials.find(renderable.material));
        let material_state = snapshot.material_states.find(*entity);
        let Some(light) =
            window_dynamic_light_for_entity(tags, material, material_state, transform, frame_index)
        else {
            continue;
        };
        let score = light.intensity * light.radius_meters.max(0.0);
        if score > best.map(|(score, _)| score).unwrap_or(0.0) {
            best = Some((score, light));
        }
    }

    best.map(|(_, light)| light)
        .unwrap_or_else(WindowDynamicLight::disabled)
}

pub fn window_dynamic_light_from_city_material_placements(
    placements: &[WindowCityMaterialPlacementVisual],
    camera_position_meters: [f32; 3],
    frame_index: u64,
) -> WindowDynamicLight {
    let mut best: Option<(f32, WindowDynamicLight)> = None;

    for placement in placements {
        let Some(light) =
            window_dynamic_light_for_city_material(*placement, camera_position_meters, frame_index)
        else {
            continue;
        };
        let dx = light.position_meters[0] - camera_position_meters[0];
        let dy = light.position_meters[1] - camera_position_meters[1];
        let dz = light.position_meters[2] - camera_position_meters[2];
        let distance = (dx * dx + dy * dy + dz * dz).sqrt();
        let attenuation = 1.0 / (1.0 + distance / light.radius_meters.max(1.0));
        let score = light.intensity * light.radius_meters.max(0.0) * attenuation;
        if score > best.map(|(score, _)| score).unwrap_or(0.0) {
            best = Some((score, light));
        }
    }

    best.map(|(_, light)| light)
        .unwrap_or_else(WindowDynamicLight::disabled)
}

fn window_dynamic_light_for_city_material(
    placement: WindowCityMaterialPlacementVisual,
    camera_position_meters: [f32; 3],
    frame_index: u64,
) -> Option<WindowDynamicLight> {
    let emission = placement.surface_response[3].clamp(0.0, 1.0);
    let glow_source = matches!(
        placement.kind,
        WindowCityMaterialPlacementKind::Neon | WindowCityMaterialPlacementKind::Glass
    ) || emission > 0.08;
    if !glow_source {
        return None;
    }

    let pulse = (frame_index as f32 * 0.057
        + window_stable_unit(placement.detail_seed, 1181) * std::f32::consts::TAU)
        .sin()
        .mul_add(0.5, 0.5);
    let mut intensity = match placement.kind {
        WindowCityMaterialPlacementKind::Neon => {
            0.72 + emission * 0.72 + placement.importance * 0.3
        }
        WindowCityMaterialPlacementKind::Glass => {
            (emission * 0.26 + placement.faction_influence * 0.16 + placement.wetness * 0.08)
                .clamp(0.0, 0.56)
        }
        _ => emission * 0.42,
    };
    intensity *= 0.88 + pulse * 0.18;
    intensity += placement.damage * 0.1;

    if intensity <= 0.04 {
        return None;
    }

    let color = window_city_material_light_color(placement, pulse);
    let span = placement.half_extents_meters[0].max(placement.half_extents_meters[1]);
    let radius =
        (3.4 + span * 1.4 + placement.importance * 2.6 + placement.wetness * 0.7).clamp(1.5, 12.0);
    let z_lift = match placement.kind {
        WindowCityMaterialPlacementKind::Neon => 0.2,
        WindowCityMaterialPlacementKind::Glass => 1.05,
        _ => 0.65,
    };
    let position = [
        placement.center_meters[0],
        placement.center_meters[1],
        (placement.center_meters[2] + z_lift).max(camera_position_meters[2] - 1.2),
    ];

    Some(WindowDynamicLight::new(
        position,
        radius,
        color,
        intensity.clamp(0.0, 1.9),
    ))
}

fn window_city_material_light_color(
    placement: WindowCityMaterialPlacementVisual,
    pulse: f32,
) -> [f32; 3] {
    let mut color = [
        placement.base_color[0].max(0.01),
        placement.base_color[1].max(0.01),
        placement.base_color[2].max(0.01),
    ];
    if placement.faction_influence > 0.01 {
        color = [
            color[0]
                + (placement.faction_accent_color[0] - color[0])
                    * (placement.faction_influence * 0.42),
            color[1]
                + (placement.faction_accent_color[1] - color[1])
                    * (placement.faction_influence * 0.42),
            color[2]
                + (placement.faction_accent_color[2] - color[2])
                    * (placement.faction_influence * 0.42),
        ];
    }
    if matches!(placement.kind, WindowCityMaterialPlacementKind::Neon) {
        color[0] += 0.12 + pulse * 0.08;
        color[2] += 0.1 + placement.importance * 0.08;
    }
    normalize_color3(color)
}

fn window_dynamic_light_for_entity(
    tags: Option<&[String]>,
    material: Option<&MaterialDescriptor>,
    material_state: Option<&MaterialState>,
    transform: &Transform,
    frame_index: u64,
) -> Option<WindowDynamicLight> {
    let mut color = [0.0; 3];
    let mut intensity = 0.0;
    let mut radius: f32 = 0.0;

    if window_tags_have(tags, "light") || window_tags_have(tags, "neon") {
        let pulse = (frame_index as f32 * 0.11).sin().mul_add(0.5, 0.5);
        color = add_color3(color, [1.0, 0.18 + pulse * 0.12, 0.72 + pulse * 0.18]);
        intensity += 0.78 + pulse * 0.18;
        radius = radius.max(7.5);
    }

    if let Some(material) = material {
        let emission = material.visual.emission_linear;
        let emission_strength = (emission[0] + emission[1] + emission[2]).max(0.0);
        if emission_strength > 0.01 {
            let normalized = [
                (emission[0] / emission_strength).clamp(0.0, 1.0),
                (emission[1] / emission_strength).clamp(0.0, 1.0),
                (emission[2] / emission_strength).clamp(0.0, 1.0),
            ];
            color = add_color3(color, normalized);
            intensity += (emission_strength / 6.0).clamp(0.0, 1.2);
            radius = radius.max(4.5 + emission_strength.clamp(0.0, 10.0) * 0.38);
        }
    }

    if let Some(state) = material_state {
        let heat = ((state.temperature - 293.15) / 800.0).clamp(0.0, 1.0);
        let charge = (state.electrical_charge / 240.0).clamp(0.0, 1.0);
        if heat > 0.04 {
            color = add_color3(color, [1.0, 0.42, 0.12]);
            intensity += heat * 0.72;
            radius = radius.max(3.2 + heat * 3.8);
        }
        if charge > 0.04 {
            color = add_color3(color, [0.34, 0.82, 1.0]);
            intensity += charge * 0.86;
            radius = radius.max(3.8 + charge * 4.2);
        }
    }

    if intensity <= 0.04 || radius <= 0.05 {
        return None;
    }

    let color = normalize_color3(color);
    let position = [
        transform.translation_meters.x,
        transform.translation_meters.y,
        transform.translation_meters.z + 1.25,
    ];
    Some(WindowDynamicLight::new(
        position,
        radius.clamp(1.0, 12.0),
        color,
        intensity.clamp(0.0, 1.8),
    ))
}

fn add_color3(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}

fn normalize_color3(color: [f32; 3]) -> [f32; 3] {
    let max_channel = color[0].max(color[1]).max(color[2]);
    if max_channel <= f32::EPSILON {
        return [1.0, 1.0, 1.0];
    }
    [
        (color[0] / max_channel).clamp(0.0, 1.0),
        (color[1] / max_channel).clamp(0.0, 1.0),
        (color[2] / max_channel).clamp(0.0, 1.0),
    ]
}

fn window_proxy_kind_surface_response(kind: WindowEntityProxyKind) -> [f32; 4] {
    match kind {
        WindowEntityProxyKind::GenericRenderable => WINDOW_SURFACE_RESPONSE_DEFAULT,
        WindowEntityProxyKind::GlassWall => WINDOW_SURFACE_RESPONSE_GLASS,
        WindowEntityProxyKind::LightPanel => WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
        WindowEntityProxyKind::LowHazardSurface => WINDOW_SURFACE_RESPONSE_WET_ROAD,
    }
}

fn window_streaming_cell_state_color(state: WindowCityStreamingCellState) -> [f32; 4] {
    match state {
        WindowCityStreamingCellState::Unloaded => [0.12, 0.14, 0.16, 0.08],
        WindowCityStreamingCellState::SummaryLoaded => [0.24, 0.34, 0.44, 0.18],
        WindowCityStreamingCellState::GameplayLoaded => [0.24, 0.78, 1.0, 0.34],
        WindowCityStreamingCellState::RenderHighDetail => [0.42, 1.0, 0.66, 0.42],
        WindowCityStreamingCellState::HeroLoaded => [1.0, 0.7, 0.24, 0.52],
    }
}

fn window_generated_city_detail_budget(chunk: WindowGeneratedCityChunkVisual) -> usize {
    let base = match chunk.streaming_state {
        WindowCityStreamingCellState::Unloaded | WindowCityStreamingCellState::SummaryLoaded => 0,
        WindowCityStreamingCellState::GameplayLoaded => 2,
        WindowCityStreamingCellState::RenderHighDetail => 3,
        WindowCityStreamingCellState::HeroLoaded => 4,
    };
    if base == 0 {
        return 0;
    }

    let scale = window_generated_city_lod_scale(chunk);
    let consequence_bonus =
        usize::from(scale > 0.72 && (chunk.pollution + chunk.crime_pressure) > 0.72);
    ((base as f32 * scale).ceil() as usize + consequence_bonus).clamp(1, 4)
}

fn window_generated_city_camera_distance(chunk: WindowGeneratedCityChunkVisual) -> f32 {
    let [x, y] = chunk.center_meters;
    (x.mul_add(x, y * y)).sqrt()
}

fn window_generated_city_lod_scale(chunk: WindowGeneratedCityChunkVisual) -> f32 {
    let distance = window_generated_city_camera_distance(chunk);
    let distance_scale = if distance <= 18.0 {
        1.0
    } else if distance <= 42.0 {
        1.0 - ((distance - 18.0) / 24.0) * 0.42
    } else {
        0.42
    };
    let streaming_scale = match chunk.streaming_state {
        WindowCityStreamingCellState::Unloaded | WindowCityStreamingCellState::SummaryLoaded => 0.0,
        WindowCityStreamingCellState::GameplayLoaded => 0.55,
        WindowCityStreamingCellState::RenderHighDetail => 0.82,
        WindowCityStreamingCellState::HeroLoaded => 1.0,
    };
    distance_scale.min(streaming_scale).clamp(0.0, 1.0)
}

fn window_generated_city_major_segments(chunk: WindowGeneratedCityChunkVisual) -> usize {
    if window_generated_city_detail_budget(chunk) >= 4 {
        12
    } else {
        9
    }
}

fn window_generated_city_minor_segments(chunk: WindowGeneratedCityChunkVisual) -> usize {
    if window_generated_city_detail_budget(chunk) >= 4 {
        10
    } else {
        7
    }
}

fn window_generated_city_ellipsoid_segments(
    chunk: WindowGeneratedCityChunkVisual,
) -> (usize, usize) {
    if window_generated_city_detail_budget(chunk) >= 4 {
        (5, 12)
    } else {
        (4, 8)
    }
}

fn window_generated_city_crowd_density(chunk: WindowGeneratedCityChunkVisual) -> f32 {
    let expected_density = chunk.expected_background_crowd_count as f32 / 72.0;
    let rule_density = chunk.crowd_spawn_rule_count as f32 / 3.0;
    (expected_density * 0.72 + rule_density * 0.28).clamp(0.0, 1.0)
}

fn window_generated_city_traffic_density(chunk: WindowGeneratedCityChunkVisual) -> f32 {
    let expected_density = chunk.expected_vehicle_or_transit_count as f32 / 9.0;
    let rule_density = chunk.traffic_rule_count as f32 / 2.0;
    (expected_density * 0.66 + rule_density * 0.34).clamp(0.0, 1.0)
}

fn window_generated_city_crowd_visual_count(chunk: WindowGeneratedCityChunkVisual) -> usize {
    if chunk.expected_background_crowd_count == 0 && chunk.crowd_spawn_rule_count == 0 {
        return 0;
    }

    let expected = (chunk.expected_background_crowd_count as usize / 10).min(8);
    let rule_bonus = chunk.crowd_spawn_rule_count.min(4) as usize;
    let scaled =
        ((expected + rule_bonus) as f32 * window_generated_city_lod_scale(chunk)).ceil() as usize;
    scaled.clamp(1, 9)
}

fn window_generated_city_traffic_visual_count(chunk: WindowGeneratedCityChunkVisual) -> usize {
    if chunk.expected_vehicle_or_transit_count == 0 && chunk.traffic_rule_count == 0 {
        return 0;
    }

    let expected = (chunk.expected_vehicle_or_transit_count as usize / 4).min(4);
    let rule_bonus = chunk.traffic_rule_count.min(2) as usize;
    let scaled =
        ((expected + rule_bonus) as f32 * window_generated_city_lod_scale(chunk)).ceil() as usize;
    scaled.clamp(1, 4)
}

fn window_city_material_should_spend_micro_detail(
    placement: WindowCityMaterialPlacementVisual,
) -> bool {
    let [x, y, _] = placement.center_meters;
    let distance = (x.mul_add(x, y * y)).sqrt();
    if distance <= 18.0 {
        return true;
    }

    let consequence = placement
        .damage
        .max(placement.pollution)
        .max(placement.wetness);
    placement.importance > 0.35 || consequence > 0.46 || distance <= 32.0 && consequence > 0.24
}

fn window_generated_city_crowd_body_color(
    chunk: WindowGeneratedCityChunkVisual,
    index: usize,
) -> [f32; 4] {
    let market_tint = match chunk.district_kind {
        WindowCityDistrictVisualKind::CorporateCore => [0.2, 0.32, 0.38, 1.0],
        WindowCityDistrictVisualKind::RainAlleySlum => [0.16, 0.11, 0.08, 1.0],
        WindowCityDistrictVisualKind::IndustrialDock => [0.22, 0.17, 0.12, 1.0],
        WindowCityDistrictVisualKind::BlackMarket => [0.22, 0.07, 0.2, 1.0],
        WindowCityDistrictVisualKind::ClinicDistrict => [0.18, 0.28, 0.27, 1.0],
    };
    let faction_amount = if index.is_multiple_of(3) {
        chunk.faction_strength * 0.55
    } else {
        chunk.faction_strength * 0.18
    };
    let mut color = window_mix_color(market_tint, chunk.faction_color, faction_amount);
    color[3] = 0.86;
    color
}

fn window_generated_city_skin_color(seed: u64, salt: u64) -> [f32; 4] {
    let palette = [
        [0.68, 0.48, 0.36, 0.96],
        [0.55, 0.36, 0.25, 0.96],
        [0.42, 0.27, 0.19, 0.96],
        [0.76, 0.56, 0.42, 0.96],
        [0.36, 0.23, 0.17, 0.96],
    ];
    let index = (window_stable_unit(seed, 7_211 + salt) * palette.len() as f32)
        .floor()
        .clamp(0.0, (palette.len() - 1) as f32) as usize;
    let variation = 0.92 + window_stable_unit(seed, 7_503 + salt) * 0.16;
    window_scale_color(palette[index], variation)
}

fn window_generated_city_traffic_body_color(chunk: WindowGeneratedCityChunkVisual) -> [f32; 4] {
    match chunk.district_kind {
        WindowCityDistrictVisualKind::CorporateCore => [0.18, 0.28, 0.34, 0.96],
        WindowCityDistrictVisualKind::RainAlleySlum => [0.08, 0.07, 0.06, 0.92],
        WindowCityDistrictVisualKind::IndustrialDock => [0.28, 0.17, 0.08, 0.96],
        WindowCityDistrictVisualKind::BlackMarket => [0.12, 0.045, 0.11, 0.94],
        WindowCityDistrictVisualKind::ClinicDistrict => [0.12, 0.24, 0.25, 0.96],
    }
}

fn window_generated_city_district_color(kind: WindowCityDistrictVisualKind) -> [f32; 4] {
    match kind {
        WindowCityDistrictVisualKind::CorporateCore => [0.52, 0.68, 0.78, 1.0],
        WindowCityDistrictVisualKind::RainAlleySlum => [0.18, 0.22, 0.2, 1.0],
        WindowCityDistrictVisualKind::IndustrialDock => [0.46, 0.28, 0.18, 1.0],
        WindowCityDistrictVisualKind::BlackMarket => [0.42, 0.16, 0.36, 1.0],
        WindowCityDistrictVisualKind::ClinicDistrict => [0.62, 0.64, 0.58, 1.0],
    }
}

fn window_generated_city_surface_response(kind: WindowCityDistrictVisualKind) -> [f32; 4] {
    match kind {
        WindowCityDistrictVisualKind::CorporateCore => [0.2, 0.25, 0.2, 0.0],
        WindowCityDistrictVisualKind::RainAlleySlum => WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        WindowCityDistrictVisualKind::IndustrialDock => WINDOW_SURFACE_RESPONSE_METAL,
        WindowCityDistrictVisualKind::BlackMarket => [0.4, 0.05, 0.12, 0.0],
        WindowCityDistrictVisualKind::ClinicDistrict => [0.24, 0.08, 0.04, 0.0],
    }
}

fn window_generated_city_navigation_color(
    traversal: WindowCityTraversalVisualKind,
    priority: f32,
) -> [f32; 4] {
    let alpha = (0.34 + priority.clamp(0.0, 1.0) * 0.42).clamp(0.0, 0.9);
    match traversal {
        WindowCityTraversalVisualKind::Walk => [0.28, 0.8, 1.0, alpha],
        WindowCityTraversalVisualKind::CoverDash => [1.0, 0.66, 0.22, alpha],
        WindowCityTraversalVisualKind::StealthPath => [0.34, 1.0, 0.62, alpha],
        WindowCityTraversalVisualKind::ServiceLadder => [0.74, 0.52, 1.0, alpha],
        WindowCityTraversalVisualKind::Transit => [1.0, 0.28, 0.48, alpha],
    }
}

fn window_city_danger_color(kind: WindowCityDangerVisualKind, severity: f32) -> [f32; 4] {
    let alpha = (0.22 + severity.clamp(0.0, 1.0) * 0.44).clamp(0.0, 0.84);
    match kind {
        WindowCityDangerVisualKind::PhysicalDamage => [1.0, 0.42, 0.16, alpha],
        WindowCityDangerVisualKind::Flooding => [0.28, 0.76, 1.0, alpha],
        WindowCityDangerVisualKind::SlipperyContamination => [0.08, 0.09, 0.07, alpha],
        WindowCityDangerVisualKind::BiohazardContamination => [0.78, 0.18, 1.0, alpha],
        WindowCityDangerVisualKind::ToxicGas => [0.28, 1.0, 0.24, alpha],
        WindowCityDangerVisualKind::Surveillance => [0.24, 0.92, 1.0, alpha],
    }
}

fn window_city_route_consequence_color(
    status: WindowCityRouteConsequenceStatus,
    severity: f32,
) -> [f32; 4] {
    let alpha = (0.36 + severity.clamp(0.0, 1.0) * 0.46).clamp(0.0, 0.95);
    match status {
        WindowCityRouteConsequenceStatus::Blocked => [1.0, 0.16, 0.12, alpha],
        WindowCityRouteConsequenceStatus::Dangerous => [1.0, 0.66, 0.18, alpha],
        WindowCityRouteConsequenceStatus::Restricted => [0.84, 0.38, 1.0, alpha],
    }
}

fn window_city_persistent_cell_color(cell: WindowCityPersistentCellVisual) -> [f32; 4] {
    let severity = cell.severity.clamp(0.0, 1.0);
    if cell.blocked_route_count > 0 {
        [1.0, 0.16, 0.12, 0.2 + severity * 0.3]
    } else if cell.infrastructure_delta_count > 0 || cell.danger_field_count > 0 {
        [0.28, 0.76, 1.0, 0.18 + severity * 0.28]
    } else if cell.faction_delta_count > 0 || cell.restricted_zone_count > 0 {
        [0.84, 0.38, 1.0, 0.16 + severity * 0.26]
    } else if cell.story_thread_count > 0 || cell.ai_memory_count > 0 {
        [1.0, 0.62, 0.24, 0.16 + severity * 0.26]
    } else {
        [1.0, 0.42, 0.16, 0.14 + severity * 0.24]
    }
}

fn window_city_material_kind_color(kind: WindowCityMaterialPlacementKind) -> [f32; 4] {
    match kind {
        WindowCityMaterialPlacementKind::WetRoad => [0.018, 0.023, 0.029, 0.92],
        WindowCityMaterialPlacementKind::Glass => [0.62, 0.88, 1.0, 0.48],
        WindowCityMaterialPlacementKind::Neon => [0.94, 0.12, 0.68, 0.9],
        WindowCityMaterialPlacementKind::Water => [0.08, 0.22, 0.28, 0.44],
        WindowCityMaterialPlacementKind::HumanSkin => [0.72, 0.48, 0.36, 0.82],
        WindowCityMaterialPlacementKind::Metal => [0.24, 0.27, 0.3, 0.88],
        WindowCityMaterialPlacementKind::Generic => [0.28, 0.3, 0.32, 0.78],
    }
}

fn window_city_material_kind_surface_response(kind: WindowCityMaterialPlacementKind) -> [f32; 4] {
    match kind {
        WindowCityMaterialPlacementKind::WetRoad | WindowCityMaterialPlacementKind::Water => {
            WINDOW_SURFACE_RESPONSE_WET_ROAD
        }
        WindowCityMaterialPlacementKind::Glass => WINDOW_SURFACE_RESPONSE_GLASS,
        WindowCityMaterialPlacementKind::Neon => WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
        WindowCityMaterialPlacementKind::HumanSkin | WindowCityMaterialPlacementKind::Generic => {
            WINDOW_SURFACE_RESPONSE_DEFAULT
        }
        WindowCityMaterialPlacementKind::Metal => WINDOW_SURFACE_RESPONSE_METAL,
    }
}

fn window_surface_decal_kind_color(kind: WindowSurfaceDecalKind) -> [f32; 4] {
    match kind {
        WindowSurfaceDecalKind::GrimeStain => [0.045, 0.04, 0.034, 0.38],
        WindowSurfaceDecalKind::CrackField => [0.012, 0.014, 0.014, 0.62],
        WindowSurfaceDecalKind::WaterPuddle => [0.075, 0.2, 0.26, 0.44],
        WindowSurfaceDecalKind::RoadMarking => [0.76, 0.74, 0.58, 0.46],
        WindowSurfaceDecalKind::Poster => [0.86, 0.52, 0.28, 0.64],
        WindowSurfaceDecalKind::Graffiti => [0.28, 0.88, 1.0, 0.58],
        WindowSurfaceDecalKind::Scorch => [0.025, 0.02, 0.014, 0.6],
        WindowSurfaceDecalKind::Corrosion => [0.38, 0.16, 0.065, 0.56],
    }
}

fn window_surface_decal_kind_surface_response(kind: WindowSurfaceDecalKind) -> [f32; 4] {
    match kind {
        WindowSurfaceDecalKind::WaterPuddle => WINDOW_SURFACE_RESPONSE_WET_ROAD,
        WindowSurfaceDecalKind::RoadMarking => [0.72, 0.0, 0.18, 0.0],
        WindowSurfaceDecalKind::Graffiti => [0.44, 0.0, 0.08, 0.18],
        WindowSurfaceDecalKind::Scorch => WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
        WindowSurfaceDecalKind::Corrosion
        | WindowSurfaceDecalKind::GrimeStain
        | WindowSurfaceDecalKind::CrackField
        | WindowSurfaceDecalKind::Poster => WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
    }
}

fn window_city_material_state_color(
    placement: WindowCityMaterialPlacementVisual,
    pulse: f32,
) -> [f32; 4] {
    let mut color = placement.base_color;
    let grime = placement.pollution * 0.18 + placement.traffic_wear * 0.1;
    color[0] *= 1.0 - grime;
    color[1] *= 1.0 - grime * 0.85;
    color[2] *= 1.0 - grime * 0.66;

    if placement.wetness > 0.05 {
        color[0] *= 1.0 - placement.wetness * 0.16;
        color[1] *= 1.0 - placement.wetness * 0.12;
        color[2] += placement.wetness * 0.045;
    }

    if placement.faction_influence > 0.01 {
        color = window_mix_color(
            color,
            placement.faction_accent_color,
            placement.faction_influence * 0.2,
        );
    }

    if matches!(placement.kind, WindowCityMaterialPlacementKind::Neon) {
        color = window_scale_color(color, 0.92 + pulse * 0.22 + placement.importance * 0.18);
    }

    color[3] = (color[3] * (0.62 + placement.importance * 0.24)
        + placement.wetness * 0.1
        + placement.damage * 0.08)
        .clamp(0.16, 1.0);
    window_clamp_color(color)
}

fn window_city_material_state_surface_response(
    placement: WindowCityMaterialPlacementVisual,
) -> [f32; 4] {
    let mut response = placement.surface_response;
    response[0] =
        (response[0] - placement.wetness * 0.2 + placement.pollution * 0.12).clamp(0.03, 1.0);
    response[1] = (response[1] + placement.traffic_wear * 0.08).clamp(0.0, 1.0);
    response[2] = (response[2] + placement.wetness * 0.32).clamp(0.0, 1.0);
    response[3] = (response[3]
        + if matches!(placement.kind, WindowCityMaterialPlacementKind::Neon) {
            0.22 + placement.importance * 0.28
        } else {
            0.0
        })
    .clamp(0.0, 1.0);
    window_clamp_surface_response(response)
}

fn window_decal_packet_from_city_material_placements(
    placements: &[WindowCityMaterialPlacementVisual],
) -> WindowDecalPacketVisual {
    let mut packet = WindowDecalPacketVisual::default();
    for placement in placements {
        window_push_city_material_decals(&mut packet, *placement);
    }
    packet
}

fn window_push_city_material_decals(
    packet: &mut WindowDecalPacketVisual,
    placement: WindowCityMaterialPlacementVisual,
) {
    let [x, y, z] = placement.center_meters;
    let [half_x, half_y] = placement.half_extents_meters;
    let seed = placement.detail_seed ^ window_position_seed(placement.center_meters);
    let state_intensity = (placement.wetness * 0.34
        + placement.damage * 0.38
        + placement.pollution * 0.3
        + placement.traffic_wear * 0.22)
        .clamp(0.0, 1.0);
    let relief =
        (placement.damage * 0.44 + placement.pollution * 0.34 + placement.traffic_wear * 0.18)
            .clamp(0.0, 1.0);

    match placement.kind {
        WindowCityMaterialPlacementKind::WetRoad => {
            let top_z = z.max(0.018) + 0.079;
            if placement.traffic_wear > 0.08 {
                packet.push(
                    WindowSurfaceDecalVisual::new(
                        [x, y, top_z],
                        [half_x * 0.72, 0.035 + placement.traffic_wear * 0.028],
                        WindowSurfaceDecalKind::RoadMarking,
                    )
                    .with_color([0.76, 0.74, 0.58, 0.24 + placement.traffic_wear * 0.32])
                    .with_surface_response(window_clamp_surface_response([
                        0.7,
                        0.0,
                        placement.wetness * 0.24,
                        0.0,
                    ]))
                    .with_state_detail(1.45, state_intensity.max(0.32), relief.max(0.18))
                    .with_detail_seed(seed ^ 0xCA11_E001),
                );
            }
            if placement.wetness > 0.16 {
                let offset_x = (window_stable_unit(seed, 5_701) - 0.5) * half_x * 0.72;
                let offset_y = (window_stable_unit(seed, 5_801) - 0.5) * half_y * 0.64;
                packet.push(
                    WindowSurfaceDecalVisual::new(
                        [x + offset_x, y + offset_y, top_z + 0.004],
                        [
                            half_x * (0.16 + placement.wetness * 0.18),
                            half_y * (0.1 + placement.wetness * 0.12),
                        ],
                        WindowSurfaceDecalKind::WaterPuddle,
                    )
                    .with_color([0.075, 0.2, 0.26, 0.22 + placement.wetness * 0.32])
                    .with_state_detail(1.25 + placement.wetness, placement.wetness, 0.12)
                    .with_detail_seed(seed ^ 0x0D5E_A11E),
                );
            }
            if placement.pollution > 0.12 {
                packet.push(
                    WindowSurfaceDecalVisual::new(
                        [x - half_x * 0.22, y + half_y * 0.24, top_z + 0.008],
                        [half_x * 0.28, half_y * 0.18],
                        WindowSurfaceDecalKind::GrimeStain,
                    )
                    .with_color([0.045, 0.04, 0.034, 0.18 + placement.pollution * 0.36])
                    .with_state_detail(1.55, state_intensity.max(placement.pollution), relief)
                    .with_detail_seed(seed ^ 0x6A1D_0001),
                );
            }
            if placement.damage > 0.22 {
                packet.push(
                    WindowSurfaceDecalVisual::new(
                        [x + half_x * 0.18, y - half_y * 0.14, top_z + 0.01],
                        [half_x * 0.2, half_y * 0.12],
                        WindowSurfaceDecalKind::Scorch,
                    )
                    .with_state_detail(1.9, placement.damage, relief.max(0.34))
                    .with_detail_seed(seed ^ 0x5C0C_0001),
                );
            }
        }
        WindowCityMaterialPlacementKind::Glass => {
            let panel_height = half_y.max(0.52);
            let panel_width = half_x.max(0.42);
            let center_z = (z + panel_height).max(0.84);
            if placement.damage > 0.04 {
                packet.push(
                    WindowSurfaceDecalVisual::new(
                        [x, y + 0.032, center_z],
                        [panel_width * 0.42, panel_height * 0.34],
                        WindowSurfaceDecalKind::CrackField,
                    )
                    .with_axes([1.0, 0.0, 0.0], [0.0, 0.0, 1.0])
                    .with_color([0.82, 0.94, 1.0, 0.2 + placement.damage * 0.44])
                    .with_surface_response(WINDOW_SURFACE_RESPONSE_GLASS)
                    .with_state_detail(1.35 + placement.damage, placement.damage, placement.damage)
                    .with_detail_seed(seed ^ 0xC4AC_0001),
                );
            }
            if placement.wetness > 0.08 || placement.pollution > 0.08 {
                packet.push(
                    WindowSurfaceDecalVisual::new(
                        [
                            x - panel_width * 0.18,
                            y + 0.034,
                            center_z - panel_height * 0.08,
                        ],
                        [panel_width * 0.18, panel_height * 0.46],
                        WindowSurfaceDecalKind::GrimeStain,
                    )
                    .with_axes([1.0, 0.0, 0.0], [0.0, 0.0, 1.0])
                    .with_color([
                        0.48,
                        0.72,
                        0.82,
                        0.12 + placement.wetness * 0.22 + placement.pollution * 0.12,
                    ])
                    .with_surface_response(WINDOW_SURFACE_RESPONSE_GLASS)
                    .with_state_detail(1.5, state_intensity.max(0.28), relief.max(0.18))
                    .with_detail_seed(seed ^ 0x5712_E001),
                );
            }
        }
        WindowCityMaterialPlacementKind::Neon => {
            let sign_z = z.max(0.44);
            let half_width = half_x.max(0.36);
            let half_height = half_y.max(0.08);
            packet.push(
                WindowSurfaceDecalVisual::new(
                    [x, y + 0.036, sign_z],
                    [half_width * 0.72, half_height * 0.72],
                    WindowSurfaceDecalKind::Graffiti,
                )
                .with_axes([1.0, 0.0, 0.0], [0.0, 0.0, 1.0])
                .with_color(window_with_alpha(
                    window_mix_color(placement.base_color, placement.faction_accent_color, 0.44),
                    0.24 + placement.importance * 0.3,
                ))
                .with_state_detail(
                    1.28,
                    (placement.importance + placement.damage * 0.5).clamp(0.0, 1.0),
                    0.18,
                )
                .with_detail_seed(seed ^ 0x6B4F_0001),
            );
            if placement.damage > 0.1 {
                packet.push(
                    WindowSurfaceDecalVisual::new(
                        [x + half_width * 0.18, y + 0.038, sign_z],
                        [half_width * 0.18, half_height * 0.52],
                        WindowSurfaceDecalKind::Scorch,
                    )
                    .with_axes([1.0, 0.0, 0.0], [0.0, 0.0, 1.0])
                    .with_state_detail(2.0, placement.damage, placement.damage * 0.58)
                    .with_detail_seed(seed ^ 0x5C0C_0002),
                );
            }
        }
        WindowCityMaterialPlacementKind::Water => {
            let top_z = z.max(0.032) + 0.061;
            packet.push(
                WindowSurfaceDecalVisual::new(
                    [x, y, top_z],
                    [half_x * 0.48, half_y * 0.34],
                    WindowSurfaceDecalKind::WaterPuddle,
                )
                .with_color([0.075, 0.2, 0.26, 0.26 + placement.wetness * 0.26])
                .with_state_detail(
                    1.2 + placement.wetness * 0.7,
                    placement.wetness.max(0.44),
                    0.12,
                )
                .with_detail_seed(seed ^ 0x0D5E_A11F),
            );
            if placement.pollution > 0.08 {
                packet.push(
                    WindowSurfaceDecalVisual::new(
                        [x + half_x * 0.2, y - half_y * 0.16, top_z + 0.006],
                        [half_x * 0.22, half_y * 0.12],
                        WindowSurfaceDecalKind::GrimeStain,
                    )
                    .with_color([0.12, 0.18, 0.08, 0.18 + placement.pollution * 0.28])
                    .with_state_detail(1.55, placement.pollution, placement.pollution * 0.42)
                    .with_detail_seed(seed ^ 0x6A1D_0002),
                );
            }
        }
        WindowCityMaterialPlacementKind::HumanSkin => {
            let base_z = z.max(0.08);
            if placement.wetness > 0.08 || placement.pollution > 0.08 {
                packet.push(
                    WindowSurfaceDecalVisual::new(
                        [x, y - half_y * 0.255, base_z + 0.78],
                        [half_x * 0.16, 0.08 + half_y * 0.04],
                        WindowSurfaceDecalKind::GrimeStain,
                    )
                    .with_axes([1.0, 0.0, 0.0], [0.0, 0.0, 1.0])
                    .with_color([
                        0.12,
                        0.08,
                        0.06,
                        0.12 + placement.pollution * 0.18 + placement.wetness * 0.12,
                    ])
                    .with_surface_response(WINDOW_SURFACE_RESPONSE_HUMAN_SKIN)
                    .with_state_detail(1.2, state_intensity.max(0.18), 0.12)
                    .with_detail_seed(seed ^ 0x51A1_0001),
                );
            }
        }
        WindowCityMaterialPlacementKind::Metal | WindowCityMaterialPlacementKind::Generic => {
            let top_z = z.max(0.05) + 0.135 + placement.importance * 0.22;
            if placement.pollution > 0.12 || placement.damage > 0.08 {
                let kind = if matches!(placement.kind, WindowCityMaterialPlacementKind::Metal) {
                    WindowSurfaceDecalKind::Corrosion
                } else {
                    WindowSurfaceDecalKind::GrimeStain
                };
                packet.push(
                    WindowSurfaceDecalVisual::new(
                        [x - half_x * 0.16, y + half_y * 0.18, top_z],
                        [half_x * 0.32, half_y * 0.22],
                        kind,
                    )
                    .with_state_detail(
                        1.55 + placement.pollution * 0.6,
                        state_intensity.max(0.28),
                        relief.max(0.22),
                    )
                    .with_detail_seed(seed ^ 0xC011_0510),
                );
            }
            if placement.faction_influence > 0.08 {
                packet.push(
                    WindowSurfaceDecalVisual::new(
                        [x + half_x * 0.08, y - half_y * 0.18, top_z + 0.006],
                        [half_x * 0.34, half_y * 0.16],
                        WindowSurfaceDecalKind::Graffiti,
                    )
                    .with_color(window_with_alpha(
                        placement.faction_accent_color,
                        0.18 + placement.faction_influence * 0.34,
                    ))
                    .with_state_detail(1.32, placement.faction_influence, 0.18)
                    .with_detail_seed(seed ^ 0x6B4F_0002),
                );
            } else if placement.importance > 0.18 {
                packet.push(
                    WindowSurfaceDecalVisual::new(
                        [x + half_x * 0.1, y - half_y * 0.12, top_z + 0.006],
                        [half_x * 0.22, half_y * 0.18],
                        WindowSurfaceDecalKind::Poster,
                    )
                    .with_state_detail(1.28, state_intensity.max(0.22), relief.max(0.2))
                    .with_detail_seed(seed ^ 0x9057_0001),
                );
            }
            if placement.damage > 0.3 {
                packet.push(
                    WindowSurfaceDecalVisual::new(
                        [x + half_x * 0.26, y + half_y * 0.05, top_z + 0.01],
                        [half_x * 0.16, half_y * 0.13],
                        WindowSurfaceDecalKind::Scorch,
                    )
                    .with_state_detail(2.0, placement.damage, placement.damage * 0.55)
                    .with_detail_seed(seed ^ 0x5C0C_0003),
                );
            }
        }
    }
}

fn window_generated_city_window_color(
    chunk: WindowGeneratedCityChunkVisual,
    flicker: f32,
) -> [f32; 4] {
    let activity = (0.28 + flicker * 0.18 + chunk.surveillance_level * 0.1).clamp(0.0, 1.0);
    match chunk.district_kind {
        WindowCityDistrictVisualKind::CorporateCore => [
            0.5 + activity * 0.18,
            0.58 + activity * 0.16,
            0.68 + activity * 0.2,
            0.16 + activity * 0.24,
        ],
        WindowCityDistrictVisualKind::RainAlleySlum => [
            0.18 + activity * 0.16,
            0.22 + activity * 0.2,
            0.22 + chunk.pollution * 0.08,
            0.15 + activity * 0.2,
        ],
        WindowCityDistrictVisualKind::IndustrialDock => [
            0.82,
            0.48 + activity * 0.12,
            0.24 + chunk.pollution * 0.06,
            0.16 + activity * 0.24,
        ],
        WindowCityDistrictVisualKind::BlackMarket => [
            0.72 + activity * 0.12,
            0.2 + activity * 0.16,
            0.46 + activity * 0.18,
            0.18 + activity * 0.28,
        ],
        WindowCityDistrictVisualKind::ClinicDistrict => [
            0.74 + activity * 0.08,
            0.84,
            0.74 + activity * 0.08,
            0.16 + activity * 0.24,
        ],
    }
}

fn window_generated_city_sign_color(
    chunk: WindowGeneratedCityChunkVisual,
    segment_index: usize,
) -> [f32; 4] {
    let faction_alpha = 0.28 + chunk.faction_strength * 0.24;
    if segment_index.is_multiple_of(3) {
        return window_with_alpha(chunk.faction_color, faction_alpha);
    }

    match chunk.district_kind {
        WindowCityDistrictVisualKind::CorporateCore => [0.62, 0.74, 0.8, 0.48],
        WindowCityDistrictVisualKind::RainAlleySlum => [0.34, 0.54, 0.54, 0.42],
        WindowCityDistrictVisualKind::IndustrialDock => [0.86, 0.58, 0.24, 0.48],
        WindowCityDistrictVisualKind::BlackMarket => [0.82, 0.24, 0.56, 0.52],
        WindowCityDistrictVisualKind::ClinicDistrict => [0.86, 0.42, 0.42, 0.46],
    }
}

fn window_tags_have(tags: Option<&[String]>, expected: &str) -> bool {
    tags.is_some_and(|tags| tags.iter().any(|tag| tag == expected))
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowHumanoidProxyInstance<'a> {
    pub center_meters: [f32; 2],
    pub z_meters: f32,
    pub color: [f32; 4],
    pub human: Option<&'a HumanState>,
    pub emotion: Option<&'a EmotionState>,
    pub surface_state: Option<HumanSurfaceState>,
    pub viewer_position: [f32; 3],
    pub facing_direction: Option<[f32; 2]>,
    pub locomotion_weight_0_to_1: f32,
    pub crouch_weight_0_to_1: f32,
}

impl<'a> WindowHumanoidProxyInstance<'a> {
    pub fn new(center_meters: [f32; 2], z_meters: f32, color: [f32; 4]) -> Self {
        Self {
            center_meters,
            z_meters,
            color,
            human: None,
            emotion: None,
            surface_state: None,
            viewer_position: [0.0, -1.0, 1.65],
            facing_direction: None,
            locomotion_weight_0_to_1: 0.0,
            crouch_weight_0_to_1: 0.0,
        }
    }

    pub fn with_human(mut self, human: Option<&'a HumanState>) -> Self {
        self.human = human;
        self
    }

    pub fn with_emotion(mut self, emotion: Option<&'a EmotionState>) -> Self {
        self.emotion = emotion;
        self
    }

    pub fn with_surface_state(mut self, surface_state: Option<HumanSurfaceState>) -> Self {
        self.surface_state = surface_state;
        self
    }

    pub fn with_viewer_position(mut self, viewer_position: [f32; 3]) -> Self {
        self.viewer_position = viewer_position;
        self
    }

    pub fn with_facing_direction(mut self, facing_direction: [f32; 2]) -> Self {
        let length_squared =
            facing_direction[0] * facing_direction[0] + facing_direction[1] * facing_direction[1];
        if length_squared > f32::EPSILON
            && facing_direction[0].is_finite()
            && facing_direction[1].is_finite()
        {
            let length = length_squared.sqrt();
            self.facing_direction =
                Some([facing_direction[0] / length, facing_direction[1] / length]);
        }
        self
    }

    pub fn with_facing_yaw_radians(self, yaw_radians: f32) -> Self {
        self.with_facing_direction([yaw_radians.cos(), yaw_radians.sin()])
    }

    pub fn with_pose_weights(
        mut self,
        locomotion_weight_0_to_1: f32,
        crouch_weight_0_to_1: f32,
    ) -> Self {
        self.locomotion_weight_0_to_1 = locomotion_weight_0_to_1.clamp(0.0, 1.0);
        self.crouch_weight_0_to_1 = crouch_weight_0_to_1.clamp(0.0, 1.0);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct HumanoidDetailBasis {
    forward: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    front_body: [f32; 3],
    front_face: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct HumanoidOrientationBasis {
    forward: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct HumanoidPoseWeights {
    locomotion: f32,
    crouch: f32,
}

fn humanoid_instance_orientation_basis(
    instance: &WindowHumanoidProxyInstance<'_>,
) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let forward = instance
        .facing_direction
        .map(|direction| normalize3([direction[0], direction[1], 0.0]))
        .filter(|direction| dot3(*direction, *direction) > f32::EPSILON)
        .unwrap_or_else(|| {
            horizontal_direction([
                instance.viewer_position[0] - instance.center_meters[0],
                instance.viewer_position[1] - instance.center_meters[1],
                0.0,
            ])
        });
    let right = normalize3([-forward[1], forward[0], 0.0]);
    (forward, right, [0.0, 0.0, 1.0])
}

fn humanoid_oriented_point(
    origin: [f32; 3],
    basis: HumanoidOrientationBasis,
    local: [f32; 3],
) -> [f32; 3] {
    add3(
        origin,
        add3(
            add3(
                scale3(basis.right, local[0]),
                scale3(basis.forward, local[1]),
            ),
            scale3(basis.up, local[2]),
        ),
    )
}

impl WindowProceduralMeshInstance {
    pub fn new(
        mesh: MeshAssetHandle,
        center_meters: [f32; 2],
        z_meters: f32,
        color: [f32; 4],
    ) -> Self {
        Self {
            mesh,
            center_meters,
            z_meters,
            color,
            crack_density: 0.0,
            moisture: 0.0,
            soot: 0.0,
            corrosion: 0.0,
            heat: 0.0,
            electrical_charge: 0.0,
            plastic_strain: 0.0,
            biological_contamination: 0.0,
            oil_contamination: 0.0,
            billboard_right: [1.0, 0.0, 0.0],
        }
    }

    pub fn with_material_state(mut self, material_state: Option<&MaterialState>) -> Self {
        if let Some(material_state) = material_state {
            self.crack_density = material_state.crack_density;
            self.moisture = material_state.moisture;
            self.soot = material_state.soot;
            self.corrosion = material_state.corrosion;
            self.heat = ((material_state.temperature - 293.15) / 800.0).clamp(0.0, 1.0);
            self.electrical_charge = (material_state.electrical_charge / 240.0).clamp(0.0, 1.0);
            self.plastic_strain = material_state.plastic_strain.clamp(0.0, 1.0);
            self.biological_contamination = material_state.biological_contamination.clamp(0.0, 1.0);
            self.oil_contamination = material_state.oil_contamination.clamp(0.0, 1.0);
        }
        self
    }

    pub fn with_billboard_right(mut self, billboard_right: [f32; 3]) -> Self {
        self.billboard_right = billboard_right;
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WindowSceneGeometry {
    pub vertices: Vec<WindowSceneVertex>,
    pub indices: Vec<u32>,
}

impl WindowSceneGeometry {
    pub fn push_indexed_quad(&mut self, vertices: [WindowSceneVertex; 4]) {
        let base = u32::try_from(self.vertices.len())
            .expect("window scene vertex count should fit u32 indices");
        self.vertices.extend(vertices);
        self.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    pub fn screen_quad(
        &mut self,
        a: [f32; 2],
        b: [f32; 2],
        c: [f32; 2],
        d: [f32; 2],
        depth: f32,
        color: [f32; 4],
    ) {
        self.push_indexed_quad([
            WindowSceneVertex::screen([a[0], a[1], depth], color),
            WindowSceneVertex::screen([b[0], b[1], depth], color),
            WindowSceneVertex::screen([c[0], c[1], depth], color),
            WindowSceneVertex::screen([d[0], d[1], depth], color),
        ]);
    }

    pub fn screen_rect(&mut self, min: [f32; 2], max: [f32; 2], depth: f32, color: [f32; 4]) {
        self.screen_quad(min, [max[0], min[1]], max, [min[0], max[1]], depth, color);
    }

    pub fn screen_ellipse(
        &mut self,
        center: [f32; 2],
        radii: [f32; 2],
        segments: usize,
        depth: f32,
        color: [f32; 4],
    ) {
        if radii[0] <= f32::EPSILON || radii[1] <= f32::EPSILON {
            return;
        }

        let segments = segments.max(6);
        let step = std::f32::consts::TAU / segments as f32;
        for index in 0..segments {
            let angle0 = index as f32 * step;
            let angle1 = (index + 1) as f32 * step;
            let p0 = [
                center[0] + angle0.cos() * radii[0],
                center[1] + angle0.sin() * radii[1],
            ];
            let p1 = [
                center[0] + angle1.cos() * radii[0],
                center[1] + angle1.sin() * radii[1],
            ];
            self.push_indexed_quad([
                WindowSceneVertex::screen([center[0], center[1], depth], color),
                WindowSceneVertex::screen([p0[0], p0[1], depth], color),
                WindowSceneVertex::screen([p1[0], p1[1], depth], color),
                WindowSceneVertex::screen([center[0], center[1], depth], color),
            ]);
        }
    }

    pub fn screen_rect_default_depth(&mut self, min: [f32; 2], max: [f32; 2], color: [f32; 4]) {
        self.screen_rect(min, max, WINDOW_SCREEN_SPACE_DEPTH, color);
    }

    pub fn add_window_natural_sky(&mut self, frame_index: u64) {
        let far_depth = 0.99;
        self.screen_rect(
            [-1.0, -1.0],
            [1.0, -0.34],
            far_depth,
            [0.58, 0.68, 0.76, 1.0],
        );
        self.screen_rect(
            [-1.0, -0.34],
            [1.0, 0.28],
            far_depth,
            [0.46, 0.56, 0.66, 1.0],
        );
        self.screen_rect([-1.0, 0.28], [1.0, 1.0], far_depth, [0.31, 0.39, 0.5, 1.0]);
        self.screen_rect(
            [-1.0, -0.58],
            [1.0, -0.34],
            far_depth - 0.001,
            [0.72, 0.68, 0.58, 0.18],
        );
        self.screen_rect(
            [-1.0, -0.36],
            [1.0, -0.18],
            far_depth - 0.002,
            [0.58, 0.64, 0.66, 0.2],
        );
        self.screen_rect(
            [-1.0, 0.52],
            [1.0, 1.0],
            far_depth - 0.001,
            [0.22, 0.28, 0.38, 0.2],
        );

        for star in 0..22 {
            let seed = 41_000 + star as u64;
            let x = -0.96 + window_stable_unit(seed, 1) * 1.92;
            let y = 0.36 + window_stable_unit(seed, 2) * 0.56;
            let size = 0.0025 + window_stable_unit(seed, 3) * 0.003;
            let twinkle =
                0.08 + ((frame_index as f32 * 0.018 + star as f32 * 0.73).sin() * 0.5 + 0.5) * 0.1;
            self.screen_rect(
                [x - size, y - size],
                [x + size, y + size],
                far_depth - 0.009,
                [0.82, 0.88, 0.95, twinkle],
            );
        }

        self.screen_rect(
            [0.30, -0.66],
            [0.92, -0.60],
            far_depth - 0.006,
            [1.0, 0.82, 0.56, 0.22],
        );
        self.screen_rect(
            [0.48, -0.61],
            [0.78, -0.588],
            far_depth - 0.008,
            [1.0, 0.90, 0.68, 0.46],
        );
        self.screen_rect(
            [-0.86, -0.74],
            [-0.58, -0.70],
            far_depth - 0.007,
            [0.72, 0.78, 0.82, 0.20],
        );
        self.screen_rect(
            [-0.79, -0.704],
            [-0.66, -0.694],
            far_depth - 0.009,
            [0.82, 0.86, 0.84, 0.36],
        );

        for (cluster, base) in [
            (0_u64, [-0.78, -0.47]),
            (1, [-0.36, -0.26]),
            (2, [0.18, -0.38]),
            (3, [0.48, -0.08]),
            (4, [-0.58, 0.16]),
            (5, [0.78, 0.28]),
            (6, [-0.08, 0.38]),
        ] {
            let drift = ((frame_index as f32 * (0.0018 + cluster as f32 * 0.00035))
                + cluster as f32 * 0.29)
                .fract();
            let base_x = base[0] + (drift - 0.5) * 0.12;
            let base_y =
                base[1] + ((frame_index as f32 * 0.001 + cluster as f32 * 0.13).sin()) * 0.018;
            let cloud_alpha = 0.12 + (cluster as f32 * 0.017).min(0.06);
            self.screen_rect(
                [base_x - 0.28, base_y - 0.02],
                [base_x + 0.3, base_y + 0.015],
                far_depth - 0.003,
                [0.54, 0.6, 0.64, cloud_alpha * 0.42],
            );
            self.screen_rect(
                [base_x - 0.22, base_y - 0.052],
                [base_x + 0.24, base_y - 0.034],
                far_depth - 0.004,
                [0.34, 0.4, 0.46, cloud_alpha * 0.36],
            );
        }
    }

    pub fn screen_hud_bar(
        &mut self,
        origin: [f32; 2],
        width: f32,
        normalized_value: f32,
        color: [f32; 4],
    ) {
        let height = 0.028;
        let value = normalized_value.clamp(0.0, 1.0);
        self.screen_rect_default_depth(
            origin,
            [origin[0] + width, origin[1] + height],
            [0.11, 0.13, 0.16, 0.68],
        );
        self.screen_rect_default_depth(
            [origin[0] + 0.006, origin[1] + 0.006],
            [
                origin[0] + 0.006 + (width - 0.012).max(0.0) * value,
                origin[1] + height - 0.006,
            ],
            color,
        );
    }

    pub fn add_window_status_hud(
        &mut self,
        frame_index: u64,
        event_count: usize,
        gpu_pass_count: usize,
        queued_action_count: u64,
    ) {
        let pulse = ((frame_index as f32 * 0.06).sin() * 0.5 + 0.5) * 0.12;
        self.screen_rect_default_depth(
            [-0.98, 0.78],
            [-0.38, 0.96],
            [0.025 + pulse, 0.032, 0.048, 0.82],
        );
        self.screen_hud_bar(
            [-0.94, 0.91],
            0.5,
            event_count as f32 / 42.0,
            [0.24, 0.78, 0.96, 0.95],
        );
        self.screen_hud_bar(
            [-0.94, 0.85],
            0.5,
            gpu_pass_count as f32 / 48.0,
            [0.94, 0.58, 0.22, 0.92],
        );
        self.screen_hud_bar(
            [-0.94, 0.79],
            0.5,
            queued_action_count as f32 / 8.0,
            [0.34, 1.0, 0.68, 0.92],
        );
    }

    pub fn add_window_interaction_reticle(&mut self, color: [f32; 4]) {
        self.screen_rect_default_depth([-0.012, -0.09], [0.012, -0.035], color);
        self.screen_rect_default_depth([-0.012, 0.035], [0.012, 0.09], color);
        self.screen_rect_default_depth([-0.09, -0.012], [-0.035, 0.012], color);
        self.screen_rect_default_depth([0.035, -0.012], [0.09, 0.012], color);
    }

    pub fn add_window_hud_event_strip(
        &mut self,
        visible_pulses: &[WindowHudPulseVisual],
        max_pulses: usize,
    ) {
        self.screen_rect_default_depth([-0.98, -0.95], [0.98, -0.84], [0.018, 0.022, 0.03, 0.72]);

        if visible_pulses.is_empty() || max_pulses == 0 {
            self.screen_rect_default_depth(
                [-0.94, -0.905],
                [-0.82, -0.885],
                [0.2, 0.26, 0.34, 0.28],
            );
            return;
        }

        let slot_width = 1.86 / max_pulses as f32;
        for (index, pulse) in visible_pulses.iter().take(max_pulses).enumerate() {
            let x0 = -0.94 + index as f32 * slot_width;
            let x1 = x0 + slot_width * 0.72;
            let center_y = -0.895;
            let half_height = 0.018 + pulse.weight * 0.026;
            self.screen_rect_default_depth(
                [x0, center_y - half_height],
                [x1, center_y + half_height],
                pulse.color,
            );
        }
    }

    pub fn world_quad(
        &mut self,
        a: [f32; 3],
        b: [f32; 3],
        c: [f32; 3],
        d: [f32; 3],
        color: [f32; 4],
    ) {
        self.world_quad_with_surface_response(a, b, c, d, color, WINDOW_SURFACE_RESPONSE_DEFAULT);
    }

    pub fn world_quad_with_surface_response(
        &mut self,
        a: [f32; 3],
        b: [f32; 3],
        c: [f32; 3],
        d: [f32; 3],
        color: [f32; 4],
        surface_response: [f32; 4],
    ) {
        let normal = quad_normal(a, b, c);
        let material_detail = window_surface_detail_for_quad(a, b, c, d, color, surface_response);
        let generated_cache = window_generated_surface_cache_for_quad(
            a,
            b,
            c,
            d,
            color,
            surface_response,
            material_detail,
        );
        self.push_indexed_quad([
            WindowSceneVertex::world_with_surface_detail_and_cache(
                a,
                normal,
                color,
                surface_response,
                material_detail,
                generated_cache,
            ),
            WindowSceneVertex::world_with_surface_detail_and_cache(
                b,
                normal,
                color,
                surface_response,
                material_detail,
                generated_cache,
            ),
            WindowSceneVertex::world_with_surface_detail_and_cache(
                c,
                normal,
                color,
                surface_response,
                material_detail,
                generated_cache,
            ),
            WindowSceneVertex::world_with_surface_detail_and_cache(
                d,
                normal,
                color,
                surface_response,
                material_detail,
                generated_cache,
            ),
        ]);
    }

    fn world_quad_with_surface_state_detail(
        &mut self,
        corners: [[f32; 3]; 4],
        color: [f32; 4],
        surface_response: [f32; 4],
        state_detail: WindowMaterialStateDetail,
    ) {
        let [a, b, c, d] = corners;
        let normal = quad_normal(a, b, c);
        let material_detail = window_apply_material_state_detail(
            window_surface_detail_for_quad(a, b, c, d, color, surface_response),
            state_detail,
        );
        let generated_cache = window_generated_surface_cache_for_quad(
            a,
            b,
            c,
            d,
            color,
            surface_response,
            material_detail,
        );
        self.push_indexed_quad([
            WindowSceneVertex::world_with_surface_detail_and_cache(
                a,
                normal,
                color,
                surface_response,
                material_detail,
                generated_cache,
            ),
            WindowSceneVertex::world_with_surface_detail_and_cache(
                b,
                normal,
                color,
                surface_response,
                material_detail,
                generated_cache,
            ),
            WindowSceneVertex::world_with_surface_detail_and_cache(
                c,
                normal,
                color,
                surface_response,
                material_detail,
                generated_cache,
            ),
            WindowSceneVertex::world_with_surface_detail_and_cache(
                d,
                normal,
                color,
                surface_response,
                material_detail,
                generated_cache,
            ),
        ]);
    }

    pub fn world_box(&mut self, min: [f32; 3], max: [f32; 3], color: [f32; 4]) {
        self.world_box_with_surface_response(min, max, color, WINDOW_SURFACE_RESPONSE_DEFAULT);
    }

    pub fn world_box_with_surface_response(
        &mut self,
        min: [f32; 3],
        max: [f32; 3],
        color: [f32; 4],
        surface_response: [f32; 4],
    ) {
        let [x0, y0, z0] = min;
        let [x1, y1, z1] = max;
        let p000 = [x0, y0, z0];
        let p100 = [x1, y0, z0];
        let p110 = [x1, y1, z0];
        let p010 = [x0, y1, z0];
        let p001 = [x0, y0, z1];
        let p101 = [x1, y0, z1];
        let p111 = [x1, y1, z1];
        let p011 = [x0, y1, z1];

        self.world_quad_with_surface_response(p000, p010, p110, p100, color, surface_response);
        self.world_quad_with_surface_response(p001, p101, p111, p011, color, surface_response);
        self.world_quad_with_surface_response(p000, p001, p011, p010, color, surface_response);
        self.world_quad_with_surface_response(p100, p110, p111, p101, color, surface_response);
        self.world_quad_with_surface_response(p000, p100, p101, p001, color, surface_response);
        self.world_quad_with_surface_response(p010, p011, p111, p110, color, surface_response);
    }

    fn world_box_with_surface_state_detail(
        &mut self,
        min: [f32; 3],
        max: [f32; 3],
        color: [f32; 4],
        surface_response: [f32; 4],
        state_detail: WindowMaterialStateDetail,
    ) {
        if state_detail.is_neutral() {
            self.world_box_with_surface_response(min, max, color, surface_response);
            return;
        }

        let [x0, y0, z0] = min;
        let [x1, y1, z1] = max;
        let p000 = [x0, y0, z0];
        let p100 = [x1, y0, z0];
        let p110 = [x1, y1, z0];
        let p010 = [x0, y1, z0];
        let p001 = [x0, y0, z1];
        let p101 = [x1, y0, z1];
        let p111 = [x1, y1, z1];
        let p011 = [x0, y1, z1];

        self.world_quad_with_surface_state_detail(
            [p000, p010, p110, p100],
            color,
            surface_response,
            state_detail,
        );
        self.world_quad_with_surface_state_detail(
            [p001, p101, p111, p011],
            color,
            surface_response,
            state_detail,
        );
        self.world_quad_with_surface_state_detail(
            [p000, p001, p011, p010],
            color,
            surface_response,
            state_detail,
        );
        self.world_quad_with_surface_state_detail(
            [p100, p110, p111, p101],
            color,
            surface_response,
            state_detail,
        );
        self.world_quad_with_surface_state_detail(
            [p000, p100, p101, p001],
            color,
            surface_response,
            state_detail,
        );
        self.world_quad_with_surface_state_detail(
            [p010, p011, p111, p110],
            color,
            surface_response,
            state_detail,
        );
    }

    pub fn world_micro_detailed_box_with_surface_response(
        &mut self,
        min: [f32; 3],
        max: [f32; 3],
        color: [f32; 4],
        surface_response: [f32; 4],
        seed: u64,
        detail_density: f32,
    ) {
        self.world_box_with_surface_response(min, max, color, surface_response);
        self.add_world_box_micro_detail(min, max, color, surface_response, seed, detail_density);
    }

    fn world_micro_detailed_box_with_surface_state_detail(
        &mut self,
        min: [f32; 3],
        max: [f32; 3],
        color: [f32; 4],
        surface_response: [f32; 4],
        micro_detail: WindowMicroDetailRecipe,
        state_detail: WindowMaterialStateDetail,
    ) {
        self.world_box_with_surface_state_detail(min, max, color, surface_response, state_detail);
        let detail_density =
            (micro_detail.density * (0.86 + state_detail.scale_boost * 0.18)).clamp(0.0, 1.0);
        self.add_world_box_micro_detail(
            min,
            max,
            color,
            surface_response,
            micro_detail.seed,
            detail_density,
        );
        self.add_world_box_state_edge_wear(
            min,
            max,
            color,
            surface_response,
            micro_detail.seed,
            state_detail,
        );
        self.add_world_box_state_fracture_detail(
            min,
            max,
            color,
            surface_response,
            micro_detail.seed,
            state_detail,
        );
        self.add_world_box_state_energy_detail(
            min,
            max,
            color,
            surface_response,
            micro_detail.seed,
            state_detail,
        );
        self.add_world_box_state_deformation_layer_detail(
            min,
            max,
            color,
            surface_response,
            micro_detail.seed,
            state_detail,
        );
        self.add_world_box_state_contact_detail(
            min,
            max,
            color,
            surface_response,
            micro_detail.seed,
            state_detail,
        );
    }

    fn add_world_box_micro_detail(
        &mut self,
        min: [f32; 3],
        max: [f32; 3],
        color: [f32; 4],
        surface_response: [f32; 4],
        seed: u64,
        detail_density: f32,
    ) {
        if min.iter().chain(max.iter()).any(|value| !value.is_finite()) {
            return;
        }

        let [x0, y0, z0] = min;
        let [x1, y1, z1] = max;
        let span_x = (x1 - x0).abs();
        let span_y = (y1 - y0).abs();
        let span_z = (z1 - z0).abs();
        if span_x <= f32::EPSILON || span_y <= f32::EPSILON || span_z <= f32::EPSILON {
            return;
        }

        let detail = detail_density.clamp(0.0, 1.0);
        if detail <= 0.01 {
            return;
        }

        let center_x = (x0 + x1) * 0.5;
        let center_z = (z0 + z1) * 0.5;
        let grime = surface_response[0].clamp(0.0, 1.0);
        let metallic = surface_response[1].clamp(0.0, 1.0);
        let wetness = surface_response[2].clamp(0.0, 1.0);
        let emissive = surface_response[3].clamp(0.0, 1.0);
        let edge_radius =
            (span_x.min(span_y).min(span_z.max(0.18)) * 0.018 * (0.65 + detail)).clamp(0.006, 0.04);
        let edge_alpha = (color[3] * (0.34 + detail * 0.26)).clamp(0.0, 1.0);
        let edge_color = window_with_alpha(
            window_mix_color(
                window_scale_color(color, 0.58 + metallic * 0.26 + wetness * 0.1),
                [0.72, 0.82, 0.9, color[3]],
                metallic * 0.18,
            ),
            edge_alpha,
        );

        for (start, end) in [
            ([x0, y0, z1], [x1, y0, z1]),
            ([x0, y1, z1], [x1, y1, z1]),
            ([x0, y0, z1], [x0, y1, z1]),
            ([x1, y0, z1], [x1, y1, z1]),
        ] {
            self.world_cylinder_between_with_surface_response(
                start,
                end,
                edge_radius,
                5,
                edge_color,
                surface_response,
            );
        }

        if span_z > 0.38 {
            for (start, end) in [
                ([x0, y0, z0], [x0, y0, z1]),
                ([x1, y0, z0], [x1, y0, z1]),
                ([x0, y1, z0], [x0, y1, z1]),
                ([x1, y1, z0], [x1, y1, z1]),
            ] {
                self.world_cylinder_between_with_surface_response(
                    start,
                    end,
                    edge_radius * 0.72,
                    5,
                    edge_color,
                    surface_response,
                );
            }
        }

        let panel_color = window_with_alpha(
            window_scale_color(color, (0.42 + grime * 0.16).clamp(0.24, 0.7)),
            (0.1 + detail * 0.2 + grime * 0.12).clamp(0.0, 0.48),
        );
        let panel_surface = if metallic > 0.28 {
            WINDOW_SURFACE_RESPONSE_METAL
        } else {
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
        };

        if span_z > 0.62 && span_x > 0.58 {
            let row_count = (1.0 + detail * 3.0 + span_z * 0.12).ceil().clamp(1.0, 5.0) as usize;
            for row in 0..row_count {
                let row_z = z0 + span_z * (row as f32 + 1.0) / (row_count as f32 + 1.0);
                for face_y in [y0 - edge_radius * 0.62, y1 + edge_radius * 0.62] {
                    self.world_oriented_rect_with_surface_response(
                        [center_x, face_y, row_z],
                        [1.0, 0.0, 0.0],
                        [0.0, 0.0, 1.0],
                        [span_x * 0.43, edge_radius * 0.34],
                        panel_color,
                        panel_surface,
                    );
                }
            }
        }

        if span_z > 0.62 && span_y > 0.58 {
            let column_count = (1.0 + detail * 2.0 + span_y * 0.08).ceil().clamp(1.0, 4.0) as usize;
            for column in 0..column_count {
                let column_y = y0 + span_y * (column as f32 + 1.0) / (column_count as f32 + 1.0);
                for face_x in [x0 - edge_radius * 0.62, x1 + edge_radius * 0.62] {
                    self.world_oriented_rect_with_surface_response(
                        [face_x, column_y, center_z],
                        [0.0, 1.0, 0.0],
                        [0.0, 0.0, 1.0],
                        [edge_radius * 0.34, span_z * 0.34],
                        panel_color,
                        panel_surface,
                    );
                }
            }
        }

        let scratch_count = (1.0 + detail * 5.0 + grime * 2.0 + wetness)
            .ceil()
            .clamp(1.0, 8.0) as usize;
        for index in 0..scratch_count {
            let rx = x0 + span_x * window_stable_unit(seed, 9_101 + index as u64);
            let ry = y0 + span_y * window_stable_unit(seed, 9_201 + index as u64);
            let top_bias = window_stable_unit(seed, 9_301 + index as u64);
            let angle =
                (window_stable_unit(seed, 9_401 + index as u64) - 0.5) * std::f32::consts::PI;
            let length = (span_x.min(span_y)
                * (0.04 + window_stable_unit(seed, 9_501 + index as u64) * 0.12))
                .clamp(0.05, 0.72);
            let alpha = (0.12 + detail * 0.18 + grime * 0.1).clamp(0.0, 0.46);
            let scratch_color = if index % 3 == 0 && metallic > 0.2 {
                [0.68, 0.74, 0.78, alpha]
            } else {
                [0.035, 0.032, 0.028, alpha]
            };
            self.world_oriented_rect_with_surface_response(
                [rx, ry, z1 + edge_radius * (0.92 + top_bias * 0.25)],
                [angle.cos(), angle.sin(), 0.0],
                [-angle.sin(), angle.cos(), 0.0],
                [length, edge_radius * (0.18 + detail * 0.12)],
                scratch_color,
                panel_surface,
            );
        }

        if emissive < 0.05 && span_z > 0.45 && detail > 0.35 {
            let chip_count = (detail * 4.0 + grime).ceil().clamp(1.0, 5.0) as usize;
            for chip in 0..chip_count {
                let t = window_stable_unit(seed, 9_701 + chip as u64);
                let chip_x = x0 + span_x * t;
                let chip_z =
                    z0 + span_z * (0.22 + window_stable_unit(seed, 9_801 + chip as u64) * 0.62);
                let chip_y = if chip % 2 == 0 {
                    y0 - edge_radius * 0.7
                } else {
                    y1 + edge_radius * 0.7
                };
                self.world_oriented_rect_with_surface_response(
                    [chip_x, chip_y, chip_z],
                    [1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0],
                    [
                        (span_x * 0.035 + detail * 0.045).clamp(0.04, 0.24),
                        (span_z * 0.018 + grime * 0.035).clamp(0.025, 0.16),
                    ],
                    [
                        0.18 + grime * 0.16,
                        0.12 + wetness * 0.08,
                        0.075,
                        (0.18 + detail * 0.22).clamp(0.0, 0.5),
                    ],
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                );
            }
        }
    }

    fn add_world_box_state_edge_wear(
        &mut self,
        min: [f32; 3],
        max: [f32; 3],
        color: [f32; 4],
        surface_response: [f32; 4],
        seed: u64,
        state_detail: WindowMaterialStateDetail,
    ) {
        if state_detail.is_neutral() || min.iter().chain(max.iter()).any(|value| !value.is_finite())
        {
            return;
        }

        let x0 = min[0].min(max[0]);
        let x1 = min[0].max(max[0]);
        let y0 = min[1].min(max[1]);
        let y1 = min[1].max(max[1]);
        let z0 = min[2].min(max[2]);
        let z1 = min[2].max(max[2]);
        let span_x = x1 - x0;
        let span_y = y1 - y0;
        let span_z = z1 - z0;
        if span_x <= f32::EPSILON || span_y <= f32::EPSILON || span_z <= f32::EPSILON {
            return;
        }

        let wear = (state_detail.state_intensity * 0.68
            + state_detail.relief * 0.56
            + (state_detail.scale_boost - 1.0).max(0.0) * 0.16)
            .clamp(0.0, 1.0);
        if wear <= 0.035 {
            return;
        }

        let grime = surface_response[0].clamp(0.0, 1.0);
        let metallic = surface_response[1].clamp(0.0, 1.0);
        let wetness = surface_response[2].clamp(0.0, 1.0);
        let emissive = surface_response[3].clamp(0.0, 1.0);
        let min_span = span_x.min(span_y).min(span_z.max(0.08));
        let protrusion = (min_span * (0.018 + wear * 0.035)).clamp(0.004, 0.045);
        let chip_alpha = (color[3] * (0.24 + wear * 0.34 + grime * 0.1)).clamp(0.0, 0.74);
        let exposed_color = window_mix_color(
            window_scale_color(color, 0.58 + wetness * 0.08),
            [0.74, 0.78, 0.74, color[3]],
            metallic * 0.34,
        );
        let dirty_color = window_mix_color(
            [0.3, 0.12, 0.045, color[3]],
            [0.028, 0.024, 0.02, color[3]],
            grime * 0.46,
        );
        let wear_color = window_with_alpha(
            window_mix_color(
                exposed_color,
                dirty_color,
                wear * 0.62 + (1.0 - metallic) * 0.16,
            ),
            chip_alpha,
        );
        let wear_surface = if wetness > 0.5 {
            WINDOW_SURFACE_RESPONSE_WET_ROAD
        } else if metallic > 0.22 {
            WINDOW_SURFACE_RESPONSE_METAL
        } else {
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
        };
        let wear_state = WindowMaterialStateDetail {
            scale_boost: (state_detail.scale_boost + 0.72 + wear * 0.86).clamp(1.2, 4.4),
            state_intensity: state_detail.state_intensity.max(wear),
            relief: (state_detail.relief + wear * 0.3).clamp(0.0, 1.0),
            crack_density: state_detail.crack_density,
            moisture: state_detail.moisture,
            soot: state_detail.soot,
            corrosion: state_detail.corrosion,
            heat: state_detail.heat,
            electrical_charge: state_detail.electrical_charge,
            plastic_strain: state_detail.plastic_strain,
            biological_contamination: state_detail.biological_contamination,
            oil_contamination: state_detail.oil_contamination,
            seed_salt: state_detail.seed_salt ^ seed.rotate_left(11) ^ 0xC0DE_E0E0_DA7A_51A7,
        };

        let edge_count = (2.0 + wear * 7.0 + state_detail.relief * 3.0)
            .ceil()
            .clamp(2.0, 10.0) as usize;
        let edge_seed = seed ^ state_detail.seed_salt.rotate_left(7);
        for index in 0..edge_count {
            let pick = window_stable_unit(edge_seed, 10_301 + index as u64);
            let t = window_stable_unit(edge_seed, 10_401 + index as u64);
            let wobble = window_stable_unit(edge_seed, 10_501 + index as u64);
            let length_factor = (0.05
                + window_stable_unit(edge_seed, 10_601 + index as u64) * 0.14)
                * (0.72 + wear);
            let max_half_x = (span_x * 0.42).max(0.006);
            let max_half_y = (span_y * 0.42).max(0.006);
            let half_len_x = (span_x * length_factor).max(0.006).min(max_half_x);
            let half_len_y = (span_y * length_factor).max(0.006).min(max_half_y);
            let half_height = (span_z * (0.012 + state_detail.relief * 0.035 + wobble * 0.015))
                .clamp(0.014, 0.14);
            let local_color = window_with_alpha(
                window_mix_color(
                    wear_color,
                    [0.9, 0.9, 0.82, chip_alpha],
                    wobble * metallic * 0.2,
                ),
                (chip_alpha * (0.72 + wobble * 0.32)).clamp(0.0, 0.82),
            );

            if pick < 0.34 {
                let chip_x = if span_x > half_len_x * 2.0 {
                    (x0 + span_x * t).clamp(x0 + half_len_x, x1 - half_len_x)
                } else {
                    (x0 + x1) * 0.5
                };
                let side_y = if index.is_multiple_of(2) { y0 } else { y1 };
                let outward = if index.is_multiple_of(2) { -1.0 } else { 1.0 };
                let z = z1 - half_height * (0.85 + wobble * 1.2);
                self.world_box_with_surface_state_detail(
                    [
                        chip_x - half_len_x,
                        side_y - protrusion * 0.28 + protrusion * outward,
                        z - half_height * 0.28,
                    ],
                    [
                        chip_x + half_len_x,
                        side_y + protrusion * 0.28 + protrusion * outward,
                        z + half_height * 0.34,
                    ],
                    local_color,
                    wear_surface,
                    wear_state,
                );
                self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                    center: [chip_x, side_y + outward * protrusion * 0.92, z],
                    right: [1.0, 0.0, 0.0],
                    up: [0.0, 0.0, 1.0],
                    half_extents: [half_len_x, half_height],
                    color: local_color,
                    surface_response: wear_surface,
                    state_detail: wear_state,
                });
            } else if pick < 0.68 {
                let chip_y = if span_y > half_len_y * 2.0 {
                    (y0 + span_y * t).clamp(y0 + half_len_y, y1 - half_len_y)
                } else {
                    (y0 + y1) * 0.5
                };
                let side_x = if index.is_multiple_of(2) { x0 } else { x1 };
                let outward = if index.is_multiple_of(2) { -1.0 } else { 1.0 };
                let z = z1 - half_height * (0.85 + wobble * 1.2);
                self.world_box_with_surface_state_detail(
                    [
                        side_x - protrusion * 0.28 + protrusion * outward,
                        chip_y - half_len_y,
                        z - half_height * 0.28,
                    ],
                    [
                        side_x + protrusion * 0.28 + protrusion * outward,
                        chip_y + half_len_y,
                        z + half_height * 0.34,
                    ],
                    local_color,
                    wear_surface,
                    wear_state,
                );
                self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                    center: [side_x + outward * protrusion * 0.92, chip_y, z],
                    right: [0.0, 1.0, 0.0],
                    up: [0.0, 0.0, 1.0],
                    half_extents: [half_len_y, half_height],
                    color: local_color,
                    surface_response: wear_surface,
                    state_detail: wear_state,
                });
            } else if emissive < 0.35 {
                let corner_x = if pick < 0.84 { x0 } else { x1 };
                let corner_y = if index.is_multiple_of(2) { y0 } else { y1 };
                let outward_x = if corner_x <= x0 { -1.0 } else { 1.0 };
                let outward_y = if corner_y <= y0 { -1.0 } else { 1.0 };
                let chip_z = z0 + span_z * (0.12 + t * 0.76);
                let corner_half = (protrusion * (0.78 + wobble * 0.75)).clamp(0.004, 0.052);
                self.world_box_with_surface_state_detail(
                    [
                        corner_x - corner_half * 0.35 + outward_x * corner_half,
                        corner_y - corner_half * 0.35 + outward_y * corner_half,
                        chip_z - half_height,
                    ],
                    [
                        corner_x + corner_half * 0.35 + outward_x * corner_half,
                        corner_y + corner_half * 0.35 + outward_y * corner_half,
                        chip_z + half_height,
                    ],
                    local_color,
                    wear_surface,
                    wear_state,
                );
            }
        }
    }

    fn add_world_box_state_fracture_detail(
        &mut self,
        min: [f32; 3],
        max: [f32; 3],
        color: [f32; 4],
        surface_response: [f32; 4],
        seed: u64,
        state_detail: WindowMaterialStateDetail,
    ) {
        if state_detail.crack_density <= 0.035
            || min.iter().chain(max.iter()).any(|value| !value.is_finite())
        {
            return;
        }

        let x0 = min[0].min(max[0]);
        let x1 = min[0].max(max[0]);
        let y0 = min[1].min(max[1]);
        let y1 = min[1].max(max[1]);
        let z0 = min[2].min(max[2]);
        let z1 = min[2].max(max[2]);
        let span_x = x1 - x0;
        let span_y = y1 - y0;
        let span_z = z1 - z0;
        if span_x <= f32::EPSILON || span_y <= f32::EPSILON || span_z <= 0.14 {
            return;
        }

        let crack = (state_detail.crack_density * 0.78
            + state_detail.relief * 0.22
            + (state_detail.scale_boost - 1.0).max(0.0) * 0.05)
            .clamp(0.0, 1.0);
        if crack <= 0.04 {
            return;
        }

        let metallic = surface_response[1].clamp(0.0, 1.0);
        let wetness = surface_response[2].clamp(0.0, 1.0);
        let emissive = surface_response[3].clamp(0.0, 1.0);
        let glass_like = surface_response[0] < 0.16 && wetness > 0.2 && emissive < 0.12;
        let face_offset = (span_x.min(span_y).min(span_z) * 0.012).clamp(0.003, 0.018);
        let interior_alpha = (0.28 + crack * 0.42 + state_detail.soot * 0.12).clamp(0.0, 0.78);
        let exposed_surface = if glass_like {
            WINDOW_SURFACE_RESPONSE_GLASS
        } else if metallic > 0.28 && state_detail.corrosion < 0.42 {
            WINDOW_SURFACE_RESPONSE_METAL
        } else {
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
        };
        let interior_color = window_with_alpha(
            window_mix_color(
                window_mix_color(
                    [0.024, 0.022, 0.02, color[3]],
                    [0.28, 0.11, 0.035, color[3]],
                    state_detail.corrosion * 0.7,
                ),
                [0.72, 0.9, 1.0, color[3]],
                if glass_like { 0.42 } else { 0.0 },
            ),
            interior_alpha,
        );
        let exposed_edge_color = window_with_alpha(
            window_mix_color(
                window_scale_color(color, 0.72 + metallic * 0.2),
                [0.86, 0.82, 0.68, color[3]],
                crack * (0.25 + metallic * 0.12),
            ),
            (0.22 + crack * 0.3).clamp(0.0, 0.64),
        );
        let fracture_state = WindowMaterialStateDetail {
            scale_boost: (state_detail.scale_boost + 0.9 + crack * 1.15).clamp(1.3, 4.4),
            state_intensity: state_detail.state_intensity.max(crack),
            relief: state_detail
                .relief
                .max((0.55 + crack * 0.38).clamp(0.0, 1.0)),
            crack_density: state_detail.crack_density.max(crack),
            moisture: state_detail.moisture,
            soot: state_detail.soot,
            corrosion: state_detail.corrosion,
            heat: state_detail.heat,
            electrical_charge: state_detail.electrical_charge,
            plastic_strain: state_detail.plastic_strain,
            biological_contamination: state_detail.biological_contamination,
            oil_contamination: state_detail.oil_contamination,
            seed_salt: state_detail.seed_salt ^ seed.rotate_left(31) ^ 0xF2AC_7A11_C0DE_E12E,
        };

        let crack_count = (2.0 + crack * 8.0 + span_z * 0.28).ceil().clamp(2.0, 11.0) as usize;
        let crack_seed = seed ^ state_detail.seed_salt.rotate_left(13);
        for index in 0..crack_count {
            let face_pick = window_stable_unit(crack_seed, 13_001 + index as u64);
            let along = window_stable_unit(crack_seed, 13_101 + index as u64);
            let height_t = window_stable_unit(crack_seed, 13_201 + index as u64);
            let slope = (window_stable_unit(crack_seed, 13_301 + index as u64) - 0.5) * 0.86;
            let wobble = window_stable_unit(crack_seed, 13_401 + index as u64);
            let main_length =
                (span_z * (0.08 + crack * 0.16 + wobble * 0.12)).clamp(0.08, span_z * 0.46);
            let width =
                (span_x.min(span_y).max(0.06) * (0.008 + crack * 0.018)).clamp(0.004, 0.035);
            let center_z = (z0 + span_z * (0.16 + height_t * 0.7))
                .clamp(z0 + main_length * 0.45, z1 - main_length * 0.35);

            let (center, along_axis, up_axis) = if face_pick < 0.5 {
                let y = if face_pick < 0.25 {
                    y0 - face_offset
                } else {
                    y1 + face_offset
                };
                (
                    [
                        (x0 + span_x * along).clamp(x0 + width, x1 - width),
                        y,
                        center_z,
                    ],
                    normalize3([1.0, 0.0, slope]),
                    [0.0, 0.0, 1.0],
                )
            } else {
                let x = if face_pick < 0.75 {
                    x0 - face_offset
                } else {
                    x1 + face_offset
                };
                (
                    [
                        x,
                        (y0 + span_y * along).clamp(y0 + width, y1 - width),
                        center_z,
                    ],
                    normalize3([0.0, 1.0, slope]),
                    [0.0, 0.0, 1.0],
                )
            };

            self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                center,
                right: along_axis,
                up: up_axis,
                half_extents: [main_length, width * (2.4 + crack * 2.2)],
                color: interior_color,
                surface_response: exposed_surface,
                state_detail: fracture_state,
            });

            for edge_offset in [-1.0_f32, 1.0] {
                self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                    center: add3(center, scale3(up_axis, edge_offset * width * 2.1)),
                    right: along_axis,
                    up: up_axis,
                    half_extents: [main_length * (0.78 + wobble * 0.16), width * 0.46],
                    color: exposed_edge_color,
                    surface_response: exposed_surface,
                    state_detail: fracture_state,
                });
            }

            if crack > 0.38 && index.is_multiple_of(2) {
                let branch_offset = (window_stable_unit(crack_seed, 13_501 + index as u64) - 0.5)
                    * main_length
                    * 0.7;
                let branch_center = add3(center, scale3(along_axis, branch_offset));
                let branch_slope = slope + if index.is_multiple_of(4) { 0.62 } else { -0.62 };
                let branch_axis = if face_pick < 0.5 {
                    normalize3([1.0, 0.0, branch_slope])
                } else {
                    normalize3([0.0, 1.0, branch_slope])
                };
                self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                    center: branch_center,
                    right: branch_axis,
                    up: up_axis,
                    half_extents: [
                        (main_length * (0.26 + wobble * 0.18)).clamp(0.035, 0.36),
                        width * 1.35,
                    ],
                    color: interior_color,
                    surface_response: exposed_surface,
                    state_detail: fracture_state,
                });
            }

            if crack > 0.54 && index % 3 == 0 {
                let missing = (width * (1.6 + wobble * 1.8)).clamp(0.008, 0.07);
                self.world_box_with_surface_state_detail(
                    [
                        center[0] - missing,
                        center[1] - missing,
                        center[2] - missing * 0.65,
                    ],
                    [
                        center[0] + missing,
                        center[1] + missing,
                        center[2] + missing * 0.65,
                    ],
                    interior_color,
                    exposed_surface,
                    fracture_state,
                );
            }
        }
    }

    fn add_world_box_state_energy_detail(
        &mut self,
        min: [f32; 3],
        max: [f32; 3],
        color: [f32; 4],
        surface_response: [f32; 4],
        seed: u64,
        state_detail: WindowMaterialStateDetail,
    ) {
        if (state_detail.heat <= 0.04 && state_detail.electrical_charge <= 0.04)
            || min.iter().chain(max.iter()).any(|value| !value.is_finite())
        {
            return;
        }

        let x0 = min[0].min(max[0]);
        let x1 = min[0].max(max[0]);
        let y0 = min[1].min(max[1]);
        let y1 = min[1].max(max[1]);
        let z0 = min[2].min(max[2]);
        let z1 = min[2].max(max[2]);
        let span_x = x1 - x0;
        let span_y = y1 - y0;
        let span_z = z1 - z0;
        if span_x <= f32::EPSILON || span_y <= f32::EPSILON || span_z <= 0.12 {
            return;
        }

        let heat = state_detail.heat.clamp(0.0, 1.0);
        let charge = state_detail.electrical_charge.clamp(0.0, 1.0);
        let metallic = surface_response[1].clamp(0.0, 1.0);
        let energy = (heat * 0.72 + charge * 0.58 + metallic * charge * 0.18).clamp(0.0, 1.0);
        let face_offset = (span_x.min(span_y).min(span_z) * 0.013).clamp(0.003, 0.02);
        let energy_state = WindowMaterialStateDetail {
            scale_boost: (state_detail.scale_boost + 0.62 + energy * 0.94).clamp(1.2, 4.4),
            state_intensity: state_detail.state_intensity.max(energy),
            relief: (state_detail.relief + charge * 0.2 + heat * 0.1).clamp(0.0, 1.0),
            crack_density: state_detail.crack_density,
            moisture: state_detail.moisture,
            soot: state_detail.soot.max(heat * 0.18),
            corrosion: state_detail.corrosion,
            heat,
            electrical_charge: charge,
            plastic_strain: state_detail.plastic_strain,
            biological_contamination: state_detail.biological_contamination,
            oil_contamination: state_detail.oil_contamination,
            seed_salt: state_detail.seed_salt ^ seed.rotate_left(37) ^ 0xE11E_C712_0B51_D1A5,
        };

        if heat > 0.05 {
            let scorch_count = (1.0 + heat * 5.0 + state_detail.soot * 2.0)
                .ceil()
                .clamp(1.0, 8.0) as usize;
            for index in 0..scorch_count {
                let face_pick = window_stable_unit(seed, 14_001 + index as u64);
                let along = window_stable_unit(seed, 14_101 + index as u64);
                let height_t = window_stable_unit(seed, 14_201 + index as u64);
                let wobble = window_stable_unit(seed, 14_301 + index as u64);
                let patch_height =
                    (span_z * (0.06 + heat * 0.13 + wobble * 0.08)).clamp(0.06, span_z * 0.38);
                let patch_width_x = (span_x * (0.08 + wobble * 0.12))
                    .max(0.006)
                    .min((span_x * 0.42).max(0.006));
                let patch_width_y = (span_y * (0.08 + wobble * 0.12))
                    .max(0.006)
                    .min((span_y * 0.42).max(0.006));
                let center_z = (z0 + span_z * (0.2 + height_t * 0.58))
                    .clamp(z0 + patch_height * 0.5, z1 - patch_height * 0.35);
                let scorch_color = window_with_alpha(
                    window_mix_color(
                        [0.035, 0.022, 0.014, color[3]],
                        [0.46, 0.16, 0.04, color[3]],
                        heat * 0.64,
                    ),
                    (0.18 + heat * 0.34 + state_detail.soot * 0.12).clamp(0.0, 0.7),
                );

                let (center, right, up, half_extents) = if face_pick < 0.5 {
                    let y = if face_pick < 0.25 {
                        y0 - face_offset
                    } else {
                        y1 + face_offset
                    };
                    let x = if span_x > patch_width_x * 2.0 {
                        (x0 + span_x * along).clamp(x0 + patch_width_x, x1 - patch_width_x)
                    } else {
                        (x0 + x1) * 0.5
                    };
                    (
                        [x, y, center_z],
                        [1.0, 0.0, 0.0],
                        [0.0, 0.0, 1.0],
                        [patch_width_x, patch_height],
                    )
                } else {
                    let x = if face_pick < 0.75 {
                        x0 - face_offset
                    } else {
                        x1 + face_offset
                    };
                    let y = if span_y > patch_width_y * 2.0 {
                        (y0 + span_y * along).clamp(y0 + patch_width_y, y1 - patch_width_y)
                    } else {
                        (y0 + y1) * 0.5
                    };
                    (
                        [x, y, center_z],
                        [0.0, 1.0, 0.0],
                        [0.0, 0.0, 1.0],
                        [patch_width_y, patch_height],
                    )
                };

                self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                    center,
                    right,
                    up,
                    half_extents,
                    color: scorch_color,
                    surface_response: WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                    state_detail: energy_state,
                });

                if heat > 0.22 {
                    let core_color = [
                        1.0,
                        0.28 + heat * 0.32,
                        0.08 + heat * 0.08,
                        (0.12 + heat * 0.34).clamp(0.0, 0.62),
                    ];
                    self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                        center: add3(center, scale3(up, patch_height * (0.05 + wobble * 0.14))),
                        right,
                        up,
                        half_extents: [
                            half_extents[0] * (0.36 + wobble * 0.18),
                            patch_height * 0.2,
                        ],
                        color: core_color,
                        surface_response: WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
                        state_detail: energy_state,
                    });
                }
            }
        }

        if charge > 0.06 {
            let arc_count = (1.0 + charge * 6.0 + metallic * 2.0).ceil().clamp(1.0, 9.0) as usize;
            for arc in 0..arc_count {
                let face_pick = window_stable_unit(seed, 14_701 + arc as u64);
                let along = window_stable_unit(seed, 14_801 + arc as u64);
                let height_t = window_stable_unit(seed, 14_901 + arc as u64);
                let angle_jitter = window_stable_unit(seed, 15_001 + arc as u64) - 0.5;
                let length = (span_x.max(span_y).min(span_z)
                    * (0.05 + charge * 0.08 + angle_jitter.abs() * 0.08))
                    .clamp(0.045, 0.42);
                let width = (0.003 + charge * 0.008).clamp(0.003, 0.014);
                let center_z = z0 + span_z * (0.16 + height_t * 0.74);
                let arc_color = [
                    0.38 + charge * 0.18,
                    0.82 + charge * 0.16,
                    1.0,
                    (0.28 + charge * 0.5).clamp(0.0, 0.9),
                ];

                let (center, right, up) = if face_pick < 0.5 {
                    let y = if face_pick < 0.25 {
                        y0 - face_offset * 1.45
                    } else {
                        y1 + face_offset * 1.45
                    };
                    let x = if span_x > length {
                        (x0 + span_x * along).clamp(x0 + length * 0.5, x1 - length * 0.5)
                    } else {
                        (x0 + x1) * 0.5
                    };
                    (
                        [x, y, center_z],
                        normalize3([1.0, 0.0, angle_jitter * 1.2]),
                        [0.0, 0.0, 1.0],
                    )
                } else {
                    let x = if face_pick < 0.75 {
                        x0 - face_offset * 1.45
                    } else {
                        x1 + face_offset * 1.45
                    };
                    let y = if span_y > length {
                        (y0 + span_y * along).clamp(y0 + length * 0.5, y1 - length * 0.5)
                    } else {
                        (y0 + y1) * 0.5
                    };
                    (
                        [x, y, center_z],
                        normalize3([0.0, 1.0, angle_jitter * 1.2]),
                        [0.0, 0.0, 1.0],
                    )
                };

                self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                    center,
                    right,
                    up,
                    half_extents: [length, width],
                    color: arc_color,
                    surface_response: WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
                    state_detail: energy_state,
                });

                if arc.is_multiple_of(2) {
                    self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                        center: add3(center, scale3(right, length * 0.38)),
                        right: up,
                        up: right,
                        half_extents: [width * 1.8, length * 0.22],
                        color: [1.0, 0.86, 0.32, (0.18 + charge * 0.44).clamp(0.0, 0.72)],
                        surface_response: WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
                        state_detail: energy_state,
                    });
                }
            }
        }
    }

    fn add_world_box_state_deformation_layer_detail(
        &mut self,
        min: [f32; 3],
        max: [f32; 3],
        color: [f32; 4],
        surface_response: [f32; 4],
        seed: u64,
        state_detail: WindowMaterialStateDetail,
    ) {
        if (state_detail.plastic_strain <= 0.035
            && state_detail.biological_contamination <= 0.035
            && state_detail.oil_contamination <= 0.035)
            || min.iter().chain(max.iter()).any(|value| !value.is_finite())
        {
            return;
        }

        let x0 = min[0].min(max[0]);
        let x1 = min[0].max(max[0]);
        let y0 = min[1].min(max[1]);
        let y1 = min[1].max(max[1]);
        let z0 = min[2].min(max[2]);
        let z1 = min[2].max(max[2]);
        let span_x = x1 - x0;
        let span_y = y1 - y0;
        let span_z = z1 - z0;
        if span_x <= f32::EPSILON || span_y <= f32::EPSILON || span_z <= 0.12 {
            return;
        }

        let strain = state_detail.plastic_strain.clamp(0.0, 1.0);
        let biological = state_detail.biological_contamination.clamp(0.0, 1.0);
        let oil = state_detail.oil_contamination.clamp(0.0, 1.0);
        let contamination =
            (biological * 0.72 + oil * 0.62 + state_detail.moisture * 0.12).clamp(0.0, 1.0);
        if strain <= 0.035 && contamination <= 0.035 {
            return;
        }

        let grime = surface_response[0].clamp(0.0, 1.0);
        let metallic = surface_response[1].clamp(0.0, 1.0);
        let wetness = surface_response[2].clamp(0.0, 1.0);
        let min_span = span_x.min(span_y).min(span_z.max(0.12));
        let face_offset = (min_span * 0.012).clamp(0.003, 0.02);

        if strain > 0.04 {
            let deformation =
                (strain * 0.82 + state_detail.relief * 0.24 + state_detail.crack_density * 0.08)
                    .clamp(0.0, 1.0);
            let dent_count = (1.0 + deformation * 7.0 + span_z * 0.2)
                .ceil()
                .clamp(1.0, 10.0) as usize;
            let dent_seed = seed ^ state_detail.seed_salt.rotate_left(19);
            let dent_state = WindowMaterialStateDetail {
                scale_boost: (state_detail.scale_boost + 0.78 + deformation * 0.92).clamp(1.2, 4.4),
                state_intensity: state_detail.state_intensity.max(deformation),
                relief: state_detail
                    .relief
                    .max((0.42 + deformation * 0.44).clamp(0.0, 1.0)),
                crack_density: state_detail.crack_density,
                moisture: state_detail.moisture,
                soot: state_detail.soot,
                corrosion: state_detail.corrosion,
                heat: state_detail.heat,
                electrical_charge: state_detail.electrical_charge,
                plastic_strain: strain,
                biological_contamination: biological,
                oil_contamination: oil,
                seed_salt: state_detail.seed_salt ^ seed.rotate_left(43) ^ 0xD3F0_12ED_5CA1_A710,
            };
            let compressed_surface = if metallic > 0.24 {
                WINDOW_SURFACE_RESPONSE_METAL
            } else {
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
            };

            for dent in 0..dent_count {
                let face_pick = window_stable_unit(dent_seed, 15_401 + dent as u64);
                let along = window_stable_unit(dent_seed, 15_501 + dent as u64);
                let height_t = window_stable_unit(dent_seed, 15_601 + dent as u64);
                let wobble = window_stable_unit(dent_seed, 15_701 + dent as u64);
                let skew =
                    (window_stable_unit(dent_seed, 15_801 + dent as u64) - 0.5) * deformation * 0.9;
                let patch_height = (span_z * (0.055 + deformation * 0.12 + wobble * 0.08))
                    .clamp(0.06, span_z * 0.38);
                let patch_width_x = (span_x * (0.055 + wobble * 0.12 + deformation * 0.08))
                    .max(0.018)
                    .min((span_x * 0.45).max(0.018));
                let patch_width_y = (span_y * (0.055 + wobble * 0.12 + deformation * 0.08))
                    .max(0.018)
                    .min((span_y * 0.45).max(0.018));
                let center_z = (z0 + span_z * (0.18 + height_t * 0.66))
                    .clamp(z0 + patch_height * 0.48, z1 - patch_height * 0.32);
                let lip_depth = (min_span * (0.016 + deformation * 0.04)).clamp(0.004, 0.055);
                let lip_thickness = (min_span * (0.01 + deformation * 0.023)).clamp(0.004, 0.035);
                let dent_alpha = (0.18 + deformation * 0.38 + grime * 0.08).clamp(0.0, 0.68);
                let dent_color = window_with_alpha(
                    window_mix_color(
                        window_scale_color(color, 0.58 + metallic * 0.16 + wetness * 0.08),
                        [0.78, 0.74, 0.64, color[3]],
                        metallic * 0.26 + wobble * 0.1,
                    ),
                    dent_alpha,
                );
                let shadow_color = window_with_alpha(
                    window_mix_color(
                        window_scale_color(color, 0.34 + wetness * 0.06),
                        [0.08, 0.07, 0.06, color[3]],
                        0.46 + deformation * 0.28,
                    ),
                    (0.12 + deformation * 0.28).clamp(0.0, 0.54),
                );

                if face_pick < 0.5 {
                    let outward = if face_pick < 0.25 { -1.0 } else { 1.0 };
                    let face_y = if outward < 0.0 { y0 } else { y1 };
                    let x = if span_x > patch_width_x * 2.0 {
                        (x0 + span_x * along).clamp(x0 + patch_width_x, x1 - patch_width_x)
                    } else {
                        (x0 + x1) * 0.5
                    };
                    let center = [x, face_y + outward * face_offset, center_z];
                    self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                        center,
                        right: normalize3([1.0, outward * skew * 0.22, 0.0]),
                        up: normalize3([skew * 0.18, outward * skew * 0.16, 1.0]),
                        half_extents: [patch_width_x, patch_height],
                        color: shadow_color,
                        surface_response: compressed_surface,
                        state_detail: dent_state,
                    });

                    let y_a = face_y + outward * lip_depth;
                    let y_b = face_y + outward * (lip_depth + lip_thickness);
                    self.world_box_with_surface_state_detail(
                        [
                            x - patch_width_x * (0.52 + wobble * 0.12),
                            y_a.min(y_b),
                            (center_z + patch_height * (0.48 + wobble * 0.08))
                                .min(z1 - lip_thickness),
                        ],
                        [
                            x + patch_width_x * (0.52 + wobble * 0.12),
                            y_a.max(y_b),
                            (center_z + patch_height * (0.62 + wobble * 0.12))
                                .clamp(z0 + lip_thickness, z1 + lip_thickness),
                        ],
                        dent_color,
                        compressed_surface,
                        dent_state,
                    );

                    if deformation > 0.3 && dent.is_multiple_of(2) {
                        let side_x = if wobble < 0.5 {
                            x - patch_width_x * 0.72
                        } else {
                            x + patch_width_x * 0.72
                        };
                        self.world_box_with_surface_state_detail(
                            [
                                side_x - lip_thickness,
                                y_a.min(y_b),
                                center_z - patch_height * 0.34,
                            ],
                            [
                                side_x + lip_thickness,
                                y_a.max(y_b),
                                center_z + patch_height * 0.42,
                            ],
                            dent_color,
                            compressed_surface,
                            dent_state,
                        );
                    }
                } else {
                    let outward = if face_pick < 0.75 { -1.0 } else { 1.0 };
                    let face_x = if outward < 0.0 { x0 } else { x1 };
                    let y = if span_y > patch_width_y * 2.0 {
                        (y0 + span_y * along).clamp(y0 + patch_width_y, y1 - patch_width_y)
                    } else {
                        (y0 + y1) * 0.5
                    };
                    let center = [face_x + outward * face_offset, y, center_z];
                    self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                        center,
                        right: normalize3([outward * skew * 0.22, 1.0, 0.0]),
                        up: normalize3([outward * skew * 0.16, skew * 0.18, 1.0]),
                        half_extents: [patch_width_y, patch_height],
                        color: shadow_color,
                        surface_response: compressed_surface,
                        state_detail: dent_state,
                    });

                    let x_a = face_x + outward * lip_depth;
                    let x_b = face_x + outward * (lip_depth + lip_thickness);
                    self.world_box_with_surface_state_detail(
                        [
                            x_a.min(x_b),
                            y - patch_width_y * (0.52 + wobble * 0.12),
                            (center_z + patch_height * (0.48 + wobble * 0.08))
                                .min(z1 - lip_thickness),
                        ],
                        [
                            x_a.max(x_b),
                            y + patch_width_y * (0.52 + wobble * 0.12),
                            (center_z + patch_height * (0.62 + wobble * 0.12))
                                .clamp(z0 + lip_thickness, z1 + lip_thickness),
                        ],
                        dent_color,
                        compressed_surface,
                        dent_state,
                    );

                    if deformation > 0.3 && dent.is_multiple_of(2) {
                        let side_y = if wobble < 0.5 {
                            y - patch_width_y * 0.72
                        } else {
                            y + patch_width_y * 0.72
                        };
                        self.world_box_with_surface_state_detail(
                            [
                                x_a.min(x_b),
                                side_y - lip_thickness,
                                center_z - patch_height * 0.34,
                            ],
                            [
                                x_a.max(x_b),
                                side_y + lip_thickness,
                                center_z + patch_height * 0.42,
                            ],
                            dent_color,
                            compressed_surface,
                            dent_state,
                        );
                    }
                }
            }
        }

        if contamination > 0.04 {
            let layer_seed = seed ^ state_detail.seed_salt.rotate_left(29);
            let layer_count = (1.0 + contamination * 7.0 + oil * 2.0)
                .ceil()
                .clamp(1.0, 10.0) as usize;
            let layer_state = WindowMaterialStateDetail {
                scale_boost: (state_detail.scale_boost + 0.58 + contamination * 0.82)
                    .clamp(1.2, 4.4),
                state_intensity: state_detail.state_intensity.max(contamination),
                relief: (state_detail.relief + biological * 0.18 + oil * 0.14).clamp(0.0, 1.0),
                crack_density: state_detail.crack_density,
                moisture: state_detail.moisture.max(oil * 0.42),
                soot: state_detail.soot.max(contamination * 0.16),
                corrosion: state_detail.corrosion,
                heat: state_detail.heat,
                electrical_charge: state_detail.electrical_charge,
                plastic_strain: strain,
                biological_contamination: biological,
                oil_contamination: oil,
                seed_salt: state_detail.seed_salt ^ seed.rotate_left(47) ^ 0xB10C_0A71_0A1E_51CC,
            };
            let layer_surface = if oil > 0.08 || wetness > 0.18 {
                WINDOW_SURFACE_RESPONSE_WET_ROAD
            } else {
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
            };

            for layer in 0..layer_count {
                let face_pick = window_stable_unit(layer_seed, 16_401 + layer as u64);
                let along = window_stable_unit(layer_seed, 16_501 + layer as u64);
                let height_t = window_stable_unit(layer_seed, 16_601 + layer as u64);
                let wobble = window_stable_unit(layer_seed, 16_701 + layer as u64);
                let drip = biological * (0.5 + wobble * 0.7) + oil * 0.28;
                let patch_height = (span_z * (0.045 + contamination * 0.11 + drip * 0.08))
                    .clamp(0.045, span_z * 0.48);
                let patch_width_x = (span_x * (0.045 + contamination * 0.1 + wobble * 0.08))
                    .max(0.016)
                    .min((span_x * 0.5).max(0.016));
                let patch_width_y = (span_y * (0.045 + contamination * 0.1 + wobble * 0.08))
                    .max(0.016)
                    .min((span_y * 0.5).max(0.016));
                let center_z = (z0 + span_z * (0.08 + height_t * 0.68))
                    .clamp(z0 + patch_height * 0.4, z1 - patch_height * 0.22);
                let alpha =
                    (0.14 + contamination * 0.34 + oil * 0.16 + wobble * 0.05).clamp(0.0, 0.72);
                let biological_color = [0.2 + biological * 0.12, 0.035, 0.028, color[3]];
                let oil_color = [0.03, 0.034 + oil * 0.035, 0.03 + oil * 0.028, color[3]];
                let layer_color = window_with_alpha(
                    window_mix_color(biological_color, oil_color, oil * 0.58),
                    alpha,
                );

                let (center, right, up, half_extents) = if face_pick < 0.5 {
                    let outward = if face_pick < 0.25 { -1.0 } else { 1.0 };
                    let y = if outward < 0.0 { y0 } else { y1 };
                    let x = if span_x > patch_width_x * 2.0 {
                        (x0 + span_x * along).clamp(x0 + patch_width_x, x1 - patch_width_x)
                    } else {
                        (x0 + x1) * 0.5
                    };
                    (
                        [x, y + outward * face_offset * 1.55, center_z],
                        [1.0, 0.0, 0.0],
                        normalize3([0.0, outward * drip * 0.18, 1.0]),
                        [patch_width_x, patch_height],
                    )
                } else {
                    let outward = if face_pick < 0.75 { -1.0 } else { 1.0 };
                    let x = if outward < 0.0 { x0 } else { x1 };
                    let y = if span_y > patch_width_y * 2.0 {
                        (y0 + span_y * along).clamp(y0 + patch_width_y, y1 - patch_width_y)
                    } else {
                        (y0 + y1) * 0.5
                    };
                    (
                        [x + outward * face_offset * 1.55, y, center_z],
                        [0.0, 1.0, 0.0],
                        normalize3([outward * drip * 0.18, 0.0, 1.0]),
                        [patch_width_y, patch_height],
                    )
                };

                self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                    center,
                    right,
                    up,
                    half_extents,
                    color: layer_color,
                    surface_response: layer_surface,
                    state_detail: layer_state,
                });

                if drip > 0.32 {
                    self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                        center: add3(center, scale3(up, -patch_height * (0.42 + wobble * 0.2))),
                        right,
                        up,
                        half_extents: [
                            half_extents[0] * (0.18 + wobble * 0.22),
                            patch_height * (0.22 + drip * 0.16),
                        ],
                        color: window_with_alpha(layer_color, (alpha * 0.78).clamp(0.0, 0.62)),
                        surface_response: layer_surface,
                        state_detail: layer_state,
                    });
                }
            }
        }
    }

    fn add_world_box_state_contact_detail(
        &mut self,
        min: [f32; 3],
        max: [f32; 3],
        color: [f32; 4],
        surface_response: [f32; 4],
        seed: u64,
        state_detail: WindowMaterialStateDetail,
    ) {
        if state_detail.is_neutral() || min.iter().chain(max.iter()).any(|value| !value.is_finite())
        {
            return;
        }

        let x0 = min[0].min(max[0]);
        let x1 = min[0].max(max[0]);
        let y0 = min[1].min(max[1]);
        let y1 = min[1].max(max[1]);
        let z0 = min[2].min(max[2]);
        let z1 = min[2].max(max[2]);
        let span_x = x1 - x0;
        let span_y = y1 - y0;
        let span_z = z1 - z0;
        if span_x <= f32::EPSILON || span_y <= f32::EPSILON || span_z <= 0.12 || z0 > 0.36 {
            return;
        }

        let grime = surface_response[0].clamp(0.0, 1.0);
        let surface_wetness = surface_response[2].clamp(0.0, 1.0);
        let wet_layer = (state_detail.moisture * 0.78
            + surface_wetness * 0.38
            + state_detail.oil_contamination * 0.44
            + state_detail.state_intensity * 0.08)
            .clamp(0.0, 1.0);
        let dirty_layer = (state_detail.soot * 0.7
            + state_detail.corrosion * 0.42
            + state_detail.biological_contamination * 0.38
            + state_detail.oil_contamination * 0.2
            + grime * 0.2
            + state_detail.state_intensity * 0.2)
            .clamp(0.0, 1.0);
        let contact =
            (wet_layer * 0.58 + dirty_layer * 0.64 + state_detail.relief * 0.18).clamp(0.0, 1.0);
        if contact <= 0.035 {
            return;
        }

        let contact_z = z0.max(0.002) + 0.004;
        let spread_x = (span_x * (0.055 + contact * 0.06)).clamp(0.035, 0.24);
        let spread_y = (span_y * (0.055 + contact * 0.06)).clamp(0.035, 0.24);
        let strip_alpha = (0.14 + contact * 0.3 + state_detail.soot * 0.12).clamp(0.0, 0.62);
        let strip_color = window_with_alpha(
            window_mix_color(
                window_mix_color(
                    [0.018, 0.017, 0.015, color[3]],
                    [0.18, 0.035, 0.028, color[3]],
                    state_detail.biological_contamination * 0.46,
                ),
                [0.22, 0.105, 0.035, color[3]],
                (state_detail.corrosion * 0.55 + state_detail.oil_contamination * 0.18)
                    .clamp(0.0, 1.0),
            ),
            strip_alpha,
        );
        let contact_state = WindowMaterialStateDetail {
            scale_boost: (state_detail.scale_boost + 0.55 + contact * 0.7).clamp(1.2, 4.4),
            state_intensity: state_detail.state_intensity.max(contact),
            relief: (state_detail.relief * 0.6 + dirty_layer * 0.28).clamp(0.0, 1.0),
            crack_density: state_detail.crack_density,
            moisture: wet_layer.max(state_detail.moisture),
            soot: dirty_layer.max(state_detail.soot),
            corrosion: state_detail.corrosion,
            heat: state_detail.heat,
            electrical_charge: state_detail.electrical_charge,
            plastic_strain: state_detail.plastic_strain,
            biological_contamination: state_detail.biological_contamination,
            oil_contamination: state_detail.oil_contamination,
            seed_salt: state_detail.seed_salt ^ seed.rotate_left(23) ^ 0xC0A7_AC7E_DA7A_1EAF,
        };

        for (center, right, up, half_extents) in [
            (
                [(x0 + x1) * 0.5, y0 - spread_y * 0.52, contact_z],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [span_x * 0.5 + spread_x, spread_y],
            ),
            (
                [(x0 + x1) * 0.5, y1 + spread_y * 0.52, contact_z],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [span_x * 0.5 + spread_x, spread_y],
            ),
            (
                [x0 - spread_x * 0.52, (y0 + y1) * 0.5, contact_z + 0.001],
                [0.0, 1.0, 0.0],
                [1.0, 0.0, 0.0],
                [span_y * 0.5 + spread_y, spread_x],
            ),
            (
                [x1 + spread_x * 0.52, (y0 + y1) * 0.5, contact_z + 0.001],
                [0.0, 1.0, 0.0],
                [1.0, 0.0, 0.0],
                [span_y * 0.5 + spread_y, spread_x],
            ),
        ] {
            self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                center,
                right,
                up,
                half_extents,
                color: strip_color,
                surface_response: WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                state_detail: contact_state,
            });
        }

        if wet_layer > 0.12 {
            let puddle_count = (1.0 + wet_layer * 4.0).ceil().clamp(1.0, 5.0) as usize;
            for puddle in 0..puddle_count {
                let side_pick = window_stable_unit(seed, 11_101 + puddle as u64);
                let along = window_stable_unit(seed, 11_201 + puddle as u64);
                let width_jitter = window_stable_unit(seed, 11_301 + puddle as u64);
                let alpha = (0.12 + wet_layer * 0.25 + width_jitter * 0.06).clamp(0.0, 0.48);
                let wet_color = [
                    0.16 + wet_layer * 0.08,
                    0.32 + surface_wetness * 0.14,
                    0.42 + wet_layer * 0.22,
                    alpha,
                ];

                let (center, right, up, half_extents) = if side_pick < 0.5 {
                    let y = if side_pick < 0.25 {
                        y0 - spread_y * (1.35 + width_jitter)
                    } else {
                        y1 + spread_y * (1.35 + width_jitter)
                    };
                    (
                        [
                            x0 + span_x * along,
                            y,
                            contact_z + 0.003 + puddle as f32 * 0.0005,
                        ],
                        [1.0, 0.0, 0.0],
                        [0.0, 1.0, 0.0],
                        [
                            (span_x * (0.06 + width_jitter * 0.09)).clamp(0.04, 0.4),
                            (spread_y * (0.38 + wet_layer * 0.56)).clamp(0.018, 0.16),
                        ],
                    )
                } else {
                    let x = if side_pick < 0.75 {
                        x0 - spread_x * (1.35 + width_jitter)
                    } else {
                        x1 + spread_x * (1.35 + width_jitter)
                    };
                    (
                        [
                            x,
                            y0 + span_y * along,
                            contact_z + 0.003 + puddle as f32 * 0.0005,
                        ],
                        [0.0, 1.0, 0.0],
                        [1.0, 0.0, 0.0],
                        [
                            (span_y * (0.06 + width_jitter * 0.09)).clamp(0.04, 0.4),
                            (spread_x * (0.38 + wet_layer * 0.56)).clamp(0.018, 0.16),
                        ],
                    )
                };

                self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                    center,
                    right,
                    up,
                    half_extents,
                    color: wet_color,
                    surface_response: WINDOW_SURFACE_RESPONSE_WET_ROAD,
                    state_detail: contact_state,
                });
            }
        }

        let debris_count = (dirty_layer * 7.0 + state_detail.corrosion * 3.0)
            .ceil()
            .clamp(0.0, 10.0) as usize;
        for debris in 0..debris_count {
            let side_pick = window_stable_unit(seed, 11_701 + debris as u64);
            let along = window_stable_unit(seed, 11_801 + debris as u64);
            let size = (0.008 + window_stable_unit(seed, 11_901 + debris as u64) * 0.024)
                * (0.8 + contact);
            let rust_bias =
                state_detail.corrosion * window_stable_unit(seed, 12_001 + debris as u64);
            let debris_color = window_with_alpha(
                window_mix_color(
                    [0.025, 0.022, 0.018, color[3]],
                    [0.34, 0.13, 0.035, color[3]],
                    rust_bias,
                ),
                (0.22 + dirty_layer * 0.28).clamp(0.0, 0.62),
            );
            let (debris_x, debris_y) = if side_pick < 0.5 {
                let y = if side_pick < 0.25 {
                    y0 - spread_y * (0.9 + along)
                } else {
                    y1 + spread_y * (0.9 + along)
                };
                (
                    x0 + span_x * window_stable_unit(seed, 12_101 + debris as u64),
                    y,
                )
            } else {
                let x = if side_pick < 0.75 {
                    x0 - spread_x * (0.9 + along)
                } else {
                    x1 + spread_x * (0.9 + along)
                };
                (
                    x,
                    y0 + span_y * window_stable_unit(seed, 12_201 + debris as u64),
                )
            };
            self.world_box_with_surface_state_detail(
                [debris_x - size, debris_y - size * 0.6, contact_z + 0.001],
                [
                    debris_x + size,
                    debris_y + size * 0.6,
                    contact_z + (size * 0.45).clamp(0.004, 0.018),
                ],
                debris_color,
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                contact_state,
            );
        }
    }

    pub fn add_world_marker(&mut self, marker: WindowWorldMarkerVisual) {
        let [x, y] = marker.position;
        let radius = marker.radius.max(0.0);
        let axis = radius * marker.axis_half_extent_scale;
        let spoke = radius * marker.spoke_half_width_scale;
        let z = 0.07;

        self.world_quad(
            [x, y + radius, z],
            [x + radius, y, z],
            [x, y - radius, z],
            [x - radius, y, z],
            marker.color,
        );
        self.world_quad(
            [x - spoke, y - axis, 0.045],
            [x + spoke, y - axis, 0.045],
            [x + spoke, y + axis, 0.045],
            [x - spoke, y + axis, 0.045],
            marker.color,
        );
        self.world_quad(
            [x - axis, y - spoke, 0.045],
            [x + axis, y - spoke, 0.045],
            [x + axis, y + spoke, 0.045],
            [x - axis, y + spoke, 0.045],
            marker.color,
        );
    }

    pub fn add_window_alley_environment(&mut self) {
        self.world_micro_detailed_box_with_surface_response(
            [-5.4, -10.0, -0.08],
            [5.4, 14.0, 0.0],
            [0.035, 0.04, 0.046, 1.0],
            [0.38, 0.0, 0.42, 0.0],
            10_001,
            0.48,
        );
        self.world_micro_detailed_box_with_surface_response(
            [-6.05, -10.0, 0.0],
            [-5.45, 14.0, 3.4],
            [0.11, 0.105, 0.1, 1.0],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            10_002,
            0.44,
        );
        self.world_micro_detailed_box_with_surface_response(
            [5.45, -10.0, 0.0],
            [6.05, 14.0, 3.4],
            [0.11, 0.105, 0.1, 1.0],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            10_003,
            0.44,
        );

        for y in (-8..=12).step_by(2) {
            self.world_flat_ellipse_with_surface_response(
                [0.0, y as f32, 0.031],
                [0.105, 0.48],
                10,
                [0.18, 0.18, 0.145, 0.34],
                [0.24, 0.0, 0.68, 0.0],
            );
        }

        for y in (-9..=13).step_by(3) {
            self.world_cylinder_between(
                [-5.375, y as f32, 0.05],
                [-5.375, y as f32, 3.28],
                0.034,
                5,
                [0.055, 0.06, 0.068, 1.0],
            );
            self.world_cylinder_between(
                [5.375, y as f32, 0.05],
                [5.375, y as f32, 3.28],
                0.034,
                5,
                [0.055, 0.06, 0.068, 1.0],
            );
        }

        for z in [0.72, 1.55, 2.38, 3.18] {
            self.world_cylinder_between(
                [-5.38, -9.8, z],
                [-5.38, 13.8, z + 0.025],
                0.025,
                5,
                [0.045, 0.048, 0.054, 1.0],
            );
            self.world_cylinder_between(
                [5.38, -9.8, z],
                [5.38, 13.8, z + 0.025],
                0.025,
                5,
                [0.045, 0.048, 0.054, 1.0],
            );
        }

        for (x, y, color) in [
            (-5.18, -6.9, [0.18, 0.11, 0.16, 0.92]),
            (-5.18, 3.4, [0.07, 0.18, 0.21, 0.9]),
            (5.18, -1.2, [0.21, 0.16, 0.08, 0.88]),
            (5.18, 8.2, [0.14, 0.12, 0.22, 0.9]),
        ] {
            let x0 = if x < 0.0 { x - 0.022 } else { x };
            let x1 = if x < 0.0 { x } else { x + 0.022 };
            self.world_oriented_rect_with_surface_response(
                [(x0 + x1) * 0.5, y, 1.36],
                [0.0, 1.0, 0.0],
                normalize3([0.0, 0.08, 1.0]),
                [0.46, 0.32],
                color,
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
            self.world_cylinder_between(
                [(x0 + x1) * 0.5, y - 0.4, 1.72],
                [(x0 + x1) * 0.5, y + 0.4, 1.74],
                0.018,
                5,
                [0.35, 0.38, 0.42, 0.84],
            );
        }

        for y in [-7.2, -0.6, 6.8, 11.6] {
            self.world_box(
                [-4.45, y - 0.08, 0.012],
                [4.45, y + 0.08, 0.045],
                [0.012, 0.015, 0.018, 1.0],
            );
            for x in (-4..=4).step_by(2) {
                self.world_box(
                    [x as f32 - 0.045, y - 0.28, 0.047],
                    [x as f32 + 0.045, y + 0.28, 0.065],
                    [0.032, 0.042, 0.05, 1.0],
                );
            }
        }

        for y in [-8.8, -4.2, 1.2, 5.8, 10.4] {
            self.world_ellipsoid_with_surface_response(
                [-5.02, y, 0.5],
                [0.13, 0.32, 0.34],
                4,
                7,
                [0.05, 0.075, 0.09, 1.0],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            self.world_ellipsoid_with_surface_response(
                [5.02, y, 0.68],
                [0.14, 0.38, 0.34],
                4,
                7,
                [0.075, 0.055, 0.045, 1.0],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        for y in [-6.0, -2.0, 2.0, 6.0, 10.0] {
            self.world_cylinder_between(
                [-5.65, y, 2.96],
                [5.65, y + 0.38, 3.16],
                0.012,
                8,
                [0.06, 0.08, 0.09, 1.0],
            );
        }

        for (index, (center, outer, _inner, color)) in [
            (
                [-2.6, -7.8, 0.018],
                [1.15, 0.28],
                [0.44, 0.1],
                [0.08, 0.16, 0.2, 0.28],
            ),
            (
                [3.15, 2.85, 0.019],
                [0.92, 0.24],
                [0.32, 0.08],
                [0.13, 0.1, 0.18, 0.25],
            ),
            (
                [-1.1, 10.5, 0.017],
                [1.42, 0.34],
                [0.5, 0.13],
                [0.06, 0.17, 0.14, 0.24],
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let seed = 44_700 + index as u64 * 73;
            let axis = normalize3([
                0.52 + window_stable_unit(seed, 1) * 0.38,
                -0.36 + window_stable_unit(seed, 2) * 0.72,
                0.0,
            ]);
            let side = normalize3([-axis[1], axis[0], 0.0]);
            self.world_quad_with_surface_response(
                [
                    center[0] - axis[0] * outer[0] * 0.82 - side[0] * outer[1] * 0.50,
                    center[1] - axis[1] * outer[0] * 0.82 - side[1] * outer[1] * 0.50,
                    center[2],
                ],
                [
                    center[0] + axis[0] * outer[0] * 0.22 - side[0] * outer[1] * 0.92,
                    center[1] + axis[1] * outer[0] * 0.22 - side[1] * outer[1] * 0.92,
                    center[2] + 0.002,
                ],
                [
                    center[0] + axis[0] * outer[0] * 0.96 + side[0] * outer[1] * 0.44,
                    center[1] + axis[1] * outer[0] * 0.96 + side[1] * outer[1] * 0.44,
                    center[2] + 0.001,
                ],
                [
                    center[0] - axis[0] * outer[0] * 0.46 + side[0] * outer[1] * 0.86,
                    center[1] - axis[1] * outer[0] * 0.46 + side[1] * outer[1] * 0.86,
                    center[2],
                ],
                color,
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            );
        }

        self.add_window_alley_natural_breakup();
        self.add_window_alley_irregular_ground_detail();
        self.add_window_alley_version13_realism_pass();

        self.world_cylinder_between(
            [-5.72, -9.4, 2.65],
            [-5.72, 13.4, 2.92],
            0.025,
            8,
            [0.025, 0.028, 0.032, 1.0],
        );
        self.world_cylinder_between(
            [5.72, -9.6, 2.88],
            [5.72, 13.2, 2.58],
            0.021,
            8,
            [0.026, 0.029, 0.035, 1.0],
        );
        self.world_cylinder_between(
            [-5.7, -2.4, 2.4],
            [5.7, -1.75, 2.18],
            0.018,
            8,
            [0.04, 0.05, 0.06, 1.0],
        );

        self.add_window_alley_vertical_city_detail();
        self.add_window_alley_surface_detail();
    }

    fn add_window_alley_irregular_ground_detail(&mut self) {
        for (index, (center, _inner, outer, color, _segments)) in [
            (
                [-2.15, -5.35, 0.064],
                [0.18, 0.06],
                [0.54, 0.18],
                [0.018, 0.015, 0.012, 0.78],
                12,
            ),
            (
                [2.58, -0.85, 0.064],
                [0.14, 0.052],
                [0.46, 0.16],
                [0.022, 0.017, 0.012, 0.72],
                10,
            ),
            (
                [-1.02, 5.65, 0.064],
                [0.2, 0.07],
                [0.62, 0.2],
                [0.016, 0.014, 0.012, 0.68],
                12,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let seed = 45_900 + index as u64 * 97;
            let axis = normalize3([
                0.46 + window_stable_unit(seed, 1) * 0.44,
                -0.30 + window_stable_unit(seed, 2) * 0.60,
                0.0,
            ]);
            let side = normalize3([-axis[1], axis[0], 0.0]);
            self.world_quad_with_surface_response(
                [
                    center[0] - axis[0] * outer[0] * 0.80 - side[0] * outer[1] * 0.48,
                    center[1] - axis[1] * outer[0] * 0.80 - side[1] * outer[1] * 0.48,
                    center[2],
                ],
                [
                    center[0] + axis[0] * outer[0] * 0.30 - side[0] * outer[1] * 0.86,
                    center[1] + axis[1] * outer[0] * 0.30 - side[1] * outer[1] * 0.86,
                    center[2] + 0.002,
                ],
                [
                    center[0] + axis[0] * outer[0] * 0.94 + side[0] * outer[1] * 0.50,
                    center[1] + axis[1] * outer[0] * 0.94 + side[1] * outer[1] * 0.50,
                    center[2] + 0.001,
                ],
                [
                    center[0] - axis[0] * outer[0] * 0.42 + side[0] * outer[1] * 0.88,
                    center[1] - axis[1] * outer[0] * 0.42 + side[1] * outer[1] * 0.88,
                    center[2],
                ],
                color,
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        for (center, right, up, half_extents, color) in [
            (
                [-1.84, -5.28, 0.071],
                normalize3([1.0, 0.18, 0.0]),
                [0.0, 1.0, 0.0],
                [0.42, 0.035],
                [0.055, 0.044, 0.03, 0.42],
            ),
            (
                [2.78, -0.8, 0.071],
                normalize3([1.0, -0.24, 0.0]),
                [0.0, 1.0, 0.0],
                [0.36, 0.03],
                [0.065, 0.05, 0.033, 0.38],
            ),
            (
                [-0.68, 5.72, 0.071],
                normalize3([1.0, 0.12, 0.0]),
                [0.0, 1.0, 0.0],
                [0.48, 0.04],
                [0.046, 0.038, 0.028, 0.44],
            ),
        ] {
            self.world_oriented_rect_with_surface_response(
                center,
                right,
                up,
                half_extents,
                color,
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        for lane in [-1.0_f32, 1.0] {
            for (index, y) in [-7.4_f32, -3.6, 0.9, 4.8, 9.2].into_iter().enumerate() {
                let seed = 30_101 + index as u64 + u64::from(lane > 0.0) * 100;
                let x = lane * (1.42 + window_stable_unit(seed, 1) * 0.18);
                let width = 0.09 + window_stable_unit(seed, 2) * 0.05;
                self.world_oriented_rect_with_surface_response(
                    [x, y, 0.069 + index as f32 * 0.0006],
                    normalize3([0.18 * lane, 1.0, 0.0]),
                    [1.0, 0.0, 0.0],
                    [0.5 + window_stable_unit(seed, 3) * 0.34, width],
                    [
                        0.07,
                        0.056,
                        0.038,
                        0.18 + window_stable_unit(seed, 4) * 0.12,
                    ],
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                );
            }
        }

        for (index, (x, y, radii, color)) in [
            (
                -4.82,
                -1.85,
                [0.08, 0.05, 0.032],
                [0.05, 0.043, 0.032, 0.82],
            ),
            (
                -4.64,
                4.75,
                [0.105, 0.058, 0.036],
                [0.072, 0.052, 0.034, 0.84],
            ),
            (
                4.72,
                -5.92,
                [0.09, 0.052, 0.03],
                [0.058, 0.046, 0.034, 0.78],
            ),
            (4.56, 2.18, [0.12, 0.065, 0.038], [0.04, 0.036, 0.03, 0.8]),
            (
                -3.18,
                8.65,
                [0.075, 0.042, 0.03],
                [0.09, 0.064, 0.038, 0.76],
            ),
            (
                3.08,
                10.12,
                [0.068, 0.04, 0.026],
                [0.055, 0.044, 0.032, 0.74],
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let z = 0.095 + index as f32 * 0.001;
            self.world_ellipsoid_with_surface_response(
                [x, y, z],
                radii,
                3,
                6,
                color,
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        for (center, right, up, half_extents, color) in [
            (
                [-3.72, -2.35, 0.078],
                normalize3([0.86, 0.2, 0.0]),
                [0.0, 1.0, 0.0],
                [0.18, 0.035],
                [0.12, 0.095, 0.068, 0.5],
            ),
            (
                [3.42, 5.18, 0.079],
                normalize3([0.62, -0.36, 0.0]),
                [0.0, 1.0, 0.0],
                [0.16, 0.04],
                [0.07, 0.082, 0.072, 0.46],
            ),
            (
                [-0.44, 1.85, 0.08],
                normalize3([0.4, 0.9, 0.0]),
                [0.0, 1.0, 0.0],
                [0.22, 0.032],
                [0.18, 0.15, 0.095, 0.34],
            ),
        ] {
            self.world_oriented_rect_with_surface_response(
                center,
                right,
                up,
                half_extents,
                color,
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        for (x, y, side) in [(-5.28, -6.35, 1.0_f32), (5.28, 6.15, -1.0)] {
            self.world_cylinder_between_with_surface_response(
                [x, y - 0.44, 0.116],
                [x + side * 0.06, y + 0.44, 0.128],
                0.045,
                6,
                [0.075, 0.068, 0.056, 0.84],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
            self.world_oriented_rect_with_surface_response(
                [x + side * 0.04, y, 0.146],
                [0.0, 1.0, 0.0],
                normalize3([side * 0.22, 0.0, 1.0]),
                [0.42, 0.018],
                [0.018, 0.015, 0.012, 0.46],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }
    }

    fn add_window_alley_version13_realism_pass(&mut self) {
        for (side, x, color) in [
            (-1.0_f32, -4.92, [0.13, 0.12, 0.105, 0.98]),
            (1.0, 4.92, [0.126, 0.118, 0.102, 0.98]),
        ] {
            self.world_cylinder_between_with_surface_response(
                [x, -9.55, 0.116],
                [x + side * 0.06, 13.55, 0.142],
                0.07,
                10,
                color,
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );

            for (index, y) in [-8.4_f32, -6.05, -3.2, -0.35, 2.1, 4.85, 7.9, 10.75, 12.45]
                .into_iter()
                .enumerate()
            {
                let seed = 52_000 + index as u64 + u64::from(side > 0.0) * 100;
                let chip_x = x - side * (0.03 + window_stable_unit(seed, 1) * 0.08);
                self.world_ellipsoid_with_surface_response(
                    [chip_x, y + (window_stable_unit(seed, 2) - 0.5) * 0.42, 0.16],
                    [
                        0.12 + window_stable_unit(seed, 3) * 0.08,
                        0.05 + window_stable_unit(seed, 4) * 0.045,
                        0.025 + window_stable_unit(seed, 5) * 0.022,
                    ],
                    3,
                    6,
                    window_with_alpha(color, 0.82),
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                );
            }
        }

        for (center, radii, color, response) in [
            (
                [-3.15, -6.7, 0.019],
                [0.68, 0.18],
                [0.055, 0.12, 0.15, 0.26],
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            ),
            (
                [1.55, -4.15, 0.018],
                [0.5, 0.14],
                [0.075, 0.065, 0.05, 0.34],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            ),
            (
                [3.25, 0.65, 0.019],
                [0.62, 0.17],
                [0.05, 0.13, 0.16, 0.22],
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            ),
            (
                [-2.25, 4.1, 0.018],
                [0.46, 0.13],
                [0.08, 0.055, 0.035, 0.32],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            ),
            (
                [0.45, 9.35, 0.019],
                [0.72, 0.21],
                [0.05, 0.14, 0.15, 0.2],
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            ),
        ] {
            self.world_flat_ellipse_with_surface_response(center, radii, 14, color, response);
        }

        for (center, right, width, color) in [
            (
                [-3.35, -7.25, 0.074],
                normalize3([0.94, 0.18, 0.0]),
                0.68,
                [0.018, 0.016, 0.013, 0.5],
            ),
            (
                [1.82, -2.95, 0.075],
                normalize3([0.7, -0.22, 0.0]),
                0.48,
                [0.026, 0.021, 0.016, 0.44],
            ),
            (
                [-0.58, 2.4, 0.076],
                normalize3([0.54, 0.38, 0.0]),
                0.56,
                [0.032, 0.027, 0.02, 0.38],
            ),
            (
                [2.6, 7.75, 0.075],
                normalize3([0.88, -0.12, 0.0]),
                0.72,
                [0.018, 0.017, 0.014, 0.42],
            ),
        ] {
            self.world_oriented_rect_with_surface_response(
                center,
                right,
                [0.0, 1.0, 0.0],
                [width, 0.018],
                color,
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        for (west_wall, y, z, height, width, color) in [
            (true, -8.3, 0.62, 0.82, 0.035, [0.03, 0.026, 0.021, 0.48]),
            (true, -4.4, 0.42, 1.26, 0.045, [0.052, 0.058, 0.05, 0.36]),
            (true, 1.65, 0.78, 1.05, 0.032, [0.026, 0.022, 0.018, 0.44]),
            (true, 8.4, 0.55, 1.42, 0.038, [0.045, 0.054, 0.048, 0.34]),
            (false, -6.8, 0.48, 1.18, 0.04, [0.034, 0.027, 0.022, 0.46]),
            (false, -1.45, 0.68, 1.0, 0.032, [0.042, 0.05, 0.048, 0.34]),
            (false, 4.35, 0.36, 1.36, 0.046, [0.026, 0.022, 0.018, 0.44]),
            (false, 10.3, 0.82, 0.9, 0.03, [0.05, 0.058, 0.052, 0.32]),
        ] {
            let x = if west_wall { -5.43 } else { 5.43 };
            let side = if west_wall { 1.0 } else { -1.0 };
            self.world_oriented_rect_with_surface_response(
                [x, y, z + height * 0.5],
                [0.0, 1.0, 0.0],
                normalize3([side * 0.08, 0.0, 1.0]),
                [width, height * 0.5],
                color,
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
            self.world_flat_ellipse_with_surface_response(
                [x + side * 0.18, y, 0.022],
                [0.14 + height * 0.1, 0.08],
                10,
                [0.04, 0.07, 0.068, 0.16],
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            );
        }

        for (x, y, warm) in [(-5.33, -2.65, true), (5.33, 6.85, false)] {
            let light_color = if warm {
                [0.92, 0.68, 0.36, 0.3]
            } else {
                [0.5, 0.62, 0.66, 0.18]
            };
            let metal_color = [0.064, 0.06, 0.052, 0.9];
            self.world_ellipsoid_with_surface_response(
                [x, y, 2.24],
                [0.12, 0.08, 0.07],
                4,
                8,
                light_color,
                WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
            );
            self.world_cylinder_between_with_surface_response(
                [x, y - 0.16, 2.26],
                [x, y + 0.16, 2.26],
                0.022,
                8,
                metal_color,
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            self.world_oriented_rect_with_surface_response(
                [x, y, 1.46],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.64, 0.72],
                window_with_alpha(light_color, if warm { 0.08 } else { 0.05 }),
                WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
            );
        }

        for (x, y, radius, color) in [
            (-4.46, -0.75, 0.18, [0.022, 0.02, 0.018, 0.9]),
            (-4.18, -0.48, 0.13, [0.038, 0.032, 0.024, 0.84]),
            (4.28, 3.65, 0.16, [0.028, 0.027, 0.025, 0.88]),
            (4.52, 3.88, 0.11, [0.052, 0.042, 0.03, 0.82]),
        ] {
            self.world_ellipsoid_with_surface_response(
                [x, y, 0.11 + radius * 0.42],
                [radius * 1.15, radius * 0.82, radius * 0.58],
                4,
                8,
                color,
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
            self.world_oriented_rect_with_surface_response(
                [x, y - radius * 0.14, 0.09 + radius * 0.86],
                [1.0, 0.0, 0.0],
                normalize3([0.0, 0.32, 1.0]),
                [radius * 0.58, 0.018],
                [0.01, 0.009, 0.008, 0.32],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        for (index, (x, y, angle, color)) in [
            (-2.8, -8.85, 0.18_f32, [0.72, 0.66, 0.52, 0.52]),
            (1.25, -6.15, -0.36, [0.46, 0.52, 0.5, 0.46]),
            (3.85, -1.15, 0.52, [0.58, 0.46, 0.34, 0.5]),
            (-3.55, 2.45, -0.12, [0.36, 0.38, 0.34, 0.46]),
            (2.7, 6.55, 0.28, [0.7, 0.62, 0.42, 0.44]),
            (-1.15, 11.35, -0.44, [0.5, 0.54, 0.48, 0.42]),
        ]
        .into_iter()
        .enumerate()
        {
            let right = normalize3([angle.cos(), angle.sin(), 0.0]);
            self.world_oriented_rect_with_surface_response(
                [x, y, 0.092 + index as f32 * 0.0005],
                right,
                normalize3([0.0, 0.08, 1.0]),
                [0.14, 0.045],
                color,
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        for (x, y) in [(-3.95, -3.75), (4.18, 0.95), (-4.22, 7.15)] {
            self.world_ellipse_ring_with_surface_response(
                [x, y, 0.105],
                [0.12, 0.045],
                [0.28, 0.095],
                12,
                [0.012, 0.012, 0.011, 0.84],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
            self.world_cylinder_between_with_surface_response(
                [x - 0.18, y + 0.04, 0.118],
                [x + 0.2, y - 0.02, 0.13],
                0.012,
                6,
                [0.014, 0.014, 0.013, 0.82],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        for (x, y, z, length, color, response) in [
            (
                -3.18,
                -2.4,
                0.15,
                0.34,
                [0.06, 0.072, 0.07, 0.74],
                WINDOW_SURFACE_RESPONSE_GLASS,
            ),
            (
                2.94,
                4.95,
                0.145,
                0.28,
                [0.09, 0.082, 0.068, 0.78],
                WINDOW_SURFACE_RESPONSE_METAL,
            ),
            (
                -1.8,
                8.4,
                0.142,
                0.24,
                [0.08, 0.09, 0.086, 0.72],
                WINDOW_SURFACE_RESPONSE_GLASS,
            ),
        ] {
            self.world_cylinder_between_with_surface_response(
                [x - length * 0.5, y, z],
                [x + length * 0.5, y + 0.06, z + 0.02],
                0.028,
                8,
                color,
                response,
            );
        }

        let van_x = -3.72;
        let van_y = 8.85;
        for (offset_y, color) in [
            (-0.96, [0.07, 0.072, 0.066, 0.86]),
            (1.16, [0.062, 0.064, 0.058, 0.84]),
        ] {
            self.world_cylinder_between_with_surface_response(
                [van_x - 0.54, van_y + offset_y, 0.48],
                [van_x + 0.54, van_y + offset_y, 0.48],
                0.045,
                10,
                color,
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        }

        for (wheel_x, wheel_y) in [
            (van_x - 0.58, van_y - 0.68),
            (van_x + 0.58, van_y - 0.68),
            (van_x - 0.58, van_y + 0.66),
            (van_x + 0.58, van_y + 0.66),
        ] {
            for tread in [-0.08_f32, 0.0, 0.08] {
                self.world_cylinder_between_with_surface_response(
                    [wheel_x - 0.13, wheel_y + tread, 0.305],
                    [wheel_x + 0.13, wheel_y + tread, 0.31],
                    0.006,
                    5,
                    [0.032, 0.031, 0.029, 0.82],
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                );
            }
        }
    }

    fn add_window_alley_natural_breakup(&mut self) {
        for (center, radii, color) in [
            (
                [-4.15, -9.15, 0.055],
                [0.7, 0.2],
                [0.055, 0.045, 0.032, 0.48],
            ),
            (
                [3.55, -6.35, 0.056],
                [0.52, 0.18],
                [0.064, 0.052, 0.036, 0.42],
            ),
            (
                [-3.85, -1.05, 0.057],
                [0.82, 0.24],
                [0.04, 0.036, 0.028, 0.52],
            ),
            (
                [3.75, 4.35, 0.056],
                [0.74, 0.22],
                [0.066, 0.048, 0.034, 0.44],
            ),
            (
                [-4.3, 9.45, 0.057],
                [0.58, 0.18],
                [0.052, 0.043, 0.031, 0.46],
            ),
        ] {
            self.world_flat_ellipse_with_surface_response(
                center,
                radii,
                10,
                color,
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        for (x, y, sx, sy, sz, color) in [
            (-4.82, -8.72, 0.15, 0.28, 0.07, [0.072, 0.058, 0.04, 0.9]),
            (-5.02, -5.4, 0.1, 0.18, 0.045, [0.048, 0.042, 0.032, 0.82]),
            (4.86, -2.95, 0.13, 0.24, 0.06, [0.064, 0.052, 0.036, 0.86]),
            (-4.92, 2.42, 0.12, 0.22, 0.055, [0.044, 0.04, 0.032, 0.82]),
            (4.8, 7.88, 0.16, 0.3, 0.072, [0.068, 0.054, 0.038, 0.88]),
            (-4.78, 11.2, 0.11, 0.2, 0.052, [0.05, 0.044, 0.034, 0.84]),
        ] {
            self.world_ellipsoid_with_surface_response(
                [x, y, 0.07 + sz * 0.45],
                [sx, sy, sz],
                3,
                6,
                color,
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        for (x, y, radii, color) in [
            (-4.62, -6.45, [0.34, 0.19, 0.16], [0.034, 0.03, 0.026, 0.92]),
            (4.7, 1.95, [0.28, 0.2, 0.14], [0.044, 0.038, 0.032, 0.9]),
            (-4.55, 6.65, [0.3, 0.18, 0.13], [0.07, 0.052, 0.038, 0.86]),
        ] {
            self.world_ellipsoid_with_surface_response(
                [x, y, 0.16],
                radii,
                4,
                8,
                color,
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
            self.world_oriented_rect_with_surface_response(
                [x, y - 0.03, 0.26],
                [1.0, 0.0, 0.0],
                normalize3([0.0, 0.25, 1.0]),
                [radii[0] * 0.52, 0.018],
                [0.012, 0.01, 0.009, 0.26],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        for (x, y) in [(-4.45, -3.2), (4.52, 6.55)] {
            self.world_ellipse_ring_with_surface_response(
                [x, y, 0.11],
                [0.16, 0.058],
                [0.31, 0.12],
                12,
                [0.012, 0.011, 0.01, 0.92],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
            self.world_cylinder_between_with_surface_response(
                [x - 0.02, y - 0.13, 0.15],
                [x + 0.02, y + 0.13, 0.15],
                0.13,
                8,
                [0.018, 0.017, 0.016, 0.88],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        for (x, y, height, color) in [
            (-4.78, 0.35, 0.58, [0.17, 0.105, 0.055, 0.94]),
            (4.68, -7.35, 0.52, [0.105, 0.12, 0.12, 0.92]),
        ] {
            self.world_cylinder_between_with_surface_response(
                [x, y, 0.08],
                [x, y, height],
                0.17,
                10,
                color,
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            for z in [0.11, height - 0.02] {
                self.world_ellipse_ring_with_surface_response(
                    [x, y, z],
                    [0.11, 0.048],
                    [0.18, 0.075],
                    10,
                    [0.035, 0.03, 0.024, 0.82],
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                );
            }
        }

        self.add_window_alley_parked_delivery_bike();
        self.add_window_alley_parked_utility_van();

        for (center, right, up, half_extents) in [
            (
                [-4.72, -7.8, 2.72],
                [0.0, 1.0, 0.0],
                normalize3([0.0, 0.1, 1.0]),
                [1.1, 0.2],
            ),
            (
                [4.72, 3.2, 2.54],
                [0.0, 1.0, 0.0],
                normalize3([0.0, -0.12, 1.0]),
                [1.28, 0.24],
            ),
            (
                [0.4, -4.8, 0.066],
                [1.0, 0.08, 0.0],
                [0.0, 1.0, 0.0],
                [1.6, 0.16],
            ),
        ] {
            self.world_oriented_rect_with_surface_response(
                center,
                right,
                up,
                half_extents,
                [0.76, 0.62, 0.42, 0.12],
                WINDOW_SURFACE_RESPONSE_DEFAULT,
            );
        }
    }

    fn add_window_alley_parked_delivery_bike(&mut self) {
        let x = 3.65;
        let y = -8.55;
        let z = 0.12;
        let metal = [0.12, 0.13, 0.118, 0.95];
        let dark = [0.018, 0.017, 0.015, 0.95];
        let rubber = [0.01, 0.01, 0.01, 0.92];
        let cargo = [0.28, 0.16, 0.07, 0.9];

        self.world_ellipse_ring_with_surface_response(
            [x, y, 0.075],
            [0.52, 0.14],
            [1.28, 0.36],
            14,
            [0.014, 0.012, 0.01, 0.26],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );

        for wheel_x in [x - 0.52, x + 0.52] {
            self.world_ellipse_ring_with_surface_response(
                [wheel_x, y, z + 0.18],
                [0.14, 0.04],
                [0.28, 0.09],
                14,
                rubber,
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
            self.world_cylinder_between_with_surface_response(
                [wheel_x, y - 0.055, z + 0.18],
                [wheel_x, y + 0.055, z + 0.18],
                0.19,
                10,
                [0.018, 0.018, 0.017, 0.86],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
            self.world_ellipsoid_with_surface_response(
                [wheel_x, y, z + 0.18],
                [0.052, 0.026, 0.052],
                4,
                8,
                [0.18, 0.17, 0.14, 0.82],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        }

        self.world_cylinder_between_with_surface_response(
            [x - 0.5, y, z + 0.22],
            [x + 0.18, y, z + 0.42],
            0.032,
            8,
            metal,
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        self.world_cylinder_between_with_surface_response(
            [x + 0.18, y, z + 0.42],
            [x + 0.5, y, z + 0.23],
            0.026,
            8,
            metal,
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        self.world_ellipsoid_with_surface_response(
            [x - 0.04, y, z + 0.43],
            [0.36, 0.11, 0.095],
            5,
            12,
            [0.11, 0.105, 0.092, 0.96],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        self.world_ellipsoid_with_surface_response(
            [x - 0.14, y - 0.01, z + 0.58],
            [0.24, 0.075, 0.045],
            4,
            9,
            dark,
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
        self.world_ellipsoid_with_surface_response(
            [x + 0.45, y, z + 0.62],
            [0.26, 0.16, 0.18],
            5,
            10,
            cargo,
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
        self.world_cylinder_between_with_surface_response(
            [x + 0.34, y, z + 0.49],
            [x + 0.54, y, z + 0.88],
            0.022,
            8,
            metal,
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        self.world_cylinder_between_with_surface_response(
            [x + 0.5, y - 0.18, z + 0.88],
            [x + 0.5, y + 0.18, z + 0.88],
            0.018,
            8,
            metal,
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        self.world_oriented_rect_with_surface_response(
            [x - 0.56, y - 0.01, z + 0.34],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.052, 0.026],
            [0.86, 0.72, 0.42, 0.28],
            WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
        );
    }

    fn add_window_alley_parked_utility_van(&mut self) {
        let x = -3.72;
        let y = 8.85;
        let z = 0.08;
        let body = [0.18, 0.21, 0.19, 0.96];
        let oxidized = [0.11, 0.13, 0.12, 0.86];
        let glass = [0.18, 0.31, 0.34, 0.46];
        let rubber = [0.012, 0.011, 0.01, 0.94];
        let dirt = [0.17, 0.11, 0.056, 0.72];

        self.world_ellipse_ring_with_surface_response(
            [x, y, 0.065],
            [0.52, 0.72],
            [0.92, 1.42],
            14,
            [0.012, 0.011, 0.009, 0.28],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
        self.world_ellipsoid_with_surface_response(
            [x, y, z + 0.46],
            [0.72, 1.02, 0.35],
            6,
            12,
            body,
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        self.world_ellipsoid_with_surface_response(
            [x + 0.02, y - 0.26, z + 0.78],
            [0.56, 0.48, 0.32],
            5,
            10,
            oxidized,
            WINDOW_SURFACE_RESPONSE_METAL,
        );

        for wheel_y in [y - 0.68, y + 0.66] {
            for wheel_x in [x - 0.58, x + 0.58] {
                self.world_cylinder_between_with_surface_response(
                    [wheel_x, wheel_y - 0.08, z + 0.24],
                    [wheel_x, wheel_y + 0.08, z + 0.24],
                    0.2,
                    10,
                    rubber,
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                );
                self.world_ellipsoid_with_surface_response(
                    [wheel_x, wheel_y, z + 0.24],
                    [0.07, 0.035, 0.07],
                    4,
                    8,
                    [0.18, 0.17, 0.14, 0.86],
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
            }
        }

        for side_x in [x - 0.64, x + 0.64] {
            self.world_oriented_rect_with_surface_response(
                [side_x, y - 0.24, z + 0.78],
                [0.0, 1.0, 0.0],
                normalize3([0.0, -0.12, 1.0]),
                [0.24, 0.11],
                glass,
                WINDOW_SURFACE_RESPONSE_GLASS,
            );
            self.world_oriented_rect_with_surface_response(
                [side_x, y + 0.26, z + 0.68],
                [0.0, 1.0, 0.0],
                normalize3([0.0, 0.08, 1.0]),
                [0.28, 0.095],
                window_with_alpha(glass, 0.34),
                WINDOW_SURFACE_RESPONSE_GLASS,
            );
        }

        self.world_oriented_rect_with_surface_response(
            [x, y - 0.82, z + 0.74],
            [1.0, 0.0, 0.0],
            normalize3([0.0, -0.24, 1.0]),
            [0.36, 0.13],
            [0.2, 0.34, 0.36, 0.42],
            WINDOW_SURFACE_RESPONSE_GLASS,
        );
        self.world_cylinder_between_with_surface_response(
            [x - 0.36, y - 0.98, z + 0.54],
            [x + 0.36, y - 0.98, z + 0.54],
            0.02,
            8,
            [0.78, 0.64, 0.38, 0.34],
            WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
        );
        self.world_cylinder_between_with_surface_response(
            [x - 0.38, y + 1.02, z + 0.5],
            [x + 0.38, y + 1.02, z + 0.5],
            0.018,
            8,
            [0.52, 0.12, 0.075, 0.3],
            WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
        );

        for rack_x in [x - 0.34, x + 0.34] {
            self.world_cylinder_between_with_surface_response(
                [rack_x, y - 0.72, z + 1.1],
                [rack_x, y + 0.78, z + 1.08],
                0.018,
                7,
                [0.07, 0.075, 0.068, 0.88],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        }
        for rack_y in [y - 0.58, y + 0.58] {
            self.world_cylinder_between_with_surface_response(
                [x - 0.44, rack_y, z + 1.09],
                [x + 0.44, rack_y, z + 1.09],
                0.016,
                7,
                [0.07, 0.075, 0.068, 0.88],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        }

        for (patch_y, patch_z, radii) in [
            (y - 0.44, z + 0.44, [0.08, 0.24, 0.035]),
            (y + 0.42, z + 0.36, [0.075, 0.28, 0.032]),
            (y + 0.84, z + 0.26, [0.09, 0.18, 0.03]),
        ] {
            self.world_ellipsoid_with_surface_response(
                [x - 0.66, patch_y, patch_z],
                radii,
                4,
                8,
                dirt,
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        for side_x in [x - 0.78, x + 0.78] {
            self.world_ellipsoid_with_surface_response(
                [side_x, y - 0.55, z + 0.72],
                [0.052, 0.03, 0.04],
                4,
                7,
                [0.035, 0.038, 0.036, 0.88],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            self.world_cylinder_between_with_surface_response(
                [side_x * 0.992 + x * 0.008, y - 0.5, z + 0.7],
                [side_x * 0.982 + x * 0.018, y - 0.38, z + 0.68],
                0.011,
                5,
                [0.035, 0.038, 0.036, 0.88],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        }

        for wiper_x in [x - 0.16, x + 0.16] {
            self.world_cylinder_between_with_surface_response(
                [wiper_x, y - 0.83, z + 0.64],
                [wiper_x + 0.18, y - 0.84, z + 0.75],
                0.006,
                5,
                [0.018, 0.019, 0.018, 0.82],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        for side_x in [x - 0.67, x + 0.67] {
            for seam_y in [y - 0.15, y + 0.34] {
                self.world_oriented_rect_with_surface_response(
                    [side_x, seam_y, z + 0.56],
                    [0.0, 1.0, 0.0],
                    [0.0, 0.0, 1.0],
                    [0.006, 0.19],
                    [0.024, 0.026, 0.024, 0.54],
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
            }
            self.world_ellipsoid_with_surface_response(
                [side_x, y + 0.1, z + 0.55],
                [0.018, 0.008, 0.012],
                3,
                6,
                [0.052, 0.058, 0.054, 0.78],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        }

        for wheel_y in [y - 0.68, y + 0.66] {
            for wheel_x in [x - 0.58, x + 0.58] {
                self.world_ellipsoid_with_surface_response(
                    [wheel_x, wheel_y, z + 0.38],
                    [0.19, 0.05, 0.045],
                    3,
                    7,
                    window_scale_color(body, 0.64),
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
            }
        }

        for (dent_x, dent_y, dent_z, radii) in [
            (x - 0.54, y + 0.18, z + 0.58, [0.045, 0.11, 0.018]),
            (x + 0.58, y - 0.34, z + 0.48, [0.04, 0.09, 0.016]),
            (x - 0.1, y + 0.76, z + 0.33, [0.12, 0.035, 0.014]),
        ] {
            self.world_ellipsoid_with_surface_response(
                [dent_x, dent_y, dent_z],
                radii,
                3,
                6,
                [0.052, 0.058, 0.052, 0.42],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        }

        self.world_oriented_rect_with_surface_response(
            [x, y - 1.035, z + 0.42],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.16, 0.04],
            [0.62, 0.58, 0.42, 0.38],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        self.world_oriented_rect_with_surface_response(
            [x, y + 1.045, z + 0.42],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.14, 0.035],
            [0.32, 0.08, 0.055, 0.28],
            WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
        );
    }

    fn add_window_alley_vertical_city_detail(&mut self) {
        for (west_wall, y0, y1, z0, z1, depth, color) in [
            (
                true,
                -10.2,
                -5.4,
                3.15,
                7.4,
                0.85,
                [0.055, 0.062, 0.074, 1.0],
            ),
            (true, -4.8, 1.7, 3.35, 8.6, 1.05, [0.064, 0.058, 0.07, 1.0]),
            (true, 2.4, 13.9, 3.05, 6.9, 0.75, [0.048, 0.056, 0.062, 1.0]),
            (false, -9.6, -1.8, 3.0, 6.8, 0.72, [0.06, 0.052, 0.046, 1.0]),
            (
                false,
                -1.0,
                5.8,
                3.25,
                8.1,
                1.12,
                [0.052, 0.064, 0.072, 1.0],
            ),
            (
                false,
                6.4,
                14.2,
                3.05,
                7.25,
                0.88,
                [0.058, 0.05, 0.066, 1.0],
            ),
        ] {
            let (x0, x1) = if west_wall {
                (-6.08 - depth, -6.08)
            } else {
                (6.08, 6.08 + depth)
            };
            self.world_micro_detailed_box_with_surface_response(
                [x0, y0, z0],
                [x1, y1, z1],
                color,
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                ((y0.to_bits() as u64) << 24) ^ ((z1.to_bits() as u64) << 3) ^ 20_000,
                0.5,
            );

            let face_x = if west_wall { -6.095 } else { 6.095 };
            let trim_color = window_scale_color(color, 1.22);
            for z in [z0 + 0.75, z0 + 1.65, z0 + 2.55, z1 - 0.55] {
                if z < z1 - 0.18 {
                    self.world_box_with_surface_response(
                        [face_x - 0.018, y0 + 0.18, z],
                        [face_x + 0.018, y1 - 0.18, z + 0.035],
                        trim_color,
                        WINDOW_SURFACE_RESPONSE_METAL,
                    );
                }
            }

            let window_count = (((y1 - y0).abs() / 2.4).ceil() as usize).clamp(1, 5);
            for index in 0..window_count {
                let window_y = y0 + 0.95 + index as f32 * ((y1 - y0) / window_count as f32);
                for (level, glow) in [
                    (z0 + 1.15, [0.05, 0.16, 0.2, 0.5]),
                    (z0 + 2.65, [0.19, 0.1, 0.2, 0.46]),
                    (z0 + 4.05, [0.08, 0.22, 0.24, 0.4]),
                ] {
                    if level > z1 - 0.42 {
                        continue;
                    }
                    self.world_oriented_rect_with_surface_response(
                        [face_x, window_y, level],
                        [0.0, 1.0, 0.0],
                        [0.0, 0.0, 1.0],
                        [0.32, 0.24],
                        glow,
                        if glow[1] > 0.18 {
                            WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
                        } else {
                            WINDOW_SURFACE_RESPONSE_GLASS
                        },
                    );
                }
            }
        }

        for (y, z, width, color) in [
            (-6.6, 3.52, 2.9, [0.04, 0.047, 0.052, 1.0]),
            (-1.0, 4.08, 3.6, [0.046, 0.052, 0.058, 1.0]),
            (5.2, 3.78, 3.15, [0.038, 0.044, 0.05, 1.0]),
            (10.8, 4.32, 2.55, [0.048, 0.044, 0.054, 1.0]),
        ] {
            self.world_box_with_surface_response(
                [-width, y - 0.18, z],
                [width, y + 0.18, z + 0.12],
                color,
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            for x in -4..=4 {
                let rung_x = x as f32 * width / 4.0;
                if rung_x.abs() > width - 0.18 {
                    continue;
                }
                self.world_box_with_surface_response(
                    [rung_x - 0.025, y - 0.24, z + 0.13],
                    [rung_x + 0.025, y + 0.24, z + 0.18],
                    [0.07, 0.086, 0.094, 1.0],
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
            }
            self.world_box_with_surface_response(
                [-width, y - 0.27, z + 0.18],
                [width, y - 0.21, z + 0.55],
                [0.028, 0.034, 0.04, 1.0],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            self.world_box_with_surface_response(
                [-width, y + 0.21, z + 0.18],
                [width, y + 0.27, z + 0.55],
                [0.028, 0.034, 0.04, 1.0],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            self.world_billboard_with_surface_response(
                [0.0, y - 0.08, z - 0.05],
                [1.0, 0.0, 0.0],
                width * 2.2,
                0.64,
                [0.06, 0.18, 0.22, 0.18],
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            );
        }

        for (west_wall, y, z, height) in [
            (true, -8.1, 1.0, 2.7),
            (true, 2.5, 1.18, 3.15),
            (false, -3.7, 1.05, 2.85),
            (false, 8.8, 0.95, 3.35),
        ] {
            let x = if west_wall { -5.28 } else { 5.28 };
            let face_x = if west_wall { -5.22 } else { 5.22 };
            self.world_cylinder_between_with_surface_response(
                [x, y, z],
                [x, y, z + height],
                0.018,
                8,
                [0.068, 0.076, 0.08, 1.0],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            self.world_cylinder_between_with_surface_response(
                [x + if west_wall { 0.18 } else { -0.18 }, y, z],
                [x + if west_wall { 0.18 } else { -0.18 }, y, z + height],
                0.018,
                8,
                [0.068, 0.076, 0.08, 1.0],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            for step in 0..7 {
                let rung_z = z + 0.22 + step as f32 * height / 7.0;
                self.world_cylinder_between_with_surface_response(
                    [face_x, y, rung_z],
                    [face_x + if west_wall { 0.28 } else { -0.28 }, y, rung_z],
                    0.012,
                    6,
                    [0.09, 0.102, 0.108, 1.0],
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
            }
        }

        for (start, end, radius, color) in [
            (
                [-5.85, -9.4, 4.85],
                [5.85, -7.8, 4.45],
                0.013,
                [0.018, 0.022, 0.027, 1.0],
            ),
            (
                [-5.85, -5.1, 5.35],
                [5.85, -3.9, 5.0],
                0.011,
                [0.025, 0.025, 0.032, 1.0],
            ),
            (
                [-5.85, 0.2, 4.65],
                [5.85, 1.35, 4.95],
                0.012,
                [0.018, 0.028, 0.034, 1.0],
            ),
            (
                [-5.85, 6.7, 5.2],
                [5.85, 7.55, 4.72],
                0.014,
                [0.03, 0.026, 0.036, 1.0],
            ),
        ] {
            self.world_cylinder_between_with_surface_response(
                start,
                end,
                radius,
                8,
                color,
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            self.world_cylinder_between_with_surface_response(
                [start[0], start[1] + 0.08, start[2] - 0.08],
                [end[0], end[1] + 0.08, end[2] - 0.08],
                radius * 0.72,
                8,
                window_scale_color(color, 1.22),
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        }

        for (west_wall, y, z, color) in [
            (true, -8.7, 4.1, [0.22, 0.62, 0.72, 0.34]),
            (false, -5.0, 4.65, [0.72, 0.22, 0.48, 0.32]),
            (true, 1.1, 4.85, [0.82, 0.62, 0.28, 0.34]),
            (false, 7.7, 4.25, [0.3, 0.7, 0.48, 0.32]),
        ] {
            let x = if west_wall { -5.92 } else { 5.92 };
            self.world_billboard_with_surface_response(
                [x, y, z],
                [0.0, 1.0, 0.0],
                0.52,
                1.35,
                color,
                WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
            );
            self.world_billboard_with_surface_response(
                [if west_wall { -4.95 } else { 4.95 }, y, z - 0.18],
                [1.0, 0.0, 0.0],
                1.2,
                0.72,
                window_with_alpha(color, color[3] * 0.32),
                WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
            );
        }

        for (x, y, z, sx, sy, sz) in [
            (-4.78, -6.95, 2.2, 0.62, 0.24, 0.34),
            (-4.84, 4.2, 2.72, 0.48, 0.22, 0.28),
            (4.78, -0.2, 2.54, 0.54, 0.24, 0.32),
            (4.82, 9.6, 2.18, 0.64, 0.26, 0.36),
        ] {
            self.world_box_with_surface_response(
                [x - sx * 0.5, y - sy * 0.5, z - sz * 0.5],
                [x + sx * 0.5, y + sy * 0.5, z + sz * 0.5],
                [0.072, 0.08, 0.084, 1.0],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            for stripe in [-0.18, 0.0, 0.18] {
                self.world_box_with_surface_response(
                    [x - sx * 0.42, y - sy * 0.56, z + stripe],
                    [x + sx * 0.42, y - sy * 0.5, z + stripe + 0.018],
                    [0.018, 0.024, 0.028, 1.0],
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
            }
        }
    }

    fn add_window_alley_surface_detail(&mut self) {
        for (center, half_width, half_height, angle, color) in [
            (
                [1.2, -8.65, 0.052],
                0.62,
                0.012,
                0.18,
                [0.008, 0.01, 0.012, 1.0],
            ),
            (
                [-2.4, -5.15, 0.053],
                0.44,
                0.01,
                -0.42,
                [0.012, 0.014, 0.016, 1.0],
            ),
            (
                [2.95, -2.25, 0.052],
                0.52,
                0.012,
                0.36,
                [0.008, 0.012, 0.014, 1.0],
            ),
            (
                [-1.55, 1.55, 0.053],
                0.76,
                0.011,
                -0.2,
                [0.01, 0.012, 0.014, 1.0],
            ),
            (
                [3.3, 6.2, 0.052],
                0.58,
                0.012,
                -0.54,
                [0.012, 0.014, 0.018, 1.0],
            ),
            (
                [-2.7, 10.85, 0.052],
                0.66,
                0.01,
                0.32,
                [0.008, 0.012, 0.014, 1.0],
            ),
        ] {
            let angle: f32 = angle;
            let right = [angle.cos(), angle.sin(), 0.0];
            let up = [-angle.sin(), angle.cos(), 0.0];
            self.world_oriented_rect(center, right, up, half_width, half_height, color);
        }

        for (x, y, grate_width) in [(-4.18, -8.1, 0.74), (4.04, 0.3, 0.68), (-4.28, 8.9, 0.78)] {
            self.world_box_with_surface_response(
                [x - grate_width * 0.5, y - 0.18, 0.054],
                [x + grate_width * 0.5, y + 0.18, 0.084],
                [0.018, 0.024, 0.028, 1.0],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            for slot in -2..=2 {
                let slot_x = x + slot as f32 * grate_width * 0.16;
                self.world_box_with_surface_response(
                    [slot_x - 0.014, y - 0.16, 0.086],
                    [slot_x + 0.014, y + 0.16, 0.096],
                    [0.06, 0.095, 0.11, 1.0],
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
            }
            self.world_ellipse_ring_with_surface_response(
                [x, y, 0.102],
                [grate_width * 0.42, 0.12],
                [grate_width * 0.62, 0.25],
                18,
                [0.04, 0.18, 0.2, 0.32],
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            );
        }

        for (west_wall, y, z, width, height, color) in [
            (true, -7.7, 1.28, 0.72, 0.48, [0.018, 0.04, 0.05, 0.44]),
            (true, -2.6, 2.2, 0.52, 0.62, [0.07, 0.045, 0.075, 0.38]),
            (true, 6.1, 1.6, 0.82, 0.72, [0.025, 0.03, 0.026, 0.48]),
            (false, -4.8, 1.76, 0.64, 0.58, [0.072, 0.052, 0.035, 0.36]),
            (false, 2.7, 2.32, 0.78, 0.48, [0.02, 0.056, 0.064, 0.42]),
            (false, 9.5, 1.1, 0.58, 0.52, [0.055, 0.035, 0.07, 0.34]),
        ] {
            let x = if west_wall { -5.305 } else { 5.305 };
            self.world_oriented_rect(
                [x, y, z],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                width,
                height,
                color,
            );
        }

        for (x, y, z, glow) in [
            (-5.22, -3.45, 1.34, [0.16, 0.56, 0.66, 0.46]),
            (5.22, 4.6, 1.92, [0.78, 0.22, 0.46, 0.42]),
            (-5.22, 10.9, 1.18, [0.82, 0.62, 0.24, 0.42]),
        ] {
            let west_wall = x < 0.0;
            let panel_min_x = if west_wall { x - 0.055 } else { x };
            let panel_max_x = if west_wall { x } else { x + 0.055 };
            self.world_box_with_surface_response(
                [panel_min_x, y - 0.3, z - 0.22],
                [panel_max_x, y + 0.3, z + 0.22],
                [0.032, 0.036, 0.044, 1.0],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            self.world_oriented_rect_with_surface_response(
                [x, y, z],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.21, 0.075],
                glow,
                WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
            );
            for offset in [-0.16, 0.16] {
                let cable_x = if west_wall { x - 0.012 } else { x + 0.012 };
                self.world_cylinder_between_with_surface_response(
                    [cable_x, y + offset, z + 0.2],
                    [cable_x, y + offset * 1.25, z + 1.0],
                    0.01,
                    6,
                    [0.018, 0.024, 0.028, 1.0],
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
            }
        }

        for (west_wall, y, z, color) in [
            (true, -5.9, 2.48, [0.58, 0.2, 0.76, 0.62]),
            (false, -0.7, 2.22, [0.22, 0.64, 0.72, 0.62]),
            (true, 4.75, 2.72, [0.86, 0.48, 0.2, 0.62]),
            (false, 10.4, 2.0, [0.62, 0.72, 0.3, 0.62]),
        ] {
            let x = if west_wall { -5.18 } else { 5.18 };
            let bracket_end_x = if west_wall { -4.52 } else { 4.52 };
            self.world_cylinder_between_with_surface_response(
                [x, y - 0.28, z + 0.24],
                [bracket_end_x, y - 0.28, z + 0.24],
                0.018,
                8,
                [0.055, 0.06, 0.066, 1.0],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            self.world_cylinder_between_with_surface_response(
                [x, y + 0.28, z + 0.24],
                [bracket_end_x, y + 0.28, z + 0.24],
                0.018,
                8,
                [0.055, 0.06, 0.066, 1.0],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            self.world_box_with_surface_response(
                [bracket_end_x.min(x) - 0.02, y - 0.34, z - 0.14],
                [bracket_end_x.max(x) + 0.02, y + 0.34, z + 0.14],
                [0.022, 0.026, 0.034, 1.0],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            self.world_billboard_with_surface_response(
                [bracket_end_x, y, z],
                [1.0, 0.0, 0.0],
                0.9,
                0.38,
                [color[0], color[1], color[2], 0.22],
                WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
            );
            self.world_cylinder_between_with_surface_response(
                [bracket_end_x, y - 0.22, z],
                [bracket_end_x, y + 0.22, z],
                0.017,
                10,
                color,
                WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
            );
        }
    }

    pub fn add_window_alley_infrastructure(
        &mut self,
        frame_index: u64,
        state: WindowInfrastructureVisualState,
    ) {
        let pulse = (frame_index as f32 * 0.087).sin().mul_add(0.5, 0.5);
        let surveillance = state.surveillance_coverage.clamp(0.0, 1.0);
        let power = state.power_instability.clamp(0.0, 1.0);
        let drainage = state.drainage_overflow.clamp(0.0, 1.0);
        let data = state.data_activity.clamp(0.0, 1.0);

        for (west_wall, y, z, pan) in [
            (true, -8.35, 2.48, -0.28),
            (false, -2.1, 2.22, 0.2),
            (true, 4.55, 2.7, 0.34),
            (false, 10.15, 2.38, -0.18),
        ] {
            let wall_x = if west_wall { -5.36 } else { 5.36 };
            let arm_x = if west_wall { -4.82 } else { 4.82 };
            let lens_x = if west_wall { -4.66 } else { 4.66 };
            self.world_cylinder_between_with_surface_response(
                [wall_x, y, z + 0.08],
                [arm_x, y + pan * 0.16, z],
                0.018,
                8,
                [0.055, 0.064, 0.07, 1.0],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            self.world_box_with_surface_response(
                [arm_x - 0.16, y - 0.13, z - 0.1],
                [arm_x + 0.16, y + 0.13, z + 0.1],
                [0.072, 0.084, 0.09, 1.0],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            self.world_box_with_surface_response(
                [lens_x - 0.052, y - 0.055, z - 0.04],
                [lens_x + 0.052, y + 0.055, z + 0.04],
                [0.02, 0.032, 0.038, 1.0],
                WINDOW_SURFACE_RESPONSE_GLASS,
            );

            let cone_alpha =
                (0.08 + surveillance * 0.28 + pulse * surveillance * 0.12).clamp(0.0, 0.5);
            let cone_color = if surveillance > 0.62 {
                [1.0, 0.12, 0.18, cone_alpha]
            } else {
                [0.18, 0.76, 1.0, cone_alpha]
            };
            self.world_oriented_rect_with_surface_response(
                [lens_x, y + pan * 0.44, z - 0.22],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.72 + surveillance * 0.32, 0.22 + surveillance * 0.16],
                cone_color,
                WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
            );
        }

        for (x, y, z, west_wall) in [
            (-5.14, -6.1, 0.8, true),
            (5.14, 2.2, 0.92, false),
            (-5.14, 9.2, 0.72, true),
        ] {
            let panel_x0 = if west_wall { x - 0.08 } else { x };
            let panel_x1 = if west_wall { x } else { x + 0.08 };
            self.world_box_with_surface_response(
                [panel_x0, y - 0.42, z],
                [panel_x1, y + 0.42, z + 0.72],
                [0.064, 0.068, 0.062, 1.0],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            self.world_oriented_rect_with_surface_response(
                [x, y, z + 0.5],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.24, 0.065],
                [1.0, 0.82, 0.18, (0.16 + power * 0.52).clamp(0.0, 0.82)],
                WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
            );
            if power > 0.08 {
                for spark in 0..4 {
                    let spark_y = y - 0.24 + spark as f32 * 0.16;
                    let spark_z = z + 0.2 + ((pulse + spark as f32 * 0.17).fract()) * 0.48;
                    self.world_oriented_rect_with_surface_response(
                        [x + if west_wall { 0.012 } else { -0.012 }, spark_y, spark_z],
                        normalize3([0.0, 0.18, 0.12 - spark as f32 * 0.025]),
                        [1.0, 0.0, 0.0],
                        [0.055 + power * 0.025, 0.004],
                        [1.0, 0.9, 0.28, (0.2 + power * 0.52).clamp(0.0, 0.82)],
                        WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
                    );
                }
            }
        }

        for (x, y, radius) in [(-4.15, -8.1, 0.85), (4.08, 0.3, 0.78), (-4.25, 8.9, 0.9)] {
            self.world_ellipse_ring_with_surface_response(
                [x, y, 0.114],
                [radius * 0.34, radius * 0.12],
                [
                    radius * (0.58 + drainage * 0.22),
                    radius * (0.24 + drainage * 0.12),
                ],
                22,
                [0.34, 0.74, 1.0, (0.12 + drainage * 0.36).clamp(0.0, 0.62)],
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            );
            if drainage > 0.2 {
                self.world_cylinder_between_with_surface_response(
                    [x - 0.36, y - 0.02, 0.11],
                    [x + 0.42, y + 0.12 + drainage * 0.22, 0.115],
                    0.018 + drainage * 0.012,
                    8,
                    [0.5, 0.84, 1.0, (0.22 + drainage * 0.28).clamp(0.0, 0.58)],
                    WINDOW_SURFACE_RESPONSE_WET_ROAD,
                );
            }
        }

        for (west_wall, y, z) in [(true, -2.85, 1.15), (false, 3.65, 1.35), (true, 11.1, 1.05)] {
            let x = if west_wall { -5.22 } else { 5.22 };
            let box_min_x = if west_wall { x - 0.05 } else { x };
            let box_max_x = if west_wall { x } else { x + 0.05 };
            self.world_box_with_surface_response(
                [box_min_x, y - 0.36, z - 0.22],
                [box_max_x, y + 0.36, z + 0.22],
                [0.032, 0.04, 0.048, 1.0],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            for line in 0..4 {
                let line_y = y - 0.24 + line as f32 * 0.16;
                self.world_oriented_rect_with_surface_response(
                    [x, line_y, z + 0.02],
                    [0.0, 1.0, 0.0],
                    [0.0, 0.0, 1.0],
                    [0.075, 0.012],
                    [0.24, 0.9, 1.0, (0.18 + data * 0.46).clamp(0.0, 0.76)],
                    WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
                );
            }
            self.world_cylinder_between_with_surface_response(
                [x, y + 0.34, z + 0.18],
                [x, y + 0.58, z + 0.72],
                0.01,
                6,
                [0.08, 0.1, 0.112, 1.0],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        }

        for (west_wall, y, locked) in [(true, -0.55, true), (false, 6.65, false)] {
            let x = if west_wall { -5.18 } else { 5.18 };
            let door_min_x = if west_wall { x - 0.07 } else { x };
            let door_max_x = if west_wall { x } else { x + 0.07 };
            self.world_box_with_surface_response(
                [door_min_x, y - 0.52, 0.08],
                [door_max_x, y + 0.52, 1.8],
                [0.048, 0.052, 0.058, 1.0],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            for stripe in 0..5 {
                let stripe_z = 0.24 + stripe as f32 * 0.28;
                self.world_box_with_surface_response(
                    [door_min_x - 0.006, y - 0.48, stripe_z],
                    [door_max_x + 0.006, y + 0.48, stripe_z + 0.025],
                    [0.08, 0.088, 0.095, 1.0],
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
            }
            let lock_color = if locked || surveillance > 0.5 {
                [
                    1.0,
                    0.14,
                    0.18,
                    (0.28 + surveillance * 0.44).clamp(0.0, 0.82),
                ]
            } else {
                [0.25, 1.0, 0.62, 0.42]
            };
            self.world_oriented_rect_with_surface_response(
                [x, y + 0.42, 1.04],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.055, 0.12],
                lock_color,
                WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
            );
        }
    }

    pub fn add_window_city_streaming_cells(
        &mut self,
        cells: &[WindowCityStreamingCellVisual],
        frame_index: u64,
    ) {
        let phase = (frame_index as f32 * 0.041).fract();
        for cell in cells {
            if matches!(cell.state, WindowCityStreamingCellState::Unloaded) {
                continue;
            }

            let [x, y] = cell.center_meters;
            let [half_x, half_y] = cell.half_extents_meters;
            let min_x = x - half_x;
            let max_x = x + half_x;
            let min_y = y - half_y;
            let max_y = y + half_y;
            let color = window_streaming_cell_state_color(cell.state);
            let priority = cell.priority.clamp(0.0, 4.0);
            let alignment = ((cell.camera_alignment + 1.0) * 0.5).clamp(0.0, 1.0);
            let alpha = (0.08 + priority * 0.035 + alignment * 0.08).clamp(0.08, 0.34);
            let surface = if cell.is_loaded() {
                WINDOW_SURFACE_RESPONSE_WET_ROAD
            } else {
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
            };

            self.world_box_with_surface_response(
                [min_x, min_y, 0.018],
                [max_x, min_y + 0.055, 0.036],
                window_with_alpha(color, alpha),
                surface,
            );
            self.world_box_with_surface_response(
                [min_x, max_y - 0.055, 0.018],
                [max_x, max_y, 0.036],
                window_with_alpha(color, alpha),
                surface,
            );
            self.world_box_with_surface_response(
                [min_x, min_y, 0.018],
                [min_x + 0.055, max_y, 0.036],
                window_with_alpha(color, alpha * 0.82),
                surface,
            );
            self.world_box_with_surface_response(
                [max_x - 0.055, min_y, 0.018],
                [max_x, max_y, 0.036],
                window_with_alpha(color, alpha * 0.82),
                surface,
            );

            let beacon_height = match cell.state {
                WindowCityStreamingCellState::SummaryLoaded => 0.28,
                WindowCityStreamingCellState::GameplayLoaded => 0.52,
                WindowCityStreamingCellState::RenderHighDetail => 0.78,
                WindowCityStreamingCellState::HeroLoaded => 1.05,
                WindowCityStreamingCellState::Unloaded => 0.0,
            };
            if beacon_height > 0.0 {
                let beacon_alpha = (0.2 + priority * 0.06 + phase * 0.08).clamp(0.18, 0.78);
                for corner in [
                    [min_x + 0.18, min_y + 0.18],
                    [max_x - 0.18, min_y + 0.18],
                    [min_x + 0.18, max_y - 0.18],
                    [max_x - 0.18, max_y - 0.18],
                ] {
                    self.world_cylinder_between_with_surface_response(
                        [corner[0], corner[1], 0.04],
                        [corner[0], corner[1], 0.04 + beacon_height],
                        0.028,
                        8,
                        window_with_alpha(color, beacon_alpha),
                        WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
                    );
                }
            }

            if matches!(
                cell.state,
                WindowCityStreamingCellState::RenderHighDetail
                    | WindowCityStreamingCellState::HeroLoaded
            ) {
                let marker_count = cell.dependency_count.clamp(2, 8) as usize;
                let step = (half_x * 2.0 / marker_count as f32).max(0.5);
                for marker in 0..marker_count {
                    let marker_phase = (phase + marker as f32 * 0.137).fract();
                    let marker_x = min_x + step * (marker as f32 + 0.5);
                    let marker_y = if marker % 2 == 0 {
                        min_y + 0.62
                    } else {
                        max_y - 0.62
                    };
                    let height = if matches!(cell.state, WindowCityStreamingCellState::HeroLoaded) {
                        0.46 + marker_phase * 0.28
                    } else {
                        0.3 + marker_phase * 0.18
                    };
                    self.world_box_with_surface_response(
                        [marker_x - 0.11, marker_y - 0.08, 0.042],
                        [marker_x + 0.11, marker_y + 0.08, 0.042 + height],
                        window_with_alpha(color, 0.36 + marker_phase * 0.22),
                        WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
                    );
                }
            }

            if cell.requested_streaming_megabytes > 0.1 {
                let normalized_bytes = (cell.requested_streaming_megabytes / 96.0).clamp(0.0, 1.0);
                self.world_box_with_surface_response(
                    [min_x + 0.36, y - 0.045, 0.05],
                    [
                        min_x + 0.36 + half_x * 0.46 * normalized_bytes,
                        y + 0.045,
                        0.075,
                    ],
                    window_with_alpha(color, 0.38 + normalized_bytes * 0.32),
                    WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
                );
            }
        }
    }

    pub fn add_window_generated_city(
        &mut self,
        chunks: &[WindowGeneratedCityChunkVisual],
        navigation_nodes: &[WindowGeneratedCityNavigationNodeVisual],
        navigation_edges: &[WindowGeneratedCityNavigationEdgeVisual],
        frame_index: u64,
    ) {
        for chunk in chunks {
            self.add_window_generated_city_chunk(*chunk, frame_index);
        }
        for edge in navigation_edges {
            self.add_window_generated_city_navigation_edge(*edge);
        }
        for node in navigation_nodes {
            self.add_window_generated_city_navigation_node(*node, frame_index);
        }
    }

    pub fn add_window_city_consequences(
        &mut self,
        cells: &[WindowCityPersistentCellVisual],
        danger_fields: &[WindowCityDangerFieldVisual],
        route_consequences: &[WindowCityRouteConsequenceVisual],
        frame_index: u64,
    ) {
        for cell in cells {
            self.add_window_city_persistent_cell(*cell, frame_index);
        }
        for danger in danger_fields {
            self.add_window_city_danger_field(*danger, frame_index);
        }
        for route in route_consequences {
            self.add_window_city_route_consequence(*route);
        }
    }

    pub fn add_window_city_material_placements(
        &mut self,
        placements: &[WindowCityMaterialPlacementVisual],
        frame_index: u64,
    ) {
        for placement in placements {
            self.add_window_city_material_placement(*placement, frame_index);
        }
        let decal_packet = window_decal_packet_from_city_material_placements(placements);
        self.add_window_decal_packet(&decal_packet);
        self.add_window_city_material_light_reflections(placements, frame_index);
    }

    pub fn add_window_decal_packet(&mut self, packet: &WindowDecalPacketVisual) {
        for decal in &packet.decals {
            self.add_window_surface_decal(*decal);
        }
    }

    fn add_window_surface_decal(&mut self, decal: WindowSurfaceDecalVisual) {
        if decal
            .center_meters
            .iter()
            .chain(decal.half_extents_meters.iter())
            .chain(decal.right_axis.iter())
            .chain(decal.up_axis.iter())
            .any(|value| !value.is_finite())
        {
            return;
        }

        let Some((right, up, normal)) = window_surface_decal_basis(decal) else {
            return;
        };
        let half_extents = [
            decal.half_extents_meters[0].abs().max(0.015),
            decal.half_extents_meters[1].abs().max(0.015),
        ];
        let state_detail = window_material_state_detail_from_decal(decal);
        let center = add3(decal.center_meters, scale3(normal, 0.007));
        self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
            center,
            right,
            up,
            half_extents,
            color: decal.color,
            surface_response: decal.surface_response,
            state_detail,
        });

        match decal.kind {
            WindowSurfaceDecalKind::CrackField => {
                self.add_window_crack_decal_strokes(decal, center, right, up, normal, state_detail);
            }
            WindowSurfaceDecalKind::WaterPuddle => {
                self.add_window_puddle_decal_glints(decal, center, right, up, normal, state_detail);
            }
            WindowSurfaceDecalKind::RoadMarking => {
                self.add_window_road_marking_decal_breaks(
                    decal,
                    center,
                    right,
                    up,
                    normal,
                    state_detail,
                );
            }
            WindowSurfaceDecalKind::Poster => {
                self.add_window_poster_decal_layers(decal, center, right, up, normal, state_detail);
            }
            WindowSurfaceDecalKind::Graffiti => {
                self.add_window_graffiti_decal_strokes(
                    decal,
                    center,
                    right,
                    up,
                    normal,
                    state_detail,
                );
            }
            WindowSurfaceDecalKind::Scorch
            | WindowSurfaceDecalKind::Corrosion
            | WindowSurfaceDecalKind::GrimeStain => {
                self.add_window_surface_decal_flecks(
                    decal,
                    center,
                    right,
                    up,
                    normal,
                    state_detail,
                );
            }
        }
    }

    fn add_window_crack_decal_strokes(
        &mut self,
        decal: WindowSurfaceDecalVisual,
        center: [f32; 3],
        right: [f32; 3],
        up: [f32; 3],
        normal: [f32; 3],
        state_detail: WindowMaterialStateDetail,
    ) {
        let stroke_count = (2.0 + decal.state_intensity * 5.0).ceil().clamp(2.0, 7.0) as usize;
        for index in 0..stroke_count {
            let side = if index.is_multiple_of(2) { 1.0 } else { -1.0 };
            let angle = (window_stable_unit(decal.detail_seed, 4_101 + index as u64) - 0.5) * 0.72;
            let branch_right = normalize3(add3(
                scale3(right, angle.cos()),
                scale3(up, angle.sin() * side),
            ));
            let branch_up = normalize3(cross3(normal, branch_right));
            let offset_right = (window_stable_unit(decal.detail_seed, 4_201 + index as u64) - 0.5)
                * decal.half_extents_meters[0]
                * 0.92;
            let offset_up = (window_stable_unit(decal.detail_seed, 4_301 + index as u64) - 0.5)
                * decal.half_extents_meters[1]
                * 0.78;
            let stroke_center = window_decal_plane_point(
                center,
                right,
                up,
                normal,
                offset_right,
                offset_up,
                0.004 + index as f32 * 0.0007,
            );
            let length = decal.half_extents_meters[0]
                * (0.14 + window_stable_unit(decal.detail_seed, 4_401 + index as u64) * 0.36);
            let width = 0.004 + decal.relief * 0.008 + index as f32 * 0.00035;
            self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                center: stroke_center,
                right: branch_right,
                up: branch_up,
                half_extents: [length.clamp(0.045, 0.72), width.clamp(0.003, 0.018)],
                color: [
                    0.82,
                    0.94,
                    1.0,
                    (0.24 + decal.state_intensity * 0.38).clamp(0.0, 0.7),
                ],
                surface_response: WINDOW_SURFACE_RESPONSE_GLASS,
                state_detail,
            });
        }
    }

    fn add_window_puddle_decal_glints(
        &mut self,
        decal: WindowSurfaceDecalVisual,
        center: [f32; 3],
        right: [f32; 3],
        up: [f32; 3],
        normal: [f32; 3],
        state_detail: WindowMaterialStateDetail,
    ) {
        let glint_count = (1.0 + decal.state_intensity * 3.0).ceil().clamp(1.0, 4.0) as usize;
        for index in 0..glint_count {
            let offset_right = (window_stable_unit(decal.detail_seed, 4_501 + index as u64) - 0.5)
                * decal.half_extents_meters[0]
                * 1.05;
            let offset_up = (window_stable_unit(decal.detail_seed, 4_601 + index as u64) - 0.5)
                * decal.half_extents_meters[1]
                * 0.82;
            let angle = (window_stable_unit(decal.detail_seed, 4_701 + index as u64) - 0.5) * 0.42;
            let glint_right = normalize3(add3(scale3(right, angle.cos()), scale3(up, angle.sin())));
            let glint_up = normalize3(cross3(normal, glint_right));
            self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                center: window_decal_plane_point(
                    center,
                    right,
                    up,
                    normal,
                    offset_right,
                    offset_up,
                    0.004 + index as f32 * 0.0005,
                ),
                right: glint_right,
                up: glint_up,
                half_extents: [
                    (decal.half_extents_meters[0] * 0.18).clamp(0.045, 0.38),
                    0.004 + decal.state_intensity * 0.005,
                ],
                color: [
                    0.62,
                    0.88,
                    1.0,
                    (0.16 + decal.state_intensity * 0.24).min(0.52),
                ],
                surface_response: WINDOW_SURFACE_RESPONSE_WET_ROAD,
                state_detail,
            });
        }
    }

    fn add_window_road_marking_decal_breaks(
        &mut self,
        decal: WindowSurfaceDecalVisual,
        center: [f32; 3],
        right: [f32; 3],
        up: [f32; 3],
        normal: [f32; 3],
        state_detail: WindowMaterialStateDetail,
    ) {
        let dash_count = 3 + (decal.state_intensity * 3.0).round() as usize;
        for index in 0..dash_count {
            let t = (index as f32 + 0.5) / dash_count as f32 - 0.5;
            let chipped = window_stable_unit(decal.detail_seed, 4_801 + index as u64);
            if chipped < 0.18 {
                continue;
            }
            self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                center: window_decal_plane_point(
                    center,
                    right,
                    up,
                    normal,
                    t * decal.half_extents_meters[0] * 1.55,
                    (chipped - 0.5) * decal.half_extents_meters[1] * 0.35,
                    0.004 + index as f32 * 0.0004,
                ),
                right,
                up,
                half_extents: [
                    (decal.half_extents_meters[0] / dash_count as f32 * 0.42).clamp(0.035, 0.22),
                    decal.half_extents_meters[1] * (0.42 + chipped * 0.18),
                ],
                color: window_with_alpha(
                    window_scale_color(decal.color, 0.72 + chipped * 0.26),
                    (decal.color[3] * (0.48 + chipped * 0.28)).clamp(0.0, 0.8),
                ),
                surface_response: decal.surface_response,
                state_detail,
            });
        }
    }

    fn add_window_poster_decal_layers(
        &mut self,
        decal: WindowSurfaceDecalVisual,
        center: [f32; 3],
        right: [f32; 3],
        up: [f32; 3],
        normal: [f32; 3],
        state_detail: WindowMaterialStateDetail,
    ) {
        let strip_color = window_with_alpha(
            window_mix_color(decal.color, [0.96, 0.88, 0.62, decal.color[3]], 0.34),
            (decal.color[3] * 0.72).clamp(0.0, 0.78),
        );
        for offset_up in [-0.62_f32, 0.62] {
            self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                center: window_decal_plane_point(
                    center,
                    right,
                    up,
                    normal,
                    0.0,
                    offset_up * decal.half_extents_meters[1],
                    0.004,
                ),
                right,
                up,
                half_extents: [
                    decal.half_extents_meters[0] * 0.82,
                    decal.half_extents_meters[1] * 0.08,
                ],
                color: strip_color,
                surface_response: decal.surface_response,
                state_detail,
            });
        }

        let tear_count = (1.0 + decal.relief * 4.0).ceil().clamp(1.0, 5.0) as usize;
        for index in 0..tear_count {
            let offset_right = (window_stable_unit(decal.detail_seed, 4_901 + index as u64) - 0.5)
                * decal.half_extents_meters[0]
                * 1.35;
            let length = decal.half_extents_meters[1]
                * (0.16 + window_stable_unit(decal.detail_seed, 5_001 + index as u64) * 0.32);
            self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                center: window_decal_plane_point(
                    center,
                    right,
                    up,
                    normal,
                    offset_right,
                    -decal.half_extents_meters[1] * 0.18,
                    0.006 + index as f32 * 0.0005,
                ),
                right: up,
                up: right,
                half_extents: [length.clamp(0.035, 0.28), 0.006 + decal.relief * 0.006],
                color: [0.05, 0.042, 0.034, 0.28 + decal.relief * 0.22],
                surface_response: WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                state_detail,
            });
        }
    }

    fn add_window_graffiti_decal_strokes(
        &mut self,
        decal: WindowSurfaceDecalVisual,
        center: [f32; 3],
        right: [f32; 3],
        up: [f32; 3],
        normal: [f32; 3],
        state_detail: WindowMaterialStateDetail,
    ) {
        let stroke_count = (3.0 + decal.state_intensity * 3.0).ceil().clamp(3.0, 6.0) as usize;
        for index in 0..stroke_count {
            let phase = index as f32 / stroke_count as f32;
            let stroke_color = window_with_alpha(
                window_mix_color(
                    decal.color,
                    [
                        1.0 - phase * 0.38,
                        0.24 + phase * 0.58,
                        0.92,
                        decal.color[3],
                    ],
                    0.44,
                ),
                (0.2 + decal.color[3] * 0.58).clamp(0.0, 0.72),
            );
            let angle = (phase - 0.5) * 0.82
                + (window_stable_unit(decal.detail_seed, 5_101 + index as u64) - 0.5) * 0.28;
            let stroke_right =
                normalize3(add3(scale3(right, angle.cos()), scale3(up, angle.sin())));
            let stroke_up = normalize3(cross3(normal, stroke_right));
            let offset_right = (phase - 0.5) * decal.half_extents_meters[0] * 1.15;
            let offset_up = (window_stable_unit(decal.detail_seed, 5_201 + index as u64) - 0.5)
                * decal.half_extents_meters[1]
                * 0.7;
            self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                center: window_decal_plane_point(
                    center,
                    right,
                    up,
                    normal,
                    offset_right,
                    offset_up,
                    0.004 + index as f32 * 0.0006,
                ),
                right: stroke_right,
                up: stroke_up,
                half_extents: [
                    decal.half_extents_meters[0] * (0.18 + phase * 0.08),
                    0.012 + decal.relief * 0.012,
                ],
                color: stroke_color,
                surface_response: decal.surface_response,
                state_detail,
            });
        }
    }

    fn add_window_surface_decal_flecks(
        &mut self,
        decal: WindowSurfaceDecalVisual,
        center: [f32; 3],
        right: [f32; 3],
        up: [f32; 3],
        normal: [f32; 3],
        state_detail: WindowMaterialStateDetail,
    ) {
        let fleck_count = (2.0 + decal.state_intensity * 5.0 + decal.relief * 2.0)
            .ceil()
            .clamp(2.0, 8.0) as usize;
        for index in 0..fleck_count {
            let offset_right = (window_stable_unit(decal.detail_seed, 5_301 + index as u64) - 0.5)
                * decal.half_extents_meters[0]
                * 1.42;
            let offset_up = (window_stable_unit(decal.detail_seed, 5_401 + index as u64) - 0.5)
                * decal.half_extents_meters[1]
                * 1.28;
            let angle =
                window_stable_unit(decal.detail_seed, 5_501 + index as u64) * std::f32::consts::TAU;
            let fleck_right = normalize3(add3(scale3(right, angle.cos()), scale3(up, angle.sin())));
            let fleck_up = normalize3(cross3(normal, fleck_right));
            let scale = 0.38 + window_stable_unit(decal.detail_seed, 5_601 + index as u64) * 0.76;
            self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                center: window_decal_plane_point(
                    center,
                    right,
                    up,
                    normal,
                    offset_right,
                    offset_up,
                    0.004 + index as f32 * 0.00045,
                ),
                right: fleck_right,
                up: fleck_up,
                half_extents: [
                    (decal.half_extents_meters[0] * 0.08 * scale).clamp(0.018, 0.22),
                    (decal.half_extents_meters[1] * 0.04 * scale).clamp(0.008, 0.12),
                ],
                color: window_with_alpha(
                    window_scale_color(decal.color, 0.72 + scale * 0.18),
                    (decal.color[3] * (0.38 + scale * 0.18)).clamp(0.0, 0.68),
                ),
                surface_response: decal.surface_response,
                state_detail,
            });
        }
    }

    fn add_window_city_material_placement(
        &mut self,
        placement: WindowCityMaterialPlacementVisual,
        frame_index: u64,
    ) {
        if placement
            .center_meters
            .iter()
            .chain(placement.half_extents_meters.iter())
            .any(|value| !value.is_finite())
        {
            return;
        }

        let pulse = (frame_index as f32 * 0.043
            + window_stable_unit(placement.detail_seed, 811) * std::f32::consts::TAU)
            .sin()
            .mul_add(0.5, 0.5);
        let color = window_city_material_state_color(placement, pulse);
        let surface = window_city_material_state_surface_response(placement);

        match placement.kind {
            WindowCityMaterialPlacementKind::WetRoad => {
                self.add_window_city_material_wet_road(placement, color, surface, pulse);
            }
            WindowCityMaterialPlacementKind::Glass => {
                self.add_window_city_material_glass(placement, color, pulse);
            }
            WindowCityMaterialPlacementKind::Neon => {
                self.add_window_city_material_neon(placement, color, pulse);
            }
            WindowCityMaterialPlacementKind::Water => {
                self.add_window_city_material_water(placement, color, surface, pulse);
            }
            WindowCityMaterialPlacementKind::HumanSkin => {
                self.add_window_city_material_human_skin(placement, color, surface, pulse);
            }
            WindowCityMaterialPlacementKind::Metal | WindowCityMaterialPlacementKind::Generic => {
                self.add_window_city_material_low_detail_surface(placement, color, surface);
            }
        }
        if window_city_material_should_spend_micro_detail(placement) {
            self.add_window_city_material_micro_detail(placement, pulse);
        }
    }

    fn add_window_city_material_light_reflections(
        &mut self,
        placements: &[WindowCityMaterialPlacementVisual],
        frame_index: u64,
    ) {
        let neon_sources = placements
            .iter()
            .copied()
            .filter(|placement| {
                matches!(placement.kind, WindowCityMaterialPlacementKind::Neon)
                    && placement.surface_response[3] > 0.08
            })
            .take(6)
            .collect::<Vec<_>>();
        if neon_sources.is_empty() {
            return;
        }

        for neon in neon_sources {
            let pulse = (frame_index as f32 * 0.039
                + window_stable_unit(neon.detail_seed, 1231) * std::f32::consts::TAU)
                .sin()
                .mul_add(0.5, 0.5);
            let mut linked_reflections = 0;
            for reflector in placements.iter().copied().filter(|placement| {
                matches!(
                    placement.kind,
                    WindowCityMaterialPlacementKind::WetRoad
                        | WindowCityMaterialPlacementKind::Water
                        | WindowCityMaterialPlacementKind::Glass
                )
            }) {
                if linked_reflections >= 3 {
                    break;
                }
                let dx = reflector.center_meters[0] - neon.center_meters[0];
                let dy = reflector.center_meters[1] - neon.center_meters[1];
                let distance = (dx * dx + dy * dy).sqrt();
                let max_distance = 7.5 + neon.importance * 4.0 + reflector.wetness * 2.0;
                if distance > max_distance || distance <= f32::EPSILON {
                    continue;
                }

                let attenuation = 1.0 - (distance / max_distance).clamp(0.0, 1.0);
                let reflector_gain = match reflector.kind {
                    WindowCityMaterialPlacementKind::Glass => 0.38 + reflector.wetness * 0.18,
                    WindowCityMaterialPlacementKind::Water => 0.62,
                    WindowCityMaterialPlacementKind::WetRoad => 0.32 + reflector.wetness * 0.32,
                    _ => 0.0,
                };
                let alpha = (attenuation
                    * reflector_gain
                    * (0.34 + neon.importance * 0.28)
                    * (0.82 + pulse * 0.18))
                    .clamp(0.0, 0.56);
                if alpha <= 0.035 {
                    continue;
                }

                self.add_window_city_material_light_reflection(neon, reflector, alpha, pulse);
                linked_reflections += 1;
            }
        }
    }

    fn add_window_city_material_light_reflection(
        &mut self,
        neon: WindowCityMaterialPlacementVisual,
        reflector: WindowCityMaterialPlacementVisual,
        alpha: f32,
        pulse: f32,
    ) {
        let color = window_with_alpha(
            [
                (neon.base_color[0] * 0.72 + neon.faction_accent_color[0] * 0.28).clamp(0.0, 1.0),
                (neon.base_color[1] * 0.72 + neon.faction_accent_color[1] * 0.28).clamp(0.0, 1.0),
                (neon.base_color[2] * 0.72 + neon.faction_accent_color[2] * 0.28).clamp(0.0, 1.0),
                1.0,
            ],
            alpha,
        );
        let z = reflector.center_meters[2].max(0.028) + 0.064;
        let reflected_half_x = reflector.half_extents_meters[0] * (0.42 + pulse * 0.08);
        let reflected_half_y = reflector.half_extents_meters[1] * (0.26 + pulse * 0.06);

        self.world_ellipse_ring_with_surface_response(
            [reflector.center_meters[0], reflector.center_meters[1], z],
            [reflected_half_x * 0.24, reflected_half_y * 0.28],
            [
                reflected_half_x.clamp(0.18, 4.5),
                reflected_half_y.clamp(0.08, 1.6),
            ],
            18,
            color,
            WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
        );

        if matches!(
            reflector.kind,
            WindowCityMaterialPlacementKind::WetRoad | WindowCityMaterialPlacementKind::Water
        ) {
            self.world_cylinder_between_with_surface_response(
                [
                    neon.center_meters[0],
                    neon.center_meters[1],
                    neon.center_meters[2].max(0.24),
                ],
                [
                    reflector.center_meters[0],
                    reflector.center_meters[1],
                    z + 0.02,
                ],
                0.012 + alpha * 0.018,
                6,
                window_with_alpha(color, alpha * 0.42),
                WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
            );
        }
    }

    fn add_window_city_material_wet_road(
        &mut self,
        placement: WindowCityMaterialPlacementVisual,
        color: [f32; 4],
        surface: [f32; 4],
        pulse: f32,
    ) {
        let [x, y, z] = placement.center_meters;
        let [half_x, half_y] = placement.half_extents_meters;
        let z0 = z.max(0.018);
        self.world_box_with_surface_response(
            [x - half_x, y - half_y, z0],
            [x + half_x, y + half_y, z0 + 0.028],
            color,
            surface,
        );

        let wear_alpha = (0.18 + placement.traffic_wear * 0.34).clamp(0.0, 0.64);
        for index in 0..3 {
            let offset = (index as f32 - 1.0) * half_y * 0.48;
            self.world_box_with_surface_response(
                [x - half_x * 0.86, y + offset - 0.026, z0 + 0.031],
                [x + half_x * 0.86, y + offset + 0.026, z0 + 0.048],
                [0.08, 0.13, 0.15, wear_alpha],
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            );
        }

        if placement.wetness > 0.12 {
            let puddle_alpha = (0.18 + placement.wetness * 0.28 + pulse * 0.08).clamp(0.0, 0.58);
            self.world_ellipse_ring_with_surface_response(
                [x, y, z0 + 0.052],
                [half_x * 0.16, half_y * 0.12],
                [half_x * (0.38 + placement.wetness * 0.18), half_y * 0.34],
                18,
                [0.1, 0.24, 0.3, puddle_alpha],
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            );
        }

        self.add_window_city_material_faction_accent(placement, z0 + 0.052);
        self.add_window_city_material_damage_marks(placement, z0 + 0.062);
    }

    fn add_window_city_material_glass(
        &mut self,
        placement: WindowCityMaterialPlacementVisual,
        color: [f32; 4],
        pulse: f32,
    ) {
        let [x, y, z] = placement.center_meters;
        let [half_x, half_y] = placement.half_extents_meters;
        let panel_width = half_x.max(0.42);
        let panel_height = half_y.max(0.52);
        let center_z = (z + panel_height).max(0.84);
        let pane_color = window_with_alpha(color, (0.34 + placement.wetness * 0.16).max(color[3]));

        self.world_oriented_rect_with_surface_response(
            [x, y, center_z],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [panel_width, panel_height],
            pane_color,
            WINDOW_SURFACE_RESPONSE_GLASS,
        );

        let frame_color = [0.11, 0.14, 0.16, 0.82];
        for z_edge in [center_z - panel_height, center_z + panel_height] {
            self.world_cylinder_between_with_surface_response(
                [x - panel_width, y, z_edge],
                [x + panel_width, y, z_edge],
                0.018,
                6,
                frame_color,
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        }
        for x_edge in [x - panel_width, x + panel_width] {
            self.world_cylinder_between_with_surface_response(
                [x_edge, y, center_z - panel_height],
                [x_edge, y, center_z + panel_height],
                0.018,
                6,
                frame_color,
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        }

        if placement.damage > 0.05 {
            let crack_count = (placement.damage * 4.0).ceil().clamp(1.0, 4.0) as usize;
            for index in 0..crack_count {
                let t = (index as f32 + 0.5) / crack_count as f32;
                let x0 = x - panel_width * (0.82 - t * 0.16);
                let x1 = x + panel_width * (0.28 + t * 0.34);
                let z0 = center_z - panel_height * (0.72 - t * 0.32);
                let z1 = center_z + panel_height * (0.18 + t * 0.46);
                self.world_cylinder_between_with_surface_response(
                    [x0, y + 0.012, z0],
                    [x1, y + 0.012, z1],
                    0.008 + placement.damage * 0.006,
                    5,
                    [0.9, 0.98, 1.0, 0.38 + placement.damage * 0.24],
                    WINDOW_SURFACE_RESPONSE_GLASS,
                );
            }
        }

        if placement.faction_influence > 0.01 {
            self.world_oriented_rect_with_surface_response(
                [x, y + 0.014, center_z + panel_height * 0.62],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [panel_width * 0.54, 0.035],
                window_with_alpha(
                    placement.faction_accent_color,
                    0.18 + placement.faction_influence * 0.42 + pulse * 0.06,
                ),
                WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
            );
        }
    }

    fn add_window_city_material_neon(
        &mut self,
        placement: WindowCityMaterialPlacementVisual,
        color: [f32; 4],
        pulse: f32,
    ) {
        let [x, y, z] = placement.center_meters;
        let [half_x, half_y] = placement.half_extents_meters;
        let sign_z = z.max(0.44);
        let half_width = half_x.max(0.36);
        let half_height = half_y.max(0.08);
        let glow = window_with_alpha(
            window_mix_color(color, [0.72, 0.68, 0.58, color[3]], 0.18),
            (0.34 + pulse * 0.12 + placement.importance * 0.1).min(0.72),
        );

        self.world_oriented_rect_with_surface_response(
            [x, y, sign_z],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [half_width, half_height],
            glow,
            WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
        );
        self.world_cylinder_between_with_surface_response(
            [x - half_width, y + 0.018, sign_z],
            [x + half_width, y + 0.018, sign_z],
            0.025 + placement.importance * 0.012,
            7,
            glow,
            WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
        );

        if placement.damage > 0.05 {
            let spark_x = x + half_width * (window_stable_unit(placement.detail_seed, 912) - 0.5);
            self.world_cylinder_between_with_surface_response(
                [spark_x, y + 0.02, sign_z - half_height * 0.8],
                [spark_x + 0.18, y + 0.02, sign_z + half_height * 0.8],
                0.01,
                5,
                [1.0, 0.82, 0.24, 0.4 + placement.damage * 0.42],
                WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
            );
        }

        self.world_ellipse_ring_with_surface_response(
            [x, y, 0.07],
            [half_width * 0.18, 0.08],
            [half_width * 0.76, 0.28 + pulse * 0.06],
            10,
            window_with_alpha(glow, 0.055 + placement.importance * 0.08),
            WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
        );
    }

    fn add_window_city_material_water(
        &mut self,
        placement: WindowCityMaterialPlacementVisual,
        color: [f32; 4],
        surface: [f32; 4],
        pulse: f32,
    ) {
        let [x, y, z] = placement.center_meters;
        let [half_x, half_y] = placement.half_extents_meters;
        let z0 = z.max(0.032);
        self.world_box_with_surface_response(
            [x - half_x * 0.82, y - half_y * 0.62, z0],
            [x + half_x * 0.82, y + half_y * 0.62, z0 + 0.018],
            color,
            surface,
        );
        self.world_ellipse_ring_with_surface_response(
            [x, y, z0 + 0.024],
            [half_x * 0.22, half_y * 0.16],
            [
                half_x * (0.72 + pulse * 0.08),
                half_y * (0.52 + pulse * 0.06),
            ],
            20,
            window_with_alpha(color, 0.2 + placement.wetness * 0.28 + pulse * 0.06),
            WINDOW_SURFACE_RESPONSE_WET_ROAD,
        );

        if placement.importance > 0.05 {
            let plume_height = 0.44 + placement.importance * 0.64 + pulse * 0.14;
            self.world_cylinder_between_with_surface_response(
                [x, y, z0 + 0.04],
                [x, y + 0.12, z0 + plume_height],
                0.018 + placement.importance * 0.012,
                8,
                [0.42, 0.8, 0.92, 0.22 + placement.importance * 0.24],
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            );
        }

        self.add_window_city_material_faction_accent(placement, z0 + 0.035);
    }

    fn add_window_city_material_human_skin(
        &mut self,
        placement: WindowCityMaterialPlacementVisual,
        color: [f32; 4],
        surface: [f32; 4],
        pulse: f32,
    ) {
        let [x, y, z] = placement.center_meters;
        let [half_x, half_y] = placement.half_extents_meters;
        let base_z = z.max(0.08);
        self.world_ellipsoid_with_surface_response(
            [x, y, base_z + 0.42],
            [half_x * 0.34, half_y * 0.24, 0.42],
            5,
            10,
            color,
            surface,
        );
        self.world_ellipsoid_with_surface_response(
            [x, y, base_z + 1.02],
            [half_x * 0.25, half_y * 0.2, 0.17],
            5,
            10,
            window_scale_color(color, 1.08 + pulse * 0.04),
            surface,
        );

        if placement.wetness > 0.05 || placement.pollution > 0.05 {
            self.world_oriented_rect_with_surface_response(
                [x, y - half_y * 0.25, base_z + 0.95],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [half_x * 0.18, 0.035],
                [0.42, 0.8, 0.9, 0.16 + placement.wetness * 0.22],
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            );
        }

        self.add_window_city_material_faction_accent(placement, base_z + 0.06);
    }

    fn add_window_city_material_low_detail_surface(
        &mut self,
        placement: WindowCityMaterialPlacementVisual,
        color: [f32; 4],
        surface: [f32; 4],
    ) {
        let [x, y, z] = placement.center_meters;
        let [half_x, half_y] = placement.half_extents_meters;
        let z0 = z.max(0.05);
        self.world_micro_detailed_box_with_surface_response(
            [x - half_x, y - half_y, z0],
            [
                x + half_x,
                y + half_y,
                z0 + 0.11 + placement.importance * 0.22,
            ],
            color,
            surface,
            placement.detail_seed ^ 0x51A7_EE11,
            (0.36
                + placement.damage * 0.26
                + placement.pollution * 0.22
                + placement.importance * 0.18)
                .clamp(0.0, 1.0),
        );
        self.add_window_city_material_faction_accent(placement, z0 + 0.13);
        self.add_window_city_material_damage_marks(placement, z0 + 0.14);
    }

    fn add_window_city_material_faction_accent(
        &mut self,
        placement: WindowCityMaterialPlacementVisual,
        z: f32,
    ) {
        if placement.faction_influence <= 0.01 {
            return;
        }

        let [x, y, _] = placement.center_meters;
        let [half_x, half_y] = placement.half_extents_meters;
        let alpha = (0.16 + placement.faction_influence * 0.42 + placement.importance * 0.12)
            .clamp(0.0, 0.82);
        self.world_box_with_surface_response(
            [x - half_x * 0.78, y + half_y * 0.72 - 0.035, z],
            [x + half_x * 0.78, y + half_y * 0.72 + 0.035, z + 0.04],
            window_with_alpha(placement.faction_accent_color, alpha),
            WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
        );
    }

    fn add_window_city_material_damage_marks(
        &mut self,
        placement: WindowCityMaterialPlacementVisual,
        z: f32,
    ) {
        if placement.damage <= 0.03 {
            return;
        }

        let [x, y, _] = placement.center_meters;
        let [half_x, half_y] = placement.half_extents_meters;
        let mark_count = (placement.damage * 4.0).ceil().clamp(1.0, 4.0) as usize;
        for index in 0..mark_count {
            let t = (index as f32 + 0.5) / mark_count as f32;
            let jitter = window_stable_unit(placement.detail_seed, 960 + index as u64) - 0.5;
            let y0 = y - half_y * (0.55 - t * 0.35) + jitter * half_y * 0.16;
            let y1 = y + half_y * (0.32 + t * 0.28) - jitter * half_y * 0.12;
            self.world_cylinder_between_with_surface_response(
                [x - half_x * (0.72 - t * 0.18), y0, z],
                [x + half_x * (0.24 + t * 0.32), y1, z + 0.012],
                0.012 + placement.damage * 0.012,
                5,
                [1.0, 0.38, 0.16, 0.22 + placement.damage * 0.38],
                WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
            );
        }
    }

    fn add_window_city_material_micro_detail(
        &mut self,
        placement: WindowCityMaterialPlacementVisual,
        pulse: f32,
    ) {
        match placement.kind {
            WindowCityMaterialPlacementKind::WetRoad => {
                self.add_window_city_material_wet_road_micro_detail(placement, pulse);
            }
            WindowCityMaterialPlacementKind::Glass => {
                self.add_window_city_material_glass_micro_detail(placement);
            }
            WindowCityMaterialPlacementKind::Neon => {
                self.add_window_city_material_neon_micro_detail(placement, pulse);
            }
            WindowCityMaterialPlacementKind::Water => {
                self.add_window_city_material_water_micro_detail(placement, pulse);
            }
            WindowCityMaterialPlacementKind::HumanSkin => {
                self.add_window_city_material_human_skin_micro_detail(placement);
            }
            WindowCityMaterialPlacementKind::Metal | WindowCityMaterialPlacementKind::Generic => {
                self.add_window_city_material_hard_surface_micro_detail(placement);
            }
        }
    }

    fn add_window_city_material_wet_road_micro_detail(
        &mut self,
        placement: WindowCityMaterialPlacementVisual,
        pulse: f32,
    ) {
        let [x, y, z] = placement.center_meters;
        let [half_x, half_y] = placement.half_extents_meters;
        let z0 = z.max(0.018) + 0.058;
        let scratch_count =
            (2.0 + placement.traffic_wear * 5.0 + placement.pollution * 2.0 + placement.damage)
                .ceil()
                .clamp(2.0, 9.0) as usize;
        for index in 0..scratch_count {
            let rx = x
                + (window_stable_unit(placement.detail_seed, 1_301 + index as u64) - 0.5)
                    * half_x
                    * 1.62;
            let ry = y
                + (window_stable_unit(placement.detail_seed, 1_401 + index as u64) - 0.5)
                    * half_y
                    * 1.68;
            let angle =
                (window_stable_unit(placement.detail_seed, 1_501 + index as u64) - 0.5) * 0.72;
            let length = half_x
                * (0.08 + window_stable_unit(placement.detail_seed, 1_601 + index as u64) * 0.18);
            self.world_oriented_rect_with_surface_response(
                [rx, ry, z0 + index as f32 * 0.0005],
                [angle.cos(), angle.sin(), 0.0],
                [-angle.sin(), angle.cos(), 0.0],
                [
                    length.clamp(0.08, 0.56),
                    0.005 + placement.traffic_wear * 0.006,
                ],
                [
                    0.018,
                    0.024,
                    0.028,
                    (0.52 + placement.pollution * 0.32).min(0.92),
                ],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        if placement.wetness > 0.24 {
            let ring_count = (placement.wetness * 3.0).ceil().clamp(1.0, 4.0) as usize;
            for index in 0..ring_count {
                let rx = x
                    + (window_stable_unit(placement.detail_seed, 1_701 + index as u64) - 0.5)
                        * half_x
                        * 1.22;
                let ry = y
                    + (window_stable_unit(placement.detail_seed, 1_801 + index as u64) - 0.5)
                        * half_y
                        * 1.14;
                let outer_x = half_x
                    * (0.08
                        + window_stable_unit(placement.detail_seed, 1_901 + index as u64) * 0.16)
                    * (0.88 + pulse * 0.18);
                let outer_y = half_y
                    * (0.07
                        + window_stable_unit(placement.detail_seed, 2_001 + index as u64) * 0.12)
                    * (0.9 + pulse * 0.14);
                self.world_ellipse_ring_with_surface_response(
                    [rx, ry, z0 + 0.006 + index as f32 * 0.0007],
                    [outer_x * 0.42, outer_y * 0.42],
                    [outer_x.clamp(0.08, 0.48), outer_y.clamp(0.04, 0.24)],
                    14,
                    [0.42, 0.82, 1.0, 0.12 + placement.wetness * 0.18],
                    WINDOW_SURFACE_RESPONSE_WET_ROAD,
                );
            }
        }
    }

    fn add_window_city_material_glass_micro_detail(
        &mut self,
        placement: WindowCityMaterialPlacementVisual,
    ) {
        let [x, y, z] = placement.center_meters;
        let [half_x, half_y] = placement.half_extents_meters;
        let panel_width = half_x.max(0.42);
        let panel_height = half_y.max(0.52);
        let center_z = (z + panel_height).max(0.84);
        let streak_count =
            (2.0 + placement.wetness * 5.0 + placement.pollution * 2.0 + placement.damage * 2.0)
                .ceil()
                .clamp(2.0, 9.0) as usize;

        for index in 0..streak_count {
            let sx = x
                + (window_stable_unit(placement.detail_seed, 2_101 + index as u64) - 0.5)
                    * panel_width
                    * 1.72;
            let sz = center_z
                + (window_stable_unit(placement.detail_seed, 2_201 + index as u64) - 0.5)
                    * panel_height
                    * 1.28;
            let length = panel_height
                * (0.10
                    + window_stable_unit(placement.detail_seed, 2_301 + index as u64) * 0.24
                    + placement.wetness * 0.12);
            self.world_oriented_rect_with_surface_response(
                [sx, y + 0.018, sz],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [
                    0.004 + placement.damage * 0.004,
                    length.clamp(0.06, panel_height * 0.42),
                ],
                [
                    0.72,
                    0.92,
                    1.0,
                    (0.12 + placement.wetness * 0.24 + placement.pollution * 0.08).min(0.48),
                ],
                WINDOW_SURFACE_RESPONSE_GLASS,
            );
        }
    }

    fn add_window_city_material_neon_micro_detail(
        &mut self,
        placement: WindowCityMaterialPlacementVisual,
        pulse: f32,
    ) {
        if placement.importance <= 0.02 && placement.damage <= 0.03 {
            return;
        }

        let [x, y, z] = placement.center_meters;
        let [half_x, half_y] = placement.half_extents_meters;
        let sign_z = z.max(0.44);
        let half_width = half_x.max(0.36);
        let half_height = half_y.max(0.08);
        let spark_count = (1.0 + placement.importance * 2.0 + placement.damage * 3.0)
            .ceil()
            .clamp(1.0, 5.0) as usize;

        for index in 0..spark_count {
            let sx = x
                + (window_stable_unit(placement.detail_seed, 2_401 + index as u64) - 0.5)
                    * half_width
                    * 1.74;
            let sz = sign_z
                + (window_stable_unit(placement.detail_seed, 2_501 + index as u64) - 0.5)
                    * half_height
                    * 1.48;
            let angle =
                (window_stable_unit(placement.detail_seed, 2_601 + index as u64) - 0.5) * 0.9;
            self.world_oriented_rect_with_surface_response(
                [sx, y + 0.026, sz],
                [angle.cos(), 0.0, angle.sin()],
                [-angle.sin(), 0.0, angle.cos()],
                [
                    0.035 + placement.importance * 0.018,
                    0.004 + placement.damage * 0.004,
                ],
                [
                    1.0,
                    0.92,
                    0.38,
                    (0.18 + placement.damage * 0.42 + pulse * 0.12).min(0.68),
                ],
                WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
            );
        }
    }

    fn add_window_city_material_water_micro_detail(
        &mut self,
        placement: WindowCityMaterialPlacementVisual,
        pulse: f32,
    ) {
        let [x, y, z] = placement.center_meters;
        let [half_x, half_y] = placement.half_extents_meters;
        let z0 = z.max(0.032) + 0.035;
        let ripple_count = (2.0 + placement.wetness * 3.0 + placement.pollution * 2.0)
            .ceil()
            .clamp(2.0, 7.0) as usize;

        for index in 0..ripple_count {
            let rx = x
                + (window_stable_unit(placement.detail_seed, 2_701 + index as u64) - 0.5)
                    * half_x
                    * 1.04;
            let ry = y
                + (window_stable_unit(placement.detail_seed, 2_801 + index as u64) - 0.5)
                    * half_y
                    * 0.74;
            let outer_x = half_x
                * (0.09 + window_stable_unit(placement.detail_seed, 2_901 + index as u64) * 0.18)
                * (0.88 + pulse * 0.2);
            let outer_y = half_y
                * (0.08 + window_stable_unit(placement.detail_seed, 3_001 + index as u64) * 0.16)
                * (0.88 + pulse * 0.18);
            self.world_ellipse_ring_with_surface_response(
                [rx, ry, z0 + index as f32 * 0.0007],
                [outer_x * 0.46, outer_y * 0.46],
                [outer_x.clamp(0.08, 0.62), outer_y.clamp(0.04, 0.34)],
                14,
                [0.34, 0.78, 0.92, 0.12 + placement.wetness * 0.16],
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            );
        }

        if placement.pollution > 0.08 {
            let stain_count = (1.0 + placement.pollution * 3.0).ceil().clamp(1.0, 4.0) as usize;
            for index in 0..stain_count {
                let rx = x
                    + (window_stable_unit(placement.detail_seed, 3_101 + index as u64) - 0.5)
                        * half_x
                        * 0.9;
                let ry = y
                    + (window_stable_unit(placement.detail_seed, 3_201 + index as u64) - 0.5)
                        * half_y
                        * 0.58;
                let angle = window_stable_unit(placement.detail_seed, 3_301 + index as u64)
                    * std::f32::consts::TAU;
                self.world_oriented_rect_with_surface_response(
                    [rx, ry, z0 + 0.01 + index as f32 * 0.0007],
                    [angle.cos(), angle.sin(), 0.0],
                    [-angle.sin(), angle.cos(), 0.0],
                    [half_x * 0.08, 0.008 + placement.pollution * 0.008],
                    [0.12, 0.18, 0.08, 0.18 + placement.pollution * 0.24],
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                );
            }
        }
    }

    fn add_window_city_material_human_skin_micro_detail(
        &mut self,
        placement: WindowCityMaterialPlacementVisual,
    ) {
        let [x, y, z] = placement.center_meters;
        let [half_x, half_y] = placement.half_extents_meters;
        let base_z = z.max(0.08);
        let detail_count = (2.0 + placement.wetness * 3.0 + placement.pollution * 3.0)
            .ceil()
            .clamp(2.0, 7.0) as usize;

        for index in 0..detail_count {
            let sx = x
                + (window_stable_unit(placement.detail_seed, 3_401 + index as u64) - 0.5)
                    * half_x
                    * 0.42;
            let sz = base_z
                + 0.24
                + window_stable_unit(placement.detail_seed, 3_501 + index as u64) * 0.86;
            let color = if index % 2 == 0 {
                [0.82, 0.92, 1.0, (0.08 + placement.wetness * 0.22).min(0.32)]
            } else {
                [
                    0.12,
                    0.08,
                    0.06,
                    (0.08 + placement.pollution * 0.2).min(0.32),
                ]
            };
            let response = if index % 2 == 0 {
                WINDOW_SURFACE_RESPONSE_WET_ROAD
            } else {
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
            };
            self.world_oriented_rect_with_surface_response(
                [sx, y - half_y * 0.252, sz],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.012 + half_x * 0.018, 0.006 + half_y * 0.012],
                color,
                response,
            );
        }
    }

    fn add_window_city_material_hard_surface_micro_detail(
        &mut self,
        placement: WindowCityMaterialPlacementVisual,
    ) {
        let [x, y, z] = placement.center_meters;
        let [half_x, half_y] = placement.half_extents_meters;
        let z0 = z.max(0.05) + 0.14 + placement.importance * 0.22;
        let scratch_count =
            (2.0 + placement.damage * 4.0 + placement.pollution * 2.0 + placement.traffic_wear)
                .ceil()
                .clamp(2.0, 8.0) as usize;

        for index in 0..scratch_count {
            let rx = x
                + (window_stable_unit(placement.detail_seed, 3_601 + index as u64) - 0.5)
                    * half_x
                    * 1.5;
            let ry = y
                + (window_stable_unit(placement.detail_seed, 3_701 + index as u64) - 0.5)
                    * half_y
                    * 1.5;
            let angle = window_stable_unit(placement.detail_seed, 3_801 + index as u64)
                * std::f32::consts::TAU;
            let is_corrosion = index % 2 == 0 || placement.pollution > placement.damage;
            let color = if is_corrosion {
                [
                    0.36,
                    0.18,
                    0.08,
                    (0.12 + placement.pollution * 0.28 + placement.damage * 0.12).min(0.48),
                ]
            } else {
                [0.74, 0.78, 0.82, 0.18 + placement.damage * 0.24]
            };
            let response = if is_corrosion {
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
            } else {
                WINDOW_SURFACE_RESPONSE_METAL
            };
            self.world_oriented_rect_with_surface_response(
                [rx, ry, z0 + index as f32 * 0.0006],
                [angle.cos(), angle.sin(), 0.0],
                [-angle.sin(), angle.cos(), 0.0],
                [half_x * 0.05 + 0.035, 0.006 + placement.damage * 0.006],
                color,
                response,
            );
        }
    }

    fn add_window_city_persistent_cell(
        &mut self,
        cell: WindowCityPersistentCellVisual,
        frame_index: u64,
    ) {
        if !cell.has_visible_consequence() {
            return;
        }

        let [x, y] = cell.center_meters;
        let [half_x, half_y] = cell.half_extents_meters;
        let min_x = x - half_x;
        let max_x = x + half_x;
        let min_y = y - half_y;
        let max_y = y + half_y;
        let severity = cell.severity.clamp(0.0, 1.0);
        let pulse = (frame_index as f32 * 0.039 + window_stable_unit(cell.detail_seed, 701) * 0.7)
            .sin()
            .mul_add(0.5, 0.5);
        let color = window_city_persistent_cell_color(cell);

        self.world_box_with_surface_response(
            [min_x + 0.2, min_y + 0.2, 0.09],
            [max_x - 0.2, max_y - 0.2, 0.112],
            window_with_alpha(color, color[3] * (0.55 + pulse * 0.22)),
            WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
        );

        for (edge, edge_alpha) in [
            ([min_x, min_y, max_x, min_y + 0.09], 1.0),
            ([min_x, max_y - 0.09, max_x, max_y], 1.0),
            ([min_x, min_y, min_x + 0.09, max_y], 0.82),
            ([max_x - 0.09, min_y, max_x, max_y], 0.82),
        ] {
            self.world_box_with_surface_response(
                [edge[0], edge[1], 0.12],
                [edge[2], edge[3], 0.16],
                window_with_alpha(color, (0.24 + severity * 0.44) * edge_alpha),
                WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
            );
        }

        let bars = [
            (
                cell.damaged_count + cell.material_override_count,
                [1.0, 0.42, 0.16, 0.72],
                WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
            ),
            (
                cell.infrastructure_delta_count + cell.danger_field_count,
                [0.28, 0.76, 1.0, 0.68],
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            ),
            (
                cell.faction_delta_count + cell.restricted_zone_count,
                [0.84, 0.38, 1.0, 0.7],
                WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
            ),
            (
                cell.story_thread_count + cell.ai_memory_count + cell.resident_asset_count,
                [1.0, 0.62, 0.24, 0.7],
                WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
            ),
            (
                cell.blocked_route_count + cell.dangerous_route_count,
                [1.0, 0.16, 0.12, 0.78],
                WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
            ),
        ];
        let bar_spacing = 0.34;
        for (index, (count, bar_color, surface)) in bars.into_iter().enumerate() {
            if count == 0 {
                continue;
            }
            let capped = count.clamp(1, 8);
            let bar_height = 0.22 + capped as f32 * 0.08 + severity * 0.24;
            let offset = (index as f32 - 2.0) * bar_spacing;
            self.world_box_with_surface_response(
                [x + offset - 0.07, y - 0.16, 0.16],
                [x + offset + 0.07, y + 0.16, 0.16 + bar_height],
                window_with_alpha(bar_color, bar_color[3] * (0.74 + pulse * 0.18)),
                surface,
            );
        }

        self.add_window_city_persistent_infrastructure_systems(
            cell,
            [min_x, max_x, min_y, max_y],
            severity,
            pulse,
        );

        if cell.save_required {
            let height = 0.62 + severity * 0.55 + pulse * 0.16;
            self.world_cylinder_between_with_surface_response(
                [x, y, 0.18],
                [x, y, 0.18 + height],
                0.045,
                8,
                [1.0, 0.86, 0.28, 0.78],
                WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
            );
        }
    }

    fn add_window_city_persistent_infrastructure_systems(
        &mut self,
        cell: WindowCityPersistentCellVisual,
        bounds: [f32; 4],
        severity: f32,
        pulse: f32,
    ) {
        if cell.infrastructure_system_mask == 0 {
            return;
        }

        let [min_x, max_x, min_y, max_y] = bounds;
        let [x, y] = cell.center_meters;
        let alpha_gain = (0.28 + severity * 0.42 + pulse * 0.08).clamp(0.16, 0.86);

        if cell.has_infrastructure_system(WINDOW_CITY_INFRASTRUCTURE_POWER) {
            self.world_box_with_surface_response(
                [min_x + 0.38, max_y - 0.26, 0.18],
                [max_x - 0.38, max_y - 0.16, 0.24],
                [1.0, 0.82, 0.18, alpha_gain],
                WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
            );
            self.world_cylinder_between_with_surface_response(
                [x - 0.28, max_y - 0.2, 0.24],
                [x + 0.28, max_y - 0.2, 0.64 + severity * 0.36],
                0.018,
                6,
                [1.0, 0.9, 0.28, 0.34 + severity * 0.34],
                WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
            );
        }

        if cell.has_infrastructure_system(WINDOW_CITY_INFRASTRUCTURE_WATER)
            || cell.has_infrastructure_system(WINDOW_CITY_INFRASTRUCTURE_DRAINAGE)
        {
            let water_alpha = (0.18 + severity * 0.28 + pulse * 0.08).clamp(0.12, 0.62);
            self.world_ellipse_ring_with_surface_response(
                [x, y, 0.122],
                [0.46, 0.16],
                [
                    (max_x - min_x).abs().clamp(2.0, 12.0) * 0.16,
                    (max_y - min_y).abs().clamp(1.0, 6.0) * 0.16,
                ],
                20,
                [0.34, 0.74, 1.0, water_alpha],
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            );

            if cell.has_infrastructure_system(WINDOW_CITY_INFRASTRUCTURE_DRAINAGE) {
                self.world_cylinder_between_with_surface_response(
                    [min_x + 0.55, y - 0.12, 0.14],
                    [max_x - 0.55, y + 0.16 + severity * 0.18, 0.15],
                    0.018 + severity * 0.014,
                    8,
                    [0.5, 0.84, 1.0, 0.2 + severity * 0.32],
                    WINDOW_SURFACE_RESPONSE_WET_ROAD,
                );
            }
        }

        if cell.has_infrastructure_system(WINDOW_CITY_INFRASTRUCTURE_DATA) {
            for lane in 0..3 {
                let lane_y = min_y + 0.54 + lane as f32 * 0.24;
                self.world_box_with_surface_response(
                    [min_x + 0.5, lane_y, 0.2 + lane as f32 * 0.035],
                    [max_x - 0.5, lane_y + 0.035, 0.23 + lane as f32 * 0.035],
                    [0.24, 0.9, 1.0, 0.22 + severity * 0.36],
                    WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
                );
            }
        }

        if cell.has_infrastructure_system(WINDOW_CITY_INFRASTRUCTURE_SURVEILLANCE) {
            self.world_oriented_rect_with_surface_response(
                [x, min_y + 0.42, 0.72 + severity * 0.3],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [(max_x - min_x).abs().clamp(2.0, 8.0) * 0.12, 0.34],
                [1.0, 0.12, 0.18, 0.2 + severity * 0.36],
                WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
            );
            self.world_box_with_surface_response(
                [x - 0.18, min_y + 0.28, 0.92],
                [x + 0.18, min_y + 0.5, 1.12],
                [0.24, 0.9, 1.0, 0.52 + severity * 0.2],
                WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
            );
        }

        if cell.has_infrastructure_system(WINDOW_CITY_INFRASTRUCTURE_TRANSIT) {
            self.world_cylinder_between_with_surface_response(
                [min_x + 0.34, min_y + 0.34, 0.24],
                [max_x - 0.34, max_y - 0.34, 0.24 + severity * 0.22],
                0.028 + severity * 0.012,
                8,
                [0.74, 0.52, 1.0, 0.28 + severity * 0.36],
                WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
            );
            self.world_box_with_surface_response(
                [max_x - 0.58, max_y - 0.58, 0.24],
                [max_x - 0.28, max_y - 0.28, 0.58],
                [1.0, 0.66, 0.22, 0.36 + severity * 0.26],
                WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
            );
        }
    }

    fn add_window_city_danger_field(
        &mut self,
        danger: WindowCityDangerFieldVisual,
        frame_index: u64,
    ) {
        let severity = danger.severity.clamp(0.0, 1.0);
        let color = window_city_danger_color(danger.kind, severity);
        let pulse = (frame_index as f32 * 0.045 + danger.center_meters[0] * 0.017)
            .sin()
            .mul_add(0.5, 0.5);
        let radius = danger.radius_meters.max(0.1) * (0.9 + pulse * 0.18);
        let center = [
            danger.center_meters[0],
            danger.center_meters[1],
            danger.center_meters[2] + 0.14,
        ];
        self.world_ellipse_ring_with_surface_response(
            center,
            [radius * 0.32, radius * 0.18],
            [radius, radius * 0.56],
            20,
            window_with_alpha(color, color[3] * (0.62 + pulse * 0.18)),
            match danger.kind {
                WindowCityDangerVisualKind::Flooding
                | WindowCityDangerVisualKind::SlipperyContamination => {
                    WINDOW_SURFACE_RESPONSE_WET_ROAD
                }
                WindowCityDangerVisualKind::ToxicGas
                | WindowCityDangerVisualKind::BiohazardContamination
                | WindowCityDangerVisualKind::Surveillance => WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
                WindowCityDangerVisualKind::PhysicalDamage => WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
            },
        );
        self.world_cylinder_between_with_surface_response(
            [center[0], center[1], center[2] + 0.02],
            [
                center[0],
                center[1],
                center[2] + 0.34 + severity * 0.58 + pulse * 0.12,
            ],
            0.03 + severity * 0.018,
            8,
            color,
            WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
        );
    }

    fn add_window_city_route_consequence(&mut self, route: WindowCityRouteConsequenceVisual) {
        let severity = route.severity.clamp(0.0, 1.0);
        let color = window_city_route_consequence_color(route.status, severity);
        let start = [
            route.start_meters[0],
            route.start_meters[1],
            route.start_meters[2] + 0.24,
        ];
        let end = [
            route.end_meters[0],
            route.end_meters[1],
            route.end_meters[2] + 0.24,
        ];
        self.world_cylinder_between_with_surface_response(
            start,
            end,
            0.052 + severity * 0.026,
            8,
            color,
            match route.status {
                WindowCityRouteConsequenceStatus::Dangerous => WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
                WindowCityRouteConsequenceStatus::Blocked
                | WindowCityRouteConsequenceStatus::Restricted => {
                    WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
                }
            },
        );

        let midpoint = [
            (start[0] + end[0]) * 0.5,
            (start[1] + end[1]) * 0.5,
            (start[2] + end[2]) * 0.5,
        ];
        let size = 0.14 + severity * 0.1;
        self.world_box_with_surface_response(
            [midpoint[0] - size, midpoint[1] - size, midpoint[2] - 0.04],
            [midpoint[0] + size, midpoint[1] + size, midpoint[2] + 0.04],
            color,
            WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
        );
    }

    fn add_window_generated_city_chunk(
        &mut self,
        chunk: WindowGeneratedCityChunkVisual,
        frame_index: u64,
    ) {
        if matches!(
            chunk.streaming_state,
            WindowCityStreamingCellState::Unloaded
        ) {
            return;
        }

        let [x, y] = chunk.center_meters;
        let [half_x, half_y] = chunk.half_extents_meters;
        let min_x = x - half_x;
        let max_x = x + half_x;
        let min_y = y - half_y;
        let max_y = y + half_y;
        let detail_loaded = chunk.detail_loaded();
        let surface = window_generated_city_surface_response(chunk.district_kind);
        let loaded_factor = if detail_loaded { 1.0 } else { 0.46 };
        let phase =
            (frame_index as f32 * 0.029 + window_stable_unit(chunk.detail_seed, 91) * 0.6).fract();
        let footprint_alpha = if detail_loaded { 0.2 } else { 0.1 };

        self.world_box_with_surface_response(
            [min_x, min_y, 0.004],
            [max_x, max_y, 0.022],
            window_with_alpha(chunk.district_color, footprint_alpha),
            surface,
        );

        let road_half_width =
            (0.42 + chunk.crime_pressure * 0.28 + chunk.pollution * 0.22).clamp(0.35, 0.9);
        self.world_box_with_surface_response(
            [min_x + 0.7, y - road_half_width, 0.024],
            [max_x - 0.7, y + road_half_width, 0.052],
            [0.018, 0.023, 0.029, 0.92],
            WINDOW_SURFACE_RESPONSE_WET_ROAD,
        );
        self.world_box_with_surface_response(
            [min_x + 1.0, y - 0.035, 0.055],
            [max_x - 1.0, y + 0.035, 0.074],
            [0.66, 0.64, 0.5, 0.2 + loaded_factor * 0.18],
            [0.68, 0.0, 0.22, 0.0],
        );

        self.add_window_generated_city_buildings(chunk, min_x, max_x, min_y, max_y, phase);
        self.add_window_generated_city_activity(chunk, min_x, max_x, min_y, max_y, phase);
        self.add_window_generated_city_detail_layout(chunk, min_x, max_x, min_y, max_y);
    }

    fn add_window_generated_city_buildings(
        &mut self,
        chunk: WindowGeneratedCityChunkVisual,
        min_x: f32,
        max_x: f32,
        min_y: f32,
        max_y: f32,
        phase: f32,
    ) {
        let span_x = (max_x - min_x).abs().max(1.0);
        let detail_loaded = chunk.detail_loaded();
        let detail_budget = window_generated_city_detail_budget(chunk);
        let segment_count = if detail_loaded {
            let target_width = if detail_budget >= 4 { 11.0 } else { 14.0 };
            (span_x / target_width).ceil().clamp(2.0, 6.0) as usize
        } else {
            2
        };
        let segment_width = span_x / segment_count as f32;
        let depth = match chunk.district_kind {
            WindowCityDistrictVisualKind::CorporateCore => 2.9,
            WindowCityDistrictVisualKind::RainAlleySlum => 2.25,
            WindowCityDistrictVisualKind::IndustrialDock => 2.7,
            WindowCityDistrictVisualKind::BlackMarket => 2.15,
            WindowCityDistrictVisualKind::ClinicDistrict => 2.35,
        } * if detail_loaded { 1.0 } else { 0.58 };
        let base_height = match chunk.district_kind {
            WindowCityDistrictVisualKind::CorporateCore => 8.0,
            WindowCityDistrictVisualKind::RainAlleySlum => 4.2,
            WindowCityDistrictVisualKind::IndustrialDock => 5.3,
            WindowCityDistrictVisualKind::BlackMarket => 4.6,
            WindowCityDistrictVisualKind::ClinicDistrict => 5.6,
        };
        let surface = window_generated_city_surface_response(chunk.district_kind);

        for side in [-1.0_f32, 1.0] {
            let (y0, y1, face_y) = if side < 0.0 {
                (
                    min_y + 0.35,
                    (min_y + 0.35 + depth).min(max_y - 0.6),
                    min_y + 0.35 + depth,
                )
            } else {
                (
                    (max_y - 0.35 - depth).max(min_y + 0.6),
                    max_y - 0.35,
                    max_y - 0.35 - depth,
                )
            };
            if y1 <= y0 {
                continue;
            }

            for segment in 0..segment_count {
                let jitter = window_stable_unit(
                    chunk.detail_seed,
                    300 + segment as u64 + if side < 0.0 { 0 } else { 41 },
                );
                let x0 = min_x + segment as f32 * segment_width + 0.28;
                let x1 = (x0 + segment_width * (0.66 + jitter * 0.24)).min(max_x - 0.28);
                if x1 <= x0 {
                    continue;
                }
                let height_variation = 0.75 + jitter * 1.45 + chunk.surveillance_level * 0.55;
                let height =
                    (base_height * height_variation * if detail_loaded { 1.0 } else { 0.46 })
                        .clamp(1.2, 14.5);
                let brightness = 0.56 + jitter * 0.22 - chunk.pollution * 0.18;
                let mut building_color = window_scale_color(chunk.district_color, brightness);
                building_color[3] = if detail_loaded { 0.98 } else { 0.62 };

                let use_micro_facade =
                    detail_budget >= 3 && window_generated_city_lod_scale(chunk) > 0.58;
                if use_micro_facade {
                    self.world_micro_detailed_box_with_surface_response(
                        [x0, y0, 0.03],
                        [x1, y1, height],
                        building_color,
                        surface,
                        chunk.detail_seed
                            ^ (segment as u64).wrapping_mul(17)
                            ^ if side < 0.0 { 0xA11E } else { 0xB017 },
                        (0.48
                            + chunk.pollution * 0.24
                            + chunk.crime_pressure * 0.18
                            + chunk.surveillance_level * 0.1)
                            .clamp(0.0, 1.0),
                    );
                } else {
                    self.world_box_with_surface_response(
                        [x0, y0, 0.03],
                        [x1, y1, height],
                        building_color,
                        surface,
                    );
                }

                if !detail_loaded {
                    continue;
                }

                let max_rows = if detail_budget >= 4 { 5.0 } else { 3.0 };
                let max_columns = if detail_budget >= 4 { 4.0 } else { 3.0 };
                let window_rows = ((height - 0.9) / 1.45).floor().clamp(1.0, max_rows) as usize;
                let window_columns = ((x1 - x0) / 2.25).floor().clamp(1.0, max_columns) as usize;
                for row in 0..window_rows {
                    let z = 0.82 + row as f32 * ((height - 1.2) / window_rows as f32).max(0.5);
                    for column in 0..window_columns {
                        let column_fraction = (column as f32 + 0.5) / window_columns as f32;
                        let window_x = x0 + (x1 - x0) * column_fraction;
                        let flicker = (phase
                            + window_stable_unit(
                                chunk.detail_seed,
                                row as u64 * 19 + column as u64,
                            ) * 0.45)
                            .fract();
                        let glow = window_generated_city_window_color(chunk, flicker);
                        self.world_oriented_rect_with_surface_response(
                            [window_x, face_y, z],
                            [1.0, 0.0, 0.0],
                            [0.0, 0.0, 1.0],
                            [0.28, 0.18],
                            glow,
                            WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
                        );
                    }
                }

                let sign_count = chunk
                    .light_count
                    .saturating_add(chunk.story_hook_count)
                    .clamp(1, 5) as usize;
                if detail_budget >= 2 && segment < sign_count {
                    let sign_z = (1.35 + jitter * 2.1).min(height - 0.35).max(0.8);
                    let sign_color = window_generated_city_sign_color(chunk, segment);
                    self.world_oriented_rect_with_surface_response(
                        [(x0 + x1) * 0.5, face_y + side * 0.012, sign_z],
                        [1.0, 0.0, 0.0],
                        [0.0, 0.0, 1.0],
                        [(x1 - x0).clamp(0.8, 3.2) * 0.28, 0.16],
                        sign_color,
                        WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
                    );
                }

                if chunk.destructible_count > 0 && segment % 2 == 0 {
                    self.world_oriented_rect_with_surface_response(
                        [(x0 + x1) * 0.5, face_y + side * 0.016, 0.92],
                        [1.0, 0.0, 0.0],
                        [0.0, 0.0, 1.0],
                        [(x1 - x0).clamp(0.8, 2.6) * 0.22, 0.26],
                        [0.62, 0.88, 1.0, 0.38],
                        WINDOW_SURFACE_RESPONSE_GLASS,
                    );
                }

                if detail_budget >= 3 || segment.is_multiple_of(2) {
                    self.add_window_generated_city_building_detail(
                        chunk,
                        WindowGeneratedCityBuildingSlice {
                            x0,
                            x1,
                            y0,
                            y1,
                            face_y,
                            side,
                            height,
                            segment,
                            jitter,
                        },
                    );
                }
            }
        }
    }

    fn add_window_generated_city_building_detail(
        &mut self,
        chunk: WindowGeneratedCityChunkVisual,
        slice: WindowGeneratedCityBuildingSlice,
    ) {
        let width = (slice.x1 - slice.x0).abs();
        let depth = (slice.y1 - slice.y0).abs();
        if width <= 0.24 || depth <= 0.24 || slice.height <= 0.5 {
            return;
        }

        let seed = chunk.detail_seed
            ^ (slice.segment as u64).wrapping_mul(3_757)
            ^ if slice.side < 0.0 { 0xC17E } else { 0xD17E };
        let roof_z = slice.height.max(0.58);
        let facade_y0 = (slice.face_y - 0.045).min(slice.face_y + 0.045);
        let facade_y1 = (slice.face_y - 0.045).max(slice.face_y + 0.045);
        let detail_budget = window_generated_city_detail_budget(chunk);
        let roof_detail_density = (0.34
            + chunk.pollution * 0.22
            + chunk.crime_pressure * 0.2
            + chunk.surveillance_level * 0.12)
            .clamp(0.0, 1.0);

        let parapet_color = [0.052, 0.056, 0.06, 0.92];
        let parapet_height = (0.16 + slice.jitter * 0.08).min(0.3);
        self.world_box_with_surface_response(
            [slice.x0, slice.y0, roof_z],
            [slice.x1, slice.y0 + 0.075, roof_z + parapet_height],
            parapet_color,
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        self.world_box_with_surface_response(
            [slice.x0, slice.y1 - 0.075, roof_z],
            [slice.x1, slice.y1, roof_z + parapet_height],
            parapet_color,
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        self.world_box_with_surface_response(
            [slice.x0, slice.y0, roof_z],
            [slice.x0 + 0.075, slice.y1, roof_z + parapet_height],
            parapet_color,
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        self.world_box_with_surface_response(
            [slice.x1 - 0.075, slice.y0, roof_z],
            [slice.x1, slice.y1, roof_z + parapet_height],
            parapet_color,
            WINDOW_SURFACE_RESPONSE_METAL,
        );

        let unit_count = if detail_budget >= 4 && width > 3.2 {
            2
        } else {
            1
        };
        for index in 0..unit_count {
            let t = (index as f32 + 0.65) / (unit_count as f32 + 0.3);
            let x = slice.x0 + width * t;
            let y = slice.y0
                + depth
                    * (0.32
                        + window_stable_unit(seed, 7_201 + index as u64) * 0.36
                        + slice.jitter * 0.08)
                        .clamp(0.18, 0.82);
            let hx = (0.18 + width * 0.018).clamp(0.16, 0.32);
            let hy = (0.16 + depth * 0.024).clamp(0.12, 0.28);
            let hz = 0.18 + window_stable_unit(seed, 7_301 + index as u64) * 0.16;
            if detail_budget >= 4 {
                self.world_micro_detailed_box_with_surface_response(
                    [x - hx, y - hy, roof_z + 0.05],
                    [x + hx, y + hy, roof_z + 0.05 + hz],
                    [0.13, 0.15, 0.16, 0.96],
                    WINDOW_SURFACE_RESPONSE_METAL,
                    seed ^ (index as u64).wrapping_mul(17) ^ 0xA2C1,
                    roof_detail_density,
                );
            } else {
                self.world_ellipsoid_with_surface_response(
                    [x, y, roof_z + 0.16 + hz * 0.42],
                    [hx * 0.9, hy * 0.75, hz * 0.42],
                    4,
                    8,
                    [0.13, 0.15, 0.16, 0.92],
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
            }
        }

        let vent_x = slice.x0 + width * (0.18 + window_stable_unit(seed, 7_401) * 0.64);
        let vent_y = slice.y0 + depth * (0.2 + window_stable_unit(seed, 7_501) * 0.58);
        self.world_cylinder_between_with_surface_response(
            [vent_x, vent_y, roof_z + 0.08],
            [vent_x, vent_y, roof_z + 0.5 + slice.jitter * 0.36],
            0.035,
            window_generated_city_minor_segments(chunk),
            [0.1, 0.12, 0.13, 1.0],
            WINDOW_SURFACE_RESPONSE_METAL,
        );

        let ledge_color = [0.028, 0.032, 0.036, 1.0];
        let rail_count = ((slice.height - 0.85) / 1.65).floor().clamp(1.0, 4.0) as usize;
        for row in 0..rail_count {
            let z = 0.95 + row as f32 * ((slice.height - 1.2) / rail_count as f32).max(0.62);
            if z >= slice.height - 0.22 {
                continue;
            }

            self.world_box_with_surface_response(
                [slice.x0 + 0.12, facade_y0, z - 0.018],
                [slice.x1 - 0.12, facade_y1, z + 0.018],
                ledge_color,
                WINDOW_SURFACE_RESPONSE_METAL,
            );

            if row % 2 == slice.segment % 2 {
                let ladder_x = if slice.segment.is_multiple_of(2) {
                    slice.x0 + width * 0.18
                } else {
                    slice.x1 - width * 0.18
                };
                let ladder_y = slice.face_y + slice.side * 0.06;
                let lower = (z - 0.56).max(0.22);
                let upper = (z + 0.56).min(slice.height - 0.16);
                self.world_cylinder_between_with_surface_response(
                    [ladder_x - 0.07, ladder_y, lower],
                    [ladder_x - 0.07, ladder_y, upper],
                    0.012,
                    5,
                    ledge_color,
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
                self.world_cylinder_between_with_surface_response(
                    [ladder_x + 0.07, ladder_y, lower],
                    [ladder_x + 0.07, ladder_y, upper],
                    0.012,
                    5,
                    ledge_color,
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
                self.world_cylinder_between_with_surface_response(
                    [ladder_x - 0.1, ladder_y, z],
                    [ladder_x + 0.1, ladder_y, z],
                    0.01,
                    5,
                    ledge_color,
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
            }
        }

        if chunk.pollution > 0.28 || chunk.crime_pressure > 0.42 {
            let grime_alpha =
                (0.18 + chunk.pollution * 0.28 + chunk.crime_pressure * 0.16).clamp(0.0, 0.58);
            let grime_z = (1.28 + slice.jitter * 1.8)
                .min(slice.height - 0.42)
                .max(0.8);
            self.world_oriented_rect_with_surface_response(
                [
                    slice.x0 + width * (0.38 + window_stable_unit(seed, 7_601) * 0.24),
                    slice.face_y + slice.side * 0.018,
                    grime_z,
                ],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [width.clamp(1.0, 4.5) * 0.2, 0.28 + chunk.pollution * 0.2],
                [0.026, 0.022, 0.02, grime_alpha],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        self.add_window_generated_city_building_silhouette_breakup(
            chunk, slice, seed, width, depth, roof_z,
        );

        match chunk.district_kind {
            WindowCityDistrictVisualKind::CorporateCore => {
                let mast_x = slice.x0 + width * 0.5;
                let mast_y = slice.y0 + depth * 0.5;
                self.world_cylinder_between_with_surface_response(
                    [mast_x, mast_y, roof_z + 0.12],
                    [
                        mast_x,
                        mast_y,
                        roof_z + 1.15 + chunk.surveillance_level * 0.52,
                    ],
                    0.018,
                    window_generated_city_minor_segments(chunk),
                    [0.15, 0.2, 0.23, 0.9],
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
                self.world_ellipsoid_with_surface_response(
                    [mast_x, mast_y, roof_z + 1.18],
                    [0.07, 0.07, 0.11],
                    4,
                    window_generated_city_minor_segments(chunk),
                    [0.72, 0.78, 0.68, 0.22 + chunk.surveillance_level * 0.18],
                    WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
                );
            }
            WindowCityDistrictVisualKind::RainAlleySlum => {
                self.world_oriented_rect_with_surface_response(
                    [
                        (slice.x0 + slice.x1) * 0.5,
                        slice.face_y + slice.side * 0.24,
                        1.15 + slice.jitter * 0.32,
                    ],
                    [1.0, 0.0, 0.0],
                    [0.0, slice.side, -0.28],
                    [width.clamp(0.8, 3.4) * 0.24, 0.24],
                    [0.075, 0.052, 0.035, 0.86],
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                );
            }
            WindowCityDistrictVisualKind::IndustrialDock => {
                let tank_x = slice.x0 + width * 0.38;
                let tank_y = slice.y0 + depth * 0.58;
                self.world_cylinder_between_with_surface_response(
                    [tank_x, tank_y, roof_z + 0.08],
                    [tank_x, tank_y, roof_z + 0.72],
                    0.17,
                    window_generated_city_major_segments(chunk),
                    [0.12, 0.14, 0.13, 0.96],
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
                self.world_cylinder_between_with_surface_response(
                    [tank_x, tank_y, roof_z + 0.32],
                    [
                        slice.x1 - width * 0.14,
                        slice.face_y + slice.side * 0.04,
                        roof_z + 0.32,
                    ],
                    0.026,
                    window_generated_city_minor_segments(chunk),
                    [0.09, 0.1, 0.095, 1.0],
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
            }
            WindowCityDistrictVisualKind::BlackMarket => {
                let sign_x = if slice.segment.is_multiple_of(2) {
                    slice.x1 - width * 0.18
                } else {
                    slice.x0 + width * 0.18
                };
                let sign_z = (1.32 + slice.jitter * 1.5)
                    .min(slice.height - 0.34)
                    .max(0.9);
                self.world_box_with_surface_response(
                    [
                        sign_x - 0.05,
                        slice.face_y + slice.side * 0.02,
                        sign_z + 0.32,
                    ],
                    [
                        sign_x + 0.05,
                        slice.face_y + slice.side * 0.28,
                        sign_z + 0.38,
                    ],
                    [0.04, 0.035, 0.045, 1.0],
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
                self.world_oriented_rect_with_surface_response(
                    [sign_x, slice.face_y + slice.side * 0.34, sign_z],
                    [0.0, slice.side, 0.0],
                    [0.0, 0.0, 1.0],
                    [0.16, 0.42],
                    [0.74, 0.16, 0.48, 0.44],
                    WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
                );
                self.world_oriented_rect_with_surface_response(
                    [
                        (slice.x0 + slice.x1) * 0.5,
                        slice.face_y + slice.side * 0.24,
                        0.96 + slice.jitter * 0.24,
                    ],
                    [1.0, 0.0, 0.0],
                    [0.0, slice.side, -0.18],
                    [width.clamp(0.8, 3.6) * 0.22, 0.23],
                    [0.09, 0.035, 0.07, 0.82],
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                );
            }
            WindowCityDistrictVisualKind::ClinicDistrict => {
                let cross_center = [
                    slice.x0 + width * 0.5,
                    slice.face_y + slice.side * 0.024,
                    (1.42 + slice.jitter * 1.3)
                        .min(slice.height - 0.35)
                        .max(0.9),
                ];
                let cross_color = [0.62, 0.86, 0.74, 0.42];
                self.world_oriented_rect_with_surface_response(
                    cross_center,
                    [1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0],
                    [0.42, 0.08],
                    cross_color,
                    WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
                );
                self.world_oriented_rect_with_surface_response(
                    cross_center,
                    [1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0],
                    [0.1, 0.34],
                    cross_color,
                    WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
                );
            }
        }
    }

    fn add_window_generated_city_building_silhouette_breakup(
        &mut self,
        chunk: WindowGeneratedCityChunkVisual,
        slice: WindowGeneratedCityBuildingSlice,
        seed: u64,
        width: f32,
        depth: f32,
        roof_z: f32,
    ) {
        let detail_budget = window_generated_city_detail_budget(chunk);
        if detail_budget == 0 {
            return;
        }
        let major_segments = window_generated_city_major_segments(chunk);
        let minor_segments = window_generated_city_minor_segments(chunk);
        let (ellipsoid_lat, ellipsoid_lon) = window_generated_city_ellipsoid_segments(chunk);
        let facade_y = slice.face_y + slice.side * 0.052;
        let facade_normal = [0.0, slice.side, 0.0];
        let dirty = (0.28 + chunk.pollution * 0.34 + chunk.crime_pressure * 0.12).clamp(0.0, 0.78);
        let metal_color = match chunk.district_kind {
            WindowCityDistrictVisualKind::CorporateCore => [0.12, 0.15, 0.17, 0.92],
            WindowCityDistrictVisualKind::ClinicDistrict => [0.15, 0.22, 0.23, 0.88],
            WindowCityDistrictVisualKind::IndustrialDock => [0.16, 0.14, 0.11, 0.94],
            WindowCityDistrictVisualKind::RainAlleySlum
            | WindowCityDistrictVisualKind::BlackMarket => [0.07, 0.061, 0.052, 0.88],
        };

        let tank_x = slice.x0 + width * (0.24 + window_stable_unit(seed, 8_101) * 0.52);
        let tank_y = slice.y0 + depth * (0.24 + window_stable_unit(seed, 8_203) * 0.5);
        let tank_radius =
            (0.11 + width * 0.018 + window_stable_unit(seed, 8_307) * 0.07).clamp(0.1, 0.26);
        self.world_cylinder_between_with_surface_response(
            [tank_x, tank_y, roof_z + 0.08],
            [tank_x, tank_y, roof_z + 0.42 + slice.jitter * 0.22],
            tank_radius,
            major_segments,
            metal_color,
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        self.world_ellipsoid_with_surface_response(
            [tank_x, tank_y, roof_z + 0.45 + slice.jitter * 0.22],
            [tank_radius * 1.02, tank_radius * 1.02, tank_radius * 0.36],
            ellipsoid_lat,
            ellipsoid_lon,
            window_scale_color(metal_color, 1.08),
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        self.world_ellipse_ring_with_surface_response(
            [tank_x, tank_y, roof_z + 0.11],
            [tank_radius * 0.72, tank_radius * 0.42],
            [tank_radius * 1.24, tank_radius * 0.84],
            major_segments,
            [0.028, 0.026, 0.022, dirty * 0.42],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );

        let pipe_count =
            (1 + (chunk.pollution * 2.0).ceil() as usize).clamp(1, detail_budget.max(2));
        for index in 0..pipe_count {
            let pipe_x = slice.x0
                + width
                    * (0.16
                        + (index as f32 + window_stable_unit(seed, 8_503 + index as u64))
                            / (pipe_count as f32 + 1.2)
                            * 0.72);
            let lower = 0.32 + window_stable_unit(seed, 8_701 + index as u64) * 0.48;
            let upper = (lower + 1.1 + window_stable_unit(seed, 8_907 + index as u64) * 2.1)
                .min(slice.height - 0.2)
                .max(lower + 0.38);
            let radius = 0.012 + window_stable_unit(seed, 9_101 + index as u64) * 0.012;
            self.world_cylinder_between_with_surface_response(
                [pipe_x, facade_y, lower],
                [pipe_x, facade_y, upper],
                radius,
                minor_segments,
                window_scale_color(metal_color, 0.72 + index as f32 * 0.08),
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            if index.is_multiple_of(2) {
                self.world_cylinder_between_with_surface_response(
                    [pipe_x, facade_y, upper],
                    [pipe_x + width * 0.11, facade_y, upper + 0.04],
                    radius * 0.88,
                    minor_segments,
                    metal_color,
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
            }
        }

        if detail_budget >= 3 {
            let duct_z = (slice.height * (0.38 + window_stable_unit(seed, 9_401) * 0.34))
                .min(slice.height - 0.38)
                .max(0.78);
            self.world_cylinder_between_with_surface_response(
                [slice.x0 + width * 0.2, facade_y, duct_z],
                [
                    slice.x1 - width * 0.18,
                    facade_y,
                    duct_z + slice.jitter * 0.04,
                ],
                0.032 + window_stable_unit(seed, 9_503) * 0.018,
                minor_segments,
                [0.075, 0.078, 0.074, 0.84],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            for end_x in [slice.x0 + width * 0.2, slice.x1 - width * 0.18] {
                self.world_ellipsoid_with_surface_response(
                    [end_x, facade_y, duct_z],
                    [0.052, 0.026, 0.052],
                    4,
                    minor_segments,
                    [0.048, 0.05, 0.048, 0.82],
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
            }
        }

        let awning_z = (0.86 + slice.jitter * 0.62)
            .min(slice.height - 0.34)
            .max(0.62);
        let awning_color = match chunk.district_kind {
            WindowCityDistrictVisualKind::CorporateCore => [0.14, 0.18, 0.19, 0.58],
            WindowCityDistrictVisualKind::ClinicDistrict => [0.18, 0.28, 0.26, 0.62],
            WindowCityDistrictVisualKind::IndustrialDock => [0.2, 0.13, 0.07, 0.68],
            WindowCityDistrictVisualKind::BlackMarket => [0.12, 0.035, 0.09, 0.78],
            WindowCityDistrictVisualKind::RainAlleySlum => [0.08, 0.052, 0.035, 0.76],
        };
        self.world_oriented_rect_with_surface_response(
            [
                slice.x0 + width * (0.34 + window_stable_unit(seed, 9_701) * 0.28),
                slice.face_y + slice.side * 0.2,
                awning_z,
            ],
            [1.0, 0.0, 0.0],
            [0.0, slice.side, -0.24 - chunk.pollution * 0.08],
            [
                width.clamp(0.8, 4.2) * 0.18,
                0.22 + chunk.crime_pressure * 0.08,
            ],
            awning_color,
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
        for tether in [-0.9_f32, 0.9] {
            let tether_x = slice.x0 + width * 0.5 + tether * width.clamp(0.8, 4.2) * 0.18;
            self.world_cylinder_between_with_surface_response(
                [tether_x, slice.face_y + slice.side * 0.06, awning_z + 0.18],
                [tether_x, slice.face_y + slice.side * 0.34, awning_z - 0.18],
                0.006,
                5,
                [0.035, 0.03, 0.026, 0.72],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        let rubble_count =
            (1 + (chunk.pollution * 3.0).ceil() as usize).clamp(1, detail_budget + 2);
        for rubble in 0..rubble_count {
            let rubble_seed = seed ^ 0xB455 ^ rubble as u64;
            let x = slice.x0 + width * (0.08 + window_stable_unit(rubble_seed, 10_101) * 0.84);
            let y =
                slice.face_y + slice.side * (0.04 + window_stable_unit(rubble_seed, 10_203) * 0.22);
            self.world_ellipsoid_with_surface_response(
                [x, y, 0.055 + rubble as f32 * 0.002],
                [
                    0.035 + window_stable_unit(rubble_seed, 10_307) * 0.06,
                    0.018 + window_stable_unit(rubble_seed, 10_409) * 0.04,
                    0.014 + window_stable_unit(rubble_seed, 10_503) * 0.024,
                ],
                3,
                minor_segments,
                [0.052, 0.046, 0.036, 0.62 + dirty * 0.22],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        if chunk.surveillance_level > 0.28 && detail_budget >= 3 {
            let camera_x = slice.x0 + width * (0.2 + window_stable_unit(seed, 10_801) * 0.6);
            let camera_z = (slice.height * 0.68).clamp(1.1, slice.height - 0.28);
            self.world_cylinder_between_with_surface_response(
                [camera_x, facade_y, camera_z],
                [camera_x, facade_y + slice.side * 0.18, camera_z - 0.04],
                0.014,
                minor_segments,
                [0.05, 0.054, 0.052, 0.88],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            self.world_ellipsoid_with_surface_response(
                [camera_x, facade_y + slice.side * 0.22, camera_z - 0.05],
                [0.055, 0.035, 0.035],
                4,
                minor_segments,
                [0.032, 0.034, 0.035, 0.92],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            self.world_oriented_rect_with_surface_response(
                [camera_x, facade_y + slice.side * 0.255, camera_z - 0.05],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.026, 0.018],
                [0.72, 0.78, 0.72, 0.12 + chunk.surveillance_level * 0.12],
                WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
            );
        }

        if chunk.crime_pressure > 0.48 && detail_budget >= 2 {
            let mark_count = (1 + (chunk.crime_pressure * 3.0).ceil() as usize).clamp(1, 4);
            for mark in 0..mark_count {
                let mark_seed = seed ^ 0x6AFF ^ mark as u64;
                let center = [
                    slice.x0 + width * (0.14 + window_stable_unit(mark_seed, 11_101) * 0.72),
                    slice.face_y + slice.side * 0.022,
                    (0.8 + window_stable_unit(mark_seed, 11_203) * (slice.height - 1.2).max(0.2))
                        .min(slice.height - 0.24),
                ];
                self.world_oriented_rect_with_surface_response(
                    center,
                    normalize3([
                        1.0,
                        0.0,
                        (window_stable_unit(mark_seed, 11_307) - 0.5) * 0.28,
                    ]),
                    facade_normal,
                    [
                        0.12 + window_stable_unit(mark_seed, 11_409) * 0.18,
                        0.018 + window_stable_unit(mark_seed, 11_503) * 0.024,
                    ],
                    [0.034, 0.028, 0.024, 0.34 + chunk.crime_pressure * 0.18],
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                );
            }
        }
    }

    fn add_window_generated_city_activity(
        &mut self,
        chunk: WindowGeneratedCityChunkVisual,
        min_x: f32,
        max_x: f32,
        min_y: f32,
        max_y: f32,
        phase: f32,
    ) {
        if !chunk.detail_loaded() {
            return;
        }

        let road_y = chunk.center_meters[1];
        let span_x = (max_x - min_x).abs().max(1.0);
        let detail_budget = window_generated_city_detail_budget(chunk);
        let lod_scale = window_generated_city_lod_scale(chunk);
        let npc_count =
            ((chunk.npc_count.clamp(0, 7) as f32 * lod_scale).ceil() as usize).clamp(0, 5);
        for index in 0..npc_count {
            let t = (index as f32 + 0.65) / (npc_count as f32 + 0.35);
            let x = min_x + span_x * t;
            let side = if index % 2 == 0 { -1.0 } else { 1.0 };
            let y =
                road_y + side * (0.72 + window_stable_unit(chunk.detail_seed, index as u64) * 0.5);
            let body =
                window_scale_color(chunk.faction_color, 0.72 + chunk.faction_strength * 0.34);
            let skin_color = window_generated_city_skin_color(chunk.detail_seed, index as u64);
            let stature = 0.92 + window_stable_unit(chunk.detail_seed, 1_071 + index as u64) * 0.16;
            self.add_window_generated_city_pedestrian(
                x,
                y,
                0.0,
                stature,
                body,
                skin_color,
                side,
                chunk.detail_seed ^ index as u64,
                0.78,
            );
        }

        self.add_window_generated_city_population_flow(chunk, min_x, max_x, road_y, phase);

        let story_count =
            ((chunk.story_hook_count.clamp(0, 5) as f32 * lod_scale).ceil() as usize).clamp(0, 4);
        for index in 0..story_count {
            let t = (index as f32 + 1.0) / (story_count as f32 + 1.0);
            let x = min_x + span_x * t;
            let pulse = (phase + index as f32 * 0.18).fract();
            self.world_ellipse_ring_with_surface_response(
                [x, road_y, 0.086],
                [0.2 + pulse * 0.08, 0.11 + pulse * 0.04],
                [0.48 + pulse * 0.16, 0.24 + pulse * 0.08],
                window_generated_city_minor_segments(chunk),
                [0.78, 0.56, 0.32, 0.18 + pulse * 0.12],
                WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
            );
        }

        let hazard_count =
            ((chunk.hazard_count.clamp(0, 5) as f32 * lod_scale).ceil() as usize).clamp(0, 4);
        for index in 0..hazard_count {
            let t = (index as f32 + 0.5) / hazard_count.max(1) as f32;
            let x = min_x + span_x * t;
            let y = road_y + if index % 2 == 0 { -0.48 } else { 0.48 };
            self.world_ellipse_ring_with_surface_response(
                [x, y, 0.066],
                [0.22, 0.08],
                [0.74 + chunk.pollution * 0.38, 0.24 + chunk.pollution * 0.1],
                window_generated_city_minor_segments(chunk),
                [0.16, 0.38, 0.42, 0.16 + chunk.pollution * 0.14],
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            );
        }

        self.add_window_generated_city_road_grime(chunk, min_x, max_x, road_y, phase);

        if chunk.surveillance_level > 0.35 && detail_budget >= 3 {
            let camera_count = (chunk.surveillance_level * 3.0).ceil().clamp(1.0, 3.0) as usize;
            for index in 0..camera_count {
                let t = (index as f32 + 0.5) / camera_count as f32;
                let x = min_x + span_x * t;
                let y = if index % 2 == 0 {
                    min_y + 1.1
                } else {
                    max_y - 1.1
                };
                let z = 2.0 + chunk.surveillance_level * 1.25;
                self.world_cylinder_between_with_surface_response(
                    [x, y, 0.04],
                    [x, y, z],
                    0.022,
                    window_generated_city_minor_segments(chunk),
                    [0.072, 0.084, 0.09, 1.0],
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
                self.world_ellipsoid_with_surface_response(
                    [x, y, z + 0.01],
                    [0.15, 0.06, 0.075],
                    4,
                    window_generated_city_minor_segments(chunk),
                    [0.66, 0.76, 0.68, 0.22 + chunk.surveillance_level * 0.16],
                    WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
                );
            }
        }
    }

    fn add_window_generated_city_road_grime(
        &mut self,
        chunk: WindowGeneratedCityChunkVisual,
        min_x: f32,
        max_x: f32,
        road_y: f32,
        phase: f32,
    ) {
        let span_x = (max_x - min_x).abs().max(1.0);
        let grime_count = (3
            + (chunk.pollution * 5.0).ceil() as usize
            + (chunk.crime_pressure * 2.0).ceil() as usize)
            .clamp(3, 7);
        for index in 0..grime_count {
            let seed = chunk.detail_seed ^ 0xD127 ^ index as u64;
            let t = ((index as f32 + 0.31) / grime_count as f32 + phase * 0.018).fract();
            let x = min_x + span_x * (0.06 + t * 0.88);
            let lane_offset = (window_stable_unit(seed, 31_001) * 2.0 - 1.0) * 0.58;
            let y = road_y + lane_offset;
            let wet = chunk.pollution * 0.22 + window_stable_unit(seed, 31_103) * 0.16;
            let rough_alpha =
                (0.16 + chunk.pollution * 0.24 + window_stable_unit(seed, 31_209) * 0.12)
                    .clamp(0.12, 0.56);
            self.world_flat_ellipse_with_surface_response(
                [x, y, 0.084 + index as f32 * 0.0003],
                [
                    0.16 + window_stable_unit(seed, 31_307) * 0.22,
                    0.038 + window_stable_unit(seed, 31_409) * 0.08,
                ],
                window_generated_city_minor_segments(chunk),
                [0.034, 0.028, 0.02, rough_alpha],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
            if wet > 0.18 {
                self.world_ellipse_ring_with_surface_response(
                    [x + 0.06, y - 0.018, 0.089 + index as f32 * 0.0003],
                    [0.06, 0.02],
                    [0.19, 0.064],
                    window_generated_city_minor_segments(chunk),
                    [0.08, 0.15, 0.16, wet.clamp(0.08, 0.34)],
                    WINDOW_SURFACE_RESPONSE_WET_ROAD,
                );
            }
            if index.is_multiple_of(3) {
                let debris_x = x - 0.11 + window_stable_unit(seed, 31_701) * 0.22;
                let debris_y = y + (window_stable_unit(seed, 31_803) - 0.5) * 0.12;
                self.world_ellipsoid_with_surface_response(
                    [debris_x, debris_y, 0.112],
                    [
                        0.022 + window_stable_unit(seed, 31_907) * 0.024,
                        0.012 + window_stable_unit(seed, 32_001) * 0.018,
                        0.008 + window_stable_unit(seed, 32_103) * 0.012,
                    ],
                    3,
                    window_generated_city_minor_segments(chunk),
                    [0.06, 0.052, 0.04, 0.76],
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                );
            }
        }
    }

    fn add_window_generated_city_population_flow(
        &mut self,
        chunk: WindowGeneratedCityChunkVisual,
        min_x: f32,
        max_x: f32,
        road_y: f32,
        phase: f32,
    ) {
        let span_x = (max_x - min_x).abs().max(1.0);
        let crowd_density = window_generated_city_crowd_density(chunk);
        let traffic_density = window_generated_city_traffic_density(chunk);

        if !chunk.population_validation_passed {
            self.world_ellipse_ring_with_surface_response(
                [chunk.center_meters[0], road_y, 0.092],
                [0.36, 0.14],
                [0.74, 0.28],
                window_generated_city_minor_segments(chunk),
                [0.78, 0.16, 0.1, 0.38],
                WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
            );
        }

        if crowd_density > 0.0 {
            let crowd_count = window_generated_city_crowd_visual_count(chunk);
            let flow_alpha = (0.1 + crowd_density * 0.2).clamp(0.0, 0.34);
            for side in [-1.0_f32, 1.0] {
                self.world_oriented_rect_with_surface_response(
                    [chunk.center_meters[0], road_y + side * 1.05, 0.068],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [span_x * 0.39, 0.045],
                    window_with_alpha(
                        window_scale_color(chunk.faction_color, 0.46),
                        flow_alpha * 0.72,
                    ),
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                );
            }

            for index in 0..crowd_count {
                let seed = 8_001 + index as u64;
                let lane_shift = window_stable_unit(chunk.detail_seed, seed) * 0.42;
                let t = ((index as f32 + 0.45) / crowd_count as f32 + phase * 0.08).fract();
                let x = min_x + span_x * (0.07 + t * 0.86);
                let side = if index % 2 == 0 { -1.0 } else { 1.0 };
                let y = road_y + side * (0.86 + lane_shift);
                let stature = 0.82 + window_stable_unit(chunk.detail_seed, seed + 97) * 0.24;
                let body_color = window_generated_city_crowd_body_color(chunk, index);
                let skin_color = [
                    0.44 + window_stable_unit(chunk.detail_seed, seed + 211) * 0.18,
                    0.29 + window_stable_unit(chunk.detail_seed, seed + 307) * 0.11,
                    0.2 + window_stable_unit(chunk.detail_seed, seed + 401) * 0.08,
                    0.95,
                ];

                self.add_window_generated_city_pedestrian(
                    x,
                    y,
                    0.0,
                    stature,
                    body_color,
                    skin_color,
                    side,
                    chunk.detail_seed ^ seed,
                    crowd_density,
                );
            }
        }

        if traffic_density <= 0.0 {
            return;
        }

        let traffic_count = window_generated_city_traffic_visual_count(chunk);
        let traffic_alpha = (0.16 + traffic_density * 0.34).clamp(0.0, 0.58);
        for lane in [-0.34_f32, 0.34] {
            self.world_oriented_rect_with_surface_response(
                [chunk.center_meters[0], road_y + lane, 0.082],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [span_x * 0.42, 0.028],
                [0.68, 0.66, 0.5, traffic_alpha * 0.7],
                [0.7, 0.0, 0.2, 0.0],
            );
        }

        if chunk.traffic_rule_count > 1 {
            self.world_cylinder_between_with_surface_response(
                [min_x + span_x * 0.08, road_y + 1.72, 1.18],
                [max_x - span_x * 0.08, road_y + 1.72, 1.18],
                0.025,
                window_generated_city_minor_segments(chunk),
                [0.15, 0.22, 0.25, 0.86],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            self.world_oriented_rect_with_surface_response(
                [chunk.center_meters[0], road_y + 1.72, 1.24],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [span_x * 0.24, 0.09],
                [0.62, 0.58, 0.44, 0.16 + traffic_density * 0.1],
                [0.58, 0.0, 0.08, 0.04],
            );
        }

        for index in 0..traffic_count {
            let seed = 9_001 + index as u64;
            let t = ((index as f32 + 0.35) / traffic_count as f32
                + phase * (0.12 + traffic_density * 0.08))
                .fract();
            let x = min_x + span_x * (0.08 + t * 0.84);
            let lane = if index % 2 == 0 { -0.32 } else { 0.32 };
            let y = road_y + lane;
            let body_color = window_generated_city_traffic_body_color(chunk);

            if matches!(
                chunk.district_kind,
                WindowCityDistrictVisualKind::CorporateCore
                    | WindowCityDistrictVisualKind::ClinicDistrict
            ) && index.is_multiple_of(2)
            {
                let z = 1.28 + window_stable_unit(chunk.detail_seed, seed) * 0.42;
                self.add_window_city_hover_vehicle(x, y, z, body_color, traffic_density, seed);
            } else {
                let length = if matches!(
                    chunk.district_kind,
                    WindowCityDistrictVisualKind::IndustrialDock
                ) {
                    0.58
                } else {
                    0.46
                };
                self.add_window_city_ground_vehicle(
                    x,
                    y,
                    if lane < 0.0 { 1.0 } else { -1.0 },
                    length,
                    body_color,
                    traffic_density,
                    seed,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_window_generated_city_pedestrian(
        &mut self,
        x: f32,
        y: f32,
        z: f32,
        stature: f32,
        body_color: [f32; 4],
        skin_color: [f32; 4],
        side: f32,
        seed: u64,
        detail_strength: f32,
    ) {
        let forward = [0.0, -side, 0.0];
        let right = [1.0, 0.0, 0.0];
        let up = [0.0, 0.0, 1.0];
        let height = 1.54 * stature;
        let shoulder_z = z + height * 0.58;
        let hip_z = z + height * 0.34;
        let head_z = z + height * 0.84;
        let front = scale3(forward, 0.055 + detail_strength * 0.018);
        let cloth_shadow = window_scale_color(body_color, 0.72);
        let hair_color = window_clamp_color([
            skin_color[0] * 0.08 + 0.025 + window_stable_unit(seed, 23_011) * 0.04,
            skin_color[1] * 0.07 + 0.02,
            skin_color[2] * 0.06 + 0.018,
            0.96,
        ]);

        self.world_ellipsoid_with_surface_response(
            [x, y, z + height * 0.46],
            [0.135 * stature, 0.085 * stature, 0.28 * stature],
            7,
            12,
            body_color,
            WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH,
        );
        self.world_ellipsoid_with_surface_response(
            [x, y, hip_z],
            [0.12 * stature, 0.075 * stature, 0.1 * stature],
            5,
            10,
            cloth_shadow,
            WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH,
        );
        self.world_ellipsoid_with_surface_response(
            [x, y - side * 0.006, shoulder_z],
            [0.18 * stature, 0.052 * stature, 0.052 * stature],
            5,
            12,
            window_scale_color(body_color, 0.86),
            WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH,
        );
        self.world_oriented_rect_with_surface_response(
            add3([x, y, z + height * 0.59], front),
            right,
            up,
            [0.048 * stature, 0.13 * stature],
            [0.028, 0.026, 0.024, 0.7],
            WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH,
        );
        self.world_cylinder_between_with_surface_response(
            [x, y, shoulder_z + 0.02],
            [x, y, head_z - 0.13 * stature],
            0.045 * stature,
            10,
            skin_color,
            WINDOW_SURFACE_RESPONSE_HUMAN_SKIN,
        );
        self.world_ellipsoid_with_surface_response(
            [x, y, head_z],
            [0.088 * stature, 0.072 * stature, 0.108 * stature],
            8,
            14,
            skin_color,
            WINDOW_SURFACE_RESPONSE_HUMAN_SKIN,
        );
        self.world_ellipsoid_with_surface_response(
            [x, y - side * 0.01, head_z + 0.052 * stature],
            [0.094 * stature, 0.074 * stature, 0.054 * stature],
            5,
            12,
            hair_color,
            WINDOW_SURFACE_RESPONSE_HUMAN_HAIR,
        );
        for ear_side in [-1.0_f32, 1.0] {
            self.world_ellipsoid_with_surface_response(
                [
                    x + ear_side * 0.082 * stature,
                    y + side * 0.006,
                    head_z + 0.006 * stature,
                ],
                [0.014 * stature, 0.01 * stature, 0.024 * stature],
                4,
                8,
                window_scale_color(skin_color, 0.92),
                WINDOW_SURFACE_RESPONSE_HUMAN_SKIN,
            );
        }
        self.world_ellipsoid_with_surface_response(
            add3(
                [x, y, head_z - 0.012 * stature],
                scale3(forward, 0.067 * stature),
            ),
            [0.013 * stature, 0.014 * stature, 0.024 * stature],
            4,
            8,
            window_scale_color(skin_color, 0.88),
            WINDOW_SURFACE_RESPONSE_HUMAN_SKIN,
        );
        for strand in 0..4 {
            let lateral = (-0.045 + strand as f32 * 0.03) * stature;
            let drop = 0.036 + window_stable_unit(seed, 23_201 + strand) * 0.035;
            self.world_oriented_rect_with_surface_response(
                add3(
                    [x + lateral, y, head_z + 0.024 * stature - drop * 0.3],
                    scale3(forward, 0.055 * stature),
                ),
                right,
                up,
                [0.0035 * stature, drop * stature],
                window_scale_color(hair_color, 0.82 + strand as f32 * 0.05),
                WINDOW_SURFACE_RESPONSE_HUMAN_HAIR,
            );
        }

        for arm_side in [-1.0_f32, 1.0] {
            let shoulder = [x + arm_side * 0.135 * stature, y, shoulder_z];
            let hand = [
                x + arm_side * 0.16 * stature,
                y - side
                    * (0.035 + window_stable_unit(seed, 23_401 + (arm_side > 0.0) as u64) * 0.035),
                z + height * 0.31,
            ];
            self.world_cylinder_between_with_surface_response(
                shoulder,
                hand,
                0.029 * stature,
                10,
                window_mix_color(body_color, skin_color, 0.16),
                WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH,
            );
            self.world_ellipsoid_with_surface_response(
                hand,
                [0.034 * stature, 0.022 * stature, 0.029 * stature],
                5,
                8,
                skin_color,
                WINDOW_SURFACE_RESPONSE_HUMAN_SKIN,
            );
            self.world_ellipsoid_with_surface_response(
                [
                    hand[0] - arm_side * 0.018 * stature,
                    hand[1] + side * 0.012,
                    hand[2] + 0.055 * stature,
                ],
                [0.032 * stature, 0.018 * stature, 0.022 * stature],
                4,
                8,
                window_scale_color(body_color, 0.68),
                WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH,
            );
        }

        for leg_side in [-1.0_f32, 1.0] {
            let hip = [x + leg_side * 0.055 * stature, y, z + height * 0.3];
            let foot = [
                x + leg_side * 0.07 * stature,
                y - side * 0.03,
                z + height * 0.055,
            ];
            self.world_cylinder_between_with_surface_response(
                hip,
                foot,
                0.036 * stature,
                10,
                cloth_shadow,
                WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH,
            );
            self.world_ellipsoid_with_surface_response(
                [foot[0], foot[1] - side * 0.022, z + height * 0.035],
                [0.055 * stature, 0.034 * stature, 0.026 * stature],
                5,
                8,
                [0.025, 0.023, 0.022, body_color[3]],
                WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH,
            );
        }

        for eye_side in [-1.0_f32, 1.0] {
            self.world_oriented_rect_with_surface_response(
                add3(
                    [x + eye_side * 0.027 * stature, y, head_z + 0.018 * stature],
                    front,
                ),
                right,
                up,
                [0.008 * stature, 0.0035 * stature],
                [0.9, 0.94, 0.92, 0.82],
                WINDOW_SURFACE_RESPONSE_HUMAN_EYE,
            );
        }
        self.world_oriented_rect_with_surface_response(
            add3([x, y, head_z - 0.042 * stature], front),
            right,
            up,
            [0.026 * stature, 0.003 * stature],
            [0.18, 0.052, 0.048, 0.7],
            WINDOW_SURFACE_RESPONSE_HUMAN_SKIN,
        );
        self.world_oriented_rect_with_surface_response(
            add3([x, y, z + height * 0.49], front),
            right,
            up,
            [0.095 * stature, 0.005 * stature],
            [0.12, 0.11, 0.095, 0.34],
            WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH,
        );

        if window_stable_unit(seed, 24_101) > 0.42 {
            let bag_side = if window_stable_unit(seed, 24_203) > 0.5 {
                1.0
            } else {
                -1.0
            };
            let bag_center = [
                x + bag_side * 0.14 * stature,
                y + side * 0.082,
                z + height * 0.39,
            ];
            self.world_ellipsoid_with_surface_response(
                bag_center,
                [0.06 * stature, 0.035 * stature, 0.1 * stature],
                5,
                8,
                [0.052, 0.044, 0.036, 0.88],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
            self.world_oriented_rect_with_surface_response(
                [
                    x + bag_side * 0.075 * stature,
                    y + side * 0.052,
                    z + height * 0.54,
                ],
                normalize3([0.36 * bag_side, 0.0, -0.68]),
                [0.0, 1.0, 0.0],
                [0.11 * stature, 0.006 * stature],
                [0.035, 0.03, 0.026, 0.72],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        if detail_strength > 0.45 {
            self.world_oriented_rect_with_surface_response(
                add3([x, y, z + height * 0.45], front),
                right,
                up,
                [0.004 * stature, 0.105 * stature],
                [0.034, 0.03, 0.028, 0.82],
                WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH,
            );
            self.world_oriented_rect_with_surface_response(
                add3([x - 0.055 * stature, y, z + height * 0.27], front),
                right,
                up,
                [0.032 * stature, 0.045 * stature],
                [0.06, 0.052, 0.04, 0.36],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }
    }

    fn add_window_city_hover_vehicle(
        &mut self,
        x: f32,
        y: f32,
        z: f32,
        body_color: [f32; 4],
        traffic_density: f32,
        seed: u64,
    ) {
        self.world_ellipsoid_with_surface_response(
            [x, y, z],
            [0.32, 0.15, 0.09],
            5,
            12,
            body_color,
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        self.world_ellipsoid_with_surface_response(
            [x - 0.12, y, z - 0.025],
            [0.23, 0.105, 0.036],
            4,
            8,
            window_scale_color(body_color, 0.76),
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        self.world_ellipsoid_with_surface_response(
            [x + 0.02, y, z + 0.055],
            [0.15, 0.105, 0.052],
            4,
            8,
            window_mix_color(body_color, [0.58, 0.72, 0.78, 0.86], 0.36),
            WINDOW_SURFACE_RESPONSE_GLASS,
        );
        for rotor_side in [-1.0_f32, 1.0] {
            self.world_ellipse_ring_with_surface_response(
                [x, y + rotor_side * 0.23, z + 0.012],
                [0.12, 0.024],
                [0.22, 0.048],
                12,
                [0.09, 0.1, 0.105, 0.64],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            self.world_oriented_rect_with_surface_response(
                [x, y + rotor_side * 0.23, z + 0.018],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.22, 0.008],
                [0.12, 0.13, 0.13, 0.52],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        }
        for fin_side in [-1.0_f32, 1.0] {
            self.world_oriented_rect_with_surface_response(
                [x - 0.05, y + fin_side * 0.16, z - 0.015],
                [1.0, 0.0, 0.0],
                normalize3([0.0, fin_side * 0.16, 0.72]),
                [0.17, 0.026],
                [0.07, 0.08, 0.082, 0.66],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        }
        let nav_alpha = 0.18 + traffic_density * 0.14 + window_stable_unit(seed, 25_109) * 0.04;
        self.world_oriented_rect_with_surface_response(
            [x + 0.27, y, z - 0.005],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.026, 0.014],
            [1.0, 0.78, 0.42, nav_alpha],
            WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn add_window_city_ground_vehicle(
        &mut self,
        x: f32,
        y: f32,
        heading: f32,
        length: f32,
        body_color: [f32; 4],
        traffic_density: f32,
        seed: u64,
    ) {
        let body_z = 0.29 + traffic_density * 0.025;
        self.world_ellipse_ring_with_surface_response(
            [x, y, 0.083],
            [length * 0.38, 0.12],
            [length * 1.08, 0.34],
            22,
            [0.026, 0.022, 0.018, 0.18 + traffic_density * 0.1],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
        self.world_ellipsoid_with_surface_response(
            [x, y, body_z],
            [length * 0.86, 0.18, 0.18],
            5,
            14,
            body_color,
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        self.world_ellipsoid_with_surface_response(
            [x + heading * length * 0.3, y, body_z + 0.012],
            [length * 0.36, 0.15, 0.08],
            4,
            9,
            window_scale_color(body_color, 0.9),
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        self.world_ellipsoid_with_surface_response(
            [x - heading * length * 0.06, y, body_z + 0.16],
            [length * 0.42, 0.13, 0.11],
            5,
            10,
            window_mix_color(body_color, [0.5, 0.64, 0.7, 0.86], 0.34),
            WINDOW_SURFACE_RESPONSE_GLASS,
        );
        self.world_oriented_rect_with_surface_response(
            [x + heading * length * 0.31, y, body_z + 0.23],
            [0.0, 1.0, 0.0],
            normalize3([-heading * 0.26, 0.0, 0.72]),
            [0.095, 0.072],
            [0.5, 0.64, 0.68, 0.58],
            WINDOW_SURFACE_RESPONSE_GLASS,
        );
        for side in [-1.0_f32, 1.0] {
            self.world_oriented_rect_with_surface_response(
                [
                    x - heading * length * 0.08,
                    y + side * 0.184,
                    body_z + 0.045,
                ],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [length * 0.34, 0.052],
                window_scale_color(body_color, 0.72),
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            self.world_ellipsoid_with_surface_response(
                [x + heading * length * 0.36, y + side * 0.22, body_z + 0.12],
                [0.038, 0.018, 0.028],
                4,
                8,
                [0.04, 0.044, 0.044, 0.82],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        }

        for axle in [-0.48_f32, 0.48] {
            for wheel_side in [-1.0_f32, 1.0] {
                let wheel_x = x + axle * length;
                let wheel_y = y + wheel_side * 0.19;
                self.world_ellipse_ring_with_surface_response(
                    [wheel_x, wheel_y, 0.172],
                    [0.062, 0.02],
                    [0.12, 0.048],
                    12,
                    [0.028, 0.026, 0.024, 0.72],
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                );
                self.world_cylinder_between_with_surface_response(
                    [wheel_x, wheel_y - wheel_side * 0.035, 0.16],
                    [wheel_x, wheel_y + wheel_side * 0.035, 0.16],
                    0.078,
                    10,
                    [0.014, 0.014, 0.015, 1.0],
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                );
                self.world_ellipsoid_with_surface_response(
                    [wheel_x, wheel_y, 0.16],
                    [0.034, 0.018, 0.034],
                    4,
                    7,
                    [0.16, 0.17, 0.17, 0.88],
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
                self.world_ellipsoid_with_surface_response(
                    [wheel_x, wheel_y + wheel_side * 0.043, 0.16],
                    [0.02, 0.01, 0.02],
                    3,
                    6,
                    [0.46, 0.45, 0.38, 0.64],
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
            }
        }

        let front_x = x + heading * length * 0.78;
        let rear_x = x - heading * length * 0.78;
        self.world_cylinder_between_with_surface_response(
            [front_x, y - 0.15, body_z - 0.055],
            [front_x, y + 0.15, body_z - 0.055],
            0.018,
            10,
            [0.06, 0.062, 0.058, 0.82],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        self.world_cylinder_between_with_surface_response(
            [rear_x, y - 0.15, body_z - 0.05],
            [rear_x, y + 0.15, body_z - 0.05],
            0.016,
            10,
            [0.035, 0.034, 0.032, 0.8],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        for lamp_side in [-1.0_f32, 1.0] {
            self.world_oriented_rect_with_surface_response(
                [front_x, y + lamp_side * 0.105, body_z + 0.02],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.022, 0.014],
                [1.0, 0.86, 0.56, 0.34 + traffic_density * 0.08],
                WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
            );
            self.world_oriented_rect_with_surface_response(
                [rear_x, y + lamp_side * 0.105, body_z + 0.015],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.018, 0.012],
                [0.76, 0.08, 0.045, 0.32],
                WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
            );
        }

        let grime = 0.22 + window_stable_unit(seed, 26_017) * 0.18 + traffic_density * 0.16;
        self.world_oriented_rect_with_surface_response(
            [x - heading * length * 0.08, y - 0.182, body_z - 0.02],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [length * 0.42, 0.035],
            [0.038, 0.031, 0.024, grime],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
    }

    fn add_window_generated_city_detail_layout(
        &mut self,
        chunk: WindowGeneratedCityChunkVisual,
        min_x: f32,
        max_x: f32,
        min_y: f32,
        max_y: f32,
    ) {
        let detail_budget = window_generated_city_detail_budget(chunk);
        if detail_budget == 0 {
            return;
        }

        self.add_window_generated_city_overhead_cables(
            chunk,
            min_x,
            max_x,
            min_y,
            max_y,
            detail_budget,
        );
        self.add_window_generated_city_street_props(chunk, min_x, max_x, detail_budget);
        self.add_window_generated_city_district_set_piece(
            chunk,
            min_x,
            max_x,
            min_y,
            max_y,
            detail_budget,
        );
        self.add_window_generated_city_wall_marks(chunk, min_x, max_x, min_y, max_y, detail_budget);
    }

    fn add_window_generated_city_overhead_cables(
        &mut self,
        chunk: WindowGeneratedCityChunkVisual,
        min_x: f32,
        max_x: f32,
        min_y: f32,
        max_y: f32,
        detail_budget: usize,
    ) {
        let span_x = (max_x - min_x).abs().max(1.0);
        let cable_count = match chunk.district_kind {
            WindowCityDistrictVisualKind::CorporateCore => (detail_budget / 2).max(1),
            WindowCityDistrictVisualKind::IndustrialDock => detail_budget.min(4),
            WindowCityDistrictVisualKind::ClinicDistrict => (detail_budget / 2).max(1),
            WindowCityDistrictVisualKind::RainAlleySlum
            | WindowCityDistrictVisualKind::BlackMarket => (detail_budget + 1).min(6),
        };

        for index in 0..cable_count {
            let t = (index as f32 + 0.62) / (cable_count as f32 + 0.24);
            let x = min_x + span_x * t;
            let sag = 0.18 + chunk.pollution * 0.18 + chunk.crime_pressure * 0.1;
            let z = 2.45
                + window_stable_unit(chunk.detail_seed, 5_101 + index as u64) * 1.15
                + chunk.surveillance_level * 0.45;
            let cable_color = match chunk.district_kind {
                WindowCityDistrictVisualKind::CorporateCore => [0.12, 0.18, 0.24, 1.0],
                WindowCityDistrictVisualKind::ClinicDistrict => [0.16, 0.22, 0.24, 1.0],
                _ => [0.018, 0.017, 0.022, 1.0],
            };
            let start = [x - 0.18, min_y + 0.86, z];
            let mid = [x, (min_y + max_y) * 0.5, z - sag];
            let end = [x + 0.18, max_y - 0.86, z + 0.08];
            self.world_cylinder_between_with_surface_response(
                start,
                mid,
                0.014,
                6,
                cable_color,
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            self.world_cylinder_between_with_surface_response(
                mid,
                end,
                0.014,
                6,
                cable_color,
                WINDOW_SURFACE_RESPONSE_METAL,
            );

            if chunk.faction_strength > 0.2 && index % 2 == 0 {
                self.world_box_with_surface_response(
                    [x - 0.045, mid[1] - 0.045, mid[2] - 0.045],
                    [x + 0.045, mid[1] + 0.045, mid[2] + 0.045],
                    window_with_alpha(chunk.faction_color, 0.38 + chunk.faction_strength * 0.26),
                    WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
                );
            }
        }
    }

    fn add_window_generated_city_street_props(
        &mut self,
        chunk: WindowGeneratedCityChunkVisual,
        min_x: f32,
        max_x: f32,
        detail_budget: usize,
    ) {
        let span_x = (max_x - min_x).abs().max(1.0);
        let road_y = chunk.center_meters[1];
        let prop_pressure =
            chunk.npc_count + chunk.hazard_count + chunk.streaming_dependency_count / 2;
        let prop_count = (detail_budget + prop_pressure.min(3) as usize).clamp(1, 5);
        let minor_segments = window_generated_city_minor_segments(chunk);

        for index in 0..prop_count {
            let t = (index as f32 + 0.42) / (prop_count as f32 + 0.3);
            let x = min_x + span_x * t;
            let side = if index % 2 == 0 { -1.0 } else { 1.0 };
            let y = road_y
                + side
                    * (0.98 + window_stable_unit(chunk.detail_seed, 5_301 + index as u64) * 0.58);
            let height = 0.18 + window_stable_unit(chunk.detail_seed, 5_401 + index as u64) * 0.36;
            let dirty_alpha =
                (0.42 + chunk.pollution * 0.34 + chunk.crime_pressure * 0.12).clamp(0.0, 0.9);
            let (color, surface) = match chunk.district_kind {
                WindowCityDistrictVisualKind::CorporateCore => {
                    ([0.12, 0.15, 0.17, 0.92], WINDOW_SURFACE_RESPONSE_METAL)
                }
                WindowCityDistrictVisualKind::IndustrialDock => {
                    ([0.24, 0.16, 0.08, 0.94], WINDOW_SURFACE_RESPONSE_METAL)
                }
                WindowCityDistrictVisualKind::ClinicDistrict => {
                    ([0.18, 0.26, 0.28, 0.86], WINDOW_SURFACE_RESPONSE_METAL)
                }
                _ => (
                    [0.045, 0.038, 0.032, dirty_alpha],
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                ),
            };

            if detail_budget >= 4 {
                self.world_micro_detailed_box_with_surface_response(
                    [x - 0.16, y - 0.11, 0.055],
                    [x + 0.16, y + 0.11, 0.055 + height],
                    color,
                    surface,
                    chunk.detail_seed ^ (index as u64).wrapping_mul(71) ^ 0x5701,
                    (0.34 + chunk.pollution * 0.22 + chunk.crime_pressure * 0.16).clamp(0.0, 1.0),
                );
            } else {
                self.world_ellipsoid_with_surface_response(
                    [x, y, 0.11 + height * 0.34],
                    [0.16, 0.1, 0.09 + height * 0.22],
                    4,
                    minor_segments,
                    window_scale_color(color, 0.82),
                    surface,
                );
            }

            let cluster_seed = chunk.detail_seed ^ 0x5711 ^ index as u64;
            let bag_offset = if side < 0.0 { -0.18 } else { 0.18 };
            self.world_ellipsoid_with_surface_response(
                [
                    x - 0.21 + window_stable_unit(cluster_seed, 12_101) * 0.08,
                    y + bag_offset,
                    0.13,
                ],
                [
                    0.09 + window_stable_unit(cluster_seed, 12_203) * 0.045,
                    0.055 + window_stable_unit(cluster_seed, 12_307) * 0.026,
                    0.075 + window_stable_unit(cluster_seed, 12_401) * 0.052,
                ],
                5,
                10,
                [0.03, 0.027, 0.023, 0.78],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
            if index.is_multiple_of(2) {
                let barrel_x = x + 0.23;
                let barrel_y = y - side * 0.16;
                self.world_cylinder_between_with_surface_response(
                    [barrel_x, barrel_y, 0.07],
                    [barrel_x, barrel_y, 0.44 + height * 0.12],
                    0.075,
                    minor_segments,
                    window_mix_color(color, [0.18, 0.12, 0.07, 0.92], 0.34),
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
                for cap_z in [0.08, 0.44 + height * 0.12] {
                    self.world_ellipse_ring_with_surface_response(
                        [barrel_x, barrel_y, cap_z],
                        [0.04, 0.024],
                        [0.088, 0.052],
                        minor_segments,
                        [0.035, 0.032, 0.028, 0.52],
                        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                    );
                }
            } else {
                self.world_ellipse_ring_with_surface_response(
                    [x + 0.22, y - side * 0.13, 0.095],
                    [0.07, 0.035],
                    [0.15, 0.082],
                    minor_segments,
                    [0.018, 0.017, 0.016, 0.86],
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                );
                self.world_cylinder_between_with_surface_response(
                    [x + 0.1, y - side * 0.24, 0.09],
                    [x + 0.36, y - side * 0.08, 0.12],
                    0.008,
                    4,
                    [0.028, 0.026, 0.024, 0.7],
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                );
            }
            if chunk.crime_pressure > 0.35 || index.is_multiple_of(3) {
                self.world_oriented_rect_with_surface_response(
                    [x, y + side * 0.02, 0.52 + height * 0.42],
                    [1.0, 0.0, 0.0],
                    normalize3([0.0, side * 0.35, -0.45]),
                    [0.18, 0.08],
                    [0.09, 0.055, 0.04, 0.42 + chunk.crime_pressure * 0.16],
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                );
            }

            if chunk.pollution > 0.35 || chunk.hazard_count > 0 {
                self.world_ellipse_ring_with_surface_response(
                    [x, y, 0.062],
                    [0.16, 0.06],
                    [0.38 + chunk.pollution * 0.18, 0.14 + chunk.pollution * 0.08],
                    minor_segments,
                    [0.14, 0.24, 0.18, 0.16 + chunk.pollution * 0.22],
                    WINDOW_SURFACE_RESPONSE_WET_ROAD,
                );
            }
        }
    }

    fn add_window_generated_city_district_set_piece(
        &mut self,
        chunk: WindowGeneratedCityChunkVisual,
        min_x: f32,
        max_x: f32,
        min_y: f32,
        max_y: f32,
        detail_budget: usize,
    ) {
        let center_x = chunk.center_meters[0];
        let road_y = chunk.center_meters[1];
        let span_x = (max_x - min_x).abs().max(1.0);
        let major_segments = window_generated_city_major_segments(chunk);
        let minor_segments = window_generated_city_minor_segments(chunk);
        let (ellipsoid_lat, ellipsoid_lon) = window_generated_city_ellipsoid_segments(chunk);

        match chunk.district_kind {
            WindowCityDistrictVisualKind::CorporateCore => {
                let pylon_count = detail_budget.clamp(1, 3);
                for index in 0..pylon_count {
                    let t = (index as f32 + 0.5) / pylon_count as f32;
                    let x = min_x + span_x * t;
                    for side in [-1.0_f32, 1.0] {
                        let y = road_y + side * 0.82;
                        self.world_cylinder_between_with_surface_response(
                            [x, y, 0.06],
                            [x, y, 0.86],
                            0.06,
                            minor_segments,
                            [0.09, 0.13, 0.16, 0.92],
                            WINDOW_SURFACE_RESPONSE_METAL,
                        );
                        self.world_ellipsoid_with_surface_response(
                            [x, y, 0.94],
                            [0.095, 0.075, 0.08],
                            ellipsoid_lat,
                            minor_segments,
                            [0.66, 0.72, 0.64, 0.2 + chunk.surveillance_level * 0.16],
                            WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
                        );
                    }
                }
            }
            WindowCityDistrictVisualKind::RainAlleySlum => {
                self.world_oriented_rect_with_surface_response(
                    [center_x, road_y - 1.34, 1.42],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, -0.18],
                    [span_x.clamp(4.0, 12.0) * 0.18, 0.34],
                    [0.08, 0.05, 0.04, 0.82],
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                );
                self.world_oriented_rect_with_surface_response(
                    [center_x + span_x * 0.18, road_y + 1.26, 1.36],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.12],
                    [span_x.clamp(4.0, 10.0) * 0.16, 0.3],
                    [0.12, 0.035, 0.05, 0.78],
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                );
            }
            WindowCityDistrictVisualKind::IndustrialDock => {
                let tank_x = center_x - span_x * 0.18;
                let tank_y = max_y - 1.28;
                self.world_cylinder_between_with_surface_response(
                    [tank_x, tank_y, 0.08],
                    [tank_x, tank_y, 1.38],
                    0.28,
                    major_segments,
                    [0.11, 0.13, 0.13, 0.96],
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
                self.world_ellipsoid_with_surface_response(
                    [tank_x, tank_y, 1.42],
                    [0.29, 0.29, 0.08],
                    ellipsoid_lat,
                    ellipsoid_lon,
                    [0.13, 0.14, 0.13, 0.9],
                    WINDOW_SURFACE_RESPONSE_METAL,
                );
                for index in 0..detail_budget.clamp(1, 3) {
                    let x = center_x + index as f32 * 0.44 - 0.55;
                    self.world_cylinder_between_with_surface_response(
                        [x, min_y + 0.96, 0.06],
                        [x, min_y + 0.96, 0.46],
                        0.14,
                        minor_segments,
                        [0.28, 0.13, 0.045, 0.88],
                        WINDOW_SURFACE_RESPONSE_METAL,
                    );
                }
            }
            WindowCityDistrictVisualKind::BlackMarket => {
                let stall_count = detail_budget.clamp(1, 4);
                for index in 0..stall_count {
                    let t = (index as f32 + 0.5) / stall_count as f32;
                    let x = min_x + span_x * t;
                    let y = road_y + if index % 2 == 0 { -1.22 } else { 1.22 };
                    self.world_ellipsoid_with_surface_response(
                        [x, y, 0.28],
                        [0.34, 0.2, 0.22],
                        ellipsoid_lat,
                        ellipsoid_lon,
                        [0.05, 0.035, 0.042, 0.86],
                        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                    );
                    self.world_oriented_rect_with_surface_response(
                        [x, y, 0.82],
                        [1.0, 0.0, 0.0],
                        normalize3([0.0, 1.0, -0.24]),
                        [0.48, 0.26],
                        [0.16, 0.045, 0.13, 0.84],
                        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                    );
                    self.world_cylinder_between_with_surface_response(
                        [x - 0.28, y - 0.21, 0.88],
                        [x + 0.28, y - 0.21, 0.88],
                        0.018,
                        minor_segments,
                        [0.82, 0.24, 0.56, 0.32],
                        WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
                    );
                }
            }
            WindowCityDistrictVisualKind::ClinicDistrict => {
                let triage_count = detail_budget.clamp(1, 3);
                for index in 0..triage_count {
                    let t = (index as f32 + 0.55) / triage_count as f32;
                    let x = min_x + span_x * t;
                    self.world_ellipsoid_with_surface_response(
                        [x, road_y - 0.96, 0.34],
                        [0.22, 0.15, 0.28],
                        ellipsoid_lat,
                        ellipsoid_lon,
                        [0.16, 0.24, 0.26, 0.86],
                        WINDOW_SURFACE_RESPONSE_METAL,
                    );
                    self.world_oriented_rect_with_surface_response(
                        [x, road_y - 0.81, 0.72],
                        [1.0, 0.0, 0.0],
                        [0.0, 0.0, 1.0],
                        [0.18, 0.1],
                        [0.56, 0.82, 0.68, 0.34],
                        WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
                    );
                }
            }
        }
    }

    fn add_window_generated_city_wall_marks(
        &mut self,
        chunk: WindowGeneratedCityChunkVisual,
        min_x: f32,
        max_x: f32,
        min_y: f32,
        max_y: f32,
        detail_budget: usize,
    ) {
        if chunk.pollution <= 0.08 && chunk.crime_pressure <= 0.08 && chunk.faction_strength <= 0.08
        {
            return;
        }

        let span_x = (max_x - min_x).abs().max(1.0);
        let mark_count = (detail_budget + (chunk.crime_pressure * 3.0).ceil() as usize).clamp(2, 8);
        for index in 0..mark_count {
            let t = (index as f32 + 0.5) / mark_count as f32;
            let x = min_x + span_x * t;
            let side = if index % 2 == 0 { -1.0 } else { 1.0 };
            let face_y = if side < 0.0 {
                min_y + 2.28
            } else {
                max_y - 2.28
            };
            let z = 0.72 + window_stable_unit(chunk.detail_seed, 5_601 + index as u64) * 1.35;
            let use_faction = chunk.faction_strength > 0.18 && index % 3 == 0;
            let color = if use_faction {
                window_with_alpha(chunk.faction_color, 0.24 + chunk.faction_strength * 0.36)
            } else {
                [
                    0.055,
                    0.05,
                    0.045,
                    (0.16 + chunk.pollution * 0.3 + chunk.crime_pressure * 0.16).clamp(0.0, 0.62),
                ]
            };
            let surface = if use_faction {
                WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
            } else {
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
            };
            self.world_oriented_rect_with_surface_response(
                [x, face_y + side * 0.014, z],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [
                    0.24 + chunk.crime_pressure * 0.16,
                    0.08 + chunk.pollution * 0.08,
                ],
                color,
                surface,
            );
        }
    }

    fn add_window_generated_city_navigation_edge(
        &mut self,
        edge: WindowGeneratedCityNavigationEdgeVisual,
    ) {
        let color = window_generated_city_navigation_color(edge.traversal, edge.route_priority);
        let start = [
            edge.start_meters[0],
            edge.start_meters[1],
            edge.start_meters[2] + 0.1,
        ];
        let end = [
            edge.end_meters[0],
            edge.end_meters[1],
            edge.end_meters[2] + 0.1,
        ];
        self.world_cylinder_between_with_surface_response(
            start,
            end,
            0.034 + edge.route_priority * 0.018,
            8,
            color,
            WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
        );
    }

    fn add_window_generated_city_navigation_node(
        &mut self,
        node: WindowGeneratedCityNavigationNodeVisual,
        frame_index: u64,
    ) {
        let pulse = (frame_index as f32 * 0.047 + node.position_meters[0] * 0.006)
            .sin()
            .mul_add(0.5, 0.5);
        let base_color = if node.story_focus {
            [1.0, 0.62, 0.24, 0.86]
        } else if node.danger_level > node.surveillance_level {
            [1.0, 0.32, 0.2, 0.68]
        } else if node.important {
            [0.28, 0.82, 1.0, 0.74]
        } else {
            [0.34, 1.0, 0.62, 0.42]
        };
        let height = if node.story_focus {
            0.78 + pulse * 0.2
        } else if node.important {
            0.48 + pulse * 0.16
        } else {
            0.28 + pulse * 0.08
        };
        let position = node.position_meters;
        self.world_cylinder_between_with_surface_response(
            [position[0], position[1], position[2] + 0.04],
            [position[0], position[1], position[2] + 0.04 + height],
            if node.important { 0.04 } else { 0.026 },
            8,
            base_color,
            WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
        );
        self.world_ellipse_ring_with_surface_response(
            [position[0], position[1], position[2] + 0.05],
            [0.12, 0.07],
            [
                0.32 + node.danger_level * 0.18,
                0.18 + node.surveillance_level * 0.12,
            ],
            14,
            window_with_alpha(base_color, base_color[3] * (0.36 + pulse * 0.24)),
            WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
        );
    }

    pub fn add_window_alley_weather(&mut self, frame_index: u64, _camera: WindowPerspectiveCamera) {
        let phase = (frame_index as f32 * 0.037).fract();
        let rain_color = [0.5, 0.62, 0.72, 0.24];
        for index in 0..34 {
            let column = index % 7;
            let row = index / 7;
            let jitter = ((index * 37) % 100) as f32 / 100.0;
            let x = -4.65 + column as f32 * 1.5 + jitter * 0.28;
            let fall = (phase + jitter * 0.73).fract();
            let y = -9.6 + ((row as f32 * 3.15 + fall * 23.8) % 23.8);
            let z = 3.55 - ((jitter * 1.9 + phase * 0.45) % 0.62);
            self.world_cylinder_between(
                [x, y, z],
                [x + 0.16, y - 0.42, z - 0.92],
                0.006,
                4,
                rain_color,
            );
        }

        for (index, center) in [
            [-3.6, -7.4, 0.021],
            [-1.2, -2.1, 0.021],
            [2.6, 1.4, 0.021],
            [3.8, 7.3, 0.021],
            [-2.4, 11.2, 0.021],
        ]
        .into_iter()
        .enumerate()
        {
            let seed = 91_001 + index as u64 * 709;
            let ripple = (phase + index as f32 * 0.19).fract();
            let axis = normalize3([
                0.48 + window_stable_unit(seed, 1) * 0.52,
                -0.42 + window_stable_unit(seed, 2) * 0.84,
                0.0,
            ]);
            let side = normalize3([-axis[1], axis[0], 0.0]);
            let length = 0.24 + ripple * 0.34 + window_stable_unit(seed, 3) * 0.16;
            let width = 0.035 + ripple * 0.070 + window_stable_unit(seed, 4) * 0.035;
            self.world_quad_with_surface_response(
                [
                    center[0] - axis[0] * length * 0.88 - side[0] * width * 0.42,
                    center[1] - axis[1] * length * 0.88 - side[1] * width * 0.42,
                    center[2] - 0.002,
                ],
                [
                    center[0] + axis[0] * length * 0.34 - side[0] * width * 0.78,
                    center[1] + axis[1] * length * 0.34 - side[1] * width * 0.78,
                    center[2] - 0.001,
                ],
                [
                    center[0] + axis[0] * length * 0.96 + side[0] * width * 0.46,
                    center[1] + axis[1] * length * 0.96 + side[1] * width * 0.46,
                    center[2],
                ],
                [
                    center[0] - axis[0] * length * 0.52 + side[0] * width * 0.92,
                    center[1] - axis[1] * length * 0.52 + side[1] * width * 0.92,
                    center[2] - 0.001,
                ],
                [0.032, 0.078, 0.092, 0.16],
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            );

            for dash in 0..4 {
                let dash_seed = seed ^ (dash as u64 * 37);
                let offset =
                    (dash as f32 - 1.5) * 0.12 + (window_stable_unit(dash_seed, 5) - 0.5) * 0.08;
                let splash_center = [
                    center[0]
                        + axis[0] * offset
                        + side[0] * (window_stable_unit(dash_seed, 6) - 0.5) * 0.12,
                    center[1]
                        + axis[1] * offset
                        + side[1] * (window_stable_unit(dash_seed, 7) - 0.5) * 0.12,
                    center[2] + 0.004 + dash as f32 * 0.0005,
                ];
                self.world_oriented_rect_with_surface_response(
                    splash_center,
                    axis,
                    side,
                    [
                        0.028 + window_stable_unit(dash_seed, 8) * 0.052,
                        0.0035 + window_stable_unit(dash_seed, 9) * 0.006,
                    ],
                    [0.44, 0.58, 0.62, 0.09 * (1.0 - ripple * 0.42)],
                    WINDOW_SURFACE_RESPONSE_WET_ROAD,
                );
            }
        }

        for (index, height) in [0.75, 1.08, 1.42, 1.78].into_iter().enumerate() {
            let drift = (phase + index as f32 * 0.23).fract();
            let center = [
                -1.38 + drift * 0.28,
                0.55 + drift * 0.42,
                height + drift * 0.08,
            ];
            let alpha = 0.075 * (1.0 - index as f32 * 0.08).max(0.35);
            for strip in 0..3 {
                let seed = 92_501 + index as u64 * 23 + strip;
                let offset = (strip as f32 - 1.0) * (0.12 + drift * 0.04);
                let right = normalize3([
                    0.22 + window_stable_unit(seed, 1) * 0.56,
                    0.78 + window_stable_unit(seed, 2) * 0.20,
                    0.0,
                ]);
                self.world_oriented_rect_with_surface_response(
                    [
                        center[0] + offset,
                        center[1] + (window_stable_unit(seed, 3) - 0.5) * 0.18,
                        center[2] + (window_stable_unit(seed, 4) - 0.5) * 0.12,
                    ],
                    right,
                    normalize3([
                        0.05 * (window_stable_unit(seed, 5) - 0.5),
                        0.08 * (window_stable_unit(seed, 6) - 0.5),
                        1.0,
                    ]),
                    [
                        0.035 + drift * 0.022 + strip as f32 * 0.006,
                        0.20 + index as f32 * 0.055,
                    ],
                    [0.50, 0.62, 0.64, alpha * (0.72 + strip as f32 * 0.10)],
                    WINDOW_SURFACE_RESPONSE_WET_ROAD,
                );
            }
        }

        for (index, (center, color)) in [
            ([1.0, 3.05, 2.48], [0.28, 0.27, 0.24, 0.026]),
            ([1.0, 2.85, 2.16], [0.25, 0.30, 0.31, 0.024]),
            ([-4.82, 3.4, 1.52], [0.22, 0.28, 0.29, 0.022]),
        ]
        .into_iter()
        .enumerate()
        {
            for strip in 0..4 {
                let seed = 93_701 + index as u64 * 41 + strip;
                let axis = normalize3([
                    0.65 + window_stable_unit(seed, 1) * 0.25,
                    -0.30 + window_stable_unit(seed, 2) * 0.60,
                    0.0,
                ]);
                self.world_oriented_rect_with_surface_response(
                    [
                        center[0] + (strip as f32 - 1.5) * 0.30,
                        center[1] + (window_stable_unit(seed, 3) - 0.5) * 0.26,
                        center[2] + (window_stable_unit(seed, 4) - 0.5) * 0.28,
                    ],
                    axis,
                    [0.0, 0.0, 1.0],
                    [
                        0.12 + window_stable_unit(seed, 5) * 0.16,
                        0.22 + window_stable_unit(seed, 6) * 0.20,
                    ],
                    color,
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                );
            }
        }
    }

    pub fn add_window_gas_volume(
        &mut self,
        volume: WindowGasVolumeVisual,
        frame_index: u64,
        camera: WindowPerspectiveCamera,
    ) {
        if volume.color[3] <= f32::EPSILON {
            return;
        }

        let radius = volume.radius_meters.max(0.05);
        let height = volume.height_meters.max(0.05);
        let visibility = volume.visibility_blocking.clamp(0.0, 1.0);
        let hazard = volume.hazard_level.clamp(0.0, 1.0);
        let phase = (frame_index as f32 * 0.021 + volume.phase_seed).fract();
        let right = camera.basis().right;
        let cross_right = [right[1], -right[0], 0.0];
        let base_alpha =
            (volume.color[3] * (0.42 + visibility * 0.46 + hazard * 0.12)).clamp(0.0, 0.76);

        for layer in 0..7 {
            let layer_fraction = layer as f32 / 6.0;
            let swirl = (phase + layer_fraction * 0.37).fract();
            let vertical = height * (0.12 + layer_fraction * 0.78);
            let lateral = (swirl * std::f32::consts::TAU).sin() * radius * 0.18;
            let forward = (swirl * std::f32::consts::TAU).cos() * radius * 0.12;
            let center = [
                volume.center[0] + right[0] * lateral + cross_right[0] * forward,
                volume.center[1] + right[1] * lateral + cross_right[1] * forward,
                volume.center[2] + vertical,
            ];
            let density = (1.0 - (layer_fraction - 0.42).abs() * 1.15).clamp(0.25, 1.0);
            let alpha = base_alpha * density * (1.0 - layer_fraction * 0.42);
            let color = [
                (volume.color[0] + hazard * 0.12).clamp(0.0, 1.0),
                (volume.color[1] + visibility * 0.06).clamp(0.0, 1.0),
                volume.color[2],
                alpha,
            ];
            self.world_billboard(
                center,
                right,
                radius * (1.18 + layer_fraction * 0.55 + swirl * 0.22),
                height * (0.18 + visibility * 0.08),
                color,
            );
            self.world_billboard(
                center,
                cross_right,
                radius * (0.85 + layer_fraction * 0.42),
                height * (0.16 + hazard * 0.05),
                window_with_alpha(color, alpha * 0.72),
            );
        }

        for ring in 0..3 {
            let pulse = (phase + ring as f32 * 0.22).fract();
            let outer = [
                radius * (0.42 + pulse * 0.58 + ring as f32 * 0.12),
                radius * (0.22 + pulse * 0.35 + ring as f32 * 0.07),
            ];
            let inner = [
                (outer[0] - radius * 0.1).max(0.03),
                (outer[1] - radius * 0.055).max(0.02),
            ];
            self.world_ellipse_ring(
                [volume.center[0], volume.center[1], volume.center[2] + 0.035],
                inner,
                outer,
                18,
                [
                    volume.color[0],
                    (volume.color[1] + visibility * 0.12).clamp(0.0, 1.0),
                    volume.color[2],
                    base_alpha * 0.34 * (1.0 - pulse * 0.5),
                ],
            );
        }
    }

    pub fn add_window_snapshot_entities(&mut self, options: WindowSnapshotSceneOptions<'_>) {
        for (entity, transform) in options.snapshot.transforms.iter() {
            let tags = options.snapshot.tags.find(*entity).map(Vec::as_slice);
            if window_tags_have(tags, "ground") {
                add_window_snapshot_entity(self, options, *entity, transform);
            }
        }

        for (entity, transform) in options.snapshot.transforms.iter() {
            let tags = options.snapshot.tags.find(*entity).map(Vec::as_slice);
            if !window_tags_have(tags, "ground") {
                add_window_snapshot_entity(self, options, *entity, transform);
            }
        }
    }

    pub fn add_window_entity_proxy(&mut self, proxy: WindowEntityProxyVisual) {
        let [x, y] = proxy.center_meters;
        let z = proxy.z_meters;
        let (min, max) = match proxy.kind {
            WindowEntityProxyKind::GenericRenderable => {
                ([x - 0.28, y - 0.28, z], [x + 0.28, y + 0.28, z + 0.8])
            }
            WindowEntityProxyKind::GlassWall => {
                ([x - 1.15, y - 0.08, z], [x + 1.15, y + 0.08, z + 2.45])
            }
            WindowEntityProxyKind::LightPanel => (
                [x - 0.64, y - 0.055, z - 0.22],
                [x + 0.64, y + 0.055, z + 0.22],
            ),
            WindowEntityProxyKind::LowHazardSurface => (
                [x - 0.56, y - 0.22, z + 0.002],
                [x + 0.56, y + 0.22, z + 0.035],
            ),
        };
        if matches!(proxy.kind, WindowEntityProxyKind::GenericRenderable) {
            self.add_window_rounded_generic_renderable_proxy(proxy, min, max);
            self.add_window_entity_proxy_state_detail(proxy, min, max);
            return;
        }
        self.world_micro_detailed_box_with_surface_state_detail(
            min,
            max,
            proxy.color,
            proxy.surface_response,
            WindowMicroDetailRecipe {
                seed: window_position_seed([x, y, z]),
                density: match proxy.kind {
                    WindowEntityProxyKind::GenericRenderable => 0.62,
                    WindowEntityProxyKind::GlassWall => 0.5 + proxy.crack_density * 0.32,
                    WindowEntityProxyKind::LightPanel => 0.28,
                    WindowEntityProxyKind::LowHazardSurface => 0.54 + proxy.moisture * 0.24,
                },
            },
            window_material_state_detail_from_proxy(proxy),
        );
        self.add_window_entity_proxy_state_detail(proxy, min, max);
    }

    fn add_window_rounded_generic_renderable_proxy(
        &mut self,
        proxy: WindowEntityProxyVisual,
        min: [f32; 3],
        max: [f32; 3],
    ) {
        let [x, y] = proxy.center_meters;
        let z = proxy.z_meters;
        let seed = window_position_seed([x, y, z]);
        let state_detail = window_material_state_detail_from_proxy(proxy);
        let body_color = window_mix_color(proxy.color, [0.24, 0.24, 0.22, proxy.color[3]], 0.22);
        let center_z = (min[2] + max[2]) * 0.5;
        let height = (max[2] - min[2]).abs().max(0.18);

        self.world_ellipsoid_with_surface_response(
            [x, y, center_z],
            [0.34, 0.24, height * 0.44],
            7,
            14,
            body_color,
            proxy.surface_response,
        );
        self.world_ellipsoid_with_surface_response(
            [x - 0.08, y + 0.02, z + height * 0.18],
            [0.24, 0.17, height * 0.16],
            5,
            10,
            [
                0.048,
                0.04,
                0.032,
                0.36 + proxy.soot * 0.18 + proxy.corrosion * 0.12,
            ],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );

        for index in 0..4 {
            let angle = index as f32 * std::f32::consts::FRAC_PI_2
                + window_stable_unit(seed, 27_003 + index as u64) * 0.42;
            let radius = 0.19 + window_stable_unit(seed, 27_403 + index as u64) * 0.07;
            let protrusion_center = [
                x + angle.cos() * radius,
                y + angle.sin() * radius,
                z + height * (0.26 + window_stable_unit(seed, 27_809 + index as u64) * 0.48),
            ];
            self.world_ellipsoid_with_surface_response(
                protrusion_center,
                [
                    0.045 + window_stable_unit(seed, 28_101 + index as u64) * 0.035,
                    0.032 + window_stable_unit(seed, 28_501 + index as u64) * 0.028,
                    0.055 + window_stable_unit(seed, 28_901 + index as u64) * 0.05,
                ],
                4,
                8,
                window_scale_color(body_color, 0.78 + index as f32 * 0.08),
                proxy.surface_response,
            );
        }

        self.world_cylinder_between_with_surface_response(
            [x - 0.22, y - 0.18, z + height * 0.22],
            [x + 0.24, y + 0.16, z + height * 0.74],
            0.018,
            8,
            [0.032, 0.035, 0.034, 0.78],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        self.world_cylinder_between_with_surface_response(
            [x - 0.17, y + 0.22, z + height * 0.32],
            [x + 0.19, y + 0.22, z + height * 0.32],
            0.014,
            8,
            [0.024, 0.026, 0.025, 0.72],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        for strap in [-0.11_f32, 0.12] {
            self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
                center: [x + strap, y - 0.25, z + height * 0.48],
                right: [0.0, 1.0, 0.0],
                up: [0.0, 0.0, 1.0],
                half_extents: [0.012, height * 0.24],
                color: [0.034, 0.03, 0.026, 0.62],
                surface_response: WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                state_detail,
            });
        }
        self.world_oriented_rect_with_surface_state_detail(WindowOrientedSurfaceRect {
            center: [x, y - 0.245, z + height * 0.58],
            right: [1.0, 0.0, 0.0],
            up: [0.0, 0.0, 1.0],
            half_extents: [0.16, 0.055],
            color: [0.08, 0.068, 0.052, 0.5],
            surface_response: WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            state_detail,
        });
        self.world_ellipse_ring_with_surface_response(
            [x, y, z + 0.012],
            [0.18, 0.12],
            [0.48, 0.36],
            20,
            [0.036, 0.03, 0.024, 0.22 + proxy.moisture * 0.12],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
        for chip in 0..5 {
            let chip_seed = seed ^ (chip as u64).wrapping_mul(0x9E37);
            let angle = window_stable_unit(chip_seed, 29_103) * std::f32::consts::TAU;
            let distance = 0.22 + window_stable_unit(chip_seed, 29_211) * 0.26;
            self.world_ellipsoid_with_surface_response(
                [
                    x + angle.cos() * distance,
                    y + angle.sin() * distance,
                    z + 0.028 + chip as f32 * 0.002,
                ],
                [
                    0.018 + window_stable_unit(chip_seed, 29_307) * 0.024,
                    0.012 + window_stable_unit(chip_seed, 29_409) * 0.018,
                    0.008 + window_stable_unit(chip_seed, 29_503) * 0.012,
                ],
                4,
                8,
                [0.056, 0.048, 0.038, 0.68],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }
    }

    fn add_window_entity_proxy_state_detail(
        &mut self,
        proxy: WindowEntityProxyVisual,
        min: [f32; 3],
        max: [f32; 3],
    ) {
        let [x, y] = proxy.center_meters;
        let z_floor = min[2].max(0.02);
        let center_z = (min[2] + max[2]) * 0.5;
        let height = (max[2] - min[2]).abs().max(0.05);

        if proxy.moisture > 0.08 {
            let radius_x = ((max[0] - min[0]).abs() * 0.72).clamp(0.22, 1.1);
            let radius_y = ((max[1] - min[1]).abs() * 1.45).clamp(0.16, 0.46);
            self.world_ellipse_ring_with_surface_response(
                [x, y, z_floor + 0.008],
                [radius_x * 0.48, radius_y * 0.42],
                [radius_x, radius_y],
                18,
                [
                    0.34,
                    0.72,
                    1.0,
                    (0.12 + proxy.moisture * 0.24).clamp(0.0, 0.42),
                ],
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            );
        }

        if proxy.crack_density > 0.05 {
            let alpha = (0.22 + proxy.crack_density * 0.42).clamp(0.0, 0.72);
            let crack_color = [0.9, 0.98, 1.0, alpha];
            for (offset, slope, length) in
                [(-0.32, 0.28, 0.22), (0.06, -0.22, 0.28), (0.36, 0.16, 0.18)]
            {
                self.world_oriented_rect_with_surface_response(
                    [x + offset, min[1] - 0.012, center_z + slope * 0.24],
                    normalize3([0.28, 0.0, slope]),
                    [0.0, 1.0, 0.0],
                    [length, 0.004],
                    crack_color,
                    if matches!(proxy.kind, WindowEntityProxyKind::GlassWall) {
                        WINDOW_SURFACE_RESPONSE_GLASS
                    } else {
                        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
                    },
                );
            }
        }

        if proxy.corrosion > 0.04 {
            let color = [
                0.34,
                0.16,
                0.06,
                (0.22 + proxy.corrosion * 0.42).clamp(0.0, 0.72),
            ];
            for band in 0..3 {
                let band_z = min[2] + height * (0.24 + band as f32 * 0.23);
                self.world_oriented_rect_with_surface_response(
                    [x, min[1] - 0.014, band_z],
                    [1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0],
                    [((max[0] - min[0]).abs() * 0.42).max(0.18), 0.026],
                    color,
                    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
                );
            }
        }

        if proxy.soot > 0.04 {
            self.world_billboard_with_surface_response(
                [x, min[1] - 0.018, center_z + height * 0.12],
                [1.0, 0.0, 0.0],
                ((max[0] - min[0]).abs() * 1.15).clamp(0.36, 1.6),
                (height * 0.35).clamp(0.16, 0.8),
                [
                    0.03,
                    0.025,
                    0.022,
                    (0.14 + proxy.soot * 0.34).clamp(0.0, 0.52),
                ],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        if proxy.heat > 0.05 || proxy.electrical_charge > 0.05 {
            let warm_alpha =
                (0.08 + proxy.heat * 0.24 + proxy.electrical_charge * 0.16).clamp(0.0, 0.48);
            self.world_billboard_with_surface_response(
                [x, y - 0.035, center_z],
                [1.0, 0.0, 0.0],
                ((max[0] - min[0]).abs() * 1.35).clamp(0.42, 1.8),
                (height * 0.62).clamp(0.22, 1.2),
                [1.0, 0.42, 0.14, warm_alpha],
                WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
            );
        }

        if proxy.electrical_charge > 0.12 {
            for spark in 0..3 {
                let offset = spark as f32 - 1.0;
                self.world_oriented_rect_with_surface_response(
                    [
                        x + offset * 0.13,
                        min[1] - 0.026,
                        center_z + height * (0.08 + spark as f32 * 0.12),
                    ],
                    normalize3([0.18, 0.0, 0.14 - spark as f32 * 0.05]),
                    [0.0, 1.0, 0.0],
                    [0.055 + proxy.electrical_charge * 0.028, 0.004],
                    [
                        1.0,
                        0.86,
                        0.32,
                        (0.28 + proxy.electrical_charge * 0.42).clamp(0.0, 0.82),
                    ],
                    WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
                );
            }
        }
    }

    pub fn add_known_procedural_mesh(&mut self, instance: WindowProceduralMeshInstance) -> bool {
        match instance.mesh.0 {
            WINDOW_MESH_ALLEY_GLASS_INTACT => {
                add_window_glass_wall_mesh(self, instance, false);
                true
            }
            WINDOW_MESH_ALLEY_GLASS_FRACTURED => {
                add_window_glass_wall_mesh(self, instance, true);
                true
            }
            WINDOW_MESH_MARA_HUMAN_BUNDLE => {
                add_window_human_bundle_mesh(self, instance);
                true
            }
            WINDOW_MESH_NEON_SIGN => {
                add_window_neon_sign_mesh(self, instance);
                true
            }
            WINDOW_MESH_SERVICE_PIPE => {
                add_window_service_pipe_mesh(self, instance);
                true
            }
            WINDOW_MESH_WET_ASPHALT => {
                add_window_wet_asphalt_mesh(self, instance);
                true
            }
            WINDOW_MESH_SECURITY_CAMERA => {
                add_window_security_camera_mesh(self, instance);
                true
            }
            WINDOW_MESH_SERVICE_DOOR => {
                add_window_service_door_mesh(self, instance);
                true
            }
            WINDOW_MESH_MARKET_STALL => {
                add_window_market_stall_mesh(self, instance);
                true
            }
            _ => false,
        }
    }

    pub fn add_humanoid_proxy(&mut self, instance: WindowHumanoidProxyInstance<'_>) {
        let [x, y] = instance.center_meters;
        let z = instance.z_meters;
        let (forward, right, up) = humanoid_instance_orientation_basis(&instance);
        let proxy = instance
            .human
            .map(human_proxy_geometry_for_state)
            .unwrap_or_else(|| human_proxy_geometry_for_quality(QualityTier::NormalRuntime));
        let orientation = HumanoidOrientationBasis { forward, right, up };
        let pose = HumanoidPoseWeights {
            locomotion: instance.locomotion_weight_0_to_1.clamp(0.0, 1.0),
            crouch: instance.crouch_weight_0_to_1.clamp(0.0, 1.0),
        };

        if proxy.body_parts.is_empty() {
            return;
        }

        self.add_humanoid_cohesive_silhouette_shell(
            [x, y, z],
            &instance,
            &proxy,
            orientation,
            pose,
        );

        if humanoid_allows_near_detail(&instance, &proxy)
            && proxy.quality_tier == QualityTier::ReferenceOfflineValidation
        {
            for body_part in &proxy.body_parts {
                self.add_humanoid_body_part(
                    [x, y, z],
                    body_part,
                    instance.color,
                    instance.surface_state.as_ref(),
                    proxy.detail_profile.silhouette_segments,
                    proxy.detail_profile.limb_volume_layer_count,
                    orientation,
                    pose,
                );
            }
        }

        self.add_humanoid_surface_details(&instance, &proxy);
        self.add_humanoid_face_features(instance, &proxy);
    }

    fn add_humanoid_cohesive_silhouette_shell(
        &mut self,
        origin: [f32; 3],
        instance: &WindowHumanoidProxyInstance<'_>,
        proxy: &HumanProxyGeometry,
        orientation: HumanoidOrientationBasis,
        pose: HumanoidPoseWeights,
    ) {
        let height = proxy.height_meters;
        let color = instance.color;
        let surface_state = instance.surface_state.as_ref();
        let skin_response = humanoid_skin_surface_response(surface_state);
        let clothing_response = humanoid_clothing_surface_response(surface_state);
        let hair_response = humanoid_hair_surface_response(surface_state);
        let skin = humanoid_default_skin_color(color);
        let skin_warm = humanoid_skin_warm_highlight_color(color, surface_state);
        let skin_shadow = humanoid_skin_soft_shadow_color(color, surface_state);
        let cloth = humanoid_clothing_color(color);
        let cloth_shadow = humanoid_clothing_crease_color(color, surface_state);
        let cloth_highlight = window_scale_color(cloth, 1.18);
        let boot = humanoid_boot_color(color);
        let hair = humanoid_hair_color(color, surface_state);
        let hair_shadow = humanoid_hairline_shadow_color(color, surface_state);
        let axes = [orientation.right, orientation.forward, orientation.up];
        let shell_segments = (proxy.detail_profile.silhouette_segments as usize).clamp(12, 24);
        let shell_rings = (shell_segments / 2).clamp(5, 10);
        let limb_segments = (shell_segments - 2).clamp(8, 16);
        let shoulder_half = (height * 0.135).clamp(0.19, 0.30);
        let hip_half = (height * 0.105).clamp(0.15, 0.24);
        let body_depth = (height * 0.075).clamp(0.10, 0.16);
        let head_radius = (height * 0.077).clamp(0.12, 0.18);
        let locomotion = pose.locomotion.clamp(0.0, 1.0);
        let crouch = pose.crouch.clamp(0.0, 1.0);
        let body_drop = height * crouch * 0.055;
        let body_forward = height * crouch * 0.030;
        let stride = height * locomotion * 0.060;
        let arm_swing = height * locomotion * 0.040;

        self.world_flat_ellipse_with_surface_response(
            [origin[0], origin[1], origin[2] + 0.006],
            [shoulder_half * 1.22 + stride.abs() * 1.2, body_depth * 1.42],
            shell_segments,
            [0.012, 0.010, 0.009, 0.24],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );

        let pelvis_center = humanoid_oriented_point(
            origin,
            orientation,
            [0.0, body_forward * 0.38, height * 0.455 - body_drop],
        );
        let abdomen_center = humanoid_oriented_point(
            origin,
            orientation,
            [0.0, body_forward * 0.50, height * 0.555 - body_drop],
        );
        let chest_center = humanoid_oriented_point(
            origin,
            orientation,
            [0.0, body_forward * 0.42, height * 0.668 - body_drop],
        );
        let neck_center = humanoid_oriented_point(
            origin,
            orientation,
            [0.0, body_forward * 0.24, height * 0.785 - body_drop * 0.45],
        );
        let head_center = humanoid_oriented_point(
            origin,
            orientation,
            [0.0, body_forward * 0.14, height * 0.885 - body_drop * 0.24],
        );

        self.world_oriented_ellipsoid_with_surface_response(
            pelvis_center,
            axes,
            [hip_half * 1.08, body_depth * 0.98, height * 0.062],
            shell_rings,
            shell_segments,
            window_scale_color(cloth, 0.86),
            clothing_response,
        );
        self.world_oriented_ellipsoid_with_surface_response(
            abdomen_center,
            axes,
            [hip_half * 0.98, body_depth * 0.92, height * 0.090],
            shell_rings,
            shell_segments,
            cloth,
            clothing_response,
        );
        self.world_oriented_ellipsoid_with_surface_response(
            chest_center,
            axes,
            [shoulder_half * 1.02, body_depth, height * 0.092],
            shell_rings,
            shell_segments,
            cloth_highlight,
            clothing_response,
        );
        self.world_oriented_ellipsoid_with_surface_response(
            add3(chest_center, scale3(orientation.forward, body_depth * 0.46)),
            axes,
            [shoulder_half * 0.78, body_depth * 0.24, height * 0.070],
            5,
            shell_segments.min(16),
            window_scale_color(cloth_highlight, 0.86),
            clothing_response,
        );
        self.world_cylinder_between_with_surface_response(
            add3(neck_center, scale3(orientation.up, -height * 0.034)),
            add3(neck_center, scale3(orientation.up, height * 0.034)),
            head_radius * 0.34,
            limb_segments,
            skin_shadow,
            skin_response,
        );
        self.world_oriented_ellipsoid_with_surface_response(
            head_center,
            axes,
            [head_radius * 0.88, head_radius * 0.72, head_radius * 1.18],
            shell_rings,
            shell_segments,
            skin,
            skin_response,
        );
        self.world_oriented_ellipsoid_with_surface_response(
            add3(
                add3(head_center, scale3(orientation.up, head_radius * 0.52)),
                scale3(orientation.forward, -head_radius * 0.05),
            ),
            axes,
            [head_radius * 0.95, head_radius * 0.76, head_radius * 0.48],
            5,
            shell_segments.min(16),
            hair,
            hair_response,
        );
        self.world_oriented_ellipsoid_with_surface_response(
            add3(
                add3(head_center, scale3(orientation.up, head_radius * 0.06)),
                scale3(orientation.forward, -head_radius * 0.34),
            ),
            axes,
            [head_radius * 0.86, head_radius * 0.16, head_radius * 0.68],
            4,
            10,
            hair_shadow,
            hair_response,
        );

        for side in [-1.0_f32, 1.0] {
            let shoulder = humanoid_oriented_point(
                origin,
                orientation,
                [
                    side * shoulder_half * 1.02,
                    body_forward * 0.35 - side * arm_swing * 0.22,
                    height * 0.704 - body_drop * 0.88,
                ],
            );
            let elbow = humanoid_oriented_point(
                origin,
                orientation,
                [
                    side * (shoulder_half * 1.13 + height * 0.018),
                    body_forward + side * arm_swing,
                    height * 0.540 - body_drop * 0.70,
                ],
            );
            let wrist = humanoid_oriented_point(
                origin,
                orientation,
                [
                    side * (hip_half * 0.92 + height * 0.030),
                    body_forward + side * arm_swing * 0.55,
                    height * 0.365 - body_drop * 0.42,
                ],
            );
            self.world_oriented_ellipsoid_with_surface_response(
                shoulder,
                axes,
                [height * 0.040, body_depth * 0.58, height * 0.047],
                5,
                shell_segments.min(14),
                cloth_highlight,
                clothing_response,
            );
            self.world_cylinder_between_with_surface_response(
                shoulder,
                elbow,
                height * 0.026,
                limb_segments,
                cloth,
                clothing_response,
            );
            self.world_cylinder_between_with_surface_response(
                elbow,
                wrist,
                height * 0.021,
                limb_segments,
                window_scale_color(cloth, 0.92),
                clothing_response,
            );
            self.world_oriented_ellipsoid_with_surface_response(
                wrist,
                axes,
                [height * 0.020, height * 0.015, height * 0.028],
                4,
                10,
                if side < 0.0 {
                    window_scale_color(skin, 0.92)
                } else {
                    skin
                },
                skin_response,
            );

            let hip = humanoid_oriented_point(
                origin,
                orientation,
                [
                    side * hip_half * 0.56,
                    body_forward * 0.38 + side * stride * 0.18,
                    height * 0.430 - body_drop,
                ],
            );
            let knee = humanoid_oriented_point(
                origin,
                orientation,
                [
                    side * hip_half * 0.42,
                    body_forward + side * stride + crouch * height * 0.040,
                    height * 0.238 - body_drop * 0.54,
                ],
            );
            let ankle = humanoid_oriented_point(
                origin,
                orientation,
                [
                    side * hip_half * 0.50,
                    -side * stride * 0.62 + crouch * height * 0.016,
                    height * 0.060,
                ],
            );
            self.world_cylinder_between_with_surface_response(
                hip,
                knee,
                height * 0.034,
                limb_segments,
                window_scale_color(cloth, if side < 0.0 { 0.86 } else { 1.02 }),
                clothing_response,
            );
            self.world_cylinder_between_with_surface_response(
                knee,
                ankle,
                height * 0.028,
                limb_segments,
                window_scale_color(cloth, if side < 0.0 { 0.78 } else { 0.92 }),
                clothing_response,
            );
            self.world_oriented_ellipsoid_with_surface_response(
                knee,
                axes,
                [height * 0.038, body_depth * 0.40, height * 0.030],
                4,
                10,
                cloth_shadow,
                clothing_response,
            );
            self.world_oriented_ellipsoid_with_surface_response(
                add3(ankle, scale3(orientation.forward, height * 0.026)),
                axes,
                [height * 0.046, height * 0.072, height * 0.022],
                4,
                12,
                boot,
                clothing_response,
            );
        }

        self.world_oriented_rect_with_surface_response(
            add3(chest_center, scale3(orientation.forward, body_depth * 0.86)),
            orientation.right,
            orientation.up,
            [shoulder_half * 0.42, height * 0.006],
            window_mix_color(cloth_highlight, [1.0, 1.0, 1.0, cloth_highlight[3]], 0.08),
            clothing_response,
        );
        self.world_oriented_rect_with_surface_response(
            add3(
                abdomen_center,
                scale3(orientation.forward, body_depth * 0.84),
            ),
            orientation.right,
            orientation.up,
            [hip_half * 0.34, height * 0.004],
            cloth_shadow,
            clothing_response,
        );
        self.world_oriented_ellipsoid_with_surface_response(
            add3(
                add3(head_center, scale3(orientation.forward, head_radius * 0.64)),
                scale3(orientation.up, -head_radius * 0.02),
            ),
            axes,
            [head_radius * 0.20, head_radius * 0.045, head_radius * 0.16],
            4,
            8,
            skin_warm,
            skin_response,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn add_humanoid_body_part(
        &mut self,
        origin: [f32; 3],
        body_part: &HumanProxyBox,
        color: [f32; 4],
        surface_state: Option<&HumanSurfaceState>,
        silhouette_segments: u16,
        limb_volume_layer_count: u8,
        orientation: HumanoidOrientationBasis,
        pose: HumanoidPoseWeights,
    ) {
        let min = body_part.local_min_meters;
        let max = body_part.local_max_meters;
        let world_min = [origin[0] + min.x, origin[1] + min.y, origin[2] + min.z];
        let world_max = [origin[0] + max.x, origin[1] + max.y, origin[2] + max.z];
        let part_color = humanoid_part_color(color, body_part.part, surface_state);
        let part_surface_response =
            humanoid_body_part_surface_response(body_part.part, surface_state);
        let hero_silhouette = silhouette_segments >= 32;
        let primary_rings = if hero_silhouette { 12 } else { 9 };
        let primary_segments = if hero_silhouette { 24 } else { 16 };
        let secondary_rings = if hero_silhouette { 7 } else { 5 };
        let secondary_segments = if hero_silhouette { 16 } else { 12 };
        let limb_segments = if hero_silhouette { 18 } else { 14 };

        if matches!(body_part.part, HumanProxyPart::Impostor) {
            self.world_box_with_surface_response(
                world_min,
                world_max,
                part_color,
                part_surface_response,
            );
            return;
        }

        let local_center = [
            (min.x + max.x) * 0.5,
            (min.y + max.y) * 0.5,
            (min.z + max.z) * 0.5,
        ];
        let center = humanoid_oriented_point(origin, orientation, local_center);
        let half_extent = [
            ((max.x - min.x) * 0.5).max(0.01),
            ((max.y - min.y) * 0.5).max(0.01),
            ((max.z - min.z) * 0.5).max(0.01),
        ];
        let axes = [orientation.right, orientation.forward, orientation.up];

        match body_part.part {
            HumanProxyPart::Torso
            | HumanProxyPart::Pelvis
            | HumanProxyPart::Abdomen
            | HumanProxyPart::Chest => {
                self.world_oriented_ellipsoid_with_surface_response(
                    center,
                    axes,
                    half_extent,
                    primary_rings,
                    primary_segments,
                    part_color,
                    part_surface_response,
                );
                self.world_oriented_ellipsoid_with_surface_response(
                    add3(center, scale3(orientation.up, -half_extent[2] * 0.18)),
                    axes,
                    [
                        half_extent[0] * 0.86,
                        half_extent[1] * 0.92,
                        half_extent[2] * 0.34,
                    ],
                    secondary_rings,
                    secondary_segments,
                    window_scale_color(part_color, 0.72),
                    part_surface_response,
                );
            }
            HumanProxyPart::Head | HumanProxyPart::HairCap => {
                self.world_oriented_ellipsoid_with_surface_response(
                    center,
                    axes,
                    half_extent,
                    primary_rings,
                    primary_segments,
                    part_color,
                    part_surface_response,
                );
                self.world_oriented_ellipsoid_with_surface_response(
                    add3(
                        add3(center, scale3(orientation.forward, -half_extent[1] * 0.08)),
                        scale3(orientation.up, half_extent[2] * 0.22),
                    ),
                    axes,
                    [
                        half_extent[0] * 1.04,
                        half_extent[1] * 1.04,
                        half_extent[2] * 0.5,
                    ],
                    secondary_rings,
                    secondary_segments,
                    humanoid_hair_color(color, surface_state),
                    humanoid_hair_surface_response(surface_state),
                );
            }
            HumanProxyPart::Neck => {
                self.world_cylinder_between_with_surface_response(
                    add3(center, scale3(orientation.up, -half_extent[2])),
                    add3(center, scale3(orientation.up, half_extent[2])),
                    half_extent[0].max(0.024),
                    limb_segments,
                    part_color,
                    part_surface_response,
                );
            }
            HumanProxyPart::LeftArm
            | HumanProxyPart::RightArm
            | HumanProxyPart::LeftLeg
            | HumanProxyPart::RightLeg
            | HumanProxyPart::LeftUpperArm
            | HumanProxyPart::RightUpperArm
            | HumanProxyPart::LeftForearm
            | HumanProxyPart::RightForearm
            | HumanProxyPart::LeftThigh
            | HumanProxyPart::RightThigh
            | HumanProxyPart::LeftCalf
            | HumanProxyPart::RightCalf => {
                let radius = half_extent[0].min(half_extent[1]).max(0.035) * 0.92;
                let side = match body_part.part {
                    HumanProxyPart::LeftArm
                    | HumanProxyPart::LeftUpperArm
                    | HumanProxyPart::LeftForearm
                    | HumanProxyPart::LeftLeg
                    | HumanProxyPart::LeftThigh
                    | HumanProxyPart::LeftCalf => -1.0,
                    _ => 1.0,
                };
                let z_low = min.z + radius * 0.35;
                let z_high = max.z - radius * 0.35;
                let local_mid_y = (min.y + max.y) * 0.5;
                let stride_offset = side * pose.locomotion * half_extent[2] * 0.30;
                let arm_swing_offset = -stride_offset * 0.68;
                let crouch_forward = pose.crouch * half_extent[2] * 0.34;
                let crouch_drop = pose.crouch * half_extent[2] * 0.16;
                let (start_local, end_local) = match body_part.part {
                    HumanProxyPart::LeftUpperArm | HumanProxyPart::RightUpperArm => (
                        [
                            local_center[0] - side * half_extent[0] * 0.18,
                            local_mid_y - half_extent[1] * 0.12 + arm_swing_offset,
                            z_high,
                        ],
                        [
                            local_center[0] + side * half_extent[0] * 0.32,
                            local_mid_y + half_extent[1] * 0.34 + arm_swing_offset,
                            z_low,
                        ],
                    ),
                    HumanProxyPart::LeftForearm | HumanProxyPart::RightForearm => (
                        [
                            local_center[0] + side * half_extent[0] * 0.24,
                            local_mid_y + half_extent[1] * 0.20 + arm_swing_offset,
                            z_high,
                        ],
                        [
                            local_center[0] - side * half_extent[0] * 0.18,
                            local_mid_y + half_extent[1] * 0.56 + arm_swing_offset,
                            z_low - crouch_drop * 0.20,
                        ],
                    ),
                    HumanProxyPart::LeftThigh | HumanProxyPart::RightThigh => (
                        [
                            local_center[0] - side * half_extent[0] * 0.10,
                            local_mid_y - half_extent[1] * 0.08 + stride_offset,
                            z_high - crouch_drop,
                        ],
                        [
                            local_center[0] + side * half_extent[0] * 0.18,
                            local_mid_y + half_extent[1] * 0.18 + stride_offset + crouch_forward,
                            z_low - crouch_drop * 0.55,
                        ],
                    ),
                    HumanProxyPart::LeftCalf | HumanProxyPart::RightCalf => (
                        [
                            local_center[0] + side * half_extent[0] * 0.16,
                            local_mid_y + half_extent[1] * 0.18 + stride_offset + crouch_forward,
                            z_high - crouch_drop * 0.45,
                        ],
                        [
                            local_center[0] - side * half_extent[0] * 0.10,
                            local_mid_y - half_extent[1] * 0.22 - stride_offset * 0.50,
                            z_low,
                        ],
                    ),
                    HumanProxyPart::LeftArm | HumanProxyPart::RightArm => (
                        [
                            local_center[0] - side * half_extent[0] * 0.12,
                            local_mid_y - half_extent[1] * 0.10 + arm_swing_offset,
                            z_high,
                        ],
                        [
                            local_center[0] + side * half_extent[0] * 0.16,
                            local_mid_y + half_extent[1] * 0.34 + arm_swing_offset,
                            z_low - crouch_drop * 0.20,
                        ],
                    ),
                    HumanProxyPart::LeftLeg | HumanProxyPart::RightLeg => (
                        [
                            local_center[0] - side * half_extent[0] * 0.08,
                            local_mid_y + stride_offset,
                            z_high - crouch_drop,
                        ],
                        [
                            local_center[0] + side * half_extent[0] * 0.08,
                            local_mid_y - half_extent[1] * 0.14 - stride_offset * 0.35,
                            z_low,
                        ],
                    ),
                    _ => unreachable!("limb match should only receive limb proxy parts"),
                };
                let start = humanoid_oriented_point(origin, orientation, start_local);
                let end = humanoid_oriented_point(origin, orientation, end_local);
                self.world_cylinder_between_with_surface_response(
                    start,
                    end,
                    radius,
                    limb_segments,
                    part_color,
                    part_surface_response,
                );
                self.add_humanoid_limb_volume_details(
                    body_part.part,
                    start,
                    end,
                    orientation,
                    radius,
                    half_extent,
                    color,
                    surface_state,
                    part_surface_response,
                    hero_silhouette,
                    limb_volume_layer_count,
                );
                let joint_radii = [radius * 1.04, radius * 1.04, radius * 0.78];
                self.world_oriented_ellipsoid_with_surface_response(
                    start,
                    axes,
                    joint_radii,
                    secondary_rings,
                    secondary_segments,
                    part_color,
                    part_surface_response,
                );
                self.world_oriented_ellipsoid_with_surface_response(
                    end,
                    axes,
                    joint_radii,
                    secondary_rings,
                    secondary_segments,
                    part_color,
                    part_surface_response,
                );
                let (extremity_color, extremity_surface_response) =
                    humanoid_skin_or_cloth_extremity_color(body_part.part, color, surface_state);
                let extremity_radii = if matches!(
                    body_part.part,
                    HumanProxyPart::LeftArm
                        | HumanProxyPart::RightArm
                        | HumanProxyPart::LeftForearm
                        | HumanProxyPart::RightForearm
                ) {
                    [radius * 0.9, radius * 0.72, radius * 0.64]
                } else {
                    [radius * 1.42, radius * 0.86, radius * 0.58]
                };
                self.world_oriented_ellipsoid_with_surface_response(
                    start,
                    axes,
                    extremity_radii,
                    secondary_rings,
                    secondary_segments,
                    extremity_color,
                    extremity_surface_response,
                );
            }
            HumanProxyPart::LeftHand | HumanProxyPart::RightHand => {
                self.world_oriented_ellipsoid_with_surface_response(
                    center,
                    axes,
                    [
                        half_extent[0] * 1.10,
                        half_extent[1] * 0.82,
                        half_extent[2] * 0.74,
                    ],
                    secondary_rings,
                    secondary_segments,
                    part_color,
                    part_surface_response,
                );
            }
            HumanProxyPart::LeftFoot | HumanProxyPart::RightFoot => {
                self.world_oriented_ellipsoid_with_surface_response(
                    add3(center, scale3(orientation.forward, half_extent[1] * 0.20)),
                    axes,
                    [
                        half_extent[0] * 1.04,
                        half_extent[1] * 1.22,
                        half_extent[2] * 0.72,
                    ],
                    secondary_rings,
                    secondary_segments,
                    part_color,
                    part_surface_response,
                );
            }
            HumanProxyPart::Impostor => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_humanoid_limb_volume_details(
        &mut self,
        part: HumanProxyPart,
        start: [f32; 3],
        end: [f32; 3],
        orientation: HumanoidOrientationBasis,
        radius: f32,
        half_extent: [f32; 3],
        color: [f32; 4],
        surface_state: Option<&HumanSurfaceState>,
        surface_response: [f32; 4],
        hero_silhouette: bool,
        limb_volume_layer_count: u8,
    ) {
        let layers = limb_volume_layer_count as usize;
        if layers == 0 {
            return;
        }

        let span_vector = sub3(end, start);
        let span_length = dot3(span_vector, span_vector).sqrt();
        if span_length <= f32::EPSILON {
            return;
        }

        let axis = normalize3(span_vector);
        let axes = [orientation.right, orientation.forward, axis];
        let is_arm = matches!(
            part,
            HumanProxyPart::LeftArm
                | HumanProxyPart::RightArm
                | HumanProxyPart::LeftUpperArm
                | HumanProxyPart::RightUpperArm
                | HumanProxyPart::LeftForearm
                | HumanProxyPart::RightForearm
        );
        let is_lower = matches!(
            part,
            HumanProxyPart::LeftForearm
                | HumanProxyPart::RightForearm
                | HumanProxyPart::LeftCalf
                | HumanProxyPart::RightCalf
                | HumanProxyPart::LeftArm
                | HumanProxyPart::RightArm
                | HumanProxyPart::LeftLeg
                | HumanProxyPart::RightLeg
        );
        let side = match part {
            HumanProxyPart::LeftArm
            | HumanProxyPart::LeftUpperArm
            | HumanProxyPart::LeftForearm
            | HumanProxyPart::LeftLeg
            | HumanProxyPart::LeftThigh
            | HumanProxyPart::LeftCalf => -1.0,
            _ => 1.0,
        };
        let volume_color = humanoid_limb_volume_color(part, color, surface_state);
        let shadow_color = humanoid_limb_shadow_color(part, color, surface_state);
        let highlight_color = humanoid_limb_highlight_color(part, color, surface_state);
        let rings = if hero_silhouette { 5 } else { 4 };
        let segments = if hero_silhouette { 12 } else { 8 };
        let primary_center = add3(start, scale3(span_vector, 0.48));
        let secondary_center = add3(
            start,
            scale3(span_vector, if is_lower { 0.68 } else { 0.32 }),
        );
        let lateral_scale = if is_arm { 0.78 } else { 1.08 };
        let front_scale = if is_arm { 0.68 } else { 0.82 };
        let length_scale = if is_lower { 0.30 } else { 0.36 };

        self.world_oriented_ellipsoid_with_surface_response(
            primary_center,
            axes,
            [
                radius * lateral_scale * if is_lower { 0.86 } else { 1.12 },
                radius * front_scale * if is_lower { 0.78 } else { 1.08 },
                (span_length * length_scale).max(half_extent[2] * 0.18),
            ],
            rings,
            segments,
            volume_color,
            surface_response,
        );

        if layers >= 6 {
            self.world_oriented_ellipsoid_with_surface_response(
                secondary_center,
                axes,
                [
                    radius * lateral_scale * if is_lower { 1.04 } else { 0.84 },
                    radius * front_scale * if is_lower { 0.92 } else { 0.76 },
                    (span_length * 0.22).max(half_extent[2] * 0.12),
                ],
                rings,
                segments,
                if is_lower {
                    highlight_color
                } else {
                    shadow_color
                },
                surface_response,
            );
        }

        if layers >= 10 {
            let trim_axis = normalize3(add3(
                orientation.forward,
                scale3(orientation.right, side * 0.18),
            ));
            for t in [0.28_f32, 0.58, 0.78] {
                let center = add3(
                    add3(start, scale3(span_vector, t)),
                    scale3(orientation.forward, radius * 0.52),
                );
                self.world_oriented_rect_with_surface_response(
                    center,
                    trim_axis,
                    axis,
                    [
                        radius * if is_arm { 0.44 } else { 0.58 },
                        span_length * 0.055,
                    ],
                    if t < 0.5 {
                        highlight_color
                    } else {
                        shadow_color
                    },
                    surface_response,
                );
            }
        }
    }

    fn add_humanoid_surface_details(
        &mut self,
        instance: &WindowHumanoidProxyInstance<'_>,
        proxy: &HumanProxyGeometry,
    ) {
        if proxy.quality_tier < QualityTier::NormalRuntime || proxy.body_parts.is_empty() {
            return;
        }

        let [x, y] = instance.center_meters;
        let z = instance.z_meters;
        let (forward, right, up) = humanoid_instance_orientation_basis(instance);
        let height = proxy.height_meters;
        let body_front_offset = (height * 0.072).clamp(0.085, 0.135);
        let face_front_offset = (height * 0.086).clamp(0.11, 0.17);
        let skin_response = humanoid_skin_surface_response(instance.surface_state.as_ref());
        let wet_skin_response = humanoid_wet_skin_surface_response(instance.surface_state.as_ref());
        let eye_response = humanoid_eye_surface_response(instance.surface_state.as_ref());
        let hair_response = humanoid_hair_surface_response(instance.surface_state.as_ref());
        let clothing_response = humanoid_clothing_surface_response(instance.surface_state.as_ref());
        let jacket_color = humanoid_clothing_color(instance.color);
        let trim_color = humanoid_trim_color();
        let boot_color = humanoid_boot_color(instance.color);
        let front_body = scale3(forward, body_front_offset);
        let front_face = scale3(forward, face_front_offset);
        let mid_detail = humanoid_allows_mid_detail(instance, proxy);

        let chest_center = add3([x, y, z + height * 0.615], front_body);
        for lateral in [-height * 0.046, height * 0.046] {
            self.world_oriented_rect_with_surface_response(
                add3(chest_center, scale3(right, lateral)),
                right,
                up,
                [height * 0.039, height * 0.17],
                jacket_color,
                clothing_response,
            );
        }
        self.world_oriented_rect_with_surface_response(
            chest_center,
            right,
            up,
            [height * 0.006, height * 0.18],
            trim_color,
            clothing_response,
        );

        let collar_center = add3([x, y, z + height * 0.755], front_body);
        self.world_oriented_rect_with_surface_response(
            add3(collar_center, scale3(right, -height * 0.045)),
            right,
            up,
            [height * 0.036, height * 0.018],
            window_scale_color(jacket_color, 1.35),
            clothing_response,
        );
        self.world_oriented_rect_with_surface_response(
            add3(collar_center, scale3(right, height * 0.045)),
            right,
            up,
            [height * 0.036, height * 0.018],
            window_scale_color(jacket_color, 1.35),
            clothing_response,
        );

        for lateral in [-height * 0.085, height * 0.085] {
            self.world_oriented_rect_with_surface_response(
                add3(
                    [x, y, z + height * 0.07],
                    add3(front_body, scale3(right, lateral)),
                ),
                right,
                up,
                [height * 0.038, height * 0.045],
                boot_color,
                clothing_response,
            );
        }

        if mid_detail && proxy.quality_tier >= QualityTier::HeroHighFidelityRuntime {
            self.world_oriented_rect_with_surface_response(
                add3(
                    [x, y, z + height * 0.875],
                    add3(front_face, scale3(right, height * 0.09)),
                ),
                right,
                up,
                [height * 0.011, height * 0.017],
                trim_color,
                clothing_response,
            );
            self.world_oriented_rect_with_surface_response(
                add3(
                    [x, y, z + height * 0.902],
                    add3(front_face, scale3(right, -height * 0.038)),
                ),
                right,
                up,
                [height * 0.008, height * 0.005],
                [0.92, 1.0, 1.0, 0.95],
                eye_response,
            );
            self.world_oriented_rect_with_surface_response(
                add3(
                    [x, y, z + height * 0.902],
                    add3(front_face, scale3(right, height * 0.038)),
                ),
                right,
                up,
                [height * 0.008, height * 0.005],
                [0.92, 1.0, 1.0, 0.95],
                eye_response,
            );
        }

        if let Some(surface) = instance.surface_state.as_ref() {
            if let Some(color) = humanoid_skin_wet_sheen_color(surface) {
                self.world_oriented_rect_with_surface_response(
                    add3(
                        [x, y, z + height * 0.91],
                        add3(front_face, scale3(right, -height * 0.056)),
                    ),
                    right,
                    up,
                    [height * 0.026, height * 0.013],
                    color,
                    wet_skin_response,
                );
                self.world_oriented_rect_with_surface_response(
                    add3(
                        [x, y, z + height * 0.91],
                        add3(front_face, scale3(right, height * 0.056)),
                    ),
                    right,
                    up,
                    [height * 0.026, height * 0.013],
                    color,
                    wet_skin_response,
                );
                self.world_oriented_rect_with_surface_response(
                    add3([x, y, z + height * 0.83], front_face),
                    right,
                    up,
                    [height * 0.034, height * 0.012],
                    color,
                    wet_skin_response,
                );
            }

            if let Some(color) = humanoid_injury_overlay_color(surface) {
                self.world_oriented_rect_with_surface_response(
                    add3(
                        [x, y, z + height * 0.872],
                        add3(front_face, scale3(right, height * 0.066)),
                    ),
                    right,
                    up,
                    [height * 0.022, height * 0.032],
                    color,
                    skin_response,
                );
            }

            if let Some(color) = humanoid_bruise_overlay_color(surface) {
                self.world_oriented_rect_with_surface_response(
                    add3(
                        [x, y, z + height * 0.69],
                        add3(front_body, scale3(right, -height * 0.082)),
                    ),
                    right,
                    up,
                    [height * 0.024, height * 0.05],
                    color,
                    skin_response,
                );
            }

            if let Some(color) = humanoid_clothing_wet_color(surface) {
                self.world_oriented_rect_with_surface_response(
                    add3([x, y, z + height * 0.54], front_body),
                    right,
                    up,
                    [height * 0.088, height * 0.07],
                    color,
                    clothing_response,
                );
            }

            if let Some(color) = humanoid_clothing_damage_color(surface) {
                for lateral in [-height * 0.05, height * 0.072] {
                    self.world_oriented_rect_with_surface_response(
                        add3(
                            [x, y, z + height * 0.64],
                            add3(front_body, scale3(right, lateral)),
                        ),
                        right,
                        up,
                        [height * 0.018, height * 0.075],
                        color,
                        clothing_response,
                    );
                }
            }

            if let Some(color) = humanoid_dirt_overlay_color(surface) {
                self.world_oriented_rect_with_surface_response(
                    add3(
                        [x, y, z + height * 0.36],
                        add3(front_body, scale3(right, -height * 0.067)),
                    ),
                    right,
                    up,
                    [height * 0.024, height * 0.1],
                    color,
                    clothing_response,
                );
                self.world_oriented_rect_with_surface_response(
                    add3(
                        [x, y, z + height * 0.22],
                        add3(front_body, scale3(right, height * 0.078)),
                    ),
                    right,
                    up,
                    [height * 0.028, height * 0.08],
                    color,
                    clothing_response,
                );
            }

            if let Some(color) = humanoid_hair_wet_color(surface) {
                self.world_oriented_rect_with_surface_response(
                    add3([x, y, z + height * 0.972], front_face),
                    right,
                    up,
                    [height * 0.07, height * 0.028],
                    color,
                    hair_response,
                );
            }
        }

        self.add_humanoid_version3_closeup_details(
            instance,
            proxy,
            HumanoidDetailBasis {
                forward,
                right,
                up,
                front_body,
                front_face,
            },
        );
    }

    fn add_humanoid_version3_closeup_details(
        &mut self,
        instance: &WindowHumanoidProxyInstance<'_>,
        proxy: &HumanProxyGeometry,
        basis: HumanoidDetailBasis,
    ) {
        if proxy.quality_tier < QualityTier::NormalRuntime {
            return;
        }

        let seed = humanoid_detail_seed(instance);
        if !humanoid_allows_mid_detail(instance, proxy) {
            return;
        }

        self.add_humanoid_clothing_seams_and_motion(instance, proxy, basis, seed);
        if !humanoid_allows_near_detail(instance, proxy) {
            return;
        }

        self.add_humanoid_version4_anatomy_and_fabric_detail(instance, proxy, basis, seed);
        self.add_humanoid_skin_microdetail(instance, proxy, basis, seed);
        self.add_humanoid_eye_layer_details(instance, proxy, basis);
        self.add_humanoid_hair_strand_cards(instance, proxy, basis, seed);
        self.add_humanoid_version5_photoreal_proxy_layers(instance, proxy, basis, seed);
        self.add_humanoid_version6_silhouette_refinement(instance, proxy, basis, seed);
        self.add_humanoid_version7_articulated_closeup_detail(instance, proxy, basis, seed);
        self.add_humanoid_version8_soft_tissue_form_detail(instance, proxy, basis, seed);

        if humanoid_has_visible_cybernetic_detail(instance, proxy) {
            self.add_humanoid_cybernetic_detail(instance, proxy, basis, seed);
        }
    }

    fn add_humanoid_skin_microdetail(
        &mut self,
        instance: &WindowHumanoidProxyInstance<'_>,
        proxy: &HumanProxyGeometry,
        basis: HumanoidDetailBasis,
        seed: u64,
    ) {
        let [x, y] = instance.center_meters;
        let z = instance.z_meters;
        let height = proxy.height_meters;
        let surface = instance.surface_state.as_ref();
        let skin_response = humanoid_skin_surface_response(surface);
        let pore_color = humanoid_skin_microdetail_color(instance.color, surface);
        let pore_count = proxy.detail_profile.skin_microdetail_points.max(10);

        for index in 0..pore_count {
            let salt = index as u64;
            let lateral = (window_stable_unit(seed, 12_001 + salt) * 2.0 - 1.0) * height * 0.083;
            let vertical = height * (0.842 + window_stable_unit(seed, 12_701 + salt) * 0.108);
            let size = height * (0.0024 + window_stable_unit(seed, 13_401 + salt) * 0.0028);
            let center = add3(
                [x, y, z + vertical],
                add3(basis.front_face, scale3(basis.right, lateral)),
            );
            self.world_oriented_rect_with_surface_response(
                center,
                basis.right,
                basis.up,
                [size, size],
                pore_color,
                skin_response,
            );
        }

        let wrinkle_color = humanoid_wrinkle_line_color(instance.color, surface);
        let wrinkle_count = proxy.detail_profile.wrinkle_line_count.max(3);
        for index in 0..wrinkle_count {
            let t = if wrinkle_count <= 1 {
                0.5
            } else {
                index as f32 / (wrinkle_count - 1) as f32
            };
            let vertical_fraction = 0.858 + t * 0.072;
            let width = height * (0.022 + t * 0.044);
            let center = add3([x, y, z + height * vertical_fraction], basis.front_face);
            self.world_oriented_rect_with_surface_response(
                center,
                basis.right,
                basis.up,
                [width, height * 0.0019],
                wrinkle_color,
                skin_response,
            );
        }

        for lateral in [-height * 0.058, height * 0.058] {
            let center = add3(
                [x, y, z + height * 0.885],
                add3(basis.front_face, scale3(basis.right, lateral)),
            );
            self.world_oriented_rect_with_surface_response(
                center,
                basis.right,
                basis.up,
                [height * 0.016, height * 0.0017],
                wrinkle_color,
                skin_response,
            );
        }

        if proxy.quality_tier >= QualityTier::HeroHighFidelityRuntime {
            let fuzz_color = humanoid_peach_fuzz_color(instance.color, surface);
            for index in 0..8 {
                let salt = index as u64;
                let lateral = (-0.076 + index as f32 * 0.0218) * height;
                let vertical = height * (0.785 + window_stable_unit(seed, 14_203 + salt) * 0.055);
                let center = add3(
                    [x, y, z + vertical],
                    add3(basis.front_face, scale3(basis.right, lateral)),
                );
                self.world_oriented_rect_with_surface_response(
                    center,
                    basis.right,
                    basis.up,
                    [height * 0.0014, height * 0.010],
                    fuzz_color,
                    skin_response,
                );
            }
        }
    }

    fn add_humanoid_eye_layer_details(
        &mut self,
        instance: &WindowHumanoidProxyInstance<'_>,
        proxy: &HumanProxyGeometry,
        basis: HumanoidDetailBasis,
    ) {
        if proxy.quality_tier < QualityTier::HeroHighFidelityRuntime {
            return;
        }

        let [x, y] = instance.center_meters;
        let z = instance.z_meters;
        let height = proxy.height_meters;
        let eye_response = humanoid_eye_surface_response(instance.surface_state.as_ref());
        let tearline_color = humanoid_eye_tearline_color(instance.surface_state.as_ref());
        let iris_color =
            humanoid_iris_detail_color(instance.color, instance.surface_state.as_ref());

        for lateral in [-height * 0.038, height * 0.038] {
            let eye_center = add3(
                [x, y, z + height * 0.897],
                add3(basis.front_face, scale3(basis.right, lateral)),
            );
            self.world_oriented_rect_with_surface_response(
                add3(eye_center, scale3(basis.up, -height * 0.008)),
                basis.right,
                basis.up,
                [height * 0.017, height * 0.0022],
                tearline_color,
                eye_response,
            );
            self.world_oriented_rect_with_surface_response(
                eye_center,
                basis.right,
                basis.up,
                [height * 0.006, height * 0.006],
                iris_color,
                eye_response,
            );
            self.world_oriented_rect_with_surface_response(
                add3(
                    eye_center,
                    add3(
                        scale3(basis.right, height * 0.005),
                        scale3(basis.up, height * 0.004),
                    ),
                ),
                basis.right,
                basis.up,
                [height * 0.0032, height * 0.0022],
                [0.96, 1.0, 1.0, 0.92],
                eye_response,
            );
        }
    }

    fn add_humanoid_hair_strand_cards(
        &mut self,
        instance: &WindowHumanoidProxyInstance<'_>,
        proxy: &HumanProxyGeometry,
        basis: HumanoidDetailBasis,
        seed: u64,
    ) {
        if proxy.quality_tier < QualityTier::NormalRuntime {
            return;
        }

        let [x, y] = instance.center_meters;
        let z = instance.z_meters;
        let height = proxy.height_meters;
        let surface = instance.surface_state.as_ref();
        let hair_response = humanoid_hair_surface_response(surface);
        let strand_count = proxy.detail_profile.hair_card_count.max(8);

        for index in 0..strand_count {
            let salt = index as u64;
            let normalized = if strand_count <= 1 {
                0.5
            } else {
                index as f32 / (strand_count - 1) as f32
            };
            let lateral = (normalized - 0.5) * height * 0.17
                + (window_stable_unit(seed, 15_101 + salt) - 0.5) * height * 0.012;
            let drop = height * (0.026 + window_stable_unit(seed, 15_809 + salt) * 0.05);
            let center = add3(
                [x, y, z + height * 0.961 - drop * 0.45],
                add3(
                    add3(basis.front_face, scale3(basis.right, lateral)),
                    scale3(
                        basis.forward,
                        (window_stable_unit(seed, 16_503 + salt) - 0.5) * height * 0.012,
                    ),
                ),
            );
            self.world_oriented_rect_with_surface_response(
                center,
                basis.right,
                basis.up,
                [
                    height * (0.0018 + window_stable_unit(seed, 17_211 + salt) * 0.0012),
                    drop * 0.55,
                ],
                humanoid_hair_strand_color(instance.color, surface, seed, salt),
                hair_response,
            );
        }
    }

    fn add_humanoid_clothing_seams_and_motion(
        &mut self,
        instance: &WindowHumanoidProxyInstance<'_>,
        proxy: &HumanProxyGeometry,
        basis: HumanoidDetailBasis,
        seed: u64,
    ) {
        let [x, y] = instance.center_meters;
        let z = instance.z_meters;
        let height = proxy.height_meters;
        let surface = instance.surface_state.as_ref();
        let clothing_response = humanoid_clothing_surface_response(surface);
        let seam_color = humanoid_clothing_seam_color(instance.color, surface);
        let crease_color = humanoid_clothing_crease_color(instance.color, surface);
        let chest_center = add3([x, y, z + height * 0.61], basis.front_body);

        self.world_oriented_rect_with_surface_response(
            chest_center,
            basis.right,
            basis.up,
            [height * 0.0036, height * 0.18],
            seam_color,
            clothing_response,
        );

        for lateral in [-height * 0.061, height * 0.061] {
            self.world_oriented_rect_with_surface_response(
                add3(chest_center, scale3(basis.right, lateral)),
                basis.right,
                basis.up,
                [height * 0.0026, height * 0.145],
                seam_color,
                clothing_response,
            );
        }

        for index in 0..5 {
            let salt = index as u64;
            let lateral = (window_stable_unit(seed, 18_101 + salt) * 2.0 - 1.0) * height * 0.065;
            let vertical = height * (0.445 + index as f32 * 0.044);
            let center = add3(
                [x, y, z + vertical],
                add3(basis.front_body, scale3(basis.right, lateral)),
            );
            self.world_oriented_rect_with_surface_response(
                center,
                basis.right,
                basis.up,
                [height * 0.036, height * 0.0025],
                crease_color,
                clothing_response,
            );
        }
    }

    fn add_humanoid_version4_anatomy_and_fabric_detail(
        &mut self,
        instance: &WindowHumanoidProxyInstance<'_>,
        proxy: &HumanProxyGeometry,
        basis: HumanoidDetailBasis,
        seed: u64,
    ) {
        if proxy.quality_tier < QualityTier::NormalRuntime {
            return;
        }

        let [x, y] = instance.center_meters;
        let z = instance.z_meters;
        let height = proxy.height_meters;
        let surface = instance.surface_state.as_ref();
        let skin_response = humanoid_skin_surface_response(surface);
        let cloth_response = humanoid_clothing_surface_response(surface);
        let skin_color = humanoid_default_skin_color(instance.color);
        let skin_shadow = humanoid_skin_soft_shadow_color(instance.color, surface);
        let skin_highlight = humanoid_skin_warm_highlight_color(instance.color, surface);
        let cloth_fold = humanoid_clothing_fold_shadow_color(instance.color, surface);
        let cloth_edge = humanoid_clothing_worn_edge_color(instance.color, surface);
        let boot_sole = humanoid_boot_sole_color(instance.color, surface);
        let hero = proxy.quality_tier >= QualityTier::HeroHighFidelityRuntime;
        let ellipsoid_segments = if hero { (5, 12) } else { (4, 8) };
        let cylinder_segments = if hero { 8 } else { 6 };
        let axes = [basis.right, basis.forward, basis.up];

        let neck_base = add3(
            [x, y, z + height * 0.754],
            scale3(basis.forward, height * 0.006),
        );
        let neck_top = add3(
            [x, y, z + height * 0.807],
            scale3(basis.forward, height * 0.008),
        );
        self.world_cylinder_between_with_surface_response(
            neck_base,
            neck_top,
            height * 0.038,
            cylinder_segments,
            window_scale_color(skin_color, 0.9),
            skin_response,
        );

        let jaw_center = add3(
            [x, y, z + height * 0.818],
            add3(basis.front_face, scale3(basis.forward, height * 0.012)),
        );
        self.world_oriented_ellipsoid_with_surface_response(
            jaw_center,
            axes,
            [height * 0.052, height * 0.026, height * 0.022],
            ellipsoid_segments.0,
            ellipsoid_segments.1,
            skin_shadow,
            skin_response,
        );

        for side in [-1.0_f32, 1.0] {
            let shoulder = add3(
                [x, y, z + height * 0.735],
                add3(
                    scale3(basis.right, side * height * 0.148),
                    scale3(basis.forward, height * 0.012),
                ),
            );
            self.world_oriented_ellipsoid_with_surface_response(
                shoulder,
                axes,
                [height * 0.036, height * 0.032, height * 0.024],
                ellipsoid_segments.0,
                ellipsoid_segments.1,
                window_scale_color(humanoid_clothing_color(instance.color), 1.08),
                cloth_response,
            );
            self.world_oriented_rect_with_surface_response(
                add3(
                    [x, y, z + height * 0.748],
                    add3(basis.front_body, scale3(basis.right, side * height * 0.073)),
                ),
                normalize3(add3(basis.right, scale3(basis.up, -side * 0.22))),
                basis.up,
                [height * 0.038, height * 0.0022],
                skin_shadow,
                skin_response,
            );
        }

        for side in [-1.0_f32, 1.0] {
            let ear_center = add3(
                [x, y, z + height * 0.878],
                add3(
                    add3(basis.front_face, scale3(basis.forward, -height * 0.018)),
                    scale3(basis.right, side * height * 0.104),
                ),
            );
            self.world_oriented_ellipsoid_with_surface_response(
                ear_center,
                axes,
                [height * 0.012, height * 0.008, height * 0.025],
                ellipsoid_segments.0,
                ellipsoid_segments.1,
                skin_shadow,
                skin_response,
            );
            if hero {
                self.world_oriented_rect_with_surface_response(
                    add3(ear_center, scale3(basis.forward, height * 0.004)),
                    basis.right,
                    basis.up,
                    [height * 0.0032, height * 0.013],
                    humanoid_skin_microdetail_color(instance.color, surface),
                    skin_response,
                );
            }
        }

        self.world_oriented_rect_with_surface_response(
            add3([x, y, z + height * 0.873], basis.front_face),
            basis.right,
            basis.up,
            [height * 0.012, height * 0.019],
            skin_highlight,
            skin_response,
        );
        for side in [-1.0_f32, 1.0] {
            self.world_oriented_rect_with_surface_response(
                add3(
                    [x, y, z + height * 0.852],
                    add3(basis.front_face, scale3(basis.right, side * height * 0.014)),
                ),
                basis.right,
                basis.up,
                [height * 0.0035, height * 0.0022],
                humanoid_inner_mouth_color(instance.color, surface),
                skin_response,
            );
        }

        if hero {
            let mouth_center = add3([x, y, z + height * 0.848], basis.front_face);
            self.world_oriented_rect_with_surface_response(
                add3(mouth_center, scale3(basis.up, height * 0.006)),
                basis.right,
                basis.up,
                [height * 0.024, height * 0.0025],
                humanoid_tooth_enamel_color(instance.color, surface),
                skin_response,
            );
            self.world_oriented_rect_with_surface_response(
                add3(mouth_center, scale3(basis.up, -height * 0.005)),
                basis.right,
                basis.up,
                [height * 0.017, height * 0.003],
                humanoid_tongue_color(instance.color, surface),
                skin_response,
            );
        }

        let belt_center = add3([x, y, z + height * 0.448], basis.front_body);
        self.world_oriented_rect_with_surface_response(
            belt_center,
            basis.right,
            basis.up,
            [height * 0.112, height * 0.007],
            cloth_edge,
            cloth_response,
        );

        for side in [-1.0_f32, 1.0] {
            let knee_center = add3(
                [x, y, z + height * 0.285],
                add3(basis.front_body, scale3(basis.right, side * height * 0.068)),
            );
            self.world_oriented_ellipsoid_with_surface_response(
                knee_center,
                axes,
                [height * 0.026, height * 0.016, height * 0.017],
                ellipsoid_segments.0,
                ellipsoid_segments.1,
                cloth_fold,
                cloth_response,
            );
            let ankle_center = add3(
                [x, y, z + height * 0.082],
                add3(basis.front_body, scale3(basis.right, side * height * 0.069)),
            );
            self.world_oriented_rect_with_surface_response(
                ankle_center,
                basis.right,
                basis.up,
                [height * 0.028, height * 0.005],
                boot_sole,
                cloth_response,
            );
            self.world_oriented_ellipsoid_with_surface_response(
                add3(ankle_center, scale3(basis.forward, height * 0.028)),
                axes,
                [height * 0.028, height * 0.046, height * 0.012],
                ellipsoid_segments.0,
                ellipsoid_segments.1,
                boot_sole,
                cloth_response,
            );
        }

        let fold_count = proxy
            .detail_profile
            .clothing_fold_count
            .max(if hero { 9 } else { 4 });
        for index in 0..fold_count {
            let salt = index as u64;
            let lateral = (window_stable_unit(seed, 21_103 + salt) * 2.0 - 1.0) * height * 0.091;
            let vertical = height * (0.482 + window_stable_unit(seed, 21_701 + salt) * 0.214);
            let tilt = (window_stable_unit(seed, 22_307 + salt) - 0.5) * 0.42;
            self.world_oriented_rect_with_surface_response(
                add3(
                    [x, y, z + vertical],
                    add3(basis.front_body, scale3(basis.right, lateral)),
                ),
                normalize3(add3(basis.right, scale3(basis.up, tilt))),
                basis.up,
                [
                    height * (0.018 + window_stable_unit(seed, 22_911 + salt) * 0.032),
                    height * 0.002,
                ],
                cloth_fold,
                cloth_response,
            );
        }

        for side in [-1.0_f32, 1.0] {
            let palm_center = add3(
                [x, y, z + height * 0.438],
                add3(
                    add3(basis.front_body, scale3(basis.right, side * height * 0.188)),
                    scale3(basis.forward, height * 0.012),
                ),
            );
            self.world_oriented_ellipsoid_with_surface_response(
                palm_center,
                axes,
                [height * 0.018, height * 0.011, height * 0.025],
                ellipsoid_segments.0,
                ellipsoid_segments.1,
                skin_color,
                skin_response,
            );

            if !hero {
                continue;
            }

            for finger in 0..4 {
                let finger_lateral = (finger as f32 - 1.5) * height * 0.006 * side;
                let finger_start = add3(
                    palm_center,
                    add3(
                        scale3(basis.right, finger_lateral),
                        scale3(basis.up, -height * 0.012),
                    ),
                );
                let finger_end = add3(
                    finger_start,
                    add3(
                        scale3(basis.up, -height * (0.024 + finger as f32 * 0.0015)),
                        scale3(basis.forward, height * 0.003),
                    ),
                );
                self.world_cylinder_between_with_surface_response(
                    finger_start,
                    finger_end,
                    height * 0.0032,
                    5,
                    window_scale_color(skin_color, 0.96),
                    skin_response,
                );
                self.world_oriented_rect_with_surface_response(
                    finger_end,
                    basis.right,
                    basis.up,
                    [height * 0.0026, height * 0.0015],
                    humanoid_fingernail_color(instance.color, surface),
                    skin_response,
                );
            }
        }
    }

    fn add_humanoid_version5_photoreal_proxy_layers(
        &mut self,
        instance: &WindowHumanoidProxyInstance<'_>,
        proxy: &HumanProxyGeometry,
        basis: HumanoidDetailBasis,
        seed: u64,
    ) {
        if !proxy.detail_profile.supports_photoreal_near_proxy() {
            return;
        }

        let [x, y] = instance.center_meters;
        let z = instance.z_meters;
        let height = proxy.height_meters;
        let surface = instance.surface_state.as_ref();
        let skin_response = humanoid_skin_surface_response(surface);
        let eye_response = humanoid_eye_surface_response(surface);
        let hair_response = humanoid_hair_surface_response(surface);
        let cloth_response = humanoid_clothing_surface_response(surface);
        let skin = humanoid_default_skin_color(instance.color);
        let skin_shadow = humanoid_skin_soft_shadow_color(instance.color, surface);
        let skin_highlight = humanoid_skin_warm_highlight_color(instance.color, surface);
        let pore_color = humanoid_skin_microdetail_color(instance.color, surface);
        let hair_color = humanoid_hair_color(instance.color, surface);
        let brow_color = window_scale_color(hair_color, 0.74);
        let cloth = humanoid_clothing_color(instance.color);
        let seam = humanoid_clothing_seam_color(instance.color, surface);
        let crease = humanoid_clothing_crease_color(instance.color, surface);
        let cloth_edge = humanoid_clothing_worn_edge_color(instance.color, surface);
        let face_front = basis.front_face;
        let body_front = basis.front_body;
        let axes = [basis.right, basis.forward, basis.up];

        let nose_bridge = add3([x, y, z + height * 0.875], face_front);
        self.world_cylinder_between_with_surface_response(
            add3(nose_bridge, scale3(basis.up, height * 0.028)),
            add3(
                add3(nose_bridge, scale3(basis.forward, height * 0.022)),
                scale3(basis.up, -height * 0.034),
            ),
            height * 0.008,
            8,
            window_scale_color(skin, 0.96),
            skin_response,
        );
        self.world_oriented_ellipsoid_with_surface_response(
            add3(
                add3(nose_bridge, scale3(basis.forward, height * 0.028)),
                scale3(basis.up, -height * 0.038),
            ),
            axes,
            [height * 0.014, height * 0.010, height * 0.012],
            6,
            12,
            skin_highlight,
            skin_response,
        );
        for side in [-1.0_f32, 1.0] {
            self.world_oriented_rect_with_surface_response(
                add3(
                    add3(nose_bridge, scale3(basis.forward, height * 0.038)),
                    add3(
                        scale3(basis.right, side * height * 0.010),
                        scale3(basis.up, -height * 0.047),
                    ),
                ),
                basis.right,
                basis.up,
                [height * 0.0042, height * 0.0026],
                [0.058, 0.034, 0.026, 0.50],
                skin_response,
            );
        }

        for side in [-1.0_f32, 1.0] {
            let eye_center = add3(
                [x, y, z + height * 0.898],
                add3(face_front, scale3(basis.right, side * height * 0.038)),
            );
            self.world_oriented_ellipsoid_with_surface_response(
                add3(eye_center, scale3(basis.forward, height * 0.004)),
                axes,
                [height * 0.014, height * 0.005, height * 0.007],
                6,
                12,
                [0.88, 0.96, 1.0, 0.82],
                eye_response,
            );
            for lid in [-1.0_f32, 1.0] {
                self.world_oriented_rect_with_surface_response(
                    add3(eye_center, scale3(basis.up, lid * height * 0.007)),
                    basis.right,
                    basis.up,
                    [height * 0.018, height * 0.0025],
                    skin_shadow,
                    skin_response,
                );
            }
            self.world_oriented_rect_with_surface_response(
                add3(eye_center, scale3(basis.up, height * 0.018)),
                basis.right,
                basis.up,
                [height * 0.026, height * 0.0032],
                brow_color,
                hair_response,
            );
        }

        for index in 0..proxy.detail_profile.eyelash_card_count {
            let salt = index as u64;
            let side = if index % 2 == 0 { -1.0 } else { 1.0 };
            let row = (index / 2) as f32;
            let lateral = side * height * (0.024 + row * 0.0058);
            let center = add3(
                [
                    x,
                    y,
                    z + height * (0.904 + window_stable_unit(seed, 24_301 + salt) * 0.010),
                ],
                add3(face_front, scale3(basis.right, lateral)),
            );
            self.world_oriented_rect_with_surface_response(
                center,
                normalize3(add3(basis.right, scale3(basis.up, side * 0.18))),
                basis.up,
                [height * 0.0018, height * 0.013],
                brow_color,
                hair_response,
            );
        }

        for index in 0..proxy.detail_profile.eyebrow_card_count {
            let salt = index as u64;
            let side = if index % 2 == 0 { -1.0 } else { 1.0 };
            let row = (index / 2) as f32;
            let lateral = side * height * (0.028 + row * 0.010);
            self.world_oriented_rect_with_surface_response(
                add3(
                    [
                        x,
                        y,
                        z + height * (0.923 + window_stable_unit(seed, 24_901 + salt) * 0.006),
                    ],
                    add3(face_front, scale3(basis.right, lateral)),
                ),
                normalize3(add3(basis.right, scale3(basis.up, side * 0.08))),
                basis.up,
                [height * 0.013, height * 0.0024],
                humanoid_hair_strand_color(instance.color, surface, seed, salt),
                hair_response,
            );
        }

        let mouth_center = add3([x, y, z + height * 0.848], face_front);
        let lip_color = window_clamp_color([
            skin[0] * 0.72 + 0.13,
            skin[1] * 0.54 + 0.045,
            skin[2] * 0.52 + 0.045,
            0.72,
        ]);
        self.world_oriented_rect_with_surface_response(
            add3(mouth_center, scale3(basis.up, height * 0.004)),
            basis.right,
            basis.up,
            [height * 0.026, height * 0.0034],
            lip_color,
            skin_response,
        );
        self.world_oriented_rect_with_surface_response(
            add3(mouth_center, scale3(basis.up, -height * 0.004)),
            basis.right,
            basis.up,
            [height * 0.023, height * 0.0038],
            window_scale_color(lip_color, 0.72),
            skin_response,
        );
        if proxy.detail_profile.has_teeth_tongue {
            self.world_oriented_rect_with_surface_response(
                add3(mouth_center, scale3(basis.forward, height * 0.003)),
                basis.right,
                basis.up,
                [height * 0.020, height * 0.0024],
                humanoid_tooth_enamel_color(instance.color, surface),
                skin_response,
            );
            self.world_oriented_rect_with_surface_response(
                add3(
                    add3(mouth_center, scale3(basis.forward, height * 0.004)),
                    scale3(basis.up, -height * 0.007),
                ),
                basis.right,
                basis.up,
                [height * 0.015, height * 0.0032],
                humanoid_tongue_color(instance.color, surface),
                skin_response,
            );
        }

        for side in [-1.0_f32, 1.0] {
            let cheek = add3(
                [x, y, z + height * 0.858],
                add3(face_front, scale3(basis.right, side * height * 0.064)),
            );
            self.world_oriented_ellipsoid_with_surface_response(
                cheek,
                axes,
                [height * 0.032, height * 0.014, height * 0.024],
                5,
                10,
                window_with_alpha(skin_highlight, 0.32),
                skin_response,
            );
            self.world_oriented_rect_with_surface_response(
                add3(cheek, scale3(basis.up, -height * 0.028)),
                normalize3(add3(basis.right, scale3(basis.up, -side * 0.20))),
                basis.up,
                [height * 0.034, height * 0.0022],
                skin_shadow,
                skin_response,
            );
        }

        for index in 0..24 {
            let salt = index as u64;
            let lateral = (window_stable_unit(seed, 25_101 + salt) * 2.0 - 1.0) * height * 0.088;
            let vertical = height * (0.803 + window_stable_unit(seed, 25_701 + salt) * 0.134);
            let size = height * (0.0016 + window_stable_unit(seed, 26_301 + salt) * 0.0024);
            self.world_oriented_rect_with_surface_response(
                add3(
                    [x, y, z + vertical],
                    add3(face_front, scale3(basis.right, lateral)),
                ),
                basis.right,
                basis.up,
                [size, size],
                if index % 5 == 0 {
                    [0.15, 0.105, 0.070, 0.32]
                } else {
                    pore_color
                },
                skin_response,
            );
        }

        let hair_shell_center = add3(
            [x, y, z + height * 0.942],
            scale3(basis.forward, -height * 0.010),
        );
        self.world_oriented_ellipsoid_with_surface_response(
            hair_shell_center,
            axes,
            [height * 0.092, height * 0.074, height * 0.076],
            7,
            16,
            window_scale_color(hair_color, 0.82),
            hair_response,
        );
        let extra_hair_cards = (proxy.detail_profile.hair_card_count / 3).max(12);
        for index in 0..extra_hair_cards {
            let salt = index as u64;
            let t = index as f32 / extra_hair_cards.max(1) as f32;
            let angle = t * std::f32::consts::TAU;
            let lateral = angle.cos() * height * 0.082;
            let drop = height * (0.040 + window_stable_unit(seed, 27_101 + salt) * 0.080);
            self.world_oriented_rect_with_surface_response(
                add3(
                    [x, y, z + height * 0.936 - drop * 0.28],
                    add3(
                        add3(face_front, scale3(basis.right, lateral)),
                        scale3(basis.forward, angle.sin() * height * 0.022),
                    ),
                ),
                basis.right,
                basis.up,
                [height * 0.0022, drop * 0.62],
                humanoid_hair_strand_color(instance.color, surface, seed, salt),
                hair_response,
            );
        }

        let collar = add3([x, y, z + height * 0.742], body_front);
        for side in [-1.0_f32, 1.0] {
            self.world_oriented_rect_with_surface_response(
                add3(collar, scale3(basis.right, side * height * 0.046)),
                normalize3(add3(basis.right, scale3(basis.up, -side * 0.32))),
                basis.up,
                [height * 0.052, height * 0.010],
                window_scale_color(cloth, 1.16),
                cloth_response,
            );
        }
        for index in 0..proxy.detail_profile.clothing_stitch_count {
            let side = if index % 2 == 0 { -1.0 } else { 1.0 };
            let row = (index / 2) as f32;
            let center = add3(
                [x, y, z + height * (0.470 + row * 0.025)],
                add3(body_front, scale3(basis.right, side * height * 0.074)),
            );
            self.world_oriented_rect_with_surface_response(
                center,
                basis.right,
                basis.up,
                [height * 0.006, height * 0.0018],
                seam,
                cloth_response,
            );
        }
        if proxy.detail_profile.has_cloth_weave {
            for index in 0..20 {
                let salt = index as u64;
                let lateral =
                    (window_stable_unit(seed, 28_101 + salt) * 2.0 - 1.0) * height * 0.092;
                let vertical = height * (0.505 + window_stable_unit(seed, 28_701 + salt) * 0.190);
                self.world_oriented_rect_with_surface_response(
                    add3(
                        [x, y, z + vertical],
                        add3(body_front, scale3(basis.right, lateral)),
                    ),
                    if index % 2 == 0 {
                        basis.right
                    } else {
                        basis.up
                    },
                    if index % 2 == 0 {
                        basis.up
                    } else {
                        basis.right
                    },
                    [height * 0.015, height * 0.0012],
                    crease,
                    cloth_response,
                );
            }
        }

        for side in [-1.0_f32, 1.0] {
            let wrist = add3(
                [x, y, z + height * 0.400],
                add3(body_front, scale3(basis.right, side * height * 0.190)),
            );
            self.world_oriented_ellipsoid_with_surface_response(
                wrist,
                axes,
                [height * 0.020, height * 0.013, height * 0.019],
                5,
                10,
                skin,
                skin_response,
            );
            for finger in 0..5 {
                let lateral = side * (finger as f32 - 2.0) * height * 0.0056;
                let start = add3(
                    wrist,
                    add3(
                        scale3(basis.right, lateral),
                        scale3(basis.up, -height * 0.012),
                    ),
                );
                let end = add3(
                    start,
                    add3(
                        scale3(basis.up, -height * (0.022 + finger as f32 * 0.0017)),
                        scale3(basis.forward, height * (0.008 + finger as f32 * 0.0012)),
                    ),
                );
                self.world_cylinder_between_with_surface_response(
                    start,
                    end,
                    height * 0.0028,
                    5,
                    window_scale_color(skin, 0.94),
                    skin_response,
                );
                self.world_oriented_rect_with_surface_response(
                    end,
                    basis.right,
                    basis.up,
                    [height * 0.0024, height * 0.0014],
                    humanoid_fingernail_color(instance.color, surface),
                    skin_response,
                );
            }
        }

        for side in [-1.0_f32, 1.0] {
            let boot = add3(
                [x, y, z + height * 0.050],
                add3(body_front, scale3(basis.right, side * height * 0.066)),
            );
            self.world_oriented_rect_with_surface_response(
                add3(boot, scale3(basis.forward, height * 0.046)),
                basis.right,
                basis.up,
                [height * 0.035, height * 0.004],
                cloth_edge,
                cloth_response,
            );
            self.world_oriented_rect_with_surface_response(
                add3(boot, scale3(basis.forward, height * 0.070)),
                basis.right,
                basis.up,
                [height * 0.028, height * 0.003],
                seam,
                cloth_response,
            );
        }
    }

    fn add_humanoid_version6_silhouette_refinement(
        &mut self,
        instance: &WindowHumanoidProxyInstance<'_>,
        proxy: &HumanProxyGeometry,
        basis: HumanoidDetailBasis,
        seed: u64,
    ) {
        if !proxy.detail_profile.supports_photoreal_near_proxy() {
            return;
        }

        let [x, y] = instance.center_meters;
        let z = instance.z_meters;
        let height = proxy.height_meters;
        let surface = instance.surface_state.as_ref();
        let skin_response = humanoid_skin_surface_response(surface);
        let cloth_response = humanoid_clothing_surface_response(surface);
        let hair_response = humanoid_hair_surface_response(surface);
        let skin_shadow = humanoid_skin_soft_shadow_color(instance.color, surface);
        let skin_highlight = humanoid_skin_warm_highlight_color(instance.color, surface);
        let cloth = humanoid_clothing_color(instance.color);
        let cloth_edge = humanoid_clothing_worn_edge_color(instance.color, surface);
        let crease = humanoid_clothing_crease_color(instance.color, surface);
        let boot = humanoid_boot_color(instance.color);
        let boot_sole = humanoid_boot_sole_color(instance.color, surface);
        let body_front = basis.front_body;
        let face_front = basis.front_face;
        let axes = [basis.right, basis.forward, basis.up];
        let pose_shift = (window_stable_unit(seed, 29_101) * 2.0 - 1.0) * height * 0.006;
        let breath_lift = window_stable_unit(seed, 29_701) * height * 0.006;

        let ribcage = add3(
            [x, y, z + height * 0.642 + breath_lift],
            add3(body_front, scale3(basis.right, pose_shift)),
        );
        self.world_oriented_ellipsoid_with_surface_response(
            ribcage,
            axes,
            [height * 0.120, height * 0.043, height * 0.098],
            8,
            18,
            window_scale_color(cloth, 1.08),
            cloth_response,
        );
        let abdomen = add3(
            [x, y, z + height * 0.538],
            add3(body_front, scale3(basis.right, pose_shift * 0.45)),
        );
        self.world_oriented_ellipsoid_with_surface_response(
            abdomen,
            axes,
            [height * 0.092, height * 0.034, height * 0.070],
            7,
            16,
            window_scale_color(cloth, 0.88),
            cloth_response,
        );
        let pelvis = add3(
            [x, y, z + height * 0.444],
            add3(body_front, scale3(basis.right, -pose_shift * 0.35)),
        );
        self.world_oriented_ellipsoid_with_surface_response(
            pelvis,
            axes,
            [height * 0.106, height * 0.037, height * 0.050],
            6,
            14,
            window_scale_color(cloth, 0.78),
            cloth_response,
        );

        for side in [-1.0_f32, 1.0] {
            let clavicle = add3(
                [x, y, z + height * 0.754],
                add3(body_front, scale3(basis.right, side * height * 0.044)),
            );
            self.world_oriented_rect_with_surface_response(
                clavicle,
                normalize3(add3(basis.right, scale3(basis.up, -side * 0.28))),
                basis.up,
                [height * 0.055, height * 0.0021],
                skin_shadow,
                skin_response,
            );

            let elbow = add3(
                [x, y, z + height * 0.572],
                add3(body_front, scale3(basis.right, side * height * 0.178)),
            );
            self.world_oriented_ellipsoid_with_surface_response(
                elbow,
                axes,
                [height * 0.019, height * 0.014, height * 0.018],
                5,
                10,
                crease,
                cloth_response,
            );
            let wrist_cuff = add3(
                [x, y, z + height * 0.388],
                add3(body_front, scale3(basis.right, side * height * 0.186)),
            );
            self.world_oriented_rect_with_surface_response(
                wrist_cuff,
                basis.right,
                basis.up,
                [height * 0.025, height * 0.0042],
                cloth_edge,
                cloth_response,
            );
            for knuckle in 0..4 {
                let lateral = side * (knuckle as f32 - 1.5) * height * 0.0068;
                self.world_oriented_ellipsoid_with_surface_response(
                    add3(
                        wrist_cuff,
                        add3(
                            add3(
                                scale3(basis.right, lateral),
                                scale3(basis.up, -height * 0.020),
                            ),
                            scale3(basis.forward, height * 0.010),
                        ),
                    ),
                    axes,
                    [height * 0.0038, height * 0.0022, height * 0.0028],
                    4,
                    8,
                    window_scale_color(skin_highlight, 0.92),
                    skin_response,
                );
            }

            let hip_shadow = add3(
                [x, y, z + height * 0.452],
                add3(body_front, scale3(basis.right, side * height * 0.088)),
            );
            self.world_oriented_rect_with_surface_response(
                hip_shadow,
                normalize3(add3(basis.up, scale3(basis.right, -side * 0.16))),
                basis.right,
                [height * 0.042, height * 0.002],
                crease,
                cloth_response,
            );
            let thigh_volume = add3(
                [x, y, z + height * 0.342],
                add3(body_front, scale3(basis.right, side * height * 0.065)),
            );
            self.world_oriented_ellipsoid_with_surface_response(
                thigh_volume,
                axes,
                [height * 0.034, height * 0.024, height * 0.090],
                6,
                14,
                window_scale_color(cloth, 0.94),
                cloth_response,
            );
            let calf_volume = add3(
                [x, y, z + height * 0.180],
                add3(body_front, scale3(basis.right, side * height * 0.069)),
            );
            self.world_oriented_ellipsoid_with_surface_response(
                calf_volume,
                axes,
                [height * 0.025, height * 0.020, height * 0.068],
                5,
                12,
                window_scale_color(cloth, 0.82),
                cloth_response,
            );
            let knee_shadow = add3(
                [x, y, z + height * 0.286],
                add3(body_front, scale3(basis.right, side * height * 0.067)),
            );
            self.world_oriented_rect_with_surface_response(
                knee_shadow,
                basis.right,
                basis.up,
                [height * 0.031, height * 0.0032],
                crease,
                cloth_response,
            );

            let shoe_center = add3(
                [x, y, z + height * 0.043],
                add3(body_front, scale3(basis.right, side * height * 0.068)),
            );
            self.world_oriented_ellipsoid_with_surface_response(
                add3(shoe_center, scale3(basis.forward, height * 0.055)),
                axes,
                [height * 0.036, height * 0.056, height * 0.017],
                5,
                12,
                boot,
                cloth_response,
            );
            self.world_oriented_rect_with_surface_response(
                add3(shoe_center, scale3(basis.forward, height * 0.080)),
                basis.right,
                basis.up,
                [height * 0.026, height * 0.0022],
                boot_sole,
                cloth_response,
            );
            for lace in 0..3 {
                self.world_oriented_rect_with_surface_response(
                    add3(
                        shoe_center,
                        add3(
                            scale3(basis.forward, height * (0.034 + lace as f32 * 0.012)),
                            scale3(basis.up, height * 0.010),
                        ),
                    ),
                    normalize3(add3(basis.right, scale3(basis.up, side * 0.18))),
                    basis.up,
                    [height * 0.019, height * 0.0012],
                    cloth_edge,
                    cloth_response,
                );
            }
        }

        for side in [-1.0_f32, 1.0] {
            let eye_under = add3(
                [x, y, z + height * 0.884],
                add3(face_front, scale3(basis.right, side * height * 0.039)),
            );
            self.world_oriented_rect_with_surface_response(
                eye_under,
                basis.right,
                basis.up,
                [height * 0.024, height * 0.002],
                skin_shadow,
                skin_response,
            );
            let nasolabial = add3(
                [x, y, z + height * 0.845],
                add3(face_front, scale3(basis.right, side * height * 0.033)),
            );
            self.world_oriented_rect_with_surface_response(
                nasolabial,
                normalize3(add3(basis.up, scale3(basis.right, -side * 0.34))),
                basis.right,
                [height * 0.042, height * 0.0018],
                window_with_alpha(skin_shadow, 0.40),
                skin_response,
            );
            let sideburn = add3(
                [x, y, z + height * 0.900],
                add3(face_front, scale3(basis.right, side * height * 0.090)),
            );
            self.world_oriented_rect_with_surface_response(
                sideburn,
                basis.right,
                basis.up,
                [height * 0.004, height * 0.045],
                humanoid_hair_strand_color(
                    instance.color,
                    surface,
                    seed,
                    30_000 + side.to_bits() as u64,
                ),
                hair_response,
            );
        }
    }

    fn add_humanoid_version7_articulated_closeup_detail(
        &mut self,
        instance: &WindowHumanoidProxyInstance<'_>,
        proxy: &HumanProxyGeometry,
        basis: HumanoidDetailBasis,
        seed: u64,
    ) {
        if !proxy.detail_profile.supports_photoreal_near_proxy() {
            return;
        }

        let [x, y] = instance.center_meters;
        let z = instance.z_meters;
        let height = proxy.height_meters;
        let surface = instance.surface_state.as_ref();
        let skin_response = humanoid_skin_surface_response(surface);
        let cloth_response = humanoid_clothing_surface_response(surface);
        let hair_response = humanoid_hair_surface_response(surface);
        let skin = humanoid_default_skin_color(instance.color);
        let skin_shadow = humanoid_skin_cool_shadow_color(instance.color, surface);
        let skin_highlight = humanoid_skin_warm_highlight_color(instance.color, surface);
        let cloth = humanoid_clothing_color(instance.color);
        let inner_layer = humanoid_clothing_inner_layer_color(instance.color, surface);
        let seam = humanoid_clothing_seam_color(instance.color, surface);
        let crease = humanoid_clothing_crease_color(instance.color, surface);
        let cloth_edge = humanoid_clothing_worn_edge_color(instance.color, surface);
        let boot = humanoid_boot_color(instance.color);
        let boot_sole = humanoid_boot_sole_color(instance.color, surface);
        let hairline = humanoid_hairline_shadow_color(instance.color, surface);
        let axes = [basis.right, basis.forward, basis.up];
        let locomotion = instance.locomotion_weight_0_to_1.clamp(0.0, 1.0);
        let crouch = instance.crouch_weight_0_to_1.clamp(0.0, 1.0);
        let gait_swing = locomotion * height * 0.028;
        let crouch_forward = crouch * height * 0.038;

        let garment_layers = proxy.detail_profile.garment_layer_count.max(3) as usize;
        let back_body = scale3(basis.forward, -height * 0.072);
        self.world_oriented_rect_with_surface_response(
            add3(
                [x, y, z + height * 0.595],
                add3(basis.front_body, scale3(basis.up, 0.0)),
            ),
            basis.right,
            basis.up,
            [height * 0.054, height * 0.112],
            inner_layer,
            cloth_response,
        );
        self.world_oriented_rect_with_surface_response(
            add3([x, y, z + height * 0.632], back_body),
            basis.right,
            basis.up,
            [height * 0.102, height * 0.126],
            window_scale_color(cloth, 0.74),
            cloth_response,
        );
        for layer in 0..garment_layers {
            let t = if garment_layers <= 1 {
                0.0
            } else {
                layer as f32 / (garment_layers - 1) as f32
            };
            let vertical = height * (0.462 + t * 0.118);
            let half_width = height * (0.096 - t * 0.010).max(0.062);
            self.world_oriented_rect_with_surface_response(
                add3([x, y, z + vertical], basis.front_body),
                basis.right,
                basis.up,
                [half_width, height * 0.0042],
                if layer == 0 { cloth_edge } else { seam },
                cloth_response,
            );
        }
        for side in [-1.0_f32, 1.0] {
            let side_panel = add3(
                [x, y, z + height * 0.590],
                add3(
                    scale3(basis.right, side * height * 0.118),
                    scale3(basis.forward, height * 0.004),
                ),
            );
            self.world_oriented_rect_with_surface_response(
                side_panel,
                basis.forward,
                basis.up,
                [height * 0.0052, height * 0.172],
                seam,
                cloth_response,
            );
            self.world_oriented_rect_with_surface_response(
                add3(side_panel, scale3(basis.forward, -height * 0.076)),
                basis.forward,
                basis.up,
                [height * 0.004, height * 0.116],
                window_scale_color(crease, 0.92),
                cloth_response,
            );
        }

        let hairline_count = proxy.detail_profile.hairline_detail_count.max(12) as usize;
        for index in 0..hairline_count {
            let salt = index as u64;
            let t = if hairline_count <= 1 {
                0.0
            } else {
                index as f32 / hairline_count as f32
            };
            let angle = t * std::f32::consts::TAU;
            let lateral = angle.cos() * height * 0.084
                + (window_stable_unit(seed, 31_101 + salt) - 0.5) * height * 0.006;
            let forward_offset = angle.sin() * height * 0.060 - height * 0.006;
            let drop = height * (0.018 + window_stable_unit(seed, 31_701 + salt) * 0.030);
            let tangent = normalize3(add3(
                scale3(basis.right, -angle.sin()),
                scale3(basis.forward, angle.cos()),
            ));
            self.world_oriented_rect_with_surface_response(
                add3(
                    [x, y, z + height * 0.932 - drop * 0.35],
                    add3(
                        scale3(basis.right, lateral),
                        scale3(basis.forward, forward_offset),
                    ),
                ),
                tangent,
                basis.up,
                [
                    height * (0.0020 + window_stable_unit(seed, 32_301 + salt) * 0.0014),
                    drop,
                ],
                if index % 3 == 0 {
                    hairline
                } else {
                    humanoid_hair_strand_color(instance.color, surface, seed, salt)
                },
                hair_response,
            );
        }
        for side in [-1.0_f32, 1.0] {
            self.world_oriented_ellipsoid_with_surface_response(
                add3(
                    [x, y, z + height * 0.906],
                    add3(
                        scale3(basis.right, side * height * 0.092),
                        scale3(basis.forward, -height * 0.016),
                    ),
                ),
                axes,
                [height * 0.016, height * 0.018, height * 0.052],
                5,
                10,
                hairline,
                hair_response,
            );
        }
        self.world_oriented_rect_with_surface_response(
            add3(
                [x, y, z + height * 0.872],
                scale3(basis.forward, -height * 0.082),
            ),
            basis.right,
            basis.up,
            [height * 0.056, height * 0.036],
            hairline,
            hair_response,
        );

        for side in [-1.0_f32, 1.0] {
            let wrist = add3(
                [x, y, z + height * (0.394 - crouch * 0.018)],
                add3(
                    add3(basis.front_body, scale3(basis.right, side * height * 0.188)),
                    scale3(basis.forward, -side * gait_swing * 0.46),
                ),
            );
            let palm = add3(wrist, scale3(basis.forward, height * 0.014));
            self.world_oriented_ellipsoid_with_surface_response(
                palm,
                axes,
                [height * 0.020, height * 0.014, height * 0.026],
                5,
                10,
                skin,
                skin_response,
            );
            self.world_oriented_rect_with_surface_response(
                add3(palm, scale3(basis.forward, height * 0.012)),
                basis.right,
                basis.up,
                [height * 0.014, height * 0.0018],
                skin_shadow,
                skin_response,
            );

            let thumb_root = add3(
                palm,
                add3(
                    add3(
                        scale3(basis.right, -side * height * 0.018),
                        scale3(basis.up, -height * 0.004),
                    ),
                    scale3(basis.forward, height * 0.012),
                ),
            );
            let thumb_mid = add3(
                thumb_root,
                add3(
                    add3(
                        scale3(basis.right, -side * height * 0.018),
                        scale3(basis.up, -height * 0.008),
                    ),
                    scale3(basis.forward, height * 0.010),
                ),
            );
            let thumb_tip = add3(
                thumb_mid,
                add3(
                    add3(
                        scale3(basis.right, -side * height * 0.012),
                        scale3(basis.up, -height * 0.008),
                    ),
                    scale3(basis.forward, height * 0.006),
                ),
            );
            self.world_cylinder_between_with_surface_response(
                thumb_root,
                thumb_mid,
                height * 0.0032,
                5,
                window_scale_color(skin, 0.95),
                skin_response,
            );
            self.world_cylinder_between_with_surface_response(
                thumb_mid,
                thumb_tip,
                height * 0.0028,
                5,
                window_scale_color(skin, 0.92),
                skin_response,
            );
            self.world_oriented_rect_with_surface_response(
                thumb_tip,
                basis.right,
                basis.up,
                [height * 0.0028, height * 0.0015],
                humanoid_fingernail_color(instance.color, surface),
                skin_response,
            );

            for finger in 0..5 {
                let finger_t = finger as f32 - 2.0;
                let lateral = side * finger_t * height * 0.0057;
                let root = add3(
                    palm,
                    add3(
                        scale3(basis.right, lateral),
                        scale3(basis.up, -height * 0.014),
                    ),
                );
                let mid = add3(
                    root,
                    add3(
                        scale3(basis.up, -height * (0.013 + finger as f32 * 0.0012)),
                        scale3(basis.forward, height * (0.011 + finger as f32 * 0.0008)),
                    ),
                );
                let tip = add3(
                    mid,
                    add3(
                        scale3(basis.up, -height * (0.010 + finger as f32 * 0.0009)),
                        scale3(basis.forward, height * 0.006),
                    ),
                );
                self.world_oriented_ellipsoid_with_surface_response(
                    root,
                    axes,
                    [height * 0.0038, height * 0.0026, height * 0.0034],
                    4,
                    8,
                    skin_highlight,
                    skin_response,
                );
                self.world_cylinder_between_with_surface_response(
                    root,
                    mid,
                    height * 0.0027,
                    5,
                    window_scale_color(skin, 0.95),
                    skin_response,
                );
                self.world_cylinder_between_with_surface_response(
                    mid,
                    tip,
                    height * 0.0023,
                    5,
                    window_scale_color(skin, 0.91),
                    skin_response,
                );
                self.world_oriented_ellipsoid_with_surface_response(
                    mid,
                    axes,
                    [height * 0.0029, height * 0.0020, height * 0.0028],
                    4,
                    8,
                    skin_shadow,
                    skin_response,
                );
                self.world_oriented_rect_with_surface_response(
                    tip,
                    basis.right,
                    basis.up,
                    [height * 0.0023, height * 0.0014],
                    humanoid_fingernail_color(instance.color, surface),
                    skin_response,
                );
            }
        }

        let footwear_detail_count = proxy.detail_profile.footwear_detail_count.max(6) as usize;
        for side in [-1.0_f32, 1.0] {
            let stride = side * gait_swing * 0.72;
            let shoe = add3(
                [x, y, z + height * 0.044],
                add3(
                    add3(basis.front_body, scale3(basis.right, side * height * 0.068)),
                    scale3(basis.forward, stride + crouch_forward + height * 0.020),
                ),
            );
            let heel = add3(shoe, scale3(basis.forward, -height * 0.030));
            let toe = add3(shoe, scale3(basis.forward, height * 0.064));
            self.world_oriented_ellipsoid_with_surface_response(
                heel,
                axes,
                [height * 0.026, height * 0.026, height * 0.014],
                5,
                10,
                window_scale_color(boot, 0.72),
                cloth_response,
            );
            self.world_oriented_ellipsoid_with_surface_response(
                toe,
                axes,
                [height * 0.037, height * 0.040, height * 0.016],
                5,
                12,
                boot,
                cloth_response,
            );
            self.world_oriented_rect_with_surface_response(
                add3(toe, scale3(basis.forward, height * 0.033)),
                basis.right,
                basis.up,
                [height * 0.031, height * 0.0032],
                boot_sole,
                cloth_response,
            );
            for detail in 0..footwear_detail_count {
                let salt = detail as u64;
                let t = if footwear_detail_count <= 1 {
                    0.0
                } else {
                    detail as f32 / (footwear_detail_count - 1) as f32
                };
                let forward_offset = height * (0.006 + t * 0.067);
                let lateral = (window_stable_unit(seed, 33_101 + salt) - 0.5) * height * 0.010;
                self.world_oriented_rect_with_surface_response(
                    add3(
                        shoe,
                        add3(
                            add3(
                                scale3(basis.forward, forward_offset),
                                scale3(basis.right, lateral),
                            ),
                            scale3(basis.up, height * 0.012),
                        ),
                    ),
                    normalize3(add3(basis.right, scale3(basis.up, side * 0.12))),
                    basis.up,
                    [height * 0.018, height * 0.0013],
                    if detail % 3 == 0 {
                        boot_sole
                    } else {
                        cloth_edge
                    },
                    cloth_response,
                );
            }
        }

        let deformation_count = proxy.detail_profile.pose_deformation_zone_count.max(8) as usize;
        for index in 0..deformation_count {
            let side = if index % 2 == 0 { -1.0 } else { 1.0 };
            let band = (index / 2) % 6;
            let (vertical, lateral, forward_offset, width, use_skin) = match band {
                0 => (0.738, side * 0.078, 0.082, 0.042, true),
                1 => (0.572, side * 0.176, 0.076, 0.030, false),
                2 => (0.448, side * 0.090, 0.078, 0.040, false),
                3 => (0.286, side * 0.067, 0.088 + crouch * 0.040, 0.036, false),
                4 => (
                    0.392,
                    side * 0.184,
                    0.090 - side * locomotion * 0.022,
                    0.024,
                    true,
                ),
                _ => (0.180, side * 0.070, 0.082 + crouch * 0.028, 0.028, false),
            };
            self.world_oriented_rect_with_surface_response(
                add3(
                    [x, y, z + height * vertical],
                    add3(
                        scale3(basis.right, lateral * height),
                        scale3(basis.forward, forward_offset * height),
                    ),
                ),
                normalize3(add3(basis.right, scale3(basis.up, -side * 0.18))),
                basis.up,
                [height * width, height * 0.0021],
                if use_skin { skin_shadow } else { crease },
                if use_skin {
                    skin_response
                } else {
                    cloth_response
                },
            );
        }
    }

    fn add_humanoid_version8_soft_tissue_form_detail(
        &mut self,
        instance: &WindowHumanoidProxyInstance<'_>,
        proxy: &HumanProxyGeometry,
        basis: HumanoidDetailBasis,
        seed: u64,
    ) {
        if !proxy.detail_profile.supports_photoreal_near_proxy() {
            return;
        }

        let [x, y] = instance.center_meters;
        let z = instance.z_meters;
        let height = proxy.height_meters;
        let surface = instance.surface_state.as_ref();
        let skin_response = humanoid_skin_surface_response(surface);
        let cloth_response = humanoid_clothing_surface_response(surface);
        let skin_subsurface = humanoid_soft_tissue_subsurface_color(instance.color, surface);
        let skin_blush = humanoid_soft_tissue_blush_color(instance.color, surface);
        let skin_shadow = humanoid_skin_soft_shadow_color(instance.color, surface);
        let skin_cool = humanoid_skin_cool_shadow_color(instance.color, surface);
        let cloth_pressure = humanoid_clothing_pressure_color(instance.color, surface);
        let cloth_crease = humanoid_clothing_crease_color(instance.color, surface);
        let axes = [basis.right, basis.forward, basis.up];
        let form_count = proxy.detail_profile.soft_tissue_form_count.max(8) as usize;
        let locomotion = instance.locomotion_weight_0_to_1.clamp(0.0, 1.0);
        let crouch = instance.crouch_weight_0_to_1.clamp(0.0, 1.0);
        let breath = height * (0.004 + window_stable_unit(seed, 34_101) * 0.004);

        for side in [-1.0_f32, 1.0] {
            let cheek = add3(
                [x, y, z + height * 0.862],
                add3(
                    add3(basis.front_face, scale3(basis.right, side * height * 0.060)),
                    scale3(basis.forward, height * 0.014),
                ),
            );
            self.world_oriented_ellipsoid_with_surface_response(
                cheek,
                axes,
                [height * 0.032, height * 0.014, height * 0.026],
                6,
                12,
                skin_blush,
                skin_response,
            );
            self.world_oriented_ellipsoid_with_surface_response(
                add3(
                    [x, y, z + height * 0.824],
                    add3(basis.front_face, scale3(basis.right, side * height * 0.050)),
                ),
                axes,
                [height * 0.034, height * 0.012, height * 0.018],
                5,
                10,
                skin_subsurface,
                skin_response,
            );
            self.world_oriented_rect_with_surface_response(
                add3(
                    [x, y, z + height * 0.888],
                    add3(basis.front_face, scale3(basis.right, side * height * 0.040)),
                ),
                basis.right,
                basis.up,
                [height * 0.023, height * 0.0024],
                skin_cool,
                skin_response,
            );
            self.world_cylinder_between_with_surface_response(
                add3(
                    [x, y, z + height * 0.804],
                    scale3(basis.right, side * height * 0.024),
                ),
                add3(
                    [x, y, z + height * 0.742],
                    add3(
                        scale3(basis.right, side * height * 0.046),
                        scale3(basis.forward, height * 0.008),
                    ),
                ),
                height * 0.0042,
                6,
                skin_shadow,
                skin_response,
            );
        }

        self.world_oriented_ellipsoid_with_surface_response(
            add3(
                [x, y, z + height * 0.835],
                add3(basis.front_face, scale3(basis.forward, height * 0.012)),
            ),
            axes,
            [height * 0.034, height * 0.014, height * 0.018],
            5,
            10,
            skin_subsurface,
            skin_response,
        );

        let chest = add3(
            [x, y, z + height * 0.640 + breath],
            add3(
                basis.front_body,
                scale3(
                    basis.right,
                    (window_stable_unit(seed, 34_701) - 0.5) * height * 0.006,
                ),
            ),
        );
        self.world_oriented_ellipsoid_with_surface_response(
            chest,
            axes,
            [height * 0.114, height * 0.030, height * 0.082],
            7,
            16,
            cloth_pressure,
            cloth_response,
        );
        self.world_oriented_ellipsoid_with_surface_response(
            add3(
                [x, y, z + height * 0.542 - crouch * height * 0.016],
                basis.front_body,
            ),
            axes,
            [height * 0.086, height * 0.026, height * 0.056],
            6,
            14,
            window_scale_color(cloth_pressure, 0.82),
            cloth_response,
        );
        self.world_oriented_rect_with_surface_response(
            add3([x, y, z + height * 0.604 + breath * 0.6], basis.front_body),
            basis.right,
            basis.up,
            [height * 0.094, height * 0.0022],
            window_scale_color(cloth_pressure, 1.18),
            cloth_response,
        );

        for index in 0..form_count {
            let side = if index % 2 == 0 { -1.0 } else { 1.0 };
            let band = (index / 2) % 5;
            let salt = index as u64;
            let gait = side * locomotion * height * 0.018;
            let (vertical, lateral, forward_offset, width, length, color, response) = match band {
                0 => (
                    0.718,
                    side * 0.126,
                    0.074 - side * gait * 0.18,
                    0.024,
                    0.052,
                    cloth_pressure,
                    cloth_response,
                ),
                1 => (
                    0.586,
                    side * 0.102,
                    0.080,
                    0.020,
                    0.064,
                    cloth_crease,
                    cloth_response,
                ),
                2 => (
                    0.420,
                    side * 0.185,
                    0.092 - side * gait * 0.34,
                    0.016,
                    0.040,
                    skin_subsurface,
                    skin_response,
                ),
                3 => (
                    0.336,
                    side * 0.068,
                    0.084 + side * gait * 0.28 + crouch * 0.026,
                    0.026,
                    0.074,
                    cloth_pressure,
                    cloth_response,
                ),
                _ => (
                    0.170,
                    side * 0.070,
                    0.088 - side * gait * 0.22,
                    0.020,
                    0.054,
                    window_scale_color(cloth_pressure, 0.72),
                    cloth_response,
                ),
            };
            let center = add3(
                [x, y, z + height * vertical],
                add3(
                    scale3(
                        basis.right,
                        lateral * height
                            + (window_stable_unit(seed, 35_101 + salt) - 0.5) * height * 0.006,
                    ),
                    scale3(basis.forward, forward_offset * height),
                ),
            );
            self.world_oriented_ellipsoid_with_surface_response(
                center,
                axes,
                [
                    height * width,
                    height * (0.010 + window_stable_unit(seed, 35_701 + salt) * 0.008),
                    height * length,
                ],
                5,
                10,
                color,
                response,
            );
            self.world_oriented_rect_with_surface_response(
                add3(center, scale3(basis.forward, height * 0.012)),
                normalize3(add3(basis.up, scale3(basis.right, -side * 0.12))),
                basis.right,
                [height * (length * 0.68), height * 0.0018],
                if response == skin_response {
                    skin_shadow
                } else {
                    cloth_crease
                },
                response,
            );
        }
    }

    fn add_humanoid_cybernetic_detail(
        &mut self,
        instance: &WindowHumanoidProxyInstance<'_>,
        proxy: &HumanProxyGeometry,
        basis: HumanoidDetailBasis,
        seed: u64,
    ) {
        let [x, y] = instance.center_meters;
        let z = instance.z_meters;
        let height = proxy.height_meters;
        let surface = instance.surface_state.as_ref();
        let plate_color = humanoid_cybernetic_plate_color(seed);
        let seam_color = humanoid_implant_seam_color(instance.color, surface);
        let glow_color = humanoid_cybernetic_glow_color(seed);
        let skin_response = humanoid_skin_surface_response(surface);

        let temple = add3(
            [x, y, z + height * 0.884],
            add3(
                add3(basis.front_face, scale3(basis.right, height * 0.073)),
                scale3(basis.forward, height * 0.006),
            ),
        );
        self.world_oriented_rect_with_surface_response(
            temple,
            basis.right,
            basis.up,
            [height * 0.017, height * 0.024],
            plate_color,
            WINDOW_SURFACE_RESPONSE_HUMAN_CYBERNETIC,
        );
        self.world_oriented_rect_with_surface_response(
            add3(temple, scale3(basis.right, -height * 0.019)),
            basis.right,
            basis.up,
            [height * 0.0024, height * 0.028],
            seam_color,
            skin_response,
        );
        self.world_oriented_rect_with_surface_response(
            add3(temple, scale3(basis.up, height * 0.011)),
            basis.right,
            basis.up,
            [height * 0.010, height * 0.0019],
            glow_color,
            WINDOW_SURFACE_RESPONSE_HUMAN_CYBERNETIC,
        );

        let forearm = add3(
            [x, y, z + height * 0.425],
            add3(
                add3(basis.front_body, scale3(basis.right, height * 0.135)),
                scale3(basis.forward, -height * 0.004),
            ),
        );
        self.world_oriented_rect_with_surface_response(
            forearm,
            basis.right,
            basis.up,
            [height * 0.020, height * 0.082],
            plate_color,
            WINDOW_SURFACE_RESPONSE_HUMAN_CYBERNETIC,
        );
        self.world_oriented_rect_with_surface_response(
            add3(forearm, scale3(basis.right, -height * 0.023)),
            basis.right,
            basis.up,
            [height * 0.0024, height * 0.086],
            seam_color,
            skin_response,
        );
        self.world_oriented_rect_with_surface_response(
            add3(forearm, scale3(basis.up, height * 0.036)),
            basis.right,
            basis.up,
            [height * 0.014, height * 0.0022],
            glow_color,
            WINDOW_SURFACE_RESPONSE_HUMAN_CYBERNETIC,
        );
    }

    fn add_humanoid_face_features(
        &mut self,
        instance: WindowHumanoidProxyInstance<'_>,
        proxy: &HumanProxyGeometry,
    ) {
        if proxy.face_features.is_empty() {
            return;
        }
        if !humanoid_allows_mid_detail(&instance, proxy) {
            return;
        }

        let [x, y] = instance.center_meters;
        let z = instance.z_meters;
        let (forward, right, up) = humanoid_instance_orientation_basis(&instance);
        let face_offset = (proxy.height_meters * 0.085).clamp(0.105, 0.17);
        let near_detail = humanoid_allows_near_detail(&instance, proxy);

        for feature in &proxy.face_features {
            if !near_detail
                && !matches!(
                    feature.kind,
                    HumanProxyFaceFeatureKind::LeftEye
                        | HumanProxyFaceFeatureKind::RightEye
                        | HumanProxyFaceFeatureKind::Mouth
                        | HumanProxyFaceFeatureKind::NoseBridge
                )
            {
                continue;
            }
            let feature_center = add3(
                [x, y, z + feature.height_meters],
                add3(
                    scale3(forward, face_offset),
                    scale3(right, feature.lateral_offset_meters),
                ),
            );
            let feature_color = humanoid_face_feature_color(
                instance.color,
                *feature,
                instance.emotion,
                instance.surface_state.as_ref(),
            );
            let feature_surface_response = humanoid_face_feature_surface_response(
                feature.kind,
                instance.surface_state.as_ref(),
            );

            match feature.kind {
                HumanProxyFaceFeatureKind::LeftEye
                | HumanProxyFaceFeatureKind::RightEye
                | HumanProxyFaceFeatureKind::Mouth => {
                    self.world_oriented_rect_with_surface_response(
                        feature_center,
                        right,
                        up,
                        [feature.half_width_meters, feature.half_height_meters],
                        feature_color,
                        feature_surface_response,
                    );
                }
                HumanProxyFaceFeatureKind::NoseBridge => {
                    self.world_cylinder_between_with_surface_response(
                        add3(
                            feature_center,
                            scale3(up, feature.half_height_meters * 0.72),
                        ),
                        add3(
                            add3(feature_center, scale3(forward, proxy.height_meters * 0.018)),
                            scale3(up, -feature.half_height_meters * 0.82),
                        ),
                        feature.half_width_meters.max(proxy.height_meters * 0.004),
                        8,
                        feature_color,
                        feature_surface_response,
                    );
                }
                HumanProxyFaceFeatureKind::NoseTip => {
                    self.world_ellipsoid_with_surface_response(
                        add3(feature_center, scale3(forward, proxy.height_meters * 0.020)),
                        [
                            feature.half_width_meters,
                            proxy.height_meters * 0.009,
                            feature.half_height_meters,
                        ],
                        6,
                        12,
                        feature_color,
                        feature_surface_response,
                    );
                }
                HumanProxyFaceFeatureKind::LeftCheek | HumanProxyFaceFeatureKind::RightCheek => {
                    self.world_ellipsoid_with_surface_response(
                        add3(feature_center, scale3(forward, proxy.height_meters * 0.006)),
                        [
                            feature.half_width_meters,
                            proxy.height_meters * 0.011,
                            feature.half_height_meters,
                        ],
                        5,
                        10,
                        feature_color,
                        feature_surface_response,
                    );
                }
                HumanProxyFaceFeatureKind::LeftBrow | HumanProxyFaceFeatureKind::RightBrow => {
                    let side = if feature.kind == HumanProxyFaceFeatureKind::LeftBrow {
                        -1.0
                    } else {
                        1.0
                    };
                    self.world_oriented_rect_with_surface_response(
                        feature_center,
                        normalize3(add3(right, scale3(up, side * 0.10))),
                        up,
                        [feature.half_width_meters, feature.half_height_meters],
                        feature_color,
                        feature_surface_response,
                    );
                }
                HumanProxyFaceFeatureKind::LeftEar | HumanProxyFaceFeatureKind::RightEar => {
                    self.world_ellipsoid_with_surface_response(
                        add3(feature_center, scale3(forward, -face_offset * 0.42)),
                        [
                            feature.half_width_meters,
                            proxy.height_meters * 0.0065,
                            feature.half_height_meters,
                        ],
                        5,
                        10,
                        feature_color,
                        feature_surface_response,
                    );
                }
                HumanProxyFaceFeatureKind::Chin => {
                    self.world_ellipsoid_with_surface_response(
                        add3(feature_center, scale3(forward, proxy.height_meters * 0.010)),
                        [
                            feature.half_width_meters,
                            proxy.height_meters * 0.012,
                            feature.half_height_meters,
                        ],
                        5,
                        10,
                        feature_color,
                        feature_surface_response,
                    );
                }
                HumanProxyFaceFeatureKind::JawShadow => {
                    self.world_oriented_rect_with_surface_response(
                        feature_center,
                        right,
                        up,
                        [feature.half_width_meters, feature.half_height_meters],
                        feature_color,
                        feature_surface_response,
                    );
                }
            }
        }

        if proxy.quality_tier >= QualityTier::NormalRuntime {
            let skin = humanoid_default_skin_color(instance.color);
            self.world_ellipsoid_with_surface_response(
                add3(
                    [x, y, z + proxy.height_meters * 0.862],
                    scale3(forward, face_offset * 1.05),
                ),
                [
                    proxy.height_meters * 0.014,
                    proxy.height_meters * 0.019,
                    proxy.height_meters * 0.026,
                ],
                5,
                8,
                window_scale_color(skin, 0.92),
                humanoid_skin_surface_response(instance.surface_state.as_ref()),
            );
            for cheek_side in [-1.0_f32, 1.0] {
                self.world_oriented_rect_with_surface_response(
                    add3(
                        [x, y, z + proxy.height_meters * 0.842],
                        add3(
                            scale3(forward, face_offset * 1.02),
                            scale3(right, cheek_side * proxy.height_meters * 0.045),
                        ),
                    ),
                    right,
                    up,
                    [proxy.height_meters * 0.018, proxy.height_meters * 0.012],
                    window_clamp_color([skin[0] * 1.04, skin[1] * 0.92, skin[2] * 0.88, 0.42]),
                    humanoid_skin_surface_response(instance.surface_state.as_ref()),
                );
            }
        }
    }

    pub fn world_cylinder_between(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        radius: f32,
        segments: usize,
        color: [f32; 4],
    ) {
        self.world_cylinder_between_with_surface_response(
            start,
            end,
            radius,
            segments,
            color,
            WINDOW_SURFACE_RESPONSE_DEFAULT,
        );
    }

    pub fn world_cylinder_between_with_surface_response(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        radius: f32,
        segments: usize,
        color: [f32; 4],
        surface_response: [f32; 4],
    ) {
        let axis = sub3(end, start);
        if dot3(axis, axis) <= f32::EPSILON || radius <= f32::EPSILON {
            return;
        }

        let forward = normalize3(axis);
        let reference = if forward[2].abs() < 0.92 {
            [0.0, 0.0, 1.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let right = normalize3(cross3(reference, forward));
        let up = normalize3(cross3(forward, right));
        let segments = segments.max(3);
        let step = std::f32::consts::TAU / segments as f32;

        for index in 0..segments {
            let angle0 = index as f32 * step;
            let angle1 = (index + 1) as f32 * step;
            let radial0 = add3(
                scale3(right, angle0.cos() * radius),
                scale3(up, angle0.sin() * radius),
            );
            let radial1 = add3(
                scale3(right, angle1.cos() * radius),
                scale3(up, angle1.sin() * radius),
            );
            self.world_quad_with_surface_response(
                add3(start, radial0),
                add3(end, radial0),
                add3(end, radial1),
                add3(start, radial1),
                color,
                surface_response,
            );
        }
    }

    pub fn world_ellipsoid(
        &mut self,
        center: [f32; 3],
        radii: [f32; 3],
        latitude_segments: usize,
        longitude_segments: usize,
        color: [f32; 4],
    ) {
        if radii
            .iter()
            .any(|radius| !radius.is_finite() || *radius <= f32::EPSILON)
        {
            return;
        }

        let latitude_segments = latitude_segments.max(3);
        let longitude_segments = longitude_segments.max(4);
        let latitude_step = std::f32::consts::PI / latitude_segments as f32;
        let longitude_step = std::f32::consts::TAU / longitude_segments as f32;

        for latitude in 0..latitude_segments {
            let theta0 = -std::f32::consts::FRAC_PI_2 + latitude as f32 * latitude_step;
            let theta1 = theta0 + latitude_step;

            for longitude in 0..longitude_segments {
                let phi0 = longitude as f32 * longitude_step;
                let phi1 = phi0 + longitude_step;
                self.push_indexed_quad([
                    WindowSceneVertex::world(
                        ellipsoid_point(center, radii, theta0, phi0),
                        ellipsoid_normal(radii, theta0, phi0),
                        color,
                    ),
                    WindowSceneVertex::world(
                        ellipsoid_point(center, radii, theta1, phi0),
                        ellipsoid_normal(radii, theta1, phi0),
                        color,
                    ),
                    WindowSceneVertex::world(
                        ellipsoid_point(center, radii, theta1, phi1),
                        ellipsoid_normal(radii, theta1, phi1),
                        color,
                    ),
                    WindowSceneVertex::world(
                        ellipsoid_point(center, radii, theta0, phi1),
                        ellipsoid_normal(radii, theta0, phi1),
                        color,
                    ),
                ]);
            }
        }
    }

    pub fn world_ellipsoid_with_surface_response(
        &mut self,
        center: [f32; 3],
        radii: [f32; 3],
        latitude_segments: usize,
        longitude_segments: usize,
        color: [f32; 4],
        surface_response: [f32; 4],
    ) {
        if radii
            .iter()
            .any(|radius| !radius.is_finite() || *radius <= f32::EPSILON)
        {
            return;
        }

        let latitude_segments = latitude_segments.max(3);
        let longitude_segments = longitude_segments.max(4);
        let latitude_step = std::f32::consts::PI / latitude_segments as f32;
        let longitude_step = std::f32::consts::TAU / longitude_segments as f32;

        for latitude in 0..latitude_segments {
            let theta0 = -std::f32::consts::FRAC_PI_2 + latitude as f32 * latitude_step;
            let theta1 = theta0 + latitude_step;

            for longitude in 0..longitude_segments {
                let phi0 = longitude as f32 * longitude_step;
                let phi1 = phi0 + longitude_step;
                let p0 = ellipsoid_point(center, radii, theta0, phi0);
                let p1 = ellipsoid_point(center, radii, theta1, phi0);
                let p2 = ellipsoid_point(center, radii, theta1, phi1);
                let p3 = ellipsoid_point(center, radii, theta0, phi1);
                let material_detail =
                    window_surface_detail_for_quad(p0, p1, p2, p3, color, surface_response);
                let generated_cache = window_generated_surface_cache_for_quad(
                    p0,
                    p1,
                    p2,
                    p3,
                    color,
                    surface_response,
                    material_detail,
                );

                self.push_indexed_quad([
                    WindowSceneVertex::world_with_surface_detail_and_cache(
                        p0,
                        ellipsoid_normal(radii, theta0, phi0),
                        color,
                        surface_response,
                        material_detail,
                        generated_cache,
                    ),
                    WindowSceneVertex::world_with_surface_detail_and_cache(
                        p1,
                        ellipsoid_normal(radii, theta1, phi0),
                        color,
                        surface_response,
                        material_detail,
                        generated_cache,
                    ),
                    WindowSceneVertex::world_with_surface_detail_and_cache(
                        p2,
                        ellipsoid_normal(radii, theta1, phi1),
                        color,
                        surface_response,
                        material_detail,
                        generated_cache,
                    ),
                    WindowSceneVertex::world_with_surface_detail_and_cache(
                        p3,
                        ellipsoid_normal(radii, theta0, phi1),
                        color,
                        surface_response,
                        material_detail,
                        generated_cache,
                    ),
                ]);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn world_oriented_ellipsoid_with_surface_response(
        &mut self,
        center: [f32; 3],
        axes: [[f32; 3]; 3],
        radii: [f32; 3],
        latitude_segments: usize,
        longitude_segments: usize,
        color: [f32; 4],
        surface_response: [f32; 4],
    ) {
        if radii
            .iter()
            .any(|radius| !radius.is_finite() || *radius <= f32::EPSILON)
        {
            return;
        }

        let axes = [
            normalize3(axes[0]),
            normalize3(axes[1]),
            normalize3(axes[2]),
        ];
        if axes.iter().any(|axis| dot3(*axis, *axis) <= f32::EPSILON) {
            return;
        }

        let latitude_segments = latitude_segments.max(3);
        let longitude_segments = longitude_segments.max(4);
        let latitude_step = std::f32::consts::PI / latitude_segments as f32;
        let longitude_step = std::f32::consts::TAU / longitude_segments as f32;

        for latitude in 0..latitude_segments {
            let theta0 = -std::f32::consts::FRAC_PI_2 + latitude as f32 * latitude_step;
            let theta1 = theta0 + latitude_step;

            for longitude in 0..longitude_segments {
                let phi0 = longitude as f32 * longitude_step;
                let phi1 = phi0 + longitude_step;
                let p0 = oriented_ellipsoid_point(center, axes, radii, theta0, phi0);
                let p1 = oriented_ellipsoid_point(center, axes, radii, theta1, phi0);
                let p2 = oriented_ellipsoid_point(center, axes, radii, theta1, phi1);
                let p3 = oriented_ellipsoid_point(center, axes, radii, theta0, phi1);
                let material_detail =
                    window_surface_detail_for_quad(p0, p1, p2, p3, color, surface_response);
                let generated_cache = window_generated_surface_cache_for_quad(
                    p0,
                    p1,
                    p2,
                    p3,
                    color,
                    surface_response,
                    material_detail,
                );

                self.push_indexed_quad([
                    WindowSceneVertex::world_with_surface_detail_and_cache(
                        p0,
                        oriented_ellipsoid_normal(axes, radii, theta0, phi0),
                        color,
                        surface_response,
                        material_detail,
                        generated_cache,
                    ),
                    WindowSceneVertex::world_with_surface_detail_and_cache(
                        p1,
                        oriented_ellipsoid_normal(axes, radii, theta1, phi0),
                        color,
                        surface_response,
                        material_detail,
                        generated_cache,
                    ),
                    WindowSceneVertex::world_with_surface_detail_and_cache(
                        p2,
                        oriented_ellipsoid_normal(axes, radii, theta1, phi1),
                        color,
                        surface_response,
                        material_detail,
                        generated_cache,
                    ),
                    WindowSceneVertex::world_with_surface_detail_and_cache(
                        p3,
                        oriented_ellipsoid_normal(axes, radii, theta0, phi1),
                        color,
                        surface_response,
                        material_detail,
                        generated_cache,
                    ),
                ]);
            }
        }
    }

    pub fn world_flat_ellipse(
        &mut self,
        center: [f32; 3],
        radii: [f32; 2],
        segments: usize,
        color: [f32; 4],
    ) {
        self.world_flat_ellipse_with_surface_response(
            center,
            radii,
            segments,
            color,
            WINDOW_SURFACE_RESPONSE_DEFAULT,
        );
    }

    pub fn world_flat_ellipse_with_surface_response(
        &mut self,
        center: [f32; 3],
        radii: [f32; 2],
        segments: usize,
        color: [f32; 4],
        surface_response: [f32; 4],
    ) {
        let segments = segments.max(3);
        let step = std::f32::consts::TAU / segments as f32;
        for index in 0..segments {
            let angle0 = index as f32 * step;
            let angle1 = (index + 1) as f32 * step;
            self.world_triangle_with_surface_response(
                center,
                [
                    center[0] + angle0.cos() * radii[0],
                    center[1] + angle0.sin() * radii[1],
                    center[2],
                ],
                [
                    center[0] + angle1.cos() * radii[0],
                    center[1] + angle1.sin() * radii[1],
                    center[2],
                ],
                color,
                surface_response,
            );
        }
    }

    pub fn world_ellipse_ring(
        &mut self,
        center: [f32; 3],
        inner_radii: [f32; 2],
        outer_radii: [f32; 2],
        segments: usize,
        color: [f32; 4],
    ) {
        self.world_ellipse_ring_with_surface_response(
            center,
            inner_radii,
            outer_radii,
            segments,
            color,
            WINDOW_SURFACE_RESPONSE_DEFAULT,
        );
    }

    pub fn world_ellipse_ring_with_surface_response(
        &mut self,
        center: [f32; 3],
        inner_radii: [f32; 2],
        outer_radii: [f32; 2],
        segments: usize,
        color: [f32; 4],
        surface_response: [f32; 4],
    ) {
        let segments = segments.max(3);
        let step = std::f32::consts::TAU / segments as f32;
        for index in 0..segments {
            let angle0 = index as f32 * step;
            let angle1 = (index + 1) as f32 * step;
            let outer0 = [
                center[0] + angle0.cos() * outer_radii[0],
                center[1] + angle0.sin() * outer_radii[1],
                center[2],
            ];
            let outer1 = [
                center[0] + angle1.cos() * outer_radii[0],
                center[1] + angle1.sin() * outer_radii[1],
                center[2],
            ];
            let inner1 = [
                center[0] + angle1.cos() * inner_radii[0],
                center[1] + angle1.sin() * inner_radii[1],
                center[2],
            ];
            let inner0 = [
                center[0] + angle0.cos() * inner_radii[0],
                center[1] + angle0.sin() * inner_radii[1],
                center[2],
            ];
            self.world_quad_with_surface_response(
                outer0,
                outer1,
                inner1,
                inner0,
                color,
                surface_response,
            );
        }
    }

    pub fn world_triangle(&mut self, a: [f32; 3], b: [f32; 3], c: [f32; 3], color: [f32; 4]) {
        self.world_triangle_with_surface_response(a, b, c, color, WINDOW_SURFACE_RESPONSE_DEFAULT);
    }

    pub fn world_triangle_with_surface_response(
        &mut self,
        a: [f32; 3],
        b: [f32; 3],
        c: [f32; 3],
        color: [f32; 4],
        surface_response: [f32; 4],
    ) {
        let normal = quad_normal(a, b, c);
        let material_detail = window_surface_detail_for_quad(a, b, c, c, color, surface_response);
        let generated_cache = window_generated_surface_cache_for_quad(
            a,
            b,
            c,
            c,
            color,
            surface_response,
            material_detail,
        );
        self.push_indexed_quad([
            WindowSceneVertex::world_with_surface_detail_and_cache(
                a,
                normal,
                color,
                surface_response,
                material_detail,
                generated_cache,
            ),
            WindowSceneVertex::world_with_surface_detail_and_cache(
                b,
                normal,
                color,
                surface_response,
                material_detail,
                generated_cache,
            ),
            WindowSceneVertex::world_with_surface_detail_and_cache(
                c,
                normal,
                color,
                surface_response,
                material_detail,
                generated_cache,
            ),
            WindowSceneVertex::world_with_surface_detail_and_cache(
                c,
                normal,
                color,
                surface_response,
                material_detail,
                generated_cache,
            ),
        ]);
    }

    pub fn world_oriented_rect(
        &mut self,
        center: [f32; 3],
        right: [f32; 3],
        up: [f32; 3],
        half_width: f32,
        half_height: f32,
        color: [f32; 4],
    ) {
        self.world_oriented_rect_with_surface_response(
            center,
            right,
            up,
            [half_width, half_height],
            color,
            WINDOW_SURFACE_RESPONSE_DEFAULT,
        );
    }

    pub fn world_oriented_rect_with_surface_response(
        &mut self,
        center: [f32; 3],
        right: [f32; 3],
        up: [f32; 3],
        half_extents: [f32; 2],
        color: [f32; 4],
        surface_response: [f32; 4],
    ) {
        let right = scale3(normalize3(right), half_extents[0]);
        let up = scale3(normalize3(up), half_extents[1]);
        self.world_quad_with_surface_response(
            sub3(sub3(center, right), up),
            add3(sub3(center, up), right),
            add3(add3(center, right), up),
            add3(sub3(center, right), up),
            color,
            surface_response,
        );
    }

    fn world_oriented_rect_with_surface_state_detail(&mut self, rect: WindowOrientedSurfaceRect) {
        let right = scale3(normalize3(rect.right), rect.half_extents[0]);
        let up = scale3(normalize3(rect.up), rect.half_extents[1]);
        self.world_quad_with_surface_state_detail(
            [
                sub3(sub3(rect.center, right), up),
                add3(sub3(rect.center, up), right),
                add3(add3(rect.center, right), up),
                add3(sub3(rect.center, right), up),
            ],
            rect.color,
            rect.surface_response,
            rect.state_detail,
        );
    }

    pub fn world_billboard(
        &mut self,
        center: [f32; 3],
        right: [f32; 3],
        width: f32,
        height: f32,
        color: [f32; 4],
    ) {
        self.world_billboard_with_surface_response(
            center,
            right,
            width,
            height,
            color,
            WINDOW_SURFACE_RESPONSE_DEFAULT,
        );
    }

    pub fn world_billboard_with_surface_response(
        &mut self,
        center: [f32; 3],
        right: [f32; 3],
        width: f32,
        height: f32,
        color: [f32; 4],
        surface_response: [f32; 4],
    ) {
        let right = if dot3(right, right) <= f32::EPSILON {
            [1.0, 0.0, 0.0]
        } else {
            normalize3(right)
        };
        let half_width = width * 0.5;
        let half_height = height * 0.5;
        self.world_quad_with_surface_response(
            [
                center[0] - right[0] * half_width,
                center[1] - right[1] * half_width,
                center[2] - right[2] * half_width - half_height,
            ],
            [
                center[0] + right[0] * half_width,
                center[1] + right[1] * half_width,
                center[2] + right[2] * half_width - half_height,
            ],
            [
                center[0] + right[0] * half_width,
                center[1] + right[1] * half_width,
                center[2] + right[2] * half_width + half_height,
            ],
            [
                center[0] - right[0] * half_width,
                center[1] - right[1] * half_width,
                center[2] - right[2] * half_width + half_height,
            ],
            color,
            surface_response,
        );
    }
}

fn add_window_snapshot_entity(
    geometry: &mut WindowSceneGeometry,
    options: WindowSnapshotSceneOptions<'_>,
    entity: EntityId,
    transform: &Transform,
) {
    let snapshot = options.snapshot;
    let tags = snapshot.tags.find(entity).map(Vec::as_slice);
    if window_tags_have(tags, "audio_zone:alley") {
        return;
    }

    let is_player = options.player_entity == Some(entity) || window_tags_have(tags, "player");
    if is_player {
        return;
    }

    let renderable = snapshot.renderables.find(entity);
    if renderable.is_some_and(|renderable| !renderable.visible) {
        return;
    }

    let material = renderable.and_then(|renderable| snapshot.materials.find(renderable.material));
    let material_state = snapshot.material_states.find(entity);
    let color = window_entity_color(tags, material, material_state, options.frame_index);
    let surface_response = window_entity_surface_response(tags, material, material_state);
    let center = window_transform_xy(transform);
    let z = transform.translation_meters.z;

    if window_tags_have(tags, "ground") {
        if let Some(renderable) = renderable {
            add_window_known_mesh_instance(
                geometry,
                renderable,
                center,
                z,
                color,
                material_state,
                &options.camera,
            );
        }
        return;
    }

    if window_tags_have(tags, "npc") || snapshot.humans.find(entity).is_some() {
        let surface_state = human_surface_response_from_snapshot(snapshot, entity);
        geometry.add_humanoid_proxy(
            WindowHumanoidProxyInstance::new(center, z, color)
                .with_human(snapshot.humans.find(entity))
                .with_emotion(
                    snapshot
                        .agents
                        .find(entity)
                        .map(|agent| &agent.emotional_state),
                )
                .with_surface_state(surface_state)
                .with_viewer_position(options.camera.position),
        );
    } else if let Some(renderable) = renderable {
        if add_window_known_mesh_instance(
            geometry,
            renderable,
            center,
            z,
            color,
            material_state,
            &options.camera,
        ) {
            return;
        }
        geometry.add_window_entity_proxy(
            WindowEntityProxyVisual::new(
                WindowEntityProxyKind::GenericRenderable,
                center,
                z,
                color,
            )
            .with_surface_response(surface_response)
            .with_material_state(material_state),
        );
    } else if window_tags_have(tags, "glass") || window_tags_have(tags, "wall") {
        geometry.add_window_entity_proxy(
            WindowEntityProxyVisual::new(WindowEntityProxyKind::GlassWall, center, z, color)
                .with_surface_response(surface_response)
                .with_material_state(material_state),
        );
    } else if window_tags_have(tags, "light") || window_tags_have(tags, "neon") {
        geometry.add_window_entity_proxy(
            WindowEntityProxyVisual::new(WindowEntityProxyKind::LightPanel, center, z, color)
                .with_surface_response(surface_response)
                .with_material_state(material_state),
        );
    } else if window_tags_have(tags, "water_leak") || window_tags_have(tags, "hazard") {
        geometry.add_window_entity_proxy(
            WindowEntityProxyVisual::new(WindowEntityProxyKind::LowHazardSurface, center, z, color)
                .with_surface_response(surface_response)
                .with_material_state(material_state),
        );
    }
}

fn add_window_known_mesh_instance(
    geometry: &mut WindowSceneGeometry,
    renderable: &Renderable,
    center: [f32; 2],
    z: f32,
    color: [f32; 4],
    material_state: Option<&MaterialState>,
    camera: &WindowPerspectiveCamera,
) -> bool {
    geometry.add_known_procedural_mesh(
        WindowProceduralMeshInstance::new(renderable.mesh, center, z, color)
            .with_material_state(material_state)
            .with_billboard_right(camera.basis().right),
    )
}

fn add_window_human_bundle_mesh(
    geometry: &mut WindowSceneGeometry,
    instance: WindowProceduralMeshInstance,
) {
    let [x, y] = instance.center_meters;
    let right = horizontal_direction(instance.billboard_right);
    let forward = normalize3([-right[1], right[0], 0.0]);
    let viewer_position = [
        x + forward[0] * 2.0,
        y + forward[1] * 2.0,
        instance.z_meters + 1.55,
    ];
    let mara = HumanState {
        human_id: 300,
        quality_tier: QualityTier::HeroHighFidelityRuntime,
    };
    let material_state = MaterialState {
        crack_density: instance.crack_density,
        moisture: instance.moisture,
        ..MaterialState::default()
    };
    let surface_state = (instance.crack_density > 0.0 || instance.moisture > 0.0).then(|| {
        evaluate_human_surface_response(
            &surface_response_profile_for_human_state(&mara),
            material_state,
            None,
        )
    });
    geometry.add_humanoid_proxy(
        WindowHumanoidProxyInstance::new(instance.center_meters, instance.z_meters, instance.color)
            .with_human(Some(&mara))
            .with_surface_state(surface_state)
            .with_viewer_position(viewer_position),
    );
}

fn window_transform_xy(transform: &Transform) -> [f32; 2] {
    [
        transform.translation_meters.x,
        transform.translation_meters.y,
    ]
}

fn window_surface_detail_for_quad(
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    d: [f32; 3],
    color: [f32; 4],
    surface_response: [f32; 4],
) -> [f32; 4] {
    let center = [
        (a[0] + b[0] + c[0] + d[0]) * 0.25,
        (a[1] + b[1] + c[1] + d[1]) * 0.25,
        (a[2] + b[2] + c[2] + d[2]) * 0.25,
    ];
    let span_u = dot3(sub3(b, a), sub3(b, a)).sqrt();
    let span_v = dot3(sub3(d, a), sub3(d, a)).sqrt();
    let footprint = (span_u * span_v).sqrt().clamp(0.1, 12.0);
    let roughness = surface_response[0].clamp(0.0, 1.0);
    let metallic = surface_response[1].clamp(0.0, 1.0);
    let wetness = surface_response[2].clamp(0.0, 1.0);
    let emissive = surface_response[3].clamp(0.0, 1.0);
    let dark_surface = (1.0 - (color[0] + color[1] + color[2]) / 3.0).clamp(0.0, 1.0);
    let green_bias = (color[1] - color[0].max(color[2])).clamp(0.0, 1.0);
    let chroma = color[0].max(color[1]).max(color[2]) - color[0].min(color[1]).min(color[2]);
    let plant_like = (green_bias * 3.0).clamp(0.0, 1.0) * (1.0 - metallic) * (1.0 - emissive);
    let natural_rough =
        (roughness - 0.55).max(0.0) * (1.0 - metallic) * (1.0 - emissive) * (1.0 - wetness * 0.32);
    let stone_like =
        (1.0 - (chroma * 5.0).clamp(0.0, 1.0)) * natural_rough * (wetness + 0.18).clamp(0.0, 0.46);
    let texture_class_boost =
        (plant_like * 0.70 + natural_rough * 0.54 + stone_like * 0.42 + wetness * 0.24)
            .clamp(0.0, 1.25);
    let seed = window_stable_unit(
        window_position_seed(center)
            ^ (color[0].to_bits() as u64).rotate_left(11)
            ^ (surface_response[0].to_bits() as u64).rotate_left(29),
        8_811,
    );
    let scale = (0.46
        + roughness * 1.42
        + metallic * 0.62
        + wetness * 0.36
        + dark_surface * 0.28
        + texture_class_boost
        - emissive * 0.58)
        .clamp(0.08, 5.2)
        / footprint.sqrt().clamp(0.55, 2.2);
    let state_intensity = (wetness * 0.42
        + dark_surface * 0.18
        + roughness * 0.16
        + color[3] * 0.08
        + texture_class_boost * 0.18)
        .clamp(0.0, 1.0);
    let relief = ((roughness * 0.48 + wetness * 0.2 + metallic * 0.12 + dark_surface * 0.1)
        * (1.0 - emissive * 0.65)
        + texture_class_boost * 0.20)
        .clamp(0.0, 1.0);

    window_clamp_surface_detail([scale, seed, state_intensity, relief])
}

fn window_generated_surface_cache_for_quad(
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    d: [f32; 3],
    color: [f32; 4],
    surface_response: [f32; 4],
    material_detail: [f32; 4],
) -> [f32; 4] {
    let center = [
        (a[0] + b[0] + c[0] + d[0]) * 0.25,
        (a[1] + b[1] + c[1] + d[1]) * 0.25,
        (a[2] + b[2] + c[2] + d[2]) * 0.25,
    ];
    let span_u = dot3(sub3(b, a), sub3(b, a)).sqrt();
    let span_v = dot3(sub3(d, a), sub3(d, a)).sqrt();
    let area_hint = dot3(
        cross3(sub3(b, a), sub3(d, a)),
        cross3(sub3(b, a), sub3(d, a)),
    )
    .sqrt()
    .clamp(0.02, 24.0);
    let footprint = (span_u * span_v)
        .sqrt()
        .max(area_hint.sqrt())
        .clamp(0.1, 12.0);
    let roughness = surface_response[0].clamp(0.0, 1.0);
    let metallic = surface_response[1].clamp(0.0, 1.0);
    let wetness = surface_response[2].clamp(0.0, 1.0);
    let emissive = surface_response[3].clamp(0.0, 1.0);
    let detail_scale = material_detail[0].clamp(0.05, 8.0);
    let state_intensity = material_detail[2].clamp(0.0, 1.0);
    let relief = material_detail[3].clamp(0.0, 1.0);
    let dark_surface = (1.0 - (color[0] + color[1] + color[2]) / 3.0).clamp(0.0, 1.0);
    let green_bias = (color[1] - color[0].max(color[2])).clamp(0.0, 1.0);
    let chroma = color[0].max(color[1]).max(color[2]) - color[0].min(color[1]).min(color[2]);
    let natural_material =
        ((roughness - 0.48).max(0.0) * (1.0 - metallic) * (1.0 - emissive)).clamp(0.0, 1.0);
    let plant_like = (green_bias * 3.2).clamp(0.0, 1.0) * natural_material;
    let stone_like =
        (1.0 - (chroma * 5.4).clamp(0.0, 1.0)) * natural_material * (wetness + 0.22).min(0.58);
    let material_texture_boost =
        (natural_material * 0.34 + plant_like * 0.28 + stone_like * 0.24 + wetness * 0.16)
            .clamp(0.0, 0.82);
    let seed = window_position_seed(center)
        ^ window_position_seed(a).rotate_left(5)
        ^ window_position_seed(c).rotate_left(17)
        ^ (color[0].to_bits() as u64).rotate_left(23)
        ^ (color[1].to_bits() as u64).rotate_left(31)
        ^ (surface_response[2].to_bits() as u64).rotate_left(43)
        ^ (material_detail[1].to_bits() as u64).rotate_left(53);
    let albedo_roughness = (0.1
        + roughness * 0.3
        + wetness * 0.24
        + dark_surface * 0.18
        + state_intensity * 0.18
        + detail_scale * 0.035
        + material_texture_boost * 0.18
        - emissive * 0.08)
        .clamp(0.0, 1.0);
    let normal_height = (0.06
        + relief * 0.66
        + roughness * 0.18
        + metallic * 0.13
        + state_intensity * 0.16
        + material_texture_boost * 0.26
        + footprint.recip() * 0.08)
        .clamp(0.0, 1.0);
    let state_mask = (state_intensity * 0.46
        + wetness * 0.3
        + metallic * 0.14
        + emissive * 0.2
        + material_texture_boost * 0.18)
        .clamp(0.0, 1.0);
    let page_id = window_stable_unit(seed, 12_019);

    window_clamp_surface_cache([albedo_roughness, normal_height, state_mask, page_id])
}

fn window_apply_material_state_detail(
    base: [f32; 4],
    state_detail: WindowMaterialStateDetail,
) -> [f32; 4] {
    if state_detail.is_neutral() {
        return base;
    }

    let seed = (base[1] + window_stable_unit(state_detail.seed_salt, 8_913) * 0.37).fract();
    window_clamp_surface_detail([
        base[0] * state_detail.scale_boost,
        seed,
        base[2].max(state_detail.state_intensity),
        base[3].max(state_detail.relief),
    ])
}

fn window_material_state_detail_from_decal(
    decal: WindowSurfaceDecalVisual,
) -> WindowMaterialStateDetail {
    WindowMaterialStateDetail {
        scale_boost: decal.scale_boost.clamp(1.0, 4.4),
        state_intensity: decal.state_intensity.clamp(0.0, 1.0),
        relief: decal.relief.clamp(0.0, 1.0),
        crack_density: if matches!(decal.kind, WindowSurfaceDecalKind::CrackField) {
            decal.state_intensity.clamp(0.0, 1.0)
        } else {
            0.0
        },
        moisture: if matches!(decal.kind, WindowSurfaceDecalKind::WaterPuddle) {
            decal.state_intensity.clamp(0.0, 1.0)
        } else {
            decal.surface_response[2].clamp(0.0, 1.0)
        },
        soot: if matches!(
            decal.kind,
            WindowSurfaceDecalKind::Scorch | WindowSurfaceDecalKind::GrimeStain
        ) {
            decal.state_intensity.clamp(0.0, 1.0)
        } else {
            0.0
        },
        corrosion: if matches!(decal.kind, WindowSurfaceDecalKind::Corrosion) {
            decal.state_intensity.clamp(0.0, 1.0)
        } else {
            0.0
        },
        heat: if matches!(decal.kind, WindowSurfaceDecalKind::Scorch) {
            decal.state_intensity.clamp(0.0, 1.0)
        } else {
            decal.surface_response[3].clamp(0.0, 1.0)
        },
        electrical_charge: 0.0,
        plastic_strain: 0.0,
        biological_contamination: 0.0,
        oil_contamination: 0.0,
        seed_salt: decal.detail_seed ^ window_position_seed(decal.center_meters),
    }
}

fn window_surface_decal_basis(
    decal: WindowSurfaceDecalVisual,
) -> Option<([f32; 3], [f32; 3], [f32; 3])> {
    let mut right = decal.right_axis;
    if dot3(right, right) <= f32::EPSILON {
        right = [1.0, 0.0, 0.0];
    }
    right = normalize3(right);

    let mut up = decal.up_axis;
    if dot3(up, up) <= f32::EPSILON {
        up = [0.0, 1.0, 0.0];
    }
    up = normalize3(up);

    if dot3(right, up).abs() > 0.96 {
        up = if right[2].abs() < 0.72 {
            [0.0, 0.0, 1.0]
        } else {
            [0.0, 1.0, 0.0]
        };
    }

    let normal = normalize3(cross3(right, up));
    if dot3(normal, normal) <= f32::EPSILON {
        return None;
    }

    up = normalize3(cross3(normal, right));
    Some((right, up, normal))
}

fn window_decal_plane_point(
    center: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    normal: [f32; 3],
    right_offset: f32,
    up_offset: f32,
    normal_offset: f32,
) -> [f32; 3] {
    add3(
        add3(
            add3(center, scale3(right, right_offset)),
            scale3(up, up_offset),
        ),
        scale3(normal, normal_offset),
    )
}

fn window_material_state_detail_from_proxy(
    proxy: WindowEntityProxyVisual,
) -> WindowMaterialStateDetail {
    window_material_state_detail_from_channels(WindowMaterialStateChannels {
        crack_density: proxy.crack_density,
        moisture: proxy.moisture,
        soot: proxy.soot,
        corrosion: proxy.corrosion,
        heat: proxy.heat,
        electrical_charge: proxy.electrical_charge,
        plastic_strain: proxy.plastic_strain,
        biological_contamination: proxy.biological_contamination,
        oil_contamination: proxy.oil_contamination,
    })
}

fn window_material_state_detail_from_instance(
    instance: WindowProceduralMeshInstance,
) -> WindowMaterialStateDetail {
    window_material_state_detail_from_channels(WindowMaterialStateChannels {
        crack_density: instance.crack_density,
        moisture: instance.moisture,
        soot: instance.soot,
        corrosion: instance.corrosion,
        heat: instance.heat,
        electrical_charge: instance.electrical_charge,
        plastic_strain: instance.plastic_strain,
        biological_contamination: instance.biological_contamination,
        oil_contamination: instance.oil_contamination,
    })
}

fn window_material_state_detail_from_channels(
    channels: WindowMaterialStateChannels,
) -> WindowMaterialStateDetail {
    let crack_density = channels.crack_density.clamp(0.0, 1.0);
    let moisture = channels.moisture.clamp(0.0, 1.0);
    let soot = channels.soot.clamp(0.0, 1.0);
    let corrosion = channels.corrosion.clamp(0.0, 1.0);
    let heat = channels.heat.clamp(0.0, 1.0);
    let electrical_charge = channels.electrical_charge.clamp(0.0, 1.0);
    let plastic_strain = channels.plastic_strain.clamp(0.0, 1.0);
    let biological_contamination = channels.biological_contamination.clamp(0.0, 1.0);
    let oil_contamination = channels.oil_contamination.clamp(0.0, 1.0);
    let state_intensity = (crack_density * 0.52
        + moisture * 0.34
        + soot * 0.58
        + corrosion * 0.66
        + heat * 0.42
        + electrical_charge * 0.28
        + plastic_strain * 0.38
        + biological_contamination * 0.44
        + oil_contamination * 0.4)
        .clamp(0.0, 1.0);
    let relief = (crack_density * 0.64
        + corrosion * 0.56
        + soot * 0.28
        + moisture * 0.18
        + heat * 0.24
        + electrical_charge * 0.12
        + plastic_strain * 0.46
        + biological_contamination * 0.2
        + oil_contamination * 0.16)
        .clamp(0.0, 1.0);
    let scale_boost = (1.0
        + crack_density * 1.08
        + corrosion * 0.92
        + soot * 0.62
        + moisture * 0.42
        + heat * 0.36
        + electrical_charge * 0.24
        + plastic_strain * 0.74
        + biological_contamination * 0.36
        + oil_contamination * 0.34)
        .clamp(1.0, 3.2);
    let seed_salt = ((crack_density * 255.0).round() as u64)
        ^ ((moisture * 255.0).round() as u64).rotate_left(9)
        ^ ((soot * 255.0).round() as u64).rotate_left(17)
        ^ ((corrosion * 255.0).round() as u64).rotate_left(25)
        ^ ((heat * 255.0).round() as u64).rotate_left(33)
        ^ ((electrical_charge * 255.0).round() as u64).rotate_left(41)
        ^ ((plastic_strain * 255.0).round() as u64).rotate_left(49)
        ^ ((biological_contamination * 255.0).round() as u64).rotate_left(57)
        ^ ((oil_contamination * 255.0).round() as u64).rotate_left(5);

    WindowMaterialStateDetail {
        scale_boost,
        state_intensity,
        relief,
        crack_density,
        moisture,
        soot,
        corrosion,
        heat,
        electrical_charge,
        plastic_strain,
        biological_contamination,
        oil_contamination,
        seed_salt,
    }
}

fn window_position_seed(position: [f32; 3]) -> u64 {
    let x = (position[0] * 100.0).round() as i64 as u64;
    let y = (position[1] * 100.0).round() as i64 as u64;
    let z = (position[2] * 100.0).round() as i64 as u64;
    x.rotate_left(7) ^ y.rotate_left(23) ^ z.rotate_left(41) ^ 0xA5F1_1C3D_7E09_44B1
}

fn window_stable_unit(seed: u64, salt: u64) -> f32 {
    let mut value = seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    ((value >> 40) as f32 / 0x00FF_FFFF_u32 as f32).clamp(0.0, 1.0)
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale3(vector: [f32; 3], scale: f32) -> [f32; 3] {
    [vector[0] * scale, vector[1] * scale, vector[2] * scale]
}

fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize3(vector: [f32; 3]) -> [f32; 3] {
    let length = dot3(vector, vector).sqrt();
    if length <= f32::EPSILON {
        return [0.0, 0.0, 1.0];
    }

    [vector[0] / length, vector[1] / length, vector[2] / length]
}

fn quad_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    normalize3(cross3(sub3(b, a), sub3(c, a)))
}

fn ellipsoid_point(center: [f32; 3], radii: [f32; 3], theta: f32, phi: f32) -> [f32; 3] {
    let theta_cos = theta.cos();
    [
        center[0] + radii[0] * theta_cos * phi.cos(),
        center[1] + radii[1] * theta_cos * phi.sin(),
        center[2] + radii[2] * theta.sin(),
    ]
}

fn ellipsoid_normal(radii: [f32; 3], theta: f32, phi: f32) -> [f32; 3] {
    let theta_cos = theta.cos();
    normalize3([
        theta_cos * phi.cos() / radii[0],
        theta_cos * phi.sin() / radii[1],
        theta.sin() / radii[2],
    ])
}

fn oriented_ellipsoid_point(
    center: [f32; 3],
    axes: [[f32; 3]; 3],
    radii: [f32; 3],
    theta: f32,
    phi: f32,
) -> [f32; 3] {
    let theta_cos = theta.cos();
    add3(
        center,
        add3(
            add3(
                scale3(axes[0], radii[0] * theta_cos * phi.cos()),
                scale3(axes[1], radii[1] * theta_cos * phi.sin()),
            ),
            scale3(axes[2], radii[2] * theta.sin()),
        ),
    )
}

fn oriented_ellipsoid_normal(
    axes: [[f32; 3]; 3],
    radii: [f32; 3],
    theta: f32,
    phi: f32,
) -> [f32; 3] {
    let theta_cos = theta.cos();
    normalize3(add3(
        add3(
            scale3(axes[0], theta_cos * phi.cos() / radii[0]),
            scale3(axes[1], theta_cos * phi.sin() / radii[1]),
        ),
        scale3(axes[2], theta.sin() / radii[2]),
    ))
}

fn humanoid_body_part_surface_response(
    part: HumanProxyPart,
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    match part {
        HumanProxyPart::Head
        | HumanProxyPart::Neck
        | HumanProxyPart::LeftArm
        | HumanProxyPart::RightArm
        | HumanProxyPart::LeftForearm
        | HumanProxyPart::RightForearm
        | HumanProxyPart::LeftHand
        | HumanProxyPart::RightHand => humanoid_skin_surface_response(surface_state),
        HumanProxyPart::HairCap => humanoid_hair_surface_response(surface_state),
        HumanProxyPart::Torso
        | HumanProxyPart::Pelvis
        | HumanProxyPart::Abdomen
        | HumanProxyPart::Chest
        | HumanProxyPart::LeftLeg
        | HumanProxyPart::RightLeg
        | HumanProxyPart::LeftUpperArm
        | HumanProxyPart::RightUpperArm
        | HumanProxyPart::LeftThigh
        | HumanProxyPart::RightThigh
        | HumanProxyPart::LeftCalf
        | HumanProxyPart::RightCalf
        | HumanProxyPart::LeftFoot
        | HumanProxyPart::RightFoot => humanoid_clothing_surface_response(surface_state),
        HumanProxyPart::Impostor => WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH,
    }
}

fn humanoid_part_is_skin(part: HumanProxyPart) -> bool {
    matches!(
        part,
        HumanProxyPart::Head
            | HumanProxyPart::Neck
            | HumanProxyPart::LeftArm
            | HumanProxyPart::RightArm
            | HumanProxyPart::LeftForearm
            | HumanProxyPart::RightForearm
            | HumanProxyPart::LeftHand
            | HumanProxyPart::RightHand
    )
}

fn humanoid_part_is_hair(part: HumanProxyPart) -> bool {
    matches!(part, HumanProxyPart::HairCap)
}

fn humanoid_part_is_arm_or_hand(part: HumanProxyPart) -> bool {
    matches!(
        part,
        HumanProxyPart::LeftArm
            | HumanProxyPart::RightArm
            | HumanProxyPart::LeftUpperArm
            | HumanProxyPart::RightUpperArm
            | HumanProxyPart::LeftForearm
            | HumanProxyPart::RightForearm
            | HumanProxyPart::LeftHand
            | HumanProxyPart::RightHand
    )
}

fn humanoid_part_is_foot(part: HumanProxyPart) -> bool {
    matches!(part, HumanProxyPart::LeftFoot | HumanProxyPart::RightFoot)
}

fn humanoid_part_is_clothing(part: HumanProxyPart) -> bool {
    !humanoid_part_is_skin(part) && !humanoid_part_is_hair(part) && part != HumanProxyPart::Impostor
}

fn humanoid_part_surface_for_color(
    part: HumanProxyPart,
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    if humanoid_part_is_skin(part) {
        humanoid_default_skin_color(color)
    } else if humanoid_part_is_hair(part) {
        humanoid_hair_color(color, surface_state)
    } else if humanoid_part_is_foot(part) {
        humanoid_boot_color(color)
    } else {
        humanoid_default_clothing_color(color)
    }
}

fn humanoid_part_state_color(
    part: HumanProxyPart,
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let base = humanoid_part_surface_for_color(part, color, surface_state);
    let Some(surface) = surface_state else {
        return base;
    };

    if humanoid_part_is_skin(part) {
        humanoid_skin_state_color(base, surface)
    } else if humanoid_part_is_clothing(part) {
        humanoid_clothing_state_color(base, surface)
    } else {
        base
    }
}

fn humanoid_skin_or_cloth_extremity_color(
    part: HumanProxyPart,
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> ([f32; 4], [f32; 4]) {
    if humanoid_part_is_arm_or_hand(part) {
        (
            humanoid_default_skin_color(color),
            humanoid_skin_surface_response(surface_state),
        )
    } else {
        (
            humanoid_boot_color(color),
            humanoid_clothing_surface_response(surface_state),
        )
    }
}

fn humanoid_face_feature_surface_response(
    kind: HumanProxyFaceFeatureKind,
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    match kind {
        HumanProxyFaceFeatureKind::LeftEye | HumanProxyFaceFeatureKind::RightEye => {
            humanoid_eye_surface_response(surface_state)
        }
        HumanProxyFaceFeatureKind::LeftBrow | HumanProxyFaceFeatureKind::RightBrow => {
            humanoid_hair_surface_response(surface_state)
        }
        HumanProxyFaceFeatureKind::Mouth
        | HumanProxyFaceFeatureKind::NoseBridge
        | HumanProxyFaceFeatureKind::NoseTip
        | HumanProxyFaceFeatureKind::LeftCheek
        | HumanProxyFaceFeatureKind::RightCheek
        | HumanProxyFaceFeatureKind::LeftEar
        | HumanProxyFaceFeatureKind::RightEar
        | HumanProxyFaceFeatureKind::Chin
        | HumanProxyFaceFeatureKind::JawShadow => humanoid_skin_surface_response(surface_state),
    }
}

fn humanoid_skin_surface_response(surface_state: Option<&HumanSurfaceState>) -> [f32; 4] {
    let Some(surface) = surface_state else {
        return WINDOW_SURFACE_RESPONSE_HUMAN_SKIN;
    };

    let wetness = (surface.skin_wetness + surface.sweat_sheen * 0.5 + surface.oil_sheen * 0.32)
        .clamp(0.0, 1.0);
    let rough_extra =
        (surface.dirt * 0.22 + surface.injury_overlay * 0.07 + surface.bruising * 0.04)
            .clamp(0.0, 0.35);
    window_clamp_surface_response([
        WINDOW_SURFACE_RESPONSE_HUMAN_SKIN[0] - wetness * 0.24 + rough_extra,
        0.0,
        WINDOW_SURFACE_RESPONSE_HUMAN_SKIN[2] + wetness * 0.56,
        0.0,
    ])
}

fn humanoid_wet_skin_surface_response(surface_state: Option<&HumanSurfaceState>) -> [f32; 4] {
    let base = humanoid_skin_surface_response(surface_state);
    window_clamp_surface_response([base[0] * 0.72, 0.0, base[2].max(0.62), 0.0])
}

fn humanoid_eye_surface_response(surface_state: Option<&HumanSurfaceState>) -> [f32; 4] {
    let redness = surface_state
        .map(|surface| surface.eye_redness)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_surface_response([
        WINDOW_SURFACE_RESPONSE_HUMAN_EYE[0] + redness * 0.08,
        0.0,
        WINDOW_SURFACE_RESPONSE_HUMAN_EYE[2],
        WINDOW_SURFACE_RESPONSE_HUMAN_EYE[3] + redness * 0.02,
    ])
}

fn humanoid_hair_surface_response(surface_state: Option<&HumanSurfaceState>) -> [f32; 4] {
    let wetness = surface_state
        .map(|surface| surface.hair_wetness)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_surface_response([
        WINDOW_SURFACE_RESPONSE_HUMAN_HAIR[0] - wetness * 0.24,
        0.0,
        WINDOW_SURFACE_RESPONSE_HUMAN_HAIR[2] + wetness * 0.46,
        0.0,
    ])
}

fn humanoid_clothing_surface_response(surface_state: Option<&HumanSurfaceState>) -> [f32; 4] {
    let Some(surface) = surface_state else {
        return WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH;
    };

    let wetness = surface.clothing_wetness.clamp(0.0, 1.0);
    let wear = (surface.clothing_damage * 0.28 + surface.dirt * 0.2).clamp(0.0, 0.42);
    window_clamp_surface_response([
        WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH[0] - wetness * 0.22 + wear,
        0.0,
        WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH[2] + wetness * 0.52,
        0.0,
    ])
}

fn humanoid_detail_seed(instance: &WindowHumanoidProxyInstance<'_>) -> u64 {
    let human_seed = instance
        .human
        .map(|human| human.human_id)
        .unwrap_or(0xA5_4F_19_2C_DA_77);
    human_seed
        ^ window_position_seed([
            instance.center_meters[0],
            instance.center_meters[1],
            instance.z_meters,
        ])
        .rotate_left(17)
}

fn humanoid_viewer_distance_meters(
    instance: &WindowHumanoidProxyInstance<'_>,
    proxy: &HumanProxyGeometry,
) -> f32 {
    let body_focus = [
        instance.center_meters[0],
        instance.center_meters[1],
        instance.z_meters + proxy.height_meters * 0.55,
    ];
    let delta = sub3(body_focus, instance.viewer_position);
    dot3(delta, delta).sqrt()
}

fn humanoid_allows_mid_detail(
    instance: &WindowHumanoidProxyInstance<'_>,
    proxy: &HumanProxyGeometry,
) -> bool {
    humanoid_viewer_distance_meters(instance, proxy) <= (proxy.height_meters * 4.35).clamp(5.8, 8.2)
}

fn humanoid_allows_near_detail(
    instance: &WindowHumanoidProxyInstance<'_>,
    proxy: &HumanProxyGeometry,
) -> bool {
    humanoid_viewer_distance_meters(instance, proxy) <= (proxy.height_meters * 2.05).clamp(2.8, 4.1)
}

fn humanoid_has_visible_cybernetic_detail(
    instance: &WindowHumanoidProxyInstance<'_>,
    proxy: &HumanProxyGeometry,
) -> bool {
    proxy.quality_tier == QualityTier::ReferenceOfflineValidation
        || instance
            .human
            .map(|human| human.human_id % 3 == 0)
            .unwrap_or(false)
}

fn humanoid_part_color(
    color: [f32; 4],
    part: HumanProxyPart,
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    if part == HumanProxyPart::Impostor {
        color
    } else {
        humanoid_part_state_color(part, color, surface_state)
    }
}

fn humanoid_default_skin_color(color: [f32; 4]) -> [f32; 4] {
    let warmth = (color[0] * 0.36 + color[1] * 0.24 + color[2] * 0.08).clamp(0.0, 1.0);
    window_clamp_color([
        0.38 + warmth * 0.34,
        0.24 + warmth * 0.24,
        0.17 + warmth * 0.16,
        color[3],
    ])
}

fn humanoid_default_clothing_color(color: [f32; 4]) -> [f32; 4] {
    window_clamp_color([
        color[0] * 0.12 + 0.035,
        color[1] * 0.1 + 0.038,
        color[2] * 0.14 + 0.045,
        color[3],
    ])
}

fn humanoid_skin_state_color(color: [f32; 4], surface: &HumanSurfaceState) -> [f32; 4] {
    window_clamp_color([
        color[0] + surface.injury_overlay * 0.18 + surface.bruising * 0.06 - surface.dirt * 0.1,
        color[1] - surface.injury_overlay * 0.08 - surface.bruising * 0.04 - surface.dirt * 0.12,
        color[2] + surface.skin_wetness * 0.09 + surface.bruising * 0.12 - surface.dirt * 0.08,
        color[3],
    ])
}

fn humanoid_clothing_state_color(color: [f32; 4], surface: &HumanSurfaceState) -> [f32; 4] {
    window_clamp_color([
        color[0] * (1.0 - surface.clothing_wetness * 0.16) + surface.dirt * 0.025,
        color[1] * (1.0 - surface.clothing_wetness * 0.14) + surface.dirt * 0.02,
        color[2] * (1.0 + surface.clothing_wetness * 0.08) + surface.clothing_damage * 0.035,
        color[3],
    ])
}

fn humanoid_hair_color(color: [f32; 4], surface_state: Option<&HumanSurfaceState>) -> [f32; 4] {
    let wetness = surface_state
        .map(|surface| surface.hair_wetness)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_color([
        (color[0] * 0.18 + 0.018) * (1.0 - wetness * 0.38),
        (color[1] * 0.13 + 0.014) * (1.0 - wetness * 0.34),
        (color[2] * 0.1 + 0.012) * (1.0 + wetness * 0.1),
        color[3],
    ])
}

fn humanoid_hairline_shadow_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let hair = humanoid_hair_color(color, surface_state);
    let wetness = surface_state
        .map(|surface| surface.hair_wetness)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_color([
        hair[0] * (0.46 - wetness * 0.08),
        hair[1] * (0.48 - wetness * 0.06),
        hair[2] * (0.54 + wetness * 0.10),
        0.82,
    ])
}

fn humanoid_clothing_color(color: [f32; 4]) -> [f32; 4] {
    window_clamp_color([
        color[0] * 0.08 + 0.028,
        color[1] * 0.09 + 0.032,
        color[2] * 0.16 + 0.052,
        color[3],
    ])
}

fn humanoid_clothing_inner_layer_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let wetness = surface_state
        .map(|surface| surface.clothing_wetness)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    let damage = surface_state
        .map(|surface| surface.clothing_damage)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_color([
        color[0] * 0.11 + 0.055 - wetness * 0.018 + damage * 0.018,
        color[1] * 0.13 + 0.052 - wetness * 0.012,
        color[2] * 0.19 + 0.075 + wetness * 0.028,
        0.88,
    ])
}

fn humanoid_limb_volume_color(
    part: HumanProxyPart,
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    if humanoid_part_is_skin(part) {
        window_mix_color(
            humanoid_default_skin_color(color),
            humanoid_skin_warm_highlight_color(color, surface_state),
            0.34,
        )
    } else {
        window_mix_color(
            humanoid_clothing_color(color),
            humanoid_clothing_fold_shadow_color(color, surface_state),
            0.24,
        )
    }
}

fn humanoid_limb_shadow_color(
    part: HumanProxyPart,
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    if humanoid_part_is_skin(part) {
        humanoid_skin_cool_shadow_color(color, surface_state)
    } else {
        window_scale_color(humanoid_clothing_crease_color(color, surface_state), 0.78)
    }
}

fn humanoid_limb_highlight_color(
    part: HumanProxyPart,
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    if humanoid_part_is_skin(part) {
        humanoid_skin_warm_highlight_color(color, surface_state)
    } else {
        window_scale_color(humanoid_clothing_color(color), 1.22)
    }
}

fn humanoid_boot_color(color: [f32; 4]) -> [f32; 4] {
    window_clamp_color([
        color[0] * 0.045 + 0.014,
        color[1] * 0.04 + 0.013,
        color[2] * 0.04 + 0.016,
        color[3],
    ])
}

fn humanoid_trim_color() -> [f32; 4] {
    [0.22, 0.62, 0.66, 0.62]
}

fn humanoid_skin_wet_sheen_color(surface: &HumanSurfaceState) -> Option<[f32; 4]> {
    let wetness = (surface.skin_wetness + surface.sweat_sheen * 0.45 + surface.oil_sheen * 0.25)
        .clamp(0.0, 1.0);
    (wetness > 0.04).then_some([0.72, 0.9, 1.0, (0.12 + wetness * 0.3).clamp(0.0, 0.48)])
}

fn humanoid_injury_overlay_color(surface: &HumanSurfaceState) -> Option<[f32; 4]> {
    let injury = surface.injury_overlay.clamp(0.0, 1.0);
    (injury > 0.04).then_some([0.62, 0.055, 0.045, (0.14 + injury * 0.36).clamp(0.0, 0.52)])
}

fn humanoid_bruise_overlay_color(surface: &HumanSurfaceState) -> Option<[f32; 4]> {
    let bruise = surface.bruising.clamp(0.0, 1.0);
    (bruise > 0.04).then_some([0.16, 0.045, 0.3, (0.12 + bruise * 0.28).clamp(0.0, 0.46)])
}

fn humanoid_dirt_overlay_color(surface: &HumanSurfaceState) -> Option<[f32; 4]> {
    let dirt = surface.dirt.clamp(0.0, 1.0);
    (dirt > 0.04).then_some([0.055, 0.045, 0.035, (0.13 + dirt * 0.32).clamp(0.0, 0.48)])
}

fn humanoid_clothing_wet_color(surface: &HumanSurfaceState) -> Option<[f32; 4]> {
    let wetness = surface.clothing_wetness.clamp(0.0, 1.0);
    (wetness > 0.04).then_some([
        0.025,
        0.045,
        0.065,
        (0.14 + wetness * 0.28).clamp(0.0, 0.46),
    ])
}

fn humanoid_clothing_damage_color(surface: &HumanSurfaceState) -> Option<[f32; 4]> {
    let damage = surface.clothing_damage.clamp(0.0, 1.0);
    (damage > 0.04).then_some([0.13, 0.095, 0.07, (0.14 + damage * 0.3).clamp(0.0, 0.48)])
}

fn humanoid_hair_wet_color(surface: &HumanSurfaceState) -> Option<[f32; 4]> {
    let wetness = surface.hair_wetness.clamp(0.0, 1.0);
    (wetness > 0.04).then_some([0.035, 0.052, 0.058, (0.1 + wetness * 0.24).clamp(0.0, 0.38)])
}

fn humanoid_skin_microdetail_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let color = humanoid_default_skin_color(color);
    let dirt = surface_state
        .map(|surface| surface.dirt)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    let injury = surface_state
        .map(|surface| surface.injury_overlay)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_color([
        color[0] * 0.58 + dirt * 0.04 + injury * 0.11,
        color[1] * 0.46 - dirt * 0.03 - injury * 0.05,
        color[2] * 0.42 - dirt * 0.02 + injury * 0.02,
        0.36,
    ])
}

fn humanoid_wrinkle_line_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let color = humanoid_default_skin_color(color);
    let wetness = surface_state
        .map(|surface| surface.skin_wetness + surface.sweat_sheen * 0.4)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_color([
        color[0] * (0.48 + wetness * 0.12),
        color[1] * (0.39 + wetness * 0.08),
        color[2] * (0.35 + wetness * 0.12),
        0.44,
    ])
}

fn humanoid_peach_fuzz_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let color = humanoid_default_skin_color(color);
    let wetness = surface_state
        .map(|surface| surface.skin_wetness)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_color([
        color[0] * 1.14 + wetness * 0.06,
        color[1] * 1.08 + wetness * 0.05,
        color[2] * 1.02 + wetness * 0.08,
        0.22,
    ])
}

fn humanoid_skin_soft_shadow_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let color = humanoid_default_skin_color(color);
    let dirt = surface_state
        .map(|surface| surface.dirt)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    let bruise = surface_state
        .map(|surface| surface.bruising)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_color([
        color[0] * 0.72 + dirt * 0.035 + bruise * 0.04,
        color[1] * 0.58 - dirt * 0.028,
        color[2] * 0.52 + bruise * 0.035,
        0.72,
    ])
}

fn humanoid_skin_cool_shadow_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let color = humanoid_default_skin_color(color);
    let wetness = surface_state
        .map(|surface| surface.skin_wetness + surface.sweat_sheen * 0.35)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    let bruise = surface_state
        .map(|surface| surface.bruising)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_color([
        color[0] * 0.56 + wetness * 0.018 + bruise * 0.035,
        color[1] * 0.50 + wetness * 0.018,
        color[2] * 0.56 + wetness * 0.050 + bruise * 0.052,
        0.66,
    ])
}

fn humanoid_skin_warm_highlight_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let color = humanoid_default_skin_color(color);
    let wetness = surface_state
        .map(|surface| surface.skin_wetness + surface.sweat_sheen * 0.45)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_color([
        color[0] * 1.08 + wetness * 0.035,
        color[1] * 1.02 + wetness * 0.03,
        color[2] * 0.96 + wetness * 0.028,
        0.48,
    ])
}

fn humanoid_soft_tissue_subsurface_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let skin = humanoid_default_skin_color(color);
    let injury = surface_state
        .map(|surface| surface.injury_overlay)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    let bruise = surface_state
        .map(|surface| surface.bruising)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    let wetness = surface_state
        .map(|surface| surface.skin_wetness + surface.sweat_sheen * 0.25)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_color([
        skin[0] * 0.96 + 0.035 + injury * 0.09 + wetness * 0.025,
        skin[1] * 0.78 + 0.020 - bruise * 0.035 + wetness * 0.020,
        skin[2] * 0.70 + 0.018 + bruise * 0.040 + wetness * 0.024,
        0.42,
    ])
}

fn humanoid_soft_tissue_blush_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let skin = humanoid_default_skin_color(color);
    let redness = surface_state
        .map(|surface| {
            surface.eye_redness * 0.18 + surface.injury_overlay * 0.26 + surface.bruising * 0.14
        })
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_color([
        skin[0] * 1.02 + 0.045 + redness * 0.12,
        skin[1] * 0.76 + 0.018 - redness * 0.040,
        skin[2] * 0.68 + 0.024 + redness * 0.035,
        0.36,
    ])
}

fn humanoid_clothing_pressure_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let wetness = surface_state
        .map(|surface| surface.clothing_wetness)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    let damage = surface_state
        .map(|surface| surface.clothing_damage)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    let cloth = humanoid_clothing_color(color);
    window_clamp_color([
        cloth[0] * 1.14 + damage * 0.030,
        cloth[1] * 1.10 + damage * 0.020,
        cloth[2] * (1.05 + wetness * 0.22),
        0.66,
    ])
}

fn humanoid_inner_mouth_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let redness = surface_state
        .map(|surface| surface.injury_overlay * 0.35 + surface.eye_redness * 0.18)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_mix_color(
        humanoid_default_skin_color(color),
        [0.11 + redness * 0.08, 0.024, 0.025 + redness * 0.04, 0.9],
        0.74,
    )
}

fn humanoid_tooth_enamel_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let dirt = surface_state
        .map(|surface| surface.dirt)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_color([
        0.82 + color[0] * 0.035 - dirt * 0.1,
        0.78 + color[1] * 0.03 - dirt * 0.08,
        0.68 + color[2] * 0.025 - dirt * 0.06,
        0.84,
    ])
}

fn humanoid_tongue_color(color: [f32; 4], surface_state: Option<&HumanSurfaceState>) -> [f32; 4] {
    let wetness = surface_state
        .map(|surface| surface.skin_wetness + surface.sweat_sheen * 0.25)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_color([
        0.34 + color[0] * 0.06 + wetness * 0.04,
        0.09 + color[1] * 0.035,
        0.08 + color[2] * 0.035 + wetness * 0.035,
        0.72,
    ])
}

fn humanoid_fingernail_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let dirt = surface_state
        .map(|surface| surface.dirt)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    let skin = humanoid_default_skin_color(color);
    window_clamp_color([
        skin[0] * 1.08 - dirt * 0.08,
        skin[1] * 1.0 - dirt * 0.06,
        skin[2] * 0.92 - dirt * 0.04,
        0.58,
    ])
}

fn humanoid_eye_tearline_color(surface_state: Option<&HumanSurfaceState>) -> [f32; 4] {
    let redness = surface_state
        .map(|surface| surface.eye_redness)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_color([
        0.86 + redness * 0.1,
        0.95 - redness * 0.18,
        1.0 - redness * 0.18,
        0.82,
    ])
}

fn humanoid_iris_detail_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let redness = surface_state
        .map(|surface| surface.eye_redness)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_color([
        0.035 + color[2] * 0.18 + redness * 0.06,
        0.16 + color[1] * 0.2 - redness * 0.04,
        0.24 + color[0] * 0.16 - redness * 0.05,
        0.95,
    ])
}

fn humanoid_hair_strand_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
    seed: u64,
    salt: u64,
) -> [f32; 4] {
    let variation = 0.84 + window_stable_unit(seed, 17_911 + salt) * 0.32;
    window_scale_color(humanoid_hair_color(color, surface_state), variation)
}

fn humanoid_clothing_seam_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let damage = surface_state
        .map(|surface| surface.clothing_damage)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_color([
        color[0] * 0.035 + 0.02 + damage * 0.04,
        color[1] * 0.038 + 0.02 + damage * 0.025,
        color[2] * 0.055 + 0.03,
        0.88,
    ])
}

fn humanoid_clothing_crease_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let wetness = surface_state
        .map(|surface| surface.clothing_wetness)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_color([
        color[0] * 0.055,
        color[1] * 0.062,
        color[2] * (0.09 + wetness * 0.08),
        0.5,
    ])
}

fn humanoid_clothing_fold_shadow_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let wetness = surface_state
        .map(|surface| surface.clothing_wetness)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    let damage = surface_state
        .map(|surface| surface.clothing_damage)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_color([
        color[0] * 0.045 + 0.018 + damage * 0.035,
        color[1] * 0.05 + 0.017 + damage * 0.02,
        color[2] * (0.065 + wetness * 0.055) + 0.02,
        0.62,
    ])
}

fn humanoid_clothing_worn_edge_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let dirt = surface_state
        .map(|surface| surface.dirt)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_color([
        color[0] * 0.075 + 0.045 - dirt * 0.018,
        color[1] * 0.074 + 0.044 - dirt * 0.018,
        color[2] * 0.082 + 0.045 - dirt * 0.012,
        0.74,
    ])
}

fn humanoid_boot_sole_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let dirt = surface_state
        .map(|surface| surface.dirt)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_color([
        color[0] * 0.025 + 0.012 + dirt * 0.03,
        color[1] * 0.024 + 0.011 + dirt * 0.024,
        color[2] * 0.022 + 0.011 + dirt * 0.016,
        0.86,
    ])
}

fn humanoid_cybernetic_plate_color(seed: u64) -> [f32; 4] {
    let tint = window_stable_unit(seed, 19_771);
    window_clamp_color([
        0.13 + tint * 0.06,
        0.16 + tint * 0.07,
        0.18 + tint * 0.08,
        0.94,
    ])
}

fn humanoid_cybernetic_glow_color(seed: u64) -> [f32; 4] {
    if window_stable_unit(seed, 20_411) > 0.5 {
        [0.24, 0.58, 0.62, 0.52]
    } else {
        [0.78, 0.44, 0.24, 0.48]
    }
}

fn humanoid_implant_seam_color(
    color: [f32; 4],
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    let injury = surface_state
        .map(|surface| surface.injury_overlay + surface.bruising * 0.45)
        .unwrap_or_default()
        .clamp(0.0, 1.0);
    window_clamp_color([
        color[0] * 0.38 + 0.16 + injury * 0.18,
        color[1] * 0.22 + 0.035,
        color[2] * 0.20 + 0.035 + injury * 0.03,
        0.62,
    ])
}

fn humanoid_face_feature_color(
    color: [f32; 4],
    feature: HumanProxyFaceFeature,
    emotion: Option<&EmotionState>,
    surface_state: Option<&HumanSurfaceState>,
) -> [f32; 4] {
    match feature.kind {
        HumanProxyFaceFeatureKind::LeftEye | HumanProxyFaceFeatureKind::RightEye => {
            let alert = emotion
                .map(|emotion| emotion.fear * 0.45 + emotion.anger * 0.2 + emotion.urgency * 0.35)
                .unwrap_or_default()
                .clamp(0.0, 1.0);
            let redness = surface_state
                .map(|surface| surface.eye_redness)
                .unwrap_or_default()
                .clamp(0.0, 1.0);
            window_clamp_color([
                0.12 + alert * 0.82 + redness * 0.18,
                0.86 - alert * 0.42 - redness * 0.22,
                1.0 - alert * 0.72 - redness * 0.2,
                1.0,
            ])
        }
        HumanProxyFaceFeatureKind::Mouth => window_mix_color(
            humanoid_default_skin_color(color),
            [0.18, 0.052, 0.048, 1.0],
            0.62,
        ),
        HumanProxyFaceFeatureKind::NoseBridge | HumanProxyFaceFeatureKind::NoseTip => {
            window_scale_color(humanoid_default_skin_color(color), 1.08)
        }
        HumanProxyFaceFeatureKind::LeftCheek | HumanProxyFaceFeatureKind::RightCheek => {
            window_mix_color(
                humanoid_default_skin_color(color),
                humanoid_skin_warm_highlight_color(color, surface_state),
                0.62,
            )
        }
        HumanProxyFaceFeatureKind::LeftBrow | HumanProxyFaceFeatureKind::RightBrow => {
            window_scale_color(humanoid_hair_color(color, surface_state), 0.72)
        }
        HumanProxyFaceFeatureKind::LeftEar | HumanProxyFaceFeatureKind::RightEar => {
            window_mix_color(
                humanoid_default_skin_color(color),
                humanoid_skin_soft_shadow_color(color, surface_state),
                0.45,
            )
        }
        HumanProxyFaceFeatureKind::Chin => {
            window_scale_color(humanoid_default_skin_color(color), 0.93)
        }
        HumanProxyFaceFeatureKind::JawShadow => {
            humanoid_skin_soft_shadow_color(color, surface_state)
        }
    }
}

fn horizontal_direction(vector: [f32; 3]) -> [f32; 3] {
    let direction = normalize3([vector[0], vector[1], 0.0]);
    if direction[0].abs() <= f32::EPSILON && direction[1].abs() <= f32::EPSILON {
        return [0.0, -1.0, 0.0];
    }
    direction
}

fn window_scale_color(mut color: [f32; 4], scale: f32) -> [f32; 4] {
    let alpha = color[3];
    color[0] *= scale;
    color[1] *= scale;
    color[2] *= scale;
    color[3] = alpha;
    window_clamp_color(color)
}

fn window_mix_color(left: [f32; 4], right: [f32; 4], amount: f32) -> [f32; 4] {
    let amount = amount.clamp(0.0, 1.0);
    window_clamp_color([
        left[0] + (right[0] - left[0]) * amount,
        left[1] + (right[1] - left[1]) * amount,
        left[2] + (right[2] - left[2]) * amount,
        left[3] + (right[3] - left[3]) * amount,
    ])
}

fn window_with_alpha(mut color: [f32; 4], alpha: f32) -> [f32; 4] {
    color[3] = alpha.clamp(0.0, 1.0);
    color
}

fn window_clamp_color(mut color: [f32; 4]) -> [f32; 4] {
    for channel in &mut color {
        *channel = channel.clamp(0.0, 1.0);
    }
    color
}

fn window_clamp_surface_response(mut surface_response: [f32; 4]) -> [f32; 4] {
    for channel in &mut surface_response {
        *channel = channel.clamp(0.0, 1.0);
    }
    surface_response
}

fn window_clamp_surface_detail(mut material_detail: [f32; 4]) -> [f32; 4] {
    material_detail[0] = material_detail[0].clamp(0.05, 8.0);
    for channel in &mut material_detail[1..] {
        *channel = channel.clamp(0.0, 1.0);
    }
    material_detail
}

fn window_clamp_surface_cache(mut generated_cache: [f32; 4]) -> [f32; 4] {
    for channel in &mut generated_cache {
        *channel = channel.clamp(0.0, 1.0);
    }
    generated_cache
}

fn add_window_glass_wall_mesh(
    geometry: &mut WindowSceneGeometry,
    instance: WindowProceduralMeshInstance,
    fractured_mesh: bool,
) {
    let fractured =
        fractured_mesh || instance.crack_density > WINDOW_FRACTURED_CRACK_DENSITY_THRESHOLD;
    let [x, y] = instance.center_meters;
    let z = instance.z_meters;
    let state_detail = window_material_state_detail_from_instance(instance);
    let frame = [0.075, 0.09, 0.105, 1.0];
    let pane = window_with_alpha(instance.color, if fractured { 0.72 } else { 0.86 });

    geometry.world_box_with_surface_response(
        [x - 1.27, y - 0.105, z - 0.02],
        [x - 1.16, y + 0.105, z + 2.55],
        frame,
        WINDOW_SURFACE_RESPONSE_METAL,
    );
    geometry.world_box_with_surface_response(
        [x + 1.16, y - 0.105, z - 0.02],
        [x + 1.27, y + 0.105, z + 2.55],
        frame,
        WINDOW_SURFACE_RESPONSE_METAL,
    );
    geometry.world_box_with_surface_response(
        [x - 1.27, y - 0.105, z + 2.43],
        [x + 1.27, y + 0.105, z + 2.55],
        window_scale_color(frame, 1.18),
        WINDOW_SURFACE_RESPONSE_METAL,
    );
    geometry.world_box_with_surface_response(
        [x - 1.27, y - 0.105, z - 0.02],
        [x + 1.27, y + 0.105, z + 0.1],
        window_scale_color(frame, 0.82),
        WINDOW_SURFACE_RESPONSE_METAL,
    );

    if fractured {
        geometry.world_box_with_surface_state_detail(
            [x - 1.12, y - 0.055, z + 0.12],
            [x - 0.24, y + 0.055, z + 2.28],
            pane,
            WINDOW_SURFACE_RESPONSE_GLASS,
            state_detail,
        );
        geometry.world_box_with_surface_state_detail(
            [x + 0.18, y - 0.055, z + 0.28],
            [x + 1.08, y + 0.055, z + 2.16],
            window_scale_color(pane, 1.08),
            WINDOW_SURFACE_RESPONSE_GLASS,
            state_detail,
        );
        geometry.world_box_with_surface_state_detail(
            [x - 0.04, y - 0.025, z + 0.2],
            [x + 0.04, y + 0.025, z + 2.42],
            [0.92, 0.98, 1.0, 0.94],
            WINDOW_SURFACE_RESPONSE_GLASS,
            state_detail,
        );
        add_window_glass_shards(geometry, instance.center_meters, z, pane);
        add_window_fracture_state_overlays(geometry, instance);
    } else {
        geometry.world_box_with_surface_state_detail(
            [x - 1.14, y - 0.05, z + 0.12],
            [x + 1.14, y + 0.05, z + 2.38],
            pane,
            WINDOW_SURFACE_RESPONSE_GLASS,
            state_detail,
        );
        if instance.crack_density > 0.05 {
            add_window_glass_crack_strips(geometry, instance);
        }
    }
}

fn add_window_glass_crack_strips(
    geometry: &mut WindowSceneGeometry,
    instance: WindowProceduralMeshInstance,
) {
    let [x, y] = instance.center_meters;
    let z = instance.z_meters;
    let color = [
        0.88,
        0.98,
        1.0,
        (0.45 + instance.crack_density * 0.4).clamp(0.0, 0.9),
    ];
    for (offset_x, height, slope) in [(-0.42, 1.42, 0.44), (0.18, 1.78, -0.36), (0.5, 1.1, 0.28)] {
        geometry.world_oriented_rect_with_surface_response(
            [x + offset_x, y - 0.062, z + height],
            normalize3([0.42, 0.0, slope]),
            [0.0, 1.0, 0.0],
            [0.34, 0.006],
            color,
            WINDOW_SURFACE_RESPONSE_GLASS,
        );
    }

    if instance.crack_density > 0.65 {
        for (offset_x, height, slope, width) in [
            (-0.2, 1.18, 0.18, 0.22),
            (0.38, 1.52, -0.24, 0.18),
            (-0.62, 0.86, -0.14, 0.16),
        ] {
            geometry.world_oriented_rect_with_surface_response(
                [x + offset_x, y - 0.066, z + height],
                normalize3([0.26, 0.0, slope]),
                [0.0, 1.0, 0.0],
                [width, 0.004],
                window_with_alpha(color, 0.42),
                WINDOW_SURFACE_RESPONSE_GLASS,
            );
        }
    }
}

fn add_window_fracture_state_overlays(
    geometry: &mut WindowSceneGeometry,
    instance: WindowProceduralMeshInstance,
) {
    let [x, y] = instance.center_meters;
    let z = instance.z_meters;
    let soot = instance.soot.clamp(0.0, 1.0);
    if soot > 0.04 {
        let color = [0.035, 0.03, 0.028, (0.18 + soot * 0.28).clamp(0.0, 0.52)];
        for (offset_x, height, half_width) in
            [(-0.62, 1.92, 0.28), (0.42, 1.42, 0.24), (-0.08, 0.74, 0.22)]
        {
            geometry.world_oriented_rect_with_surface_response(
                [x + offset_x, y - 0.071, z + height],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [half_width, 0.12 + soot * 0.08],
                color,
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }
    }

    let heat = instance.heat.clamp(0.0, 1.0);
    if heat > 0.06 {
        geometry.world_oriented_rect_with_surface_response(
            [x + 0.02, y - 0.074, z + 0.18],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.92, 0.055],
            [1.0, 0.38, 0.14, (0.12 + heat * 0.22).clamp(0.0, 0.38)],
            WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
        );
    }
}

fn add_window_glass_shards(
    geometry: &mut WindowSceneGeometry,
    center: [f32; 2],
    z: f32,
    color: [f32; 4],
) {
    let [x, y] = center;
    let shard_color = window_with_alpha(window_scale_color(color, 1.18), 0.56);
    let shard_z = z.max(0.0) + 0.035;
    for (dx, dy, sx, sy) in [
        (-0.54, -0.36, 0.32, 0.11),
        (-0.2, 0.42, 0.26, 0.13),
        (0.18, -0.22, 0.38, 0.09),
        (0.48, 0.34, 0.28, 0.12),
        (0.76, -0.48, 0.24, 0.08),
    ] {
        geometry.world_triangle_with_surface_response(
            [x + dx - sx, y + dy, shard_z],
            [x + dx + sx * 0.55, y + dy - sy, shard_z + 0.012],
            [x + dx + sx, y + dy + sy, shard_z],
            shard_color,
            WINDOW_SURFACE_RESPONSE_GLASS,
        );
    }
}

fn add_window_neon_sign_mesh(
    geometry: &mut WindowSceneGeometry,
    instance: WindowProceduralMeshInstance,
) {
    let [x, y] = instance.center_meters;
    let z = instance.z_meters;
    geometry.world_oriented_rect_with_surface_response(
        [x, y - 0.082, z],
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.86, 0.38],
        [0.034, 0.028, 0.038, 0.9],
        WINDOW_SURFACE_RESPONSE_METAL,
    );
    for edge_z in [z - 0.35, z + 0.35] {
        geometry.world_cylinder_between_with_surface_response(
            [x - 0.82, y - 0.096, edge_z],
            [x + 0.82, y - 0.096, edge_z],
            0.018,
            8,
            [0.055, 0.05, 0.056, 0.92],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
    }
    geometry.world_cylinder_between_with_surface_response(
        [x - 0.62, y - 0.095, z + 0.19],
        [x + 0.62, y - 0.095, z + 0.19],
        0.033,
        10,
        window_with_alpha(window_scale_color(instance.color, 0.66), 0.5),
        WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
    );
    geometry.world_cylinder_between_with_surface_response(
        [x - 0.58, y - 0.1, z - 0.02],
        [x + 0.58, y - 0.1, z - 0.02],
        0.026,
        10,
        [0.34, 0.54, 0.58, 0.46],
        WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
    );
    geometry.world_cylinder_between_with_surface_response(
        [x - 0.62, y - 0.095, z - 0.23],
        [x + 0.62, y - 0.095, z - 0.23],
        0.024,
        10,
        [0.74, 0.62, 0.34, 0.5],
        WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
    );
    for side_x in [x - 0.71, x + 0.71] {
        geometry.world_cylinder_between_with_surface_response(
            [side_x, y - 0.092, z - 0.28],
            [side_x, y - 0.092, z + 0.28],
            0.02,
            8,
            [0.48, 0.24, 0.58, 0.42],
            WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
        );
    }
    geometry.world_billboard_with_surface_response(
        [x, y - 0.02, z],
        instance.billboard_right,
        1.9,
        0.9,
        [0.62, 0.28, 0.42, 0.16],
        WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
    );

    let charge = instance.electrical_charge.clamp(0.0, 1.0);
    if charge > 0.05 {
        geometry.world_billboard_with_surface_response(
            [x, y - 0.04, z + 0.02],
            instance.billboard_right,
            2.35 + charge * 0.42,
            1.08 + charge * 0.18,
            [0.72, 0.32, 0.52, (0.08 + charge * 0.16).clamp(0.0, 0.32)],
            WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
        );
        for spark in 0..4 {
            let offset = spark as f32 - 1.5;
            geometry.world_oriented_rect_with_surface_response(
                [x + offset * 0.22, y - 0.118, z + 0.34 - spark as f32 * 0.07],
                normalize3([0.22, 0.0, 0.16 - spark as f32 * 0.03]),
                [0.0, 1.0, 0.0],
                [0.065 + charge * 0.025, 0.004],
                [0.9, 0.74, 0.36, (0.18 + charge * 0.28).clamp(0.0, 0.58)],
                WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
            );
        }
    }

    let heat = instance.heat.clamp(0.0, 1.0);
    if heat > 0.05 {
        geometry.world_oriented_rect_with_surface_response(
            [x, y + 0.04, z - 0.32],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.72, 0.032],
            [0.54, 0.28, 0.12, (0.22 + heat * 0.24).clamp(0.0, 0.52)],
            WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
        );
    }
}

fn add_window_security_camera_mesh(
    geometry: &mut WindowSceneGeometry,
    instance: WindowProceduralMeshInstance,
) {
    let [x, y] = instance.center_meters;
    let z = instance.z_meters;
    let charge = instance.electrical_charge.clamp(0.0, 1.0);
    let corrosion = instance.corrosion.clamp(0.0, 1.0);
    let moisture = instance.moisture.clamp(0.0, 1.0);
    let soot = instance.soot.clamp(0.0, 1.0);
    let state_detail = window_material_state_detail_from_instance(instance);
    let housing = window_scale_color(instance.color, 0.46);
    let blackened = [0.025 + soot * 0.04, 0.028, 0.032, 1.0];

    geometry.world_micro_detailed_box_with_surface_state_detail(
        [x - 0.36, y - 0.25, z - 0.3],
        [x - 0.28, y + 0.25, z + 0.3],
        [0.035, 0.038, 0.042, 1.0],
        WINDOW_SURFACE_RESPONSE_METAL,
        WindowMicroDetailRecipe {
            seed: 7_000,
            density: 0.34,
        },
        state_detail,
    );
    geometry.world_cylinder_between_with_surface_response(
        [x - 0.3, y, z + 0.03],
        [x - 0.08, y, z + 0.03],
        0.045,
        12,
        [0.075, 0.082, 0.09, 1.0],
        WINDOW_SURFACE_RESPONSE_METAL,
    );
    geometry.world_cylinder_between_with_surface_response(
        [x - 0.2, y, z - 0.18],
        [x - 0.2, y, z + 0.14],
        0.025,
        10,
        [0.055, 0.06, 0.065, 1.0],
        WINDOW_SURFACE_RESPONSE_METAL,
    );

    geometry.world_ellipsoid_with_surface_response(
        [x + 0.08, y, z - 0.005],
        [0.29, 0.18, 0.14],
        6,
        12,
        housing,
        WINDOW_SURFACE_RESPONSE_METAL,
    );
    geometry.world_oriented_rect_with_surface_response(
        [x + 0.1, y, z + 0.14],
        [0.0, 1.0, 0.0],
        normalize3([0.18, 0.0, 1.0]),
        [0.25, 0.065],
        [0.038, 0.042, 0.046, 0.9],
        WINDOW_SURFACE_RESPONSE_METAL,
    );
    geometry.world_ellipse_ring_with_surface_response(
        [x + 0.08, y, z - 0.005],
        [0.19, 0.105],
        [0.31, 0.185],
        14,
        [0.032, 0.036, 0.04, 0.62],
        WINDOW_SURFACE_RESPONSE_METAL,
    );
    geometry.world_cylinder_between_with_surface_response(
        [x + 0.2, y, z - 0.005],
        [x + 0.37, y, z - 0.005],
        0.08,
        12,
        [0.008, 0.014, 0.018, 1.0],
        WINDOW_SURFACE_RESPONSE_GLASS,
    );
    geometry.world_oriented_rect_with_surface_response(
        [x + 0.375, y, z - 0.005],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.082, 0.082],
        [0.18, 0.46, 0.56, 0.46],
        WINDOW_SURFACE_RESPONSE_GLASS,
    );
    geometry.world_ellipse_ring_with_surface_response(
        [x + 0.378, y, z - 0.005],
        [0.06, 0.06],
        [0.1, 0.1],
        14,
        [0.55, 0.68, 0.74, 1.0],
        WINDOW_SURFACE_RESPONSE_METAL,
    );
    geometry.world_oriented_rect_with_surface_response(
        [x + 0.32, y - 0.15, z + 0.055],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.018, 0.012],
        [0.8, 0.38, 0.16, (0.26 + charge * 0.28).clamp(0.0, 0.62)],
        WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
    );

    for cable in 0..3 {
        let offset = cable as f32 * 0.055 - 0.055;
        geometry.world_cylinder_between_with_surface_response(
            [x - 0.34, y + offset, z - 0.26],
            [x - 0.47, y + offset * 0.7, z - 0.48 - cable as f32 * 0.035],
            0.009,
            6,
            blackened,
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
    }

    if corrosion > 0.04 {
        geometry.world_billboard_with_surface_response(
            [x - 0.03, y + 0.19, z - 0.02],
            instance.billboard_right,
            0.36,
            0.22,
            [0.36, 0.16, 0.05, (0.18 + corrosion * 0.34).clamp(0.0, 0.56)],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
    }
    if moisture > 0.12 {
        for drip in 0..3 {
            let drip_y = y - 0.11 + drip as f32 * 0.1;
            geometry.world_cylinder_between_with_surface_response(
                [x + 0.08, drip_y, z - 0.14],
                [x + 0.09, drip_y + 0.01, z - 0.24 - drip as f32 * 0.02],
                0.006,
                5,
                [0.45, 0.78, 1.0, (0.22 + moisture * 0.32).clamp(0.0, 0.62)],
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            );
        }
    }
    if charge > 0.05 {
        geometry.world_billboard_with_surface_response(
            [x + 0.39, y, z + 0.005],
            instance.billboard_right,
            0.56,
            0.34,
            [0.24, 0.48, 0.56, (0.08 + charge * 0.14).clamp(0.0, 0.28)],
            WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
        );
    }
}

fn add_window_service_door_mesh(
    geometry: &mut WindowSceneGeometry,
    instance: WindowProceduralMeshInstance,
) {
    let [x, y] = instance.center_meters;
    let z = instance.z_meters;
    let corrosion = instance.corrosion.clamp(0.0, 1.0);
    let soot = instance.soot.clamp(0.0, 1.0);
    let moisture = instance.moisture.clamp(0.0, 1.0);
    let charge = instance.electrical_charge.clamp(0.0, 1.0);
    let state_detail = window_material_state_detail_from_instance(instance);
    let frame = [0.035, 0.04, 0.044, 1.0];
    let door = [
        (0.08 + corrosion * 0.04).clamp(0.0, 1.0),
        (0.09 + moisture * 0.04).clamp(0.0, 1.0),
        (0.095 + moisture * 0.05).clamp(0.0, 1.0),
        1.0,
    ];

    geometry.world_micro_detailed_box_with_surface_state_detail(
        [x - 0.08, y - 0.82, z],
        [x + 0.03, y + 0.82, z + 2.12],
        door,
        WINDOW_SURFACE_RESPONSE_METAL,
        WindowMicroDetailRecipe {
            seed: 7_100,
            density: 0.38,
        },
        state_detail,
    );
    geometry.world_micro_detailed_box_with_surface_state_detail(
        [x - 0.14, y - 0.98, z - 0.04],
        [x + 0.06, y - 0.86, z + 2.24],
        frame,
        WINDOW_SURFACE_RESPONSE_METAL,
        WindowMicroDetailRecipe {
            seed: 7_101,
            density: 0.32,
        },
        state_detail,
    );
    geometry.world_micro_detailed_box_with_surface_state_detail(
        [x - 0.14, y + 0.86, z - 0.04],
        [x + 0.06, y + 0.98, z + 2.24],
        frame,
        WINDOW_SURFACE_RESPONSE_METAL,
        WindowMicroDetailRecipe {
            seed: 7_102,
            density: 0.32,
        },
        state_detail,
    );
    geometry.world_micro_detailed_box_with_surface_state_detail(
        [x - 0.14, y - 0.98, z + 2.1],
        [x + 0.06, y + 0.98, z + 2.28],
        frame,
        WINDOW_SURFACE_RESPONSE_METAL,
        WindowMicroDetailRecipe {
            seed: 7_103,
            density: 0.32,
        },
        state_detail,
    );
    geometry.world_micro_detailed_box_with_surface_state_detail(
        [x - 0.12, y - 0.96, z - 0.04],
        [x + 0.05, y + 0.96, z + 0.08],
        [0.025, 0.024, 0.022, 1.0],
        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        WindowMicroDetailRecipe {
            seed: 7_104,
            density: 0.28,
        },
        state_detail,
    );

    for rib in 0..5 {
        let rib_z = z + 0.42 + rib as f32 * 0.34;
        geometry.world_cylinder_between_with_surface_response(
            [x + 0.055, y - 0.72, rib_z + 0.02],
            [x + 0.055, y + 0.72, rib_z + 0.02],
            0.026,
            7,
            [0.055, 0.062, 0.068, 1.0],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
    }

    geometry.world_ellipsoid_with_surface_response(
        [x + 0.086, y + 0.55, z + 1.01],
        [0.044, 0.145, 0.19],
        5,
        9,
        [0.018, 0.022, 0.026, 1.0],
        WINDOW_SURFACE_RESPONSE_METAL,
    );
    geometry.world_oriented_rect_with_surface_response(
        [x + 0.124, y + 0.55, z + 1.08],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.075, 0.035],
        [0.32, 0.68, 0.48, (0.12 + charge * 0.24).clamp(0.0, 0.42)],
        WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
    );
    geometry.world_cylinder_between_with_surface_response(
        [x + 0.13, y + 0.1, z + 1.05],
        [x + 0.13, y + 0.34, z + 1.05],
        0.025,
        10,
        [0.7, 0.73, 0.72, 1.0],
        WINDOW_SURFACE_RESPONSE_METAL,
    );

    for stripe in 0..3 {
        let stripe_z = z + 0.46 + stripe as f32 * 0.42;
        geometry.world_oriented_rect_with_surface_response(
            [x + 0.091, y - 0.42 + stripe as f32 * 0.07, stripe_z],
            [0.0, 1.0, 0.32],
            [0.0, -0.22, 1.0],
            [0.33, 0.028],
            [0.78, 0.58, 0.16, 0.52],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
    }

    for (patch_y, patch_z, radii) in [
        (y - 0.52, z + 0.18, [0.05, 0.18, 0.08]),
        (y + 0.38, z + 0.28, [0.04, 0.15, 0.06]),
    ] {
        geometry.world_ellipsoid_with_surface_response(
            [x + 0.082, patch_y, patch_z],
            radii,
            4,
            8,
            [0.12, 0.07, 0.036, 0.58],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
    }

    if corrosion > 0.04 {
        geometry.world_billboard_with_surface_response(
            [x + 0.11, y - 0.25, z + 0.74],
            instance.billboard_right,
            0.72,
            0.36,
            [0.36, 0.14, 0.045, (0.2 + corrosion * 0.34).clamp(0.0, 0.62)],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
    }
    if soot > 0.04 {
        geometry.world_billboard_with_surface_response(
            [x + 0.105, y + 0.02, z + 1.84],
            instance.billboard_right,
            0.84,
            0.28,
            [0.025, 0.02, 0.018, (0.15 + soot * 0.34).clamp(0.0, 0.55)],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
    }
    if moisture > 0.12 {
        geometry.world_ellipse_ring_with_surface_response(
            [x + 0.05, y - 0.08, z - 0.005],
            [0.32, 0.1],
            [0.66, 0.22],
            18,
            [0.18, 0.32, 0.42, (0.2 + moisture * 0.32).clamp(0.0, 0.58)],
            WINDOW_SURFACE_RESPONSE_WET_ROAD,
        );
    }
}

fn add_window_market_stall_mesh(
    geometry: &mut WindowSceneGeometry,
    instance: WindowProceduralMeshInstance,
) {
    let [x, y] = instance.center_meters;
    let z = instance.z_meters;
    let moisture = instance.moisture.clamp(0.0, 1.0);
    let soot = instance.soot.clamp(0.0, 1.0);
    let corrosion = instance.corrosion.clamp(0.0, 1.0);
    let charge = instance.electrical_charge.clamp(0.0, 1.0);
    let state_detail = window_material_state_detail_from_instance(instance);
    let frame_color = [0.08, 0.072, 0.062, 1.0];
    let tarp_color = [0.18, 0.075, 0.12 + moisture * 0.06, 0.92];

    geometry.world_micro_detailed_box_with_surface_state_detail(
        [x - 0.95, y - 0.42, z + 0.42],
        [x + 0.95, y + 0.38, z + 0.72],
        [0.12, 0.095, 0.072, 1.0],
        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        WindowMicroDetailRecipe {
            seed: 7_200,
            density: 0.36,
        },
        state_detail,
    );
    geometry.world_micro_detailed_box_with_surface_state_detail(
        [x - 0.9, y - 0.46, z + 0.24],
        [x + 0.9, y + 0.42, z + 0.44],
        [0.055, 0.05, 0.046, 1.0],
        WINDOW_SURFACE_RESPONSE_METAL,
        WindowMicroDetailRecipe {
            seed: 7_201,
            density: 0.28,
        },
        state_detail,
    );

    for post_x in [x - 0.82, x + 0.82] {
        for post_y in [y - 0.38, y + 0.34] {
            geometry.world_cylinder_between_with_surface_response(
                [post_x, post_y, z + 0.28],
                [post_x, post_y, z + 1.72],
                0.025,
                8,
                frame_color,
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        }
    }
    geometry.world_oriented_rect_with_surface_response(
        [x, y - 0.05, z + 1.62],
        [1.0, 0.0, 0.0],
        [0.0, 0.32, 0.95],
        [1.06, 0.48],
        tarp_color,
        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
    );
    geometry.world_oriented_rect_with_surface_response(
        [x, y - 0.48, z + 1.06],
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.98, 0.42],
        [0.16, 0.04, 0.08, 0.68],
        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
    );

    geometry.world_cylinder_between_with_surface_response(
        [x - 0.78, y - 0.52, z + 1.45],
        [x + 0.78, y - 0.52, z + 1.45],
        0.026,
        10,
        [0.72, 0.26, 0.48, 0.42],
        WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
    );
    if charge > 0.05 {
        geometry.world_billboard_with_surface_response(
            [x, y - 0.54, z + 1.44],
            instance.billboard_right,
            1.92,
            0.32,
            [0.62, 0.26, 0.42, (0.06 + charge * 0.14).clamp(0.0, 0.24)],
            WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON,
        );
    }

    for (index, goods_offset) in [
        [-0.58, 0.2, 0.0],
        [-0.16, 0.18, 0.04],
        [0.36, 0.21, 0.0],
        [0.64, -0.18, 0.02],
    ]
    .iter()
    .enumerate()
    {
        let goods_center = [
            x + goods_offset[0],
            y + goods_offset[1],
            z + 0.78 + goods_offset[2],
        ];
        if index % 2 == 0 {
            geometry.world_ellipsoid_with_surface_response(
                goods_center,
                [
                    0.21 + index as f32 * 0.018,
                    0.13 + (index % 3) as f32 * 0.018,
                    0.1 + index as f32 * 0.01,
                ],
                5,
                10,
                [0.16, 0.1, 0.052, 0.95],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
            geometry.world_cylinder_between_with_surface_response(
                [
                    goods_center[0] - 0.17,
                    goods_center[1],
                    goods_center[2] + 0.06,
                ],
                [
                    goods_center[0] + 0.17,
                    goods_center[1],
                    goods_center[2] + 0.04,
                ],
                0.012,
                5,
                [0.05, 0.038, 0.026, 0.82],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        } else {
            geometry.world_cylinder_between_with_surface_response(
                [goods_center[0], goods_center[1], goods_center[2] - 0.12],
                [goods_center[0], goods_center[1], goods_center[2] + 0.12],
                0.13,
                10,
                [0.12, 0.074, 0.042, 0.94],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
            geometry.world_ellipse_ring_with_surface_response(
                [goods_center[0], goods_center[1], goods_center[2] + 0.125],
                [0.07, 0.07],
                [0.13, 0.13],
                10,
                [0.05, 0.04, 0.03, 0.84],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        }
    }

    for (produce_x, produce_y, color) in [
        (-0.38, -0.04, [0.28, 0.09, 0.045, 0.9]),
        (-0.22, -0.08, [0.24, 0.16, 0.05, 0.9]),
        (0.08, -0.06, [0.16, 0.22, 0.08, 0.9]),
        (0.24, -0.1, [0.32, 0.14, 0.06, 0.9]),
    ] {
        geometry.world_ellipsoid_with_surface_response(
            [x + produce_x, y + produce_y, z + 0.82],
            [0.07, 0.045, 0.04],
            4,
            8,
            color,
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
    }

    for cable in 0..4 {
        let offset = cable as f32 * 0.26 - 0.39;
        geometry.world_cylinder_between_with_surface_response(
            [x + offset, y - 0.45, z + 1.36],
            [x + offset * 0.84, y - 0.35, z + 0.78],
            0.007,
            5,
            [0.018, 0.016, 0.014, 1.0],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
    }

    if corrosion > 0.04 {
        geometry.world_billboard_with_surface_response(
            [x + 0.62, y - 0.34, z + 0.66],
            instance.billboard_right,
            0.54,
            0.26,
            [0.33, 0.13, 0.04, (0.18 + corrosion * 0.3).clamp(0.0, 0.56)],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
    }
    if soot > 0.04 {
        geometry.world_billboard_with_surface_response(
            [x - 0.28, y - 0.5, z + 1.12],
            instance.billboard_right,
            0.82,
            0.38,
            [0.02, 0.018, 0.016, (0.12 + soot * 0.32).clamp(0.0, 0.5)],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
    }
    if moisture > 0.12 {
        geometry.world_ellipse_ring_with_surface_response(
            [x - 0.18, y - 0.2, z + 0.01],
            [0.42, 0.18],
            [0.84, 0.34],
            22,
            [0.12, 0.24, 0.32, (0.18 + moisture * 0.34).clamp(0.0, 0.62)],
            WINDOW_SURFACE_RESPONSE_WET_ROAD,
        );
    }
}

fn add_window_service_pipe_mesh(
    geometry: &mut WindowSceneGeometry,
    instance: WindowProceduralMeshInstance,
) {
    let [x, y] = instance.center_meters;
    let z = instance.z_meters;
    let pipe_z = z + 0.62;
    let floor_z = 0.045;
    let state_detail = window_material_state_detail_from_instance(instance);
    let metal = [0.11, 0.13, 0.14, 1.0];
    geometry.world_cylinder_between_with_surface_response(
        [x - 0.76, y, pipe_z],
        [x + 0.76, y, pipe_z],
        0.075,
        14,
        metal,
        WINDOW_SURFACE_RESPONSE_METAL,
    );
    for bracket_x in [x - 0.48, x + 0.5] {
        geometry.world_box_with_surface_state_detail(
            [bracket_x - 0.035, y - 0.13, pipe_z - 0.13],
            [bracket_x + 0.035, y + 0.13, pipe_z + 0.13],
            [0.055, 0.06, 0.066, 1.0],
            WINDOW_SURFACE_RESPONSE_METAL,
            state_detail,
        );
    }
    geometry.world_cylinder_between_with_surface_response(
        [x + 0.14, y - 0.005, pipe_z - 0.04],
        [x + 0.14, y + 0.02, floor_z + 0.03],
        0.018,
        8,
        window_with_alpha(window_scale_color(instance.color, 1.18), 0.72),
        WINDOW_SURFACE_RESPONSE_WET_ROAD,
    );
    geometry.world_flat_ellipse_with_surface_response(
        [x + 0.16, y + 0.12, floor_z],
        [0.88, 0.38],
        18,
        window_with_alpha(window_scale_color(instance.color, 0.95), 0.58),
        WINDOW_SURFACE_RESPONSE_WET_ROAD,
    );
    geometry.world_ellipse_ring_with_surface_response(
        [x + 0.16, y + 0.12, floor_z + 0.004],
        [0.36, 0.15],
        [0.62, 0.25],
        20,
        [0.52, 0.82, 1.0, 0.42],
        WINDOW_SURFACE_RESPONSE_WET_ROAD,
    );

    let corrosion = instance.corrosion.clamp(0.0, 1.0);
    if corrosion > 0.04 {
        for offset in [-0.52, -0.08, 0.42] {
            geometry.world_cylinder_between_with_surface_response(
                [x + offset, y - 0.006, pipe_z - 0.078],
                [x + offset + 0.08, y - 0.006, pipe_z + 0.078],
                0.079,
                12,
                [0.34, 0.16, 0.06, (0.32 + corrosion * 0.44).clamp(0.0, 0.82)],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }
    }

    let heat = instance.heat.clamp(0.0, 1.0);
    if heat > 0.06 {
        geometry.world_billboard_with_surface_response(
            [x + 0.12, y + 0.035, pipe_z + 0.04],
            instance.billboard_right,
            0.72,
            0.32,
            [1.0, 0.42, 0.12, (0.12 + heat * 0.28).clamp(0.0, 0.42)],
            WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
        );
    }

    let soot = instance.soot.clamp(0.0, 1.0);
    if soot > 0.04 {
        geometry.world_billboard_with_surface_response(
            [x - 0.2, y + 0.036, pipe_z + 0.16],
            instance.billboard_right,
            0.62,
            0.28,
            [0.03, 0.025, 0.022, (0.16 + soot * 0.3).clamp(0.0, 0.5)],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
    }

    if instance.moisture > 0.25 {
        for drip in 0..5 {
            let drip_x = x - 0.46 + drip as f32 * 0.22;
            let length = 0.12 + (drip % 3) as f32 * 0.045;
            geometry.world_cylinder_between_with_surface_response(
                [drip_x, y + 0.026, pipe_z - 0.1],
                [drip_x + 0.015, y + 0.03, pipe_z - 0.1 - length],
                0.007,
                5,
                [0.54, 0.86, 1.0, 0.58],
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            );
        }
    }
}

fn add_window_wet_asphalt_mesh(
    geometry: &mut WindowSceneGeometry,
    instance: WindowProceduralMeshInstance,
) {
    let z = instance.z_meters;
    let moisture = instance.moisture.clamp(0.0, 1.0);
    let base = window_with_alpha(
        window_scale_color(instance.color, 1.0 + moisture * 0.22),
        0.68,
    );
    geometry.world_quad_with_surface_state_detail(
        [
            [-5.18, -9.8, z + 0.035],
            [5.18, -9.8, z + 0.035],
            [5.18, 13.8, z + 0.035],
            [-5.18, 13.8, z + 0.035],
        ],
        base,
        WINDOW_SURFACE_RESPONSE_WET_ROAD,
        window_material_state_detail_from_instance(instance),
    );

    for (center, outer, inner) in [
        ([-1.2, 0.85, z + 0.042], [1.15, 0.36], [0.78, 0.22]),
        ([2.6, 5.6, z + 0.043], [0.88, 0.28], [0.48, 0.14]),
        ([-3.4, -5.4, z + 0.041], [0.74, 0.22], [0.36, 0.1]),
    ] {
        geometry.world_ellipse_ring_with_surface_response(
            center,
            inner,
            outer,
            18,
            [0.16, 0.22 + moisture * 0.16, 0.28 + moisture * 0.22, 0.38],
            WINDOW_SURFACE_RESPONSE_WET_ROAD,
        );
    }

    add_window_wet_asphalt_micro_detail(geometry, instance, moisture);
}

fn add_window_wet_asphalt_micro_detail(
    geometry: &mut WindowSceneGeometry,
    instance: WindowProceduralMeshInstance,
    moisture: f32,
) {
    let z = instance.z_meters + 0.052;
    for (center, width, angle) in [
        ([-3.6, -8.6, z], 0.72, 0.08),
        ([2.8, -4.2, z], 0.58, -0.32),
        ([-1.7, -0.7, z], 0.84, 0.18),
        ([3.45, 4.2, z], 0.62, 0.42),
        ([-2.85, 9.4, z], 0.76, -0.26),
    ] {
        let angle: f32 = angle;
        geometry.world_oriented_rect_with_surface_response(
            center,
            [angle.cos(), angle.sin(), 0.0],
            [-angle.sin(), angle.cos(), 0.0],
            [width, 0.008],
            [0.018, 0.024, 0.028, 0.9],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
    }

    if moisture > 0.35 {
        for (center, outer, inner, color) in [
            (
                [2.1, -6.8, z + 0.004],
                [0.82, 0.22],
                [0.42, 0.08],
                [0.42, 0.82, 1.0, 0.28],
            ),
            (
                [-3.1, 3.2, z + 0.005],
                [0.66, 0.2],
                [0.3, 0.07],
                [1.0, 0.34, 0.72, 0.18],
            ),
            (
                [0.4, 11.2, z + 0.004],
                [0.94, 0.26],
                [0.44, 0.1],
                [0.14, 0.94, 0.78, 0.2],
            ),
        ] {
            geometry.world_ellipse_ring_with_surface_response(
                center,
                inner,
                outer,
                20,
                window_with_alpha(color, color[3] * (0.62 + moisture * 0.38)),
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            );
        }
    }

    let soot = instance.soot.clamp(0.0, 1.0);
    if soot > 0.04 {
        for center in [[-4.1, -1.8, z + 0.006], [3.9, 7.8, z + 0.006]] {
            geometry.world_flat_ellipse_with_surface_response(
                center,
                [0.52, 0.18],
                18,
                [0.025, 0.02, 0.018, (0.16 + soot * 0.32).clamp(0.0, 0.5)],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }
    }
}

#[derive(BufferContents, Clone, Copy, Debug, PartialEq)]
#[repr(C)]
struct WindowScenePushConstants {
    world_to_clip: [[f32; 4]; 4],
    dynamic_light_position_radius: [f32; 4],
    dynamic_light_color_intensity: [f32; 4],
    atmosphere_color_density: [f32; 4],
    camera_exposure_bloom: [f32; 4],
}

pub trait WindowScene {
    fn update(&mut self, input: &WindowInputState, dt_seconds: f32) -> WindowFrameState;
}

pub fn run_windowed_scene<S>(scene: S, config: WindowedRendererConfig) -> Result<(), Box<dyn Error>>
where
    S: WindowScene + 'static,
{
    let event_loop = create_event_loop()?;
    let mut app = WindowedVulkanApp::new(scene, config);
    event_loop.run_app(&mut app)?;
    Ok(())
}

fn create_event_loop() -> Result<EventLoop<()>, EventLoopError> {
    let mut builder = EventLoop::builder();
    #[cfg(target_os = "windows")]
    builder.with_any_thread(true);
    builder.build()
}

struct WindowedVulkanApp<S> {
    scene: S,
    config: WindowedRendererConfig,
    context: VulkanoContext,
    windows: VulkanoWindows,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    render_pass: Option<Arc<RenderPass>>,
    pipeline: Option<Arc<GraphicsPipeline>>,
    framebuffers: Vec<Arc<Framebuffer>>,
    input: WindowInputState,
    last_frame: Instant,
}

impl<S> WindowedVulkanApp<S> {
    fn new(scene: S, config: WindowedRendererConfig) -> Self {
        let vulkano_config = VulkanoConfig {
            print_device_name: config.print_device_name,
            ..Default::default()
        };
        let context = VulkanoContext::new(vulkano_config);
        let command_buffer_allocator = Arc::new(StandardCommandBufferAllocator::new(
            context.device().clone(),
            Default::default(),
        ));

        Self {
            scene,
            config,
            context,
            windows: VulkanoWindows::default(),
            command_buffer_allocator,
            render_pass: None,
            pipeline: None,
            framebuffers: Vec::new(),
            input: WindowInputState::default(),
            last_frame: Instant::now(),
        }
    }

    fn initialize_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        if self.windows.primary_window_id().is_some() {
            return Ok(());
        }

        let descriptor = WindowDescriptor {
            title: self.config.title.clone(),
            width: self.config.width,
            height: self.config.height,
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
            .expect("primary renderer should exist after window creation");
        let render_pass =
            create_clear_render_pass(self.context.device().clone(), renderer.swapchain_format())?;
        self.framebuffers = build_framebuffers(
            render_pass.clone(),
            renderer.swapchain_image_views(),
            self.context.memory_allocator().clone(),
            renderer.swapchain_image_size(),
        )?;
        self.pipeline = Some(create_scene_pipeline(
            self.context.device().clone(),
            render_pass.clone(),
            renderer.swapchain_image_size(),
        )?);
        self.render_pass = Some(render_pass);
        Ok(())
    }

    fn render(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn Error>>
    where
        S: WindowScene,
    {
        let now = Instant::now();
        let dt_seconds = (now - self.last_frame).as_secs_f32().clamp(0.0, 0.1);
        self.last_frame = now;

        if let Some(window) = self.windows.get_primary_window() {
            let size = window.inner_size();
            self.input.viewport_size_pixels = [size.width as f32, size.height as f32];
            if size.width == 0 || size.height == 0 {
                self.input.reset_frame_delta();
                return Ok(());
            }
        }

        let frame = self.scene.update(&self.input, dt_seconds);
        self.input.reset_frame_delta();

        if let Some(window) = self.windows.get_primary_window()
            && let Some(title) = &frame.title
        {
            window.set_title(title);
        }

        let render_pass = self
            .render_pass
            .as_ref()
            .expect("render pass should exist before redraw")
            .clone();

        let (before_future, image_index, graphics_queue, swapchain_recreated) = {
            let renderer = self
                .windows
                .get_primary_renderer_mut()
                .expect("primary renderer should exist before redraw");
            let mut swapchain_recreated = false;
            let before_future = match renderer.acquire(None, |_| {
                swapchain_recreated = true;
            }) {
                Ok(future) => future,
                Err(VulkanError::OutOfDate) => return Ok(()),
                Err(error) => return Err(Box::new(error)),
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
                .expect("primary renderer should exist after swapchain recreation");
            self.framebuffers = build_framebuffers(
                render_pass.clone(),
                renderer.swapchain_image_views(),
                self.context.memory_allocator().clone(),
                renderer.swapchain_image_size(),
            )?;
            self.pipeline = Some(create_scene_pipeline(
                self.context.device().clone(),
                render_pass.clone(),
                renderer.swapchain_image_size(),
            )?);
        }

        let Some(framebuffer) = self.framebuffers.get(image_index).cloned() else {
            return Ok(());
        };
        let vertex_buffer = self.build_vertex_buffer(&frame.vertices)?;
        let vertex_count = vertex_buffer
            .as_ref()
            .map(|vertices| vertices.len() as u32)
            .unwrap_or(0);
        let index_buffer = self.build_index_buffer(&frame.indices)?;
        let index_count = index_buffer
            .as_ref()
            .map(|indices| indices.len() as u32)
            .unwrap_or(0);

        let mut builder = AutoCommandBufferBuilder::primary(
            self.command_buffer_allocator.clone(),
            graphics_queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )?;
        builder.begin_render_pass(
            RenderPassBeginInfo {
                clear_values: vec![Some(frame.clear_color.into()), Some(1.0f32.into())],
                ..RenderPassBeginInfo::framebuffer(framebuffer)
            },
            Default::default(),
        )?;

        if let (Some(pipeline), Some(vertex_buffer)) = (self.pipeline.clone(), vertex_buffer) {
            let push_constants = WindowScenePushConstants {
                world_to_clip: frame.world_to_clip,
                dynamic_light_position_radius: frame.dynamic_light.position_radius(),
                dynamic_light_color_intensity: frame.dynamic_light.color_intensity(),
                atmosphere_color_density: frame.atmosphere.color_density(),
                camera_exposure_bloom: frame.atmosphere.exposure_bloom(),
            };
            builder
                .bind_pipeline_graphics(pipeline.clone())?
                .push_constants(pipeline.layout().clone(), 0, push_constants)?
                .bind_vertex_buffers(0, vertex_buffer)?;

            if let Some(index_buffer) = index_buffer {
                builder.bind_index_buffer(index_buffer)?;
                unsafe {
                    builder.draw_indexed(index_count, 1, 0, 0, 0)?;
                }
            } else {
                unsafe {
                    builder.draw(vertex_count, 1, 0, 0)?;
                }
            }
        }

        builder.end_render_pass(Default::default())?;

        let command_buffer = builder.build()?;
        let future = before_future
            .then_execute(graphics_queue, command_buffer)?
            .boxed();

        let renderer = self
            .windows
            .get_primary_renderer_mut()
            .expect("primary renderer should exist before present");
        renderer.present(future, true);

        if self.windows.primary_window_id().is_none() {
            event_loop.exit();
        }

        Ok(())
    }

    fn set_key_state(&mut self, key: KeyCode, state: ElementState) {
        let pressed = state == ElementState::Pressed;
        match key {
            KeyCode::KeyW => self.input.move_forward = pressed,
            KeyCode::KeyS => self.input.move_backward = pressed,
            KeyCode::KeyA => self.input.move_left = pressed,
            KeyCode::KeyD => self.input.move_right = pressed,
            KeyCode::KeyR | KeyCode::Space => self.input.move_up = pressed,
            KeyCode::KeyF | KeyCode::ControlLeft | KeyCode::ControlRight => {
                self.input.move_down = pressed;
            }
            KeyCode::KeyQ | KeyCode::ArrowLeft => self.input.turn_left = pressed,
            KeyCode::KeyE | KeyCode::ArrowRight => self.input.turn_right = pressed,
            KeyCode::KeyT | KeyCode::ArrowUp => self.input.turn_up = pressed,
            KeyCode::KeyG | KeyCode::ArrowDown => self.input.turn_down = pressed,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => self.input.fast_modifier = pressed,
            KeyCode::Enter | KeyCode::KeyB if pressed => self.input.action_primary = true,
            _ => {}
        }
    }

    fn build_vertex_buffer(
        &self,
        vertices: &[WindowSceneVertex],
    ) -> Result<Option<Subbuffer<[WindowSceneVertex]>>, Box<dyn Error>> {
        if vertices.is_empty() {
            return Ok(None);
        }

        let buffer = Buffer::from_iter(
            self.context.memory_allocator().clone(),
            BufferCreateInfo {
                usage: BufferUsage::VERTEX_BUFFER,
                ..Default::default()
            },
            AllocationCreateInfo {
                memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                    | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                ..Default::default()
            },
            vertices.iter().copied(),
        )?;
        Ok(Some(buffer))
    }

    fn build_index_buffer(
        &self,
        indices: &[u32],
    ) -> Result<Option<Subbuffer<[u32]>>, Box<dyn Error>> {
        if indices.is_empty() {
            return Ok(None);
        }

        let buffer = Buffer::from_iter(
            self.context.memory_allocator().clone(),
            BufferCreateInfo {
                usage: BufferUsage::INDEX_BUFFER,
                ..Default::default()
            },
            AllocationCreateInfo {
                memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                    | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                ..Default::default()
            },
            indices.iter().copied(),
        )?;
        Ok(Some(buffer))
    }
}

impl<S> ApplicationHandler for WindowedVulkanApp<S>
where
    S: WindowScene,
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.initialize_window(event_loop) {
            eprintln!("failed to initialize Ashfall Vulkan window: {error}");
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
                    configure_game_cursor(window, focused);
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
                if let Some(window) = self.windows.get_primary_window() {
                    configure_game_cursor(window, true);
                }
                self.input.action_primary = true;
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.render(event_loop) {
                    eprintln!("Ashfall render error: {error}");
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
        if let DeviceEvent::MouseMotion { delta } = event {
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

fn create_clear_render_pass(
    device: Arc<vulkano::device::Device>,
    format: Format,
) -> Result<Arc<RenderPass>, Box<dyn Error>> {
    let render_pass = vulkano::single_pass_renderpass!(
        device,
        attachments: {
            color: {
                format: format,
                samples: 1,
                load_op: Clear,
                store_op: Store,
            },
            depth: {
                format: Format::D16_UNORM,
                samples: 1,
                load_op: Clear,
                store_op: DontCare,
            },
        },
        pass: {
            color: [color],
            depth_stencil: {depth},
        },
    )?;
    Ok(render_pass)
}

fn create_scene_pipeline(
    device: Arc<vulkano::device::Device>,
    render_pass: Arc<RenderPass>,
    image_extent: [u32; 2],
) -> Result<Arc<GraphicsPipeline>, Box<dyn Error>> {
    let vs = vs::load(device.clone())?
        .entry_point("main")
        .ok_or_else(|| std::io::Error::other("missing vertex shader entry point"))?;
    let fs = fs::load(device.clone())?
        .entry_point("main")
        .ok_or_else(|| std::io::Error::other("missing fragment shader entry point"))?;
    let stages = [
        PipelineShaderStageCreateInfo::new(vs.clone()),
        PipelineShaderStageCreateInfo::new(fs),
    ];
    let layout_create_info = PipelineDescriptorSetLayoutCreateInfo::from_stages(&stages)
        .into_pipeline_layout_create_info(device.clone())?;
    let layout = PipelineLayout::new(device.clone(), layout_create_info)?;
    let subpass = vulkano::render_pass::Subpass::from(render_pass, 0)
        .ok_or_else(|| std::io::Error::other("missing render subpass 0"))?;
    let vertex_input_state = WindowSceneVertex::per_vertex().definition(&vs)?;
    let width = image_extent[0].max(1) as f32;
    let height = image_extent[1].max(1) as f32;
    let mut viewport_state = ViewportState::default();
    viewport_state.viewports[0] = Viewport {
        offset: [0.0, 0.0],
        extent: [width, height],
        depth_range: 0.0..=1.0,
    };

    let pipeline = GraphicsPipeline::new(
        device,
        None,
        GraphicsPipelineCreateInfo {
            stages: stages.into_iter().collect(),
            vertex_input_state: Some(vertex_input_state),
            input_assembly_state: Some(InputAssemblyState::default()),
            viewport_state: Some(viewport_state),
            rasterization_state: Some(RasterizationState::default()),
            multisample_state: Some(MultisampleState::default()),
            depth_stencil_state: Some(DepthStencilState {
                depth: Some(DepthState {
                    compare_op: CompareOp::LessOrEqual,
                    write_enable: true,
                }),
                ..Default::default()
            }),
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
    )?;

    Ok(pipeline)
}

fn build_framebuffers(
    render_pass: Arc<RenderPass>,
    image_views: &[Arc<ImageView>],
    memory_allocator: Arc<StandardMemoryAllocator>,
    image_extent: [u32; 2],
) -> Result<Vec<Arc<Framebuffer>>, Box<dyn Error>> {
    image_views
        .iter()
        .map(|image_view| {
            let depth_view = create_depth_image_view(memory_allocator.clone(), image_extent)?;
            Ok(Framebuffer::new(
                render_pass.clone(),
                FramebufferCreateInfo {
                    attachments: vec![image_view.clone(), depth_view],
                    ..Default::default()
                },
            )?)
        })
        .collect()
}

fn create_depth_image_view(
    memory_allocator: Arc<StandardMemoryAllocator>,
    image_extent: [u32; 2],
) -> Result<Arc<ImageView>, Box<dyn Error>> {
    let depth_image = Image::new(
        memory_allocator,
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: Format::D16_UNORM,
            extent: [image_extent[0].max(1), image_extent[1].max(1), 1],
            usage: ImageUsage::DEPTH_STENCIL_ATTACHMENT,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        },
    )?;
    Ok(ImageView::new_default(depth_image)?)
}

mod vs {
    vulkano_shaders::shader! {
        ty: "vertex",
        src: r#"
            #version 450

            layout(location = 0) in vec3 position;
            layout(location = 1) in vec3 normal;
            layout(location = 2) in vec4 color;
            layout(location = 3) in uint coordinate_space;
            layout(location = 4) in vec4 surface_response;
            layout(location = 5) in vec4 material_detail;
            layout(location = 6) in vec4 generated_cache;
            layout(location = 0) out vec4 out_color;
            layout(location = 1) out vec3 out_world_position;
            layout(location = 2) out vec3 out_world_normal;
            layout(location = 3) flat out uint out_coordinate_space;
            layout(location = 4) out vec4 out_surface_response;
            layout(location = 5) flat out vec4 out_material_detail;
            layout(location = 6) flat out vec4 out_generated_cache;
            layout(location = 7) out float out_view_depth;

            layout(push_constant) uniform FramePushConstants {
                mat4 world_to_clip;
                vec4 dynamic_light_position_radius;
                vec4 dynamic_light_color_intensity;
                vec4 atmosphere_color_density;
                vec4 camera_exposure_bloom;
            } frame;

            void main() {
                out_coordinate_space = coordinate_space;
                out_surface_response = surface_response;
                out_material_detail = material_detail;
                out_generated_cache = generated_cache;
                if (coordinate_space == 0u) {
                    vec3 normal_dir = normalize(normal);
                    out_color = color;
                    out_world_position = position;
                    out_world_normal = normal_dir;
                    gl_Position = frame.world_to_clip * vec4(position, 1.0);
                    out_view_depth = gl_Position.w;
                } else {
                    out_color = color;
                    out_world_position = vec3(0.0);
                    out_world_normal = vec3(0.0, 0.0, 1.0);
                    out_view_depth = 0.0;
                    gl_Position = vec4(position.x, -position.y, position.z, 1.0);
                }
            }
        "#,
    }
}

mod fs {
    vulkano_shaders::shader! {
        ty: "fragment",
        src: r#"
            #version 450

            layout(location = 0) in vec4 in_color;
            layout(location = 1) in vec3 in_world_position;
            layout(location = 2) in vec3 in_world_normal;
            layout(location = 3) flat in uint in_coordinate_space;
            layout(location = 4) in vec4 in_surface_response;
            layout(location = 5) flat in vec4 in_material_detail;
            layout(location = 6) flat in vec4 in_generated_cache;
            layout(location = 7) in float in_view_depth;
            layout(location = 0) out vec4 out_color;

            layout(push_constant) uniform FramePushConstants {
                mat4 world_to_clip;
                vec4 dynamic_light_position_radius;
                vec4 dynamic_light_color_intensity;
                vec4 atmosphere_color_density;
                vec4 camera_exposure_bloom;
            } frame;

            float hash12(vec2 value) {
                vec3 p3 = fract(vec3(value.xyx) * 0.1031);
                p3 += dot(p3, p3.yzx + 33.33);
                return fract((p3.x + p3.y) * p3.z);
            }

            float value_noise(vec2 value) {
                vec2 cell = floor(value);
                vec2 local = fract(value);
                vec2 smooth_local = local * local * (3.0 - 2.0 * local);
                float a = hash12(cell);
                float b = hash12(cell + vec2(1.0, 0.0));
                float c = hash12(cell + vec2(0.0, 1.0));
                float d = hash12(cell + vec2(1.0, 1.0));
                return mix(mix(a, b, smooth_local.x), mix(c, d, smooth_local.x), smooth_local.y);
            }

            float fbm_noise(vec2 value) {
                float sum = 0.0;
                float amplitude = 0.5;
                sum += value_noise(value) * amplitude;
                value = value * mat2(1.62, 1.11, -1.11, 1.62) + vec2(7.1, 3.4);
                amplitude *= 0.5;
                sum += value_noise(value) * amplitude;
                value = value * mat2(1.47, -0.83, 0.83, 1.47) + vec2(2.6, 11.9);
                amplitude *= 0.5;
                sum += value_noise(value) * amplitude;
                return sum / 0.875;
            }

            float ridged_noise(vec2 value) {
                float ridge = 1.0 - abs(value_noise(value) * 2.0 - 1.0);
                float fine = 1.0 - abs(value_noise(value * 2.37 + vec2(5.8, 1.7)) * 2.0 - 1.0);
                return clamp(ridge * 0.68 + fine * 0.32, 0.0, 1.0);
            }

            float detail_lod_weight(float view_depth) {
                return clamp(1.0 - smoothstep(9.0, 24.0, max(view_depth, 0.0)), 0.0, 1.0);
            }

            vec2 warped_surface_uv(vec2 uv, float seed, float cache_page, float view_detail) {
                vec2 warp = vec2(
                    fbm_noise(uv * 0.74 + vec2(seed * 0.07, cache_page * 0.011)),
                    fbm_noise(uv.yx * 0.68 + vec2(3.1 + cache_page * 0.017, seed * 0.09))
                ) - vec2(0.5);
                return uv + warp * (0.008 + view_detail * 0.014);
            }

            vec2 surface_uv(vec3 position, vec3 normal_dir) {
                vec3 axis = abs(normal_dir);
                if (axis.z >= axis.x && axis.z >= axis.y) {
                    return position.xy;
                }
                if (axis.x >= axis.y) {
                    return position.yz;
                }
                return position.xz;
            }

            float color_chroma(vec3 color) {
                float highest = max(max(color.r, color.g), color.b);
                float lowest = min(min(color.r, color.g), color.b);
                return highest - lowest;
            }

            vec3 procedural_surface_normal(
                vec3 normal_dir,
                vec3 position,
                vec4 surface_response,
                vec4 material_detail,
                vec4 generated_cache,
                float view_depth
            ) {
                float roughness = clamp(surface_response.x, 0.04, 1.0);
                float emissive = clamp(surface_response.w, 0.0, 1.0);
                float detail_scale = clamp(material_detail.x, 0.05, 8.0);
                float detail_seed = material_detail.y * 173.0;
                float state_intensity = clamp(material_detail.z, 0.0, 1.0);
                float relief = clamp(material_detail.w, 0.0, 1.0);
                float cache_normal = clamp(generated_cache.y, 0.0, 1.0);
                float cache_page = generated_cache.w * 4096.0;
                float view_detail = detail_lod_weight(view_depth);
                float amplitude = (0.012 + roughness * 0.042 + relief * 0.048)
                    * (0.58 + state_intensity * 0.72)
                    * (1.0 - emissive * 0.72)
                    * (0.52 + view_detail * 0.48)
                    * (0.82 + cache_normal * 0.5);
                if (amplitude <= 0.002) {
                    return normal_dir;
                }

                vec2 uv = warped_surface_uv(surface_uv(position, normal_dir), detail_seed, cache_page, view_detail);
                vec3 tangent = normalize(abs(normal_dir.z) < 0.92 ? cross(vec3(0.0, 0.0, 1.0), normal_dir) : vec3(1.0, 0.0, 0.0));
                vec3 bitangent = normalize(cross(normal_dir, tangent));
                float broad = fbm_noise(uv * 3.4 * detail_scale + vec2(detail_seed + cache_page * 0.013, position.z * 0.31));
                float n0 = fbm_noise(uv * 18.0 * detail_scale + vec2(detail_seed + cache_page * 0.019, position.z * 0.31));
                float n1 = ridged_noise(uv.yx * 24.0 * detail_scale + vec2(4.7 + detail_seed, 1.3 + cache_page * 0.017));
                float cache_grain = view_detail > 0.04
                    ? value_noise(uv * 76.0 * detail_scale + vec2(cache_page * 0.031, cache_page * 0.047))
                    : 0.5;
                return normalize(
                    normal_dir
                    + tangent * ((broad - 0.5) * amplitude * 0.16 + (n0 - 0.5) * amplitude * view_detail * 0.72 + (cache_grain - 0.5) * amplitude * cache_normal * view_detail * 0.46)
                    + bitangent * ((n1 - 0.5) * amplitude * (0.26 + view_detail * 0.48) + (cache_grain - 0.5) * amplitude * cache_normal * view_detail * 0.28)
                );
            }

            vec3 procedural_surface_albedo(
                vec3 base_color,
                vec3 position,
                vec3 normal_dir,
                vec4 surface_response,
                vec4 material_detail,
                vec4 generated_cache,
                float view_depth
            ) {
                float roughness = clamp(surface_response.x, 0.04, 1.0);
                float metallic = clamp(surface_response.y, 0.0, 1.0);
                float wetness = clamp(surface_response.z, 0.0, 1.0);
                float emissive = clamp(surface_response.w, 0.0, 1.0);
                float detail_scale = clamp(material_detail.x, 0.05, 8.0);
                float detail_seed = material_detail.y * 173.0;
                float state_intensity = clamp(material_detail.z, 0.0, 1.0);
                float relief = clamp(material_detail.w, 0.0, 1.0);
                float cache_albedo = clamp(generated_cache.x, 0.0, 1.0);
                float cache_height = clamp(generated_cache.y, 0.0, 1.0);
                float cache_mask = clamp(generated_cache.z, 0.0, 1.0);
                float cache_page = generated_cache.w * 4096.0;
                float view_detail = detail_lod_weight(view_depth);
                vec2 uv = warped_surface_uv(surface_uv(position, normal_dir), detail_seed, cache_page, view_detail);

                float low = fbm_noise(uv * 1.10 * detail_scale + vec2(detail_seed));
                float mid = fbm_noise(uv * 6.8 * detail_scale + vec2(1.7 + detail_seed, 6.1));
                float high = view_detail > 0.04
                    ? fbm_noise(uv * 34.0 * detail_scale + vec2(position.z * 0.19, detail_seed))
                    : 0.5;
                vec3 color = base_color * (0.90 + (low - 0.5) * 0.08 + (mid - 0.5) * (0.08 + state_intensity * 0.06));
                float cache_tile = fbm_noise(uv * (2.0 + detail_scale * 0.4) + vec2(cache_page * 0.011, cache_page * 0.023));
                float cache_grain = view_detail > 0.04
                    ? value_noise(uv * (70.0 + detail_scale * 18.0) + vec2(cache_page * 0.071, detail_seed))
                    : cache_tile;
                color *= 0.985 + (cache_tile - 0.5) * cache_albedo * 0.10 + (cache_grain - 0.5) * cache_albedo * 0.18;
                float chroma = color_chroma(base_color);
                float green_bias = clamp(base_color.g - max(base_color.r, base_color.b), 0.0, 1.0);
                float plant_like = smoothstep(0.035, 0.18, green_bias)
                    * smoothstep(0.08, 0.32, wetness)
                    * (1.0 - metallic)
                    * (1.0 - emissive);
                float neutral_surface = 1.0 - smoothstep(0.045, 0.22, chroma);
                float stone_like = smoothstep(0.55, 0.88, roughness)
                    * smoothstep(0.015, 0.16, wetness)
                    * neutral_surface
                    * (1.0 - metallic)
                    * (1.0 - emissive)
                    * (1.0 - plant_like);
                float asphalt_like = smoothstep(0.45, 0.92, wetness)
                    * smoothstep(0.08, 0.34, roughness)
                    * (1.0 - metallic)
                    * (1.0 - emissive)
                    * (1.0 - plant_like);
                float soil_like = smoothstep(0.64, 0.96, roughness)
                    * (1.0 - smoothstep(0.35, 0.82, wetness))
                    * (1.0 - metallic)
                    * (1.0 - emissive)
                    * (1.0 - plant_like)
                    * (1.0 - stone_like * 0.62);
                float aggregate = smoothstep(0.42, 0.92, cache_grain) * roughness * (1.0 - metallic) * (1.0 - emissive) * view_detail;
                color = mix(color, color * vec3(0.68, 0.69, 0.65) + vec3(0.030, 0.028, 0.024), aggregate * (0.10 + wetness * 0.10));
                float light_aggregate = smoothstep(0.78, 0.98, value_noise(uv * (118.0 + detail_scale * 20.0) + vec2(detail_seed, cache_page * 0.013)));
                color += vec3(0.035, 0.034, 0.030) * light_aggregate * roughness * (1.0 - emissive) * view_detail * 0.28;
                float cache_height_shadow = smoothstep(0.58, 0.92, cache_grain) * cache_height * roughness * (1.0 - emissive);
                color = mix(color, color * vec3(0.70, 0.72, 0.68), cache_height_shadow * 0.14);
                float cache_crack = ridged_noise(vec2(uv.x * 13.0 * detail_scale + cache_page * 0.031, uv.y * 1.35 + detail_seed));
                float generated_mask = smoothstep(0.74, 0.98, cache_crack)
                    * cache_mask
                    * (roughness * 0.72 + wetness * 0.36 + metallic * 0.26)
                    * (1.0 - emissive);
                color = mix(color, color * vec3(0.52, 0.55, 0.5), generated_mask * 0.28);

                float gravel = smoothstep(0.72, 0.98, value_noise(uv * (96.0 + detail_scale * 22.0) + vec2(cache_page * 0.083, detail_seed * 1.7))) * view_detail;
                float tar_crack = smoothstep(0.80, 0.985, ridged_noise(vec2(uv.x * (9.0 + detail_scale * 3.0), uv.y * 1.45 + detail_seed)));
                vec3 asphalt_fleck = mix(vec3(0.015, 0.015, 0.014), vec3(0.18, 0.17, 0.15), gravel);
                color = mix(color, color * vec3(0.72, 0.76, 0.78) + asphalt_fleck, asphalt_like * gravel * 0.30);
                color = mix(color, color * vec3(0.28, 0.30, 0.28), asphalt_like * tar_crack * 0.24);

                float soil_clump = fbm_noise(uv * (18.0 + detail_scale * 4.0) + vec2(detail_seed * 0.31, cache_page * 0.029));
                float pebble = smoothstep(0.78, 0.99, value_noise(uv * (58.0 + detail_scale * 10.0) + vec2(cache_page * 0.057, detail_seed * 2.1)));
                vec3 soil_color = vec3(0.16, 0.095, 0.045) * (0.72 + soil_clump * 0.62);
                color = mix(color, color * vec3(0.70, 0.61, 0.48) + soil_color, soil_like * (0.24 + soil_clump * 0.22));
                color = mix(color, color + vec3(0.10, 0.085, 0.060) * pebble, soil_like * pebble * 0.28 * view_detail);

                float leaf_wave = abs(sin((uv.x * 18.0 + uv.y * 7.0) * max(detail_scale, 0.7) + detail_seed));
                float leaf_vein = 1.0 - smoothstep(0.04, 0.18, min(leaf_wave, 1.0 - leaf_wave));
                float leaf_mottle = fbm_noise(uv * 32.0 + vec2(detail_seed, cache_page * 0.041));
                color = mix(color, color * vec3(0.58, 0.86, 0.48) + vec3(0.012, 0.040, 0.010), plant_like * (0.16 + leaf_mottle * 0.20));
                color += vec3(0.035, 0.070, 0.024) * plant_like * leaf_vein * view_detail * 0.22;

                float strata = abs(sin((uv.y * 6.5 + uv.x * 1.7) * max(detail_scale, 0.45) + detail_seed * 0.7));
                float stone_pit = smoothstep(0.70, 0.98, value_noise(uv * (44.0 + detail_scale * 8.0) + vec2(cache_page * 0.037, detail_seed)));
                color = mix(color, color * vec3(0.74, 0.74, 0.70) + vec3(0.035, 0.033, 0.030), stone_like * strata * 0.22);
                color = mix(color, color * vec3(0.56, 0.57, 0.54), stone_like * stone_pit * 0.18 * view_detail);

                float pore_mask = smoothstep(0.64, 0.97, high) * roughness * (0.55 + relief * 0.65) * (1.0 - emissive) * view_detail;
                color = mix(color, color * vec3(0.64, 0.66, 0.62), pore_mask * 0.24);

                float grime = smoothstep(0.72, 0.97, mid) * roughness * (0.42 + state_intensity * 0.70) * (1.0 - emissive);
                color = mix(color, color * vec3(0.58, 0.61, 0.56), grime * 0.18);

                float scratch_noise = ridged_noise(vec2(uv.x * 24.0 * detail_scale + uv.y * 2.0, uv.y * 3.5 + detail_seed));
                float brushed = smoothstep(0.82, 0.99, ridged_noise(vec2(uv.x * 72.0, uv.y * 6.0 + detail_seed)));
                float scratch_mask = smoothstep(0.82, 0.99, scratch_noise) * (metallic * 0.9 + roughness * 0.25 + relief * 0.18) * (1.0 - emissive) * view_detail;
                color += vec3(0.11, 0.12, 0.12) * scratch_mask;
                color += vec3(0.055, 0.060, 0.060) * brushed * metallic * (1.0 - emissive) * view_detail * 0.32;

                float rust_noise = fbm_noise(uv * 7.5 * detail_scale + vec2(9.2 + detail_seed, 2.4));
                float rust_mask = smoothstep(0.54, 0.92, rust_noise) * metallic * state_intensity * (1.0 - wetness * 0.42) * (1.0 - emissive);
                vec3 rust_color = color * vec3(0.68, 0.42, 0.24) + vec3(0.12, 0.035, 0.008);
                color = mix(color, rust_color, rust_mask * (0.26 + relief * 0.28));

                float soot_noise = fbm_noise(uv * 13.0 * detail_scale + vec2(3.3, 11.7 + detail_seed));
                float soot_mask = smoothstep(0.7, 0.98, soot_noise) * roughness * state_intensity * (1.0 - emissive);
                color = mix(color, color * vec3(0.34, 0.32, 0.3), soot_mask * 0.22);

                float hot_metal = metallic * smoothstep(0.48, 0.98, state_intensity) * (1.0 - wetness) * (1.0 - emissive);
                color += vec3(0.08, 0.026, 0.004) * hot_metal;

                float fx = fract(uv.x * 0.48 * max(detail_scale, 0.35) + detail_seed * 0.07);
                float fy = fract(uv.y * 0.48 * max(detail_scale, 0.35) + detail_seed * 0.11);
                float seam_breakup = smoothstep(0.18, 0.86, fbm_noise(uv * 1.9 + vec2(cache_page * 0.041, detail_seed)));
                float grid_x = 1.0 - smoothstep(0.01, 0.052, min(fx, 1.0 - fx));
                float grid_y = 1.0 - smoothstep(0.01, 0.052, min(fy, 1.0 - fy));
                float panel_mask = max(grid_x, grid_y) * roughness * (1.0 - emissive) * 0.055 * seam_breakup;
                color = mix(color, color * vec3(0.72, 0.74, 0.72), panel_mask);

                float vertical_surface = clamp(1.0 - abs(normal_dir.z), 0.0, 1.0);
                float streak_noise = ridged_noise(vec2(uv.x * 1.2 * detail_scale + detail_seed, uv.y * 8.2 + position.z * 1.3));
                float streak_mask = smoothstep(0.74, 0.98, streak_noise) * vertical_surface * (wetness + roughness * 0.32) * (1.0 - emissive);
                color = mix(color, color * vec3(0.54, 0.6, 0.62) + vec3(0.02, 0.04, 0.045), streak_mask * 0.22);

                float ground_contact = smoothstep(0.72, 0.02, position.z) * roughness * (1.0 - emissive);
                float packed_dirt = fbm_noise(uv * 9.5 + vec2(detail_seed, cache_page * 0.027));
                vec3 earth = vec3(0.18, 0.12, 0.064) * (0.6 + packed_dirt * 0.54);
                color = mix(color, color * vec3(0.72, 0.66, 0.54) + earth, ground_contact * smoothstep(0.42, 0.9, packed_dirt) * 0.2);

                color = mix(color, color * vec3(0.82, 0.86, 0.9) + vec3(0.018, 0.035, 0.045), wetness * 0.10 * (1.0 - emissive));
                return max(color, vec3(0.0));
            }

            vec3 point_light(
                vec3 position,
                vec3 normal_dir,
                vec3 light_position,
                vec3 light_color,
                float radius,
                float intensity,
                float wetness
            ) {
                vec3 to_light = light_position - position;
                float distance_to_light = distance(position, light_position);
                float attenuation = max(1.0 - distance_to_light / radius, 0.0);
                attenuation *= attenuation;
                float facing = 0.34 + max(dot(normal_dir, normalize(to_light)), 0.0) * 0.66;
                float wet_reflect = 1.0 + wetness * 1.45;
                return light_color * attenuation * intensity * facing * wet_reflect;
            }

            vec3 filmic_tone_map(vec3 color, float exposure) {
                vec3 mapped = vec3(1.0) - exp(-max(color, vec3(0.0)) * exposure);
                return pow(clamp(mapped, vec3(0.0), vec3(1.0)), vec3(1.0 / 2.2));
            }

            void main() {
                if (in_coordinate_space != 0u) {
                    out_color = in_color;
                    return;
                }

                vec3 normal_dir = procedural_surface_normal(
                    normalize(in_world_normal),
                    in_world_position,
                    in_surface_response,
                    in_material_detail,
                    in_generated_cache,
                    in_view_depth
                );
                float roughness = clamp(in_surface_response.x, 0.04, 1.0);
                float metallic = clamp(in_surface_response.y, 0.0, 1.0);
                float wetness = clamp(in_surface_response.z, 0.0, 1.0);
                float emissive = clamp(in_surface_response.w, 0.0, 1.0);
                vec3 surface_color = procedural_surface_albedo(
                    in_color.rgb,
                    in_world_position,
                    normal_dir,
                    in_surface_response,
                    in_material_detail,
                    in_generated_cache,
                    in_view_depth
                );

                vec3 key_dir = normalize(vec3(-0.34, -0.42, 0.84));
                float direct = max(dot(normal_dir, key_dir), 0.0);
                float back = max(dot(-normal_dir, key_dir), 0.0) * 0.2;
                float diffuse = 0.34 + max(direct, back) * mix(0.88, 0.56, metallic);
                vec3 sun_warmth = vec3(1.0, 0.88, 0.68) * direct * 0.28;
                vec3 sky_fill = vec3(0.50, 0.57, 0.64) * (0.34 + max(normal_dir.z, 0.0) * 0.36);
                vec3 lit = surface_color * diffuse + surface_color * sky_fill + surface_color * sun_warmth;

                float spec_power = mix(96.0, 18.0, roughness);
                float spec_amount = (1.0 - roughness) * (0.16 + wetness * 0.72 + metallic * 0.42);
                float key_spec = pow(max(dot(normal_dir, key_dir), 0.0), spec_power) * spec_amount;
                vec3 spec_color = mix(vec3(0.82, 0.9, 1.0), surface_color, metallic);
                lit += spec_color * key_spec;

                vec3 magenta = point_light(
                    in_world_position,
                    normal_dir,
                    vec3(1.0, 3.0, 2.45),
                    vec3(0.48, 0.18, 0.34),
                    8.5,
                    0.035,
                    wetness
                );
                vec3 cyan = point_light(
                    in_world_position,
                    normal_dir,
                    vec3(-1.5, 0.5, 0.72),
                    vec3(0.22, 0.42, 0.5),
                    5.2,
                    0.030,
                    wetness
                );
                vec3 amber = point_light(
                    in_world_position,
                    normal_dir,
                    vec3(-4.95, -6.9, 1.45),
                    vec3(0.9, 0.55, 0.24),
                    4.8,
                    0.10,
                    wetness
                );
                float upward_surface = smoothstep(0.38, 0.92, normal_dir.z);
                float wet_ground = upward_surface * (1.0 - smoothstep(0.03, 0.22, abs(in_world_position.z)));
                lit += magenta * (0.012 + wet_ground * 0.034);
                lit += cyan * (0.010 + wet_ground * 0.028);
                lit += amber * 0.13;
                if (frame.dynamic_light_position_radius.w > 0.01 && frame.dynamic_light_color_intensity.w > 0.01) {
                    vec3 dynamic = point_light(
                        in_world_position,
                        normal_dir,
                        frame.dynamic_light_position_radius.xyz,
                        frame.dynamic_light_color_intensity.rgb,
                        frame.dynamic_light_position_radius.w,
                        frame.dynamic_light_color_intensity.w,
                        wetness
                    );
                    lit += dynamic * (0.12 + wet_ground * 0.22);
                }
                lit += surface_color * emissive * (0.52 + in_color.a * 0.26);

                vec3 fog_color = frame.atmosphere_color_density.rgb;
                float fog_density = frame.atmosphere_color_density.w;
                float fog_amount = clamp(
                    1.0 - exp(-max(in_view_depth, 0.0) * fog_density),
                    0.0,
                    0.82
                );
                float ground_haze = smoothstep(1.8, 0.05, in_world_position.z) * 0.18;
                lit = mix(
                    lit,
                    fog_color + vec3(0.025, 0.055, 0.068) * upward_surface,
                    clamp(fog_amount + ground_haze * wetness, 0.0, 0.88)
                );

                float exposure = frame.camera_exposure_bloom.x;
                float bloom_strength = frame.camera_exposure_bloom.y;
                lit += surface_color * emissive * bloom_strength * (0.10 + wetness * 0.10);
                lit = filmic_tone_map(lit, exposure);
                out_color = vec4(lit, in_color.a);
            }
        "#,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_scene_vertex_carries_clip_space_depth() {
        let vertex = WindowSceneVertex::screen([0.25, -0.5, 0.75], [1.0, 0.5, 0.25, 1.0]);
        let default_vertex = WindowSceneVertex::default();

        assert_eq!(vertex.position[2], 0.75);
        assert_eq!(vertex.normal, [0.0, 0.0, 1.0]);
        assert_eq!(vertex.coordinate_space, WindowSceneVertex::SCREEN_SPACE);
        assert_eq!(vertex.surface_response, WINDOW_SURFACE_RESPONSE_DEFAULT);
        assert_eq!(vertex.material_detail, WINDOW_SURFACE_DETAIL_DEFAULT);
        assert_eq!(vertex.generated_cache, WINDOW_SURFACE_CACHE_DEFAULT);
        assert_eq!(
            default_vertex.surface_response,
            WINDOW_SURFACE_RESPONSE_DEFAULT
        );
        assert_eq!(
            default_vertex.material_detail,
            WINDOW_SURFACE_DETAIL_DEFAULT
        );
        assert_eq!(default_vertex.generated_cache, WINDOW_SURFACE_CACHE_DEFAULT);
    }

    #[test]
    fn world_scene_vertex_carries_surface_normal() {
        let vertex =
            WindowSceneVertex::world([1.0, 2.0, 3.0], [0.0, 1.0, 0.0], [0.2, 0.3, 0.4, 1.0]);

        assert_eq!(vertex.normal, [0.0, 1.0, 0.0]);
        assert_eq!(vertex.coordinate_space, WindowSceneVertex::WORLD_SPACE);
        assert_eq!(vertex.surface_response, WINDOW_SURFACE_RESPONSE_DEFAULT);
        assert_eq!(vertex.material_detail, WINDOW_SURFACE_DETAIL_DEFAULT);
        assert_eq!(vertex.generated_cache, WINDOW_SURFACE_CACHE_DEFAULT);
    }

    #[test]
    fn world_scene_vertex_carries_layered_surface_response() {
        let vertex = WindowSceneVertex::world_with_surface_response(
            [1.0, 2.0, 3.0],
            [0.0, 0.0, 1.0],
            [0.2, 0.3, 0.4, 1.0],
            [-0.5, 0.4, 1.8, 0.75],
        );

        assert_eq!(vertex.surface_response, [0.0, 0.4, 1.0, 0.75]);
        assert_eq!(vertex.material_detail, WINDOW_SURFACE_DETAIL_DEFAULT);
        assert_eq!(vertex.generated_cache, WINDOW_SURFACE_CACHE_DEFAULT);
    }

    #[test]
    fn world_scene_vertex_carries_material_detail_parameters() {
        let vertex = WindowSceneVertex::world_with_surface_detail(
            [1.0, 2.0, 3.0],
            [0.0, 0.0, 1.0],
            [0.2, 0.3, 0.4, 1.0],
            WINDOW_SURFACE_RESPONSE_WET_ROAD,
            [2.4, -1.0, 1.8, 0.42],
        );

        assert_eq!(vertex.material_detail, [2.4, 0.0, 1.0, 0.42]);
        assert_eq!(vertex.generated_cache, WINDOW_SURFACE_CACHE_DEFAULT);
    }

    #[test]
    fn world_scene_vertex_carries_generated_cache_parameters() {
        let vertex = WindowSceneVertex::world_with_surface_detail_and_cache(
            [1.0, 2.0, 3.0],
            [0.0, 0.0, 1.0],
            [0.2, 0.3, 0.4, 1.0],
            WINDOW_SURFACE_RESPONSE_WET_ROAD,
            [2.4, 0.32, 0.8, 0.42],
            [-0.2, 0.4, 1.8, 0.72],
        );

        assert_eq!(vertex.material_detail, [2.4, 0.32, 0.8, 0.42]);
        assert_eq!(vertex.generated_cache, [0.0, 0.4, 1.0, 0.72]);
    }

    #[test]
    fn natural_surface_quads_emit_stronger_texture_cache_signals() {
        let mut geometry = WindowSceneGeometry::default();

        geometry.world_quad_with_surface_response(
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [1.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0],
            [0.10, 0.24, 0.08, 1.0],
            [0.70, 0.0, 0.18, 0.0],
        );

        let vertex = geometry
            .vertices
            .first()
            .expect("quad should emit vertices");
        assert!(vertex.material_detail[0] > 1.25);
        assert!(vertex.material_detail[2] > 0.42);
        assert!(vertex.material_detail[3] > 0.48);
        assert!(vertex.generated_cache[0] > 0.34);
        assert!(vertex.generated_cache[1] > 0.66);
        assert!(vertex.generated_cache[2] > 0.26);
    }

    #[test]
    fn window_decal_packet_draws_stateful_surface_overlays() {
        let packet = WindowDecalPacketVisual::new(vec![
            WindowSurfaceDecalVisual::new(
                [0.0, 0.0, 0.08],
                [0.9, 0.22],
                WindowSurfaceDecalKind::WaterPuddle,
            )
            .with_state_detail(2.0, 0.9, 0.34)
            .with_detail_seed(11),
            WindowSurfaceDecalVisual::new(
                [1.6, 0.0, 1.24],
                [0.54, 0.36],
                WindowSurfaceDecalKind::CrackField,
            )
            .with_axes([1.0, 0.0, 0.0], [0.0, 0.0, 1.0])
            .with_color([0.82, 0.94, 1.0, 0.58])
            .with_surface_response(WINDOW_SURFACE_RESPONSE_GLASS)
            .with_state_detail(2.4, 0.86, 0.72)
            .with_detail_seed(12),
            WindowSurfaceDecalVisual::new(
                [3.1, 0.0, 1.16],
                [0.44, 0.28],
                WindowSurfaceDecalKind::Poster,
            )
            .with_axes([1.0, 0.0, 0.0], [0.0, 0.0, 1.0])
            .with_state_detail(1.7, 0.48, 0.55)
            .with_detail_seed(13),
            WindowSurfaceDecalVisual::new(
                [4.4, 0.0, 1.18],
                [0.56, 0.24],
                WindowSurfaceDecalKind::Graffiti,
            )
            .with_axes([1.0, 0.0, 0.0], [0.0, 0.0, 1.0])
            .with_state_detail(1.5, 0.62, 0.34)
            .with_detail_seed(14),
        ]);
        let mut geometry = WindowSceneGeometry::default();

        geometry.add_window_decal_packet(&packet);

        assert!(geometry.vertices.len() > packet.decals.len() * 4);
        assert_eq!(geometry.vertices.len() % 4, 0);
        assert_eq!(geometry.indices.len(), geometry.vertices.len() / 4 * 6);
        assert!(
            geometry
                .vertices
                .iter()
                .all(|vertex| vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE)
        );
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD
                && vertex.color[2] > vertex.color[0]
                && vertex.material_detail[2] > 0.8
                && vertex.material_detail[3] > 0.3
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_GLASS
                && vertex.color[0] > 0.8
                && vertex.material_detail[2] > 0.8
                && vertex.material_detail[3] > 0.65
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.86
                && vertex.color[1] == 0.52
                && vertex.color[2] == 0.28
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
                && vertex.material_detail[2] > 0.45
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[1] > 0.7
                && vertex.color[2] > 0.8
                && vertex.surface_response[3] > 0.1
                && vertex.material_detail[2] > 0.55
        }));
    }

    #[test]
    fn window_scene_geometry_builds_indexed_world_primitives() {
        let mut geometry = WindowSceneGeometry::default();

        geometry.world_box([0.0, 0.0, 0.0], [1.0, 2.0, 3.0], [0.2, 0.3, 0.4, 1.0]);
        geometry.world_cylinder_between(
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            0.1,
            8,
            [0.8, 0.4, 0.2, 1.0],
        );
        geometry.world_flat_ellipse([0.0, 0.0, 0.02], [1.0, 0.4], 12, [0.1, 0.2, 0.5, 0.6]);

        assert!(!geometry.vertices.is_empty());
        assert_eq!(geometry.vertices.len() % 4, 0);
        assert_eq!(geometry.indices.len() % 6, 0);
        assert_eq!(geometry.vertices.len() / 4, geometry.indices.len() / 6);
        assert!(
            geometry
                .indices
                .iter()
                .all(|index| (*index as usize) < geometry.vertices.len())
        );
        assert!(geometry.vertices.iter().all(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && (dot3(vertex.normal, vertex.normal).sqrt() - 1.0).abs() < 0.001
        }));
        assert!(
            geometry
                .vertices
                .iter()
                .all(|vertex| vertex.surface_response == WINDOW_SURFACE_RESPONSE_DEFAULT)
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.material_detail != WINDOW_SURFACE_DETAIL_DEFAULT)
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.generated_cache != WINDOW_SURFACE_CACHE_DEFAULT)
        );
        assert!(geometry.vertices.iter().all(|vertex| {
            vertex
                .generated_cache
                .iter()
                .all(|channel| (0.0..=1.0).contains(channel))
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            [vertex.normal[0], vertex.normal[1], vertex.normal[2]]
                .into_iter()
                .filter(|component| component.abs() > 0.2)
                .count()
                >= 2
        }));
    }

    #[test]
    fn window_scene_geometry_derives_generated_cache_pages_from_material_truth() {
        let mut first = WindowSceneGeometry::default();
        let mut second = WindowSceneGeometry::default();
        let corners = [
            [-2.0, -1.5, 0.0],
            [2.0, -1.5, 0.0],
            [2.0, 1.5, 0.0],
            [-2.0, 1.5, 0.0],
        ];

        first.world_quad_with_surface_response(
            corners[0],
            corners[1],
            corners[2],
            corners[3],
            [0.035, 0.04, 0.045, 1.0],
            WINDOW_SURFACE_RESPONSE_WET_ROAD,
        );
        second.world_quad_with_surface_response(
            corners[0],
            corners[1],
            corners[2],
            corners[3],
            [0.035, 0.04, 0.045, 1.0],
            WINDOW_SURFACE_RESPONSE_WET_ROAD,
        );

        let cache = first.vertices[0].generated_cache;
        assert!(cache[0] > 0.35);
        assert!(cache[1] > 0.15);
        assert!(cache[2] > 0.25);
        assert!(cache[3] > 0.0 && cache[3] <= 1.0);
        assert!(
            first
                .vertices
                .iter()
                .all(|vertex| vertex.generated_cache == cache)
        );
        assert_eq!(second.vertices[0].generated_cache, cache);
    }

    #[test]
    fn world_micro_detailed_box_adds_non_flat_surface_features() {
        let mut geometry = WindowSceneGeometry::default();

        geometry.world_micro_detailed_box_with_surface_response(
            [-1.0, -0.6, 0.0],
            [1.0, 0.6, 1.4],
            [0.18, 0.2, 0.22, 1.0],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            42,
            0.9,
        );

        assert!(geometry.vertices.len() > 24);
        assert_eq!(geometry.vertices.len() % 4, 0);
        assert_eq!(geometry.indices.len(), geometry.vertices.len() / 4 * 6);
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
                && vertex.color[3] < 0.5
                && vertex.position[2] > 1.0
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            [vertex.normal[0], vertex.normal[1], vertex.normal[2]]
                .into_iter()
                .filter(|component| component.abs() > 0.2)
                .count()
                >= 2
        }));
    }

    #[test]
    fn window_scene_geometry_builds_screen_rects_at_explicit_depth() {
        let mut geometry = WindowSceneGeometry::default();

        geometry.screen_rect([-0.5, -0.25], [0.5, 0.25], 0.33, [1.0, 0.5, 0.25, 1.0]);

        assert_eq!(geometry.vertices.len(), 4);
        assert_eq!(geometry.indices, vec![0, 1, 2, 0, 2, 3]);
        assert!(geometry.vertices.iter().all(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::SCREEN_SPACE
                && (vertex.position[2] - 0.33).abs() < 0.001
        }));
    }

    #[test]
    fn window_natural_sky_draws_far_depth_screen_backdrop() {
        let mut geometry = WindowSceneGeometry::default();

        geometry.add_window_natural_sky(24);

        assert!(geometry.vertices.len() >= 7 * 4);
        assert_eq!(geometry.vertices.len() % 4, 0);
        assert_eq!(geometry.indices.len(), geometry.vertices.len() / 4 * 6);
        assert!(geometry.vertices.iter().all(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::SCREEN_SPACE && vertex.position[2] >= 0.98
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] > 0.5 && vertex.color[1] > 0.6 && vertex.color[2] > 0.7
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 1.0 && vertex.color[1] == 0.82 && vertex.color[2] == 0.56
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.82 && vertex.color[1] == 0.86 && vertex.color[2] == 0.84
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == [0.72, 0.68, 0.58, 0.18] && vertex.position[2] >= 0.98
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.82
                && vertex.color[1] == 0.88
                && vertex.color[2] == 0.95
                && vertex.color[3] >= 0.08
        }));
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.color[0] == 0.34
                    && vertex.color[1] == 0.4
                    && vertex.color[2] == 0.46
                    && vertex.color[3] > 0.04)
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.color[3] > 0.1 && vertex.color[3] < 0.2)
        );
    }

    #[test]
    fn window_scene_geometry_builds_hud_reticle_strip_and_world_markers() {
        let mut geometry = WindowSceneGeometry::default();
        let reticle_color = [0.2, 0.9, 1.0, 0.75];
        let pulse_color = [1.0, 0.3, 0.2, 0.8];
        let marker_color = [0.9, 0.7, 0.25, 1.0];

        geometry.add_window_status_hud(4, 21, 24, 2);
        geometry.add_window_interaction_reticle(reticle_color);
        geometry.add_window_hud_event_strip(&[WindowHudPulseVisual::new(pulse_color, 0.5)], 14);
        geometry.add_world_marker(
            WindowWorldMarkerVisual::new([2.0, -1.0], 0.4, marker_color)
                .with_axis_half_extent_scale(1.15),
        );

        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::SCREEN_SPACE
                && vertex.color == reticle_color
                && (vertex.position[2] - WINDOW_SCREEN_SPACE_DEPTH).abs() < 0.001
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::SCREEN_SPACE
                && vertex.color == pulse_color
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.color == marker_color
                && vertex.position[0] == 2.0
                && (vertex.position[1] - -0.6).abs() < 0.001
        }));
    }

    #[test]
    fn window_atmosphere_clamps_and_reacts_to_light_and_events() {
        let clamped = WindowAtmosphere::new([-1.0, 3.0, 0.5], -0.5, 9.0, 4.0);
        assert_eq!(clamped.color_density(), [0.0, 2.0, 0.5, 0.0]);
        assert_eq!(clamped.exposure_bloom(), [3.0, 1.5, 0.0, 0.0]);

        let quiet = window_atmosphere_for_alley(1, 0, WindowDynamicLight::disabled());
        let active = window_atmosphere_for_alley(
            20,
            24,
            WindowDynamicLight::new([1.0, 2.0, 2.0], 8.5, [1.0, 0.2, 0.8], 1.2),
        );

        assert!(quiet.fog_density_per_meter > 0.0);
        assert!(active.fog_density_per_meter > quiet.fog_density_per_meter);
        assert!(active.exposure > quiet.exposure);
        assert!(active.bloom_strength > quiet.bloom_strength);
        assert!(active.fog_color[0] > quiet.fog_color[0]);
    }

    #[test]
    fn window_infrastructure_visuals_draw_event_reactive_city_systems() {
        let clamped = WindowInfrastructureVisualState::new(2.0, -1.0, 0.5, 3.0);
        assert_eq!(
            clamped,
            WindowInfrastructureVisualState {
                surveillance_coverage: 1.0,
                power_instability: 0.0,
                drainage_overflow: 0.5,
                data_activity: 1.0,
            }
        );

        let mut quiet = WindowSceneGeometry::default();
        let mut active = WindowSceneGeometry::default();
        quiet.add_window_alley_infrastructure(1, WindowInfrastructureVisualState::default());
        active.add_window_alley_infrastructure(
            24,
            WindowInfrastructureVisualState::new(0.92, 1.0, 0.84, 0.96),
        );

        assert!(active.vertices.len() > quiet.vertices.len());
        assert_eq!(active.vertices.len() % 4, 0);
        assert_eq!(active.indices.len(), active.vertices.len() / 4 * 6);
        assert!(active.vertices.iter().all(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && (dot3(vertex.normal, vertex.normal).sqrt() - 1.0).abs() < 0.001
        }));
        assert!(active.vertices.iter().any(|vertex| {
            vertex.color[0] == 1.0
                && vertex.color[1] == 0.12
                && vertex.color[2] == 0.18
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
        }));
        assert!(active.vertices.iter().any(|vertex| {
            vertex.color[0] == 1.0
                && vertex.color[1] == 0.82
                && vertex.color[2] == 0.18
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE
        }));
        assert!(active.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.34
                && vertex.color[1] == 0.74
                && vertex.color[2] == 1.0
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD
        }));
        assert!(active.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.24
                && vertex.color[1] == 0.9
                && vertex.color[2] == 1.0
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
        }));
        assert!(active.vertices.iter().any(|vertex| {
            vertex.color == [0.072, 0.084, 0.09, 1.0]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_METAL
        }));
    }

    #[test]
    fn window_city_streaming_cells_draw_lod_footprints_and_detail_markers() {
        let cell = WindowCityStreamingCellVisual::new(
            [12.0, 0.0],
            [18.0, 7.5],
            WindowCityStreamingCellState::RenderHighDetail,
        )
        .with_streaming_metrics(8.0, 2.0, usize::MAX, 192 * 1024 * 1024);
        assert_eq!(cell.priority, 4.0);
        assert_eq!(cell.camera_alignment, 1.0);
        assert_eq!(cell.dependency_count, u32::MAX);
        assert_eq!(cell.requested_streaming_megabytes, 192.0);
        assert!(cell.is_loaded());

        let unloaded = WindowCityStreamingCellVisual::new(
            [-30.0, 0.0],
            [10.0, 6.0],
            WindowCityStreamingCellState::Unloaded,
        );
        let summary = WindowCityStreamingCellVisual::new(
            [-8.0, 0.0],
            [8.0, 5.0],
            WindowCityStreamingCellState::SummaryLoaded,
        )
        .with_streaming_metrics(0.4, -0.2, 1, 0);
        let mut geometry = WindowSceneGeometry::default();

        geometry.add_window_city_streaming_cells(&[unloaded, summary, cell], 12);

        assert!(geometry.vertices.len() > 180);
        assert_eq!(geometry.vertices.len() % 4, 0);
        assert_eq!(geometry.indices.len(), geometry.vertices.len() / 4 * 6);
        assert!(geometry.vertices.iter().all(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && (dot3(vertex.normal, vertex.normal).sqrt() - 1.0).abs() < 0.001
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.42
                && vertex.color[1] == 1.0
                && vertex.color[2] == 0.66
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.24
                && vertex.color[1] == 0.34
                && vertex.color[2] == 0.44
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
        }));
        assert!(!geometry.vertices.iter().any(|vertex| {
            vertex.position[0] < -35.0
                && vertex.color[0] == 0.12
                && vertex.color[1] == 0.14
                && vertex.color[2] == 0.16
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[2] > 0.4
                && vertex.color[1] == 1.0
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
        }));
    }

    #[test]
    fn window_generated_city_draws_district_massing_routes_and_activity() {
        let hero_chunk = WindowGeneratedCityChunkVisual::new(
            77,
            [20.0, 0.0],
            [18.0, 6.0],
            WindowCityDistrictVisualKind::BlackMarket,
            WindowCityStreamingCellState::HeroLoaded,
        )
        .with_district_pressure(0.52, 0.82, 0.58)
        .with_faction([0.9, 0.18, 0.78], 0.86)
        .with_activity_counts(3, 2, 2, 3, 2, 6)
        .with_population_counts(3, 2, 72, 6, true)
        .with_detail_seed(123);
        let summary_chunk = WindowGeneratedCityChunkVisual::new(
            88,
            [-18.0, 0.0],
            [12.0, 5.0],
            WindowCityDistrictVisualKind::CorporateCore,
            WindowCityStreamingCellState::SummaryLoaded,
        )
        .with_district_pressure(0.94, 0.2, 0.22)
        .with_faction([0.45, 0.72, 1.0], 0.76);
        let nodes = [
            WindowGeneratedCityNavigationNodeVisual::new([4.0, 0.0, 0.0])
                .with_importance(true, true)
                .with_pressure(0.6, 0.7),
            WindowGeneratedCityNavigationNodeVisual::new([18.0, 1.5, 0.0])
                .with_importance(true, false)
                .with_pressure(0.82, 0.42),
        ];
        let edges = [WindowGeneratedCityNavigationEdgeVisual::new(
            [4.0, 0.0, 0.0],
            [18.0, 1.5, 0.0],
            WindowCityTraversalVisualKind::StealthPath,
        )
        .with_route_priority(1.0)];
        let mut geometry = WindowSceneGeometry::default();

        geometry.add_window_generated_city(&[hero_chunk, summary_chunk], &nodes, &edges, 31);

        assert!(geometry.vertices.len() > 760);
        assert_eq!(geometry.vertices.len() % 4, 0);
        assert_eq!(geometry.indices.len(), geometry.vertices.len() / 4 * 6);
        assert!(geometry.vertices.iter().all(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && (dot3(vertex.normal, vertex.normal).sqrt() - 1.0).abs() < 0.001
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == [0.82, 0.24, 0.56, 0.52]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.34
                && vertex.color[1] == 1.0
                && vertex.color[2] == 0.62
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 1.0
                && vertex.color[1] == 0.62
                && vertex.color[2] == 0.24
                && vertex.position[2] > 0.4
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_GLASS && vertex.color[3] == 0.38
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[2] > 1.2 && vertex.color[0] > 0.55 && vertex.color[2] > 0.45
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[0] > 2.0
                && vertex.position[2] > 2.0
                && vertex.color == [0.018, 0.017, 0.022, 1.0]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_METAL
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[0] > 2.0
                && vertex.position[2] > 3.0
                && vertex.color == [0.13, 0.15, 0.16, 0.96]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_METAL
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[0] > 2.0
                && vertex.position[2] > 0.9
                && vertex.color == [0.028, 0.032, 0.036, 1.0]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_METAL
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[0] > 2.0
                && vertex.color[0] == 0.026
                && vertex.color[1] == 0.022
                && vertex.color[2] == 0.02
                && vertex.color[3] > 0.4
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[0] > 2.0
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH
                && vertex.color[3] == 0.86
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[0] > 2.0
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_SKIN
                && vertex.position[2] > 0.8
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[0] > 2.0
                && vertex.color[0] == 0.68
                && vertex.color[1] == 0.66
                && vertex.color[2] == 0.5
                && vertex.color[3] > 0.1
                && vertex.surface_response[0] == 0.7
                && vertex.surface_response[3] == 0.0
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[0] > 2.0
                && vertex.color[0] == 1.0
                && vertex.color[1] == 0.86
                && vertex.color[2] == 0.56
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[0] > 2.0
                && vertex.color == [0.12, 0.045, 0.11, 0.94]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_METAL
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[0] > 2.0
                && vertex.color == [0.74, 0.16, 0.48, 0.44]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[0] > 2.0
                && vertex.color[0] == 0.045
                && vertex.color[1] == 0.038
                && vertex.color[2] == 0.032
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[0] > 2.0
                && vertex.color == [0.16, 0.045, 0.13, 0.84]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[0] > 2.0
                && vertex.color[0] == 0.82
                && vertex.color[1] == 0.24
                && vertex.color[2] == 0.56
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
        }));
        assert!(!geometry.vertices.iter().any(|vertex| {
            vertex.position[0] < -6.0
                && vertex.color == [0.09, 0.13, 0.16, 1.0]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_METAL
        }));
    }

    #[test]
    fn window_city_consequences_draw_dirty_cells_dangers_and_route_updates() {
        let cell = WindowCityPersistentCellVisual::new(42, [10.0, 0.0], [12.0, 5.0])
            .with_severity(0.92)
            .with_persistence_counts(WindowCityPersistenceCounts {
                damaged_count: 2,
                material_override_count: 1,
                infrastructure_delta_count: 3,
                faction_delta_count: 1,
                story_thread_count: 1,
                ai_memory_count: 2,
                resident_asset_count: 1,
            })
            .with_navigation_counts(2, 1, 1, 1)
            .with_infrastructure_system_mask(
                WINDOW_CITY_INFRASTRUCTURE_POWER
                    | WINDOW_CITY_INFRASTRUCTURE_WATER
                    | WINDOW_CITY_INFRASTRUCTURE_DATA
                    | WINDOW_CITY_INFRASTRUCTURE_SURVEILLANCE
                    | WINDOW_CITY_INFRASTRUCTURE_TRANSIT
                    | WINDOW_CITY_INFRASTRUCTURE_DRAINAGE,
            )
            .with_save_required(true)
            .with_detail_seed(99);
        let dangers = [
            WindowCityDangerFieldVisual::new([5.0, 0.0, 0.0], WindowCityDangerVisualKind::Flooding)
                .with_radius(2.2)
                .with_severity(0.7),
            WindowCityDangerFieldVisual::new(
                [12.0, 1.0, 0.0],
                WindowCityDangerVisualKind::ToxicGas,
            )
            .with_radius(1.8)
            .with_severity(0.86),
        ];
        let routes = [
            WindowCityRouteConsequenceVisual::new(
                [1.0, 0.0, 0.0],
                [8.0, 0.0, 0.0],
                WindowCityRouteConsequenceStatus::Blocked,
            )
            .with_severity(1.0),
            WindowCityRouteConsequenceVisual::new(
                [8.0, 0.0, 0.0],
                [16.0, 1.0, 0.0],
                WindowCityRouteConsequenceStatus::Restricted,
            )
            .with_severity(0.65),
        ];
        let mut geometry = WindowSceneGeometry::default();

        geometry.add_window_city_consequences(&[cell], &dangers, &routes, 19);

        assert!(geometry.vertices.len() > 180);
        assert_eq!(geometry.vertices.len() % 4, 0);
        assert_eq!(geometry.indices.len(), geometry.vertices.len() / 4 * 6);
        assert!(geometry.vertices.iter().all(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && (dot3(vertex.normal, vertex.normal).sqrt() - 1.0).abs() < 0.001
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 1.0
                && vertex.color[1] == 0.16
                && vertex.color[2] == 0.12
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.28
                && vertex.color[1] == 0.76
                && vertex.color[2] == 1.0
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 1.0
                && vertex.color[1] == 0.82
                && vertex.color[2] == 0.18
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.34
                && vertex.color[1] == 0.74
                && vertex.color[2] == 1.0
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.5
                && vertex.color[1] == 0.84
                && vertex.color[2] == 1.0
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.24
                && vertex.color[1] == 0.9
                && vertex.color[2] == 1.0
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 1.0
                && vertex.color[1] == 0.12
                && vertex.color[2] == 0.18
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.74
                && vertex.color[1] == 0.52
                && vertex.color[2] == 1.0
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == [1.0, 0.86, 0.28, 0.78]
                && vertex.position[2] > 0.7
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.84 && vertex.color[1] == 0.38 && vertex.color[2] == 1.0
        }));
    }

    #[test]
    fn window_city_material_placements_draw_stateful_surfaces_and_faction_accents() {
        let placements = [
            WindowCityMaterialPlacementVisual::new(
                [0.0, 0.0, 0.0],
                [2.4, 0.8],
                WindowCityMaterialPlacementKind::WetRoad,
            )
            .with_surface_state(0.86, 0.32, 0.58, 0.74)
            .with_faction_accent([0.9, 0.18, 0.78], 0.82)
            .with_importance(0.7)
            .with_detail_seed(10),
            WindowCityMaterialPlacementVisual::new(
                [3.6, 0.0, 0.0],
                [1.1, 0.9],
                WindowCityMaterialPlacementKind::Glass,
            )
            .with_base_color([0.62, 0.9, 1.0, 0.46])
            .with_surface_state(0.22, 0.76, 0.08, 0.0)
            .with_detail_seed(20),
            WindowCityMaterialPlacementVisual::new(
                [6.0, 1.2, 2.1],
                [1.0, 0.18],
                WindowCityMaterialPlacementKind::Neon,
            )
            .with_surface_state(0.0, 0.42, 0.02, 0.0)
            .with_importance(1.0)
            .with_detail_seed(30),
            WindowCityMaterialPlacementVisual::new(
                [8.0, -0.4, 0.0],
                [1.2, 0.55],
                WindowCityMaterialPlacementKind::Water,
            )
            .with_surface_state(1.0, 0.0, 0.28, 0.0)
            .with_importance(0.6)
            .with_detail_seed(40),
            WindowCityMaterialPlacementVisual::new(
                [10.0, 0.2, 0.0],
                [0.42, 0.32],
                WindowCityMaterialPlacementKind::HumanSkin,
            )
            .with_surface_state(0.4, 0.08, 0.12, 0.0)
            .with_faction_accent([0.28, 0.82, 1.0], 0.52)
            .with_detail_seed(50),
            WindowCityMaterialPlacementVisual::new(
                [12.0, -0.2, 0.0],
                [0.76, 0.48],
                WindowCityMaterialPlacementKind::Metal,
            )
            .with_surface_state(0.08, 0.48, 0.62, 0.18)
            .with_importance(0.24)
            .with_detail_seed(60),
        ];
        let mut geometry = WindowSceneGeometry::default();

        geometry.add_window_city_material_placements(&placements, 77);

        assert!(geometry.vertices.len() > 280);
        assert_eq!(geometry.vertices.len() % 4, 0);
        assert_eq!(geometry.indices.len(), geometry.vertices.len() / 4 * 6);
        assert!(geometry.vertices.iter().all(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && (dot3(vertex.normal, vertex.normal).sqrt() - 1.0).abs() < 0.001
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_GLASS && vertex.color[2] > 0.8
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
                && vertex.color[0] > 0.8
                && vertex.color[2] > 0.5
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD
                && vertex.color[2] > vertex.color[0]
                && vertex.position[2] <= 0.09
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.9
                && vertex.color[1] == 0.18
                && vertex.color[2] == 0.78
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE
                && vertex.color[0] == 1.0
                && vertex.color[1] == 0.38
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
                && vertex.position[2] <= 0.11
                && vertex.color[3] < 0.4
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
                && vertex.color[0] == 0.018
                && vertex.color[1] == 0.024
                && vertex.color[2] == 0.028
                && vertex.position[2] <= 0.09
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_GLASS
                && vertex.color[0] == 0.72
                && vertex.color[1] == 0.92
                && vertex.color[2] == 1.0
                && vertex.color[3] < 0.5
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE
                && vertex.color[0] == 1.0
                && vertex.color[1] == 0.92
                && vertex.color[2] == 0.38
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD
                && vertex.color[0] == 0.34
                && vertex.color[1] == 0.78
                && vertex.color[2] == 0.92
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
                && vertex.color[0] == 0.12
                && vertex.color[1] == 0.08
                && vertex.color[2] == 0.06
                && vertex.position[2] > 0.2
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
                && vertex.color[0] == 0.36
                && vertex.color[1] == 0.18
                && vertex.color[2] == 0.08
                && vertex.position[0] > 11.0
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.76
                && vertex.color[1] == 0.74
                && vertex.color[2] == 0.58
                && vertex.material_detail[2] > 0.3
                && vertex.material_detail[3] > 0.15
                && vertex.position[2] <= 0.12
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.82
                && vertex.color[1] == 0.94
                && vertex.color[2] == 1.0
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_GLASS
                && vertex.material_detail[2] > 0.7
                && vertex.material_detail[3] > 0.7
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.38
                && vertex.color[1] == 0.16
                && vertex.color[2] == 0.065
                && vertex.position[0] > 11.0
                && vertex.material_detail[2] > 0.28
                && vertex.material_detail[3] > 0.2
        }));
    }

    #[test]
    fn window_city_material_placements_produce_camera_weighted_dynamic_light() {
        let distant_neon = WindowCityMaterialPlacementVisual::new(
            [34.0, 0.0, 2.2],
            [1.2, 0.18],
            WindowCityMaterialPlacementKind::Neon,
        )
        .with_surface_response(WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON)
        .with_importance(1.0)
        .with_detail_seed(91);
        let nearby_neon = WindowCityMaterialPlacementVisual::new(
            [2.5, 0.0, 2.0],
            [0.9, 0.16],
            WindowCityMaterialPlacementKind::Neon,
        )
        .with_base_color([0.18, 0.92, 1.0, 0.9])
        .with_surface_response(WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON)
        .with_faction_accent([0.18, 0.92, 1.0], 0.92)
        .with_importance(0.82)
        .with_detail_seed(92);

        let light = window_dynamic_light_from_city_material_placements(
            &[distant_neon, nearby_neon],
            [0.0, 0.0, 1.65],
            17,
        );

        assert!(light.is_enabled());
        assert!(light.position_meters[0] < 8.0);
        assert!(light.radius_meters > 4.0);
        assert!(light.intensity > 0.8);
        assert!(light.color[1] > 0.45);
        assert!(light.color[2] > 0.45);
    }

    fn window_world_event(kind: WorldEventKind) -> WorldEvent {
        WorldEvent {
            event_id: 42,
            tick: 3,
            location_meters: ashfall_core::core::Vec3::new(2.5, -1.25, 0.0),
            actors: vec![1, 2],
            kind,
            physical_evidence: Vec::new(),
            narrative_tags: Vec::new(),
        }
    }

    #[test]
    fn window_event_visuals_classify_world_events_and_fade() {
        let fracture = window_world_event(WorldEventKind::GlassWallFractured { entity: 2 });
        let audio_mix = window_world_event(WorldEventKind::AudioFrameMixed {
            active_sound_count: 3,
            material_aware_sound_count: 2,
            peak_intensity: 0.9,
            mixed_audio_buffer: ashfall_core::core::AudioBufferHandle(77),
        });

        let mut marker =
            window_world_marker_from_event(&fracture).expect("fracture should emit a marker");
        let mut pulse = window_hud_event_pulse_from_event(&audio_mix);
        let marker_radius = marker.visible_radius();
        let pulse_alpha = pulse.visible_color()[3];

        assert_eq!(marker.position, [2.5, -1.25]);
        assert!(marker.base_radius > 0.7);
        assert!(pulse.weight > 0.6);
        assert!(window_world_marker_from_event(&audio_mix).is_none());

        assert!(marker.tick(0.5));
        assert!(marker.visible_radius() > marker_radius);
        assert!(pulse.tick(0.5));
        assert!(pulse.visible_color()[3] < pulse_alpha);
    }

    fn window_snapshot_for_entity_geometry() -> WorldSnapshot {
        use ashfall_core::{
            core::{SimTime, Vec3},
            world::{AgentState, ComponentView, PhysicalBody},
        };

        WorldSnapshot {
            frame_id: 1,
            sim_time: SimTime::default(),
            materials: ComponentView::new(vec![(77, window_test_material_descriptor())]),
            names: ComponentView::new(Vec::new()),
            tags: ComponentView::new(vec![
                (1, vec!["player".to_string()]),
                (2, vec!["npc".to_string()]),
                (3, vec!["glass".to_string(), "wall".to_string()]),
                (4, vec!["ground".to_string()]),
                (5, vec!["metal_prop".to_string()]),
            ]),
            transforms: ComponentView::new(vec![
                (1, Transform::at(Vec3::new(0.0, 0.0, 0.0))),
                (2, Transform::at(Vec3::new(1.5, 0.0, 0.0))),
                (3, Transform::at(Vec3::new(2.5, 0.0, 0.0))),
                (4, Transform::at(Vec3::new(0.0, 0.0, 0.0))),
                (5, Transform::at(Vec3::new(-3.0, 1.6, 0.25))),
            ]),
            renderables: ComponentView::new(vec![
                (
                    1,
                    Renderable {
                        mesh: MeshAssetHandle(99),
                        material: 1,
                        visible: true,
                        fracture_replacement_mesh: None,
                    },
                ),
                (
                    4,
                    Renderable {
                        mesh: MeshAssetHandle(WINDOW_MESH_WET_ASPHALT),
                        material: 1,
                        visible: true,
                        fracture_replacement_mesh: None,
                    },
                ),
                (
                    5,
                    Renderable {
                        mesh: MeshAssetHandle(88_888),
                        material: 77,
                        visible: true,
                        fracture_replacement_mesh: None,
                    },
                ),
            ]),
            physical_bodies: ComponentView::<PhysicalBody>::new(Vec::new()),
            material_states: ComponentView::new(vec![
                (
                    4,
                    MaterialState {
                        moisture: 0.4,
                        ..MaterialState::default()
                    },
                ),
                (
                    5,
                    MaterialState {
                        moisture: 0.78,
                        electrical_charge: 84.0,
                        temperature: 620.0,
                        ..MaterialState::default()
                    },
                ),
            ]),
            humans: ComponentView::new(vec![(
                2,
                HumanState {
                    human_id: 300,
                    quality_tier: QualityTier::HeroHighFidelityRuntime,
                },
            )]),
            agents: ComponentView::new(vec![(
                2,
                AgentState {
                    persona: 300,
                    emotional_state: EmotionState::alarmed(),
                    ai_lod: QualityTier::NormalRuntime,
                },
            )]),
            city_cells: ComponentView::new(Vec::new()),
        }
    }

    #[test]
    fn window_snapshot_entities_build_renderer_geometry_from_core_snapshot() {
        let snapshot = window_snapshot_for_entity_geometry();
        let camera = WindowPerspectiveCamera::new([0.0, -3.0, 1.65], 0.0, 0.0);
        let mut geometry = WindowSceneGeometry::default();

        geometry.add_window_snapshot_entities(
            WindowSnapshotSceneOptions::new(&snapshot, 8, camera).with_player_entity(1),
        );

        assert!(!geometry.vertices.is_empty());
        assert!(geometry.vertices[0].position[2] <= 0.05);
        assert!(geometry.vertices[0].color[3] < 0.7);
        assert!(
            !geometry
                .vertices
                .iter()
                .any(|vertex| vertex.color == [0.34, 0.82, 1.0, 1.0])
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.color == humanoid_default_skin_color([0.96, 0.72, 0.28, 1.0]))
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.color == [0.62, 0.88, 1.0, 0.86])
        );
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == [0.62, 0.88, 1.0, 0.86]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_GLASS
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[2] <= 0.06
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[0] < -2.7
                && vertex.position[1] > 1.3
                && vertex.surface_response[1] > 0.8
                && vertex.surface_response[2] > 0.7
                && vertex.surface_response[3] > 0.3
        }));
    }

    #[test]
    fn window_dynamic_light_from_snapshot_uses_emissive_heat_and_charge() {
        let snapshot = window_snapshot_for_entity_geometry();
        let light = window_dynamic_light_from_snapshot(&snapshot, 8);

        assert!(light.is_enabled());
        assert!((light.position_meters[0] - -3.0).abs() < 0.0001);
        assert!((light.position_meters[1] - 1.6).abs() < 0.0001);
        assert!((light.position_meters[2] - 1.5).abs() < 0.0001);
        assert!(light.radius_meters > 4.0);
        assert!(light.intensity > 0.45);
        assert!(light.color[2] > 0.5);
    }

    #[test]
    fn window_entity_color_uses_tags_material_state_and_animation_pulse() {
        let glass_tags = vec!["glass".to_string()];
        let state = MaterialState {
            crack_density: 0.5,
            moisture: 0.7,
            electrical_charge: 80.0,
            ..MaterialState::default()
        };

        let glass_color = window_entity_color(Some(&glass_tags), None, Some(&state), 0);

        for (actual, expected) in glass_color.into_iter().zip([0.76, 0.97, 1.0, 0.92]) {
            assert!((actual - expected).abs() < 0.0001);
        }

        let neon_tags = vec!["neon".to_string()];
        let neon_color = window_entity_color(Some(&neon_tags), None, None, 10);

        assert!(neon_color[0] >= 0.9);
        assert!(neon_color[1] >= 0.12);
        assert!(neon_color[2] > 0.52);
        assert_eq!(neon_color[3], 1.0);
    }

    fn window_test_material_descriptor() -> MaterialDescriptor {
        MaterialDescriptor {
            id: 77,
            name: "window test layered metal".to_string(),
            visual: ashfall_core::core::VisualMaterial {
                base_color_linear: [0.32, 0.34, 0.36, 1.0],
                roughness: 0.72,
                metallic: 0.86,
                transmission: 0.0,
                subsurface: 0.0,
                emission_linear: [0.4, 0.2, 0.0],
                anisotropy: 0.34,
                clearcoat: 0.22,
                normal_displacement_strength: 0.48,
                layer_count: 3,
                transparency: 0.0,
            },
            physical: ashfall_core::core::PhysicalMaterial {
                density_kg_per_m3: 7_800.0,
                young_modulus: 180_000_000_000.0,
                poisson_ratio: 0.29,
                yield_stress: 240_000_000.0,
                fracture_toughness: 48.0,
                hardness: 0.72,
                viscosity: 0.0,
                surface_tension: 0.0,
                restitution: 0.18,
                friction_static: 0.62,
                friction_dynamic: 0.48,
            },
            acoustic: ashfall_core::core::AcousticMaterial {
                impact_brightness: 0.72,
                resonance: 0.68,
                absorption: 0.18,
                wetness_muffle: 0.22,
            },
            thermal: ashfall_core::core::ThermalMaterial {
                heat_capacity: 500.0,
                conductivity: 45.0,
                ignition_temperature: 1_400.0,
            },
            electrical: ashfall_core::core::ElectricalMaterial {
                conductivity: 0.82,
                dielectric_strength: 0.08,
            },
            procedural_source: None,
        }
    }

    #[test]
    fn window_entity_surface_response_uses_material_state_and_tags() {
        let material = window_test_material_descriptor();
        let state = MaterialState {
            moisture: 0.82,
            soot: 0.18,
            electrical_charge: 132.0,
            temperature: 690.0,
            ..MaterialState::default()
        };

        let metal_response = window_entity_surface_response(None, Some(&material), Some(&state));
        assert!(metal_response[0] < material.visual.roughness);
        assert!(metal_response[1] > 0.8);
        assert!(metal_response[2] >= 0.82);
        assert!(metal_response[3] >= 0.5);

        let glass_tags = vec!["glass".to_string()];
        let glass_response = window_entity_surface_response(Some(&glass_tags), None, None);
        assert_eq!(glass_response, WINDOW_SURFACE_RESPONSE_GLASS);

        let neon_tags = vec!["neon".to_string()];
        let neon_response = window_entity_surface_response(Some(&neon_tags), Some(&material), None);
        assert_eq!(neon_response, WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON);
    }

    #[test]
    fn window_entity_proxy_kinds_emit_fallback_shapes_with_micro_detail() {
        let mut geometry = WindowSceneGeometry::default();
        let color = [0.4, 0.6, 0.9, 1.0];

        geometry.add_window_entity_proxy(WindowEntityProxyVisual::new(
            WindowEntityProxyKind::GenericRenderable,
            [1.0, 2.0],
            0.5,
            color,
        ));
        geometry.add_window_entity_proxy(WindowEntityProxyVisual::new(
            WindowEntityProxyKind::GlassWall,
            [0.0, 0.0],
            0.0,
            color,
        ));
        geometry.add_window_entity_proxy(WindowEntityProxyVisual::new(
            WindowEntityProxyKind::LightPanel,
            [-2.0, 1.0],
            2.0,
            color,
        ));
        geometry.add_window_entity_proxy(WindowEntityProxyVisual::new(
            WindowEntityProxyKind::LowHazardSurface,
            [3.0, -1.0],
            0.0,
            color,
        ));

        assert!(geometry.vertices.len() > 4 * 6 * 4);
        assert_eq!(geometry.vertices.len() % 4, 0);
        assert_eq!(geometry.indices.len(), geometry.vertices.len() / 4 * 6);
        let has_position = |position: [f32; 3]| {
            geometry.vertices.iter().any(|vertex| {
                vertex.color == color
                    && vertex
                        .position
                        .into_iter()
                        .zip(position)
                        .all(|(actual, expected)| (actual - expected).abs() < 0.0001)
            })
        };
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[0] > 0.7
                && vertex.position[0] < 1.35
                && vertex.position[1] > 1.7
                && vertex.position[1] < 2.35
                && vertex.position[2] > 0.55
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_DEFAULT
                && vertex.normal[0].abs() > 0.1
                && vertex.normal[2].abs() > 0.1
        }));
        assert!(has_position([-1.15, -0.08, 0.0]));
        assert!(has_position([-2.64, 0.945, 1.78]));
        assert!(has_position([2.44, -1.22, 0.002]));
        assert!(geometry.vertices.iter().any(|vertex| {
            (vertex.position[0] - -1.15).abs() < 0.0001
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_GLASS
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            (vertex.position[0] - -2.64).abs() < 0.0001
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            (vertex.position[0] - 2.44).abs() < 0.0001
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[3] < 0.5
                && [vertex.normal[0], vertex.normal[1], vertex.normal[2]]
                    .into_iter()
                    .filter(|component| component.abs() > 0.2)
                    .count()
                    >= 2
        }));
    }

    #[test]
    fn window_entity_proxy_material_state_adds_visible_overlays() {
        let mut geometry = WindowSceneGeometry::default();
        let state = MaterialState {
            moisture: 0.72,
            crack_density: 0.55,
            soot: 0.46,
            corrosion: 0.5,
            temperature: 640.0,
            electrical_charge: 132.0,
            ..MaterialState::default()
        };

        geometry.add_window_entity_proxy(
            WindowEntityProxyVisual::new(
                WindowEntityProxyKind::GenericRenderable,
                [0.0, 0.0],
                0.0,
                [0.42, 0.46, 0.5, 1.0],
            )
            .with_material_state(Some(&state)),
        );

        assert!(geometry.vertices.len() > 24);
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.34
                && vertex.color[1] == 0.72
                && vertex.color[2] == 1.0
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[2] == 1.0 && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.34
                && vertex.color[1] == 0.16
                && vertex.color[2] == 0.06
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.03
                && vertex.color[1] == 0.025
                && vertex.color[2] == 0.022
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 1.0
                && vertex.color[1] == 0.42
                && vertex.color[2] == 0.14
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 1.0
                && vertex.color[1] == 0.86
                && vertex.color[2] == 0.32
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
                && vertex.material_detail[0] > 2.4
                && vertex.material_detail[2] > 0.85
                && vertex.material_detail[3] > 0.85
        }));
    }

    #[test]
    fn window_alley_environment_adds_floor_walls_lanes_and_overhead_cables() {
        let mut geometry = WindowSceneGeometry::default();

        geometry.add_window_alley_environment();

        assert!(geometry.vertices.len() > 3_200);
        assert!(
            geometry.vertices.len() < 22_000,
            "alley environment detail should stay bounded for smooth frame times, got {} vertices",
            geometry.vertices.len()
        );
        assert_eq!(geometry.vertices.len() % 4, 0);
        assert_eq!(geometry.indices.len(), geometry.vertices.len() / 4 * 6);
        assert!(geometry.vertices.iter().all(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && (dot3(vertex.normal, vertex.normal).sqrt() - 1.0).abs() < 0.001
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position == [-5.4, -10.0, -0.08] && vertex.color == [0.035, 0.04, 0.046, 1.0]
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[2] > 2.5 && vertex.color == [0.025, 0.028, 0.032, 1.0]
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[0].abs() <= 0.105 && vertex.color == [0.18, 0.18, 0.145, 0.34]
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == [0.055, 0.06, 0.068, 1.0] && vertex.position[2] > 3.0
        }));
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.color == [0.012, 0.015, 0.018, 1.0])
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.surface_response[2] > 0.4)
        );
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
                && vertex.position[0].abs() > 5.0
        }));
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON)
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.color == [0.018, 0.024, 0.028, 1.0])
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.color == [0.04, 0.18, 0.2, 0.32])
        );
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[0].abs() > 5.0 && vertex.color == [0.16, 0.56, 0.66, 0.46]
        }));
        assert!(
            geometry.vertices.iter().any(|vertex| {
                vertex.position[2] > 2.0 && vertex.color == [0.58, 0.2, 0.76, 0.62]
            })
        );
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
                && vertex.color == [0.012, 0.011, 0.01, 0.92]
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == [0.76, 0.62, 0.42, 0.12]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_DEFAULT
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == [0.28, 0.16, 0.07, 0.9]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == [0.12, 0.13, 0.118, 0.95]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_METAL
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == [0.86, 0.72, 0.42, 0.28]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == [0.18, 0.21, 0.19, 0.96]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_METAL
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == [0.18, 0.31, 0.34, 0.46]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_GLASS
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == [0.78, 0.64, 0.38, 0.34]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == [0.018, 0.015, 0.012, 0.78]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.07
                && vertex.color[1] == 0.056
                && vertex.color[2] == 0.038
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == [0.052, 0.058, 0.052, 0.42]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_METAL
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == [0.62, 0.58, 0.42, 0.38]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_METAL
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[0].abs() > 5.0 && vertex.color[3] < 0.5 && vertex.color[0] <= 0.08
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[2] > 6.5
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[2] > 3.4
                && vertex.color == [0.07, 0.086, 0.094, 1.0]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_METAL
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[2] > 4.0
                && vertex.color[3] > 0.3
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == [0.13, 0.12, 0.105, 0.98]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[2] <= 0.023
                && vertex.color[0] <= 0.055
                && vertex.color[1] >= 0.12
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == [0.92, 0.68, 0.36, 0.3]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == [0.012, 0.012, 0.011, 0.84]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == [0.032, 0.031, 0.029, 0.82]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
        }));
    }

    #[test]
    fn window_alley_weather_adds_animated_rain_grounded_splashes_and_wisps() {
        let camera = WindowPerspectiveCamera::new([0.0, -3.0, 1.65], 0.0, 0.0);
        let mut first = WindowSceneGeometry::default();
        let mut second = WindowSceneGeometry::default();

        first.add_window_alley_weather(1, camera);
        second.add_window_alley_weather(40, camera);

        assert!(first.vertices.len() > 680);
        assert_eq!(first.vertices.len(), second.vertices.len());
        assert_eq!(first.indices.len(), first.vertices.len() / 4 * 6);
        assert!(first.vertices.iter().all(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && (dot3(vertex.normal, vertex.normal).sqrt() - 1.0).abs() < 0.001
        }));
        assert!(
            first.vertices.iter().any(|vertex| {
                vertex.color == [0.5, 0.62, 0.72, 0.24] && vertex.position[2] > 2.0
            })
        );
        assert!(first.vertices.iter().any(|vertex| {
            vertex.color[0] >= 0.45
                && vertex.color[1] >= 0.6
                && vertex.color[2] >= 0.62
                && vertex.color[3] < 0.2
        }));
        assert!(first.vertices.iter().any(|vertex| {
            vertex.position[2] <= 0.022
                && vertex.color == [0.032, 0.078, 0.092, 0.16]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD
        }));
        assert!(first.vertices.iter().any(|vertex| {
            vertex.position[2] <= 0.03
                && vertex.color[0] == 0.44
                && vertex.color[1] == 0.58
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD
        }));
        assert!(!first.vertices.iter().any(|vertex| {
            vertex.color == [0.05, 0.12, 0.16, 0.12]
                || (vertex.color[0] == 0.38 && vertex.color[1] == 0.56)
        }));
        assert!(!first.vertices.iter().any(|vertex| {
            vertex.color[3] >= 0.1 && vertex.position[2] > 0.4 && vertex.color[0] == 0.55
        }));
        assert!(
            first
                .vertices
                .iter()
                .zip(second.vertices.iter())
                .any(|(left, right)| left.position != right.position)
        );
    }

    #[test]
    fn window_gas_volume_adds_layered_visibility_billboards_and_rings() {
        let camera = WindowPerspectiveCamera::new([0.0, -3.0, 1.65], 0.0, 0.0);
        let visual = WindowGasVolumeVisual::new([-1.5, 0.5, 0.4], [0.58, 0.72, 0.75, 0.36])
            .with_radius(1.7)
            .with_height(2.4)
            .with_visibility_blocking(0.45)
            .with_hazard_level(0.28)
            .with_phase_seed(0.17);
        let mut first = WindowSceneGeometry::default();
        let mut second = WindowSceneGeometry::default();

        first.add_window_gas_volume(visual, 1, camera);
        second.add_window_gas_volume(visual, 38, camera);

        assert!(first.vertices.len() > 200);
        assert_eq!(first.vertices.len(), second.vertices.len());
        assert_eq!(first.indices.len(), first.vertices.len() / 4 * 6);
        assert!(first.vertices.iter().all(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && (dot3(vertex.normal, vertex.normal).sqrt() - 1.0).abs() < 0.001
        }));
        assert!(first.vertices.iter().any(|vertex| {
            vertex.position[2] > visual.center[2] + 1.0 && vertex.color[3] > 0.1
        }));
        assert!(first.vertices.iter().any(|vertex| {
            vertex.position[2] <= visual.center[2] + 0.05 && vertex.color[3] < 0.16
        }));
        assert!(
            first
                .vertices
                .iter()
                .zip(second.vertices.iter())
                .any(|(left, right)| left.position != right.position)
        );
    }

    #[test]
    fn window_perspective_camera_projects_forward_points_and_culls_behind() {
        let camera =
            WindowPerspectiveCamera::new([2.0, -1.0, 1.65], std::f32::consts::FRAC_PI_2, 0.0);
        let forward_point = [
            camera.position[0] + 3.0,
            camera.position[1],
            camera.position[2],
        ];
        let behind_point = [
            camera.position[0] - 3.0,
            camera.position[1],
            camera.position[2],
        ];
        let farther_point = [
            camera.position[0] + 8.0,
            camera.position[1],
            camera.position[2],
        ];
        let above_point = [
            camera.position[0] + 3.0,
            camera.position[1],
            camera.position[2] + 1.0,
        ];
        let below_point = [
            camera.position[0] + 3.0,
            camera.position[1],
            camera.position[2] - 1.0,
        ];

        let projected = camera
            .projected_ndc(forward_point)
            .expect("point in front should project");
        let farther_projected = camera
            .projected_ndc(farther_point)
            .expect("farther point should project");
        let above_projected = camera
            .projected_ndc(above_point)
            .expect("above point should project");
        let below_projected = camera
            .projected_ndc(below_point)
            .expect("below point should project");

        assert!(projected[0].abs() < 0.001);
        assert!(projected[1].abs() < 0.001);
        assert!(projected[2] > 0.0);
        assert!(projected[2] < 1.0);
        assert!(farther_projected[2] > projected[2]);
        assert!(above_projected[1] < projected[1]);
        assert!(below_projected[1] > projected[1]);
        assert!(camera.projected_ndc(behind_point).is_none());
    }

    #[test]
    fn window_camera_controls_use_u61q_style_mouse_sensitivity_and_pitch_limits() {
        let mut camera = WindowPerspectiveCamera::new([0.0, 0.0, 1.65], 0.25, 0.0);
        let input = WindowInputState {
            mouse_delta: [400.0, 1_000.0],
            move_up: true,
            fast_modifier: true,
            ..WindowInputState::default()
        };

        camera.apply_controls(&input, 1.0, &WindowCameraControlSettings::default());

        assert!((camera.yaw_radians - 1.25).abs() < 0.0001);
        assert_eq!(camera.pitch_radians, -1.2);
        assert!((camera.position[2] - 10.65).abs() < 0.0001);
    }

    #[test]
    fn window_camera_aspect_tracks_viewport_and_ignores_invalid_sizes() {
        let mut camera = WindowPerspectiveCamera::new([0.0, 0.0, 1.65], 0.0, 0.0);
        let input = WindowInputState {
            viewport_size_pixels: [1024.0, 768.0],
            ..WindowInputState::default()
        };

        assert!((input.viewport_aspect_ratio() - 4.0 / 3.0).abs() < 0.0001);
        camera.set_viewport_size_pixels(input.viewport_size_pixels);
        assert!((camera.aspect_ratio - 4.0 / 3.0).abs() < 0.0001);

        camera.set_viewport_size_pixels([0.0, 768.0]);
        assert!((camera.aspect_ratio - 4.0 / 3.0).abs() < 0.0001);
    }

    #[test]
    fn window_known_mesh_library_emits_fractured_glass_shards() {
        let mut geometry = WindowSceneGeometry::default();
        let material_state = MaterialState {
            crack_density: 0.82,
            soot: 0.6,
            temperature: 460.0,
            ..MaterialState::default()
        };

        assert!(
            geometry.add_known_procedural_mesh(
                WindowProceduralMeshInstance::new(
                    MeshAssetHandle(WINDOW_MESH_ALLEY_GLASS_FRACTURED),
                    [2.0, 0.0],
                    0.0,
                    [0.62, 0.88, 1.0, 0.86],
                )
                .with_material_state(Some(&material_state)),
            )
        );

        assert!(geometry.vertices.chunks_exact(4).any(|quad| {
            quad[2].position == quad[3].position
                && quad[0].position[2] <= 0.05
                && quad[1].position[2] <= 0.06
                && quad[2].position[2] <= 0.05
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.035
                && vertex.color[1] == 0.03
                && vertex.color[2] == 0.028
                && vertex.color[3] > 0.3
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[2] > 0.9 && vertex.surface_response == WINDOW_SURFACE_RESPONSE_GLASS
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 1.0
                && vertex.color[1] == 0.38
                && vertex.color[2] == 0.14
                && vertex.color[3] > 0.15
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[2] > 0.8
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_GLASS
                && vertex.material_detail[2] > 0.75
                && vertex.material_detail[3] > 0.65
        }));
    }

    #[test]
    fn window_known_mesh_library_handles_neon_pipe_and_wet_asphalt() {
        let mut geometry = WindowSceneGeometry::default();
        let neon_state = MaterialState {
            electrical_charge: 144.0,
            temperature: 380.0,
            ..MaterialState::default()
        };
        let pipe_state = MaterialState {
            moisture: 0.75,
            corrosion: 0.48,
            soot: 0.36,
            temperature: 540.0,
            ..MaterialState::default()
        };
        let asphalt_state = MaterialState {
            moisture: 0.75,
            soot: 0.42,
            ..MaterialState::default()
        };

        assert!(
            geometry.add_known_procedural_mesh(
                WindowProceduralMeshInstance::new(
                    MeshAssetHandle(WINDOW_MESH_NEON_SIGN),
                    [1.0, 3.0],
                    2.5,
                    [0.96, 0.22, 0.75, 1.0],
                )
                .with_material_state(Some(&neon_state))
                .with_billboard_right([0.0, -1.0, 0.0]),
            )
        );
        assert!(
            geometry.add_known_procedural_mesh(
                WindowProceduralMeshInstance::new(
                    MeshAssetHandle(WINDOW_MESH_SERVICE_PIPE),
                    [-1.5, 0.5],
                    0.4,
                    [0.15, 0.52, 0.9, 0.88],
                )
                .with_material_state(Some(&pipe_state)),
            )
        );
        assert!(
            geometry.add_known_procedural_mesh(
                WindowProceduralMeshInstance::new(
                    MeshAssetHandle(WINDOW_MESH_WET_ASPHALT),
                    [-0.8, 0.0],
                    -0.02,
                    [0.05, 0.065, 0.075, 0.94],
                )
                .with_material_state(Some(&asphalt_state)),
            )
        );
        assert!(
            !geometry.add_known_procedural_mesh(WindowProceduralMeshInstance::new(
                MeshAssetHandle(999_999),
                [0.0, 0.0],
                0.0,
                [1.0, 1.0, 1.0, 1.0],
            ))
        );

        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == [0.34, 0.54, 0.58, 0.46]
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[3] < 0.65
                && vertex.color[2] > vertex.color[0]
                && vertex.position[2] <= 0.06
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.72
                && vertex.color[1] == 0.32
                && vertex.color[2] == 0.52
                && vertex.color[3] > 0.15
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.34
                && vertex.color[1] == 0.16
                && vertex.color[2] == 0.06
                && vertex.color[3] > 0.5
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
        }));
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.color == [0.54, 0.86, 1.0, 0.58]
                    && vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD)
        );
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.42
                && vertex.color[1] == 0.82
                && vertex.color[2] == 1.0
                && vertex.color[3] > 0.2
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.025
                && vertex.color[1] == 0.02
                && vertex.color[2] == 0.018
                && vertex.color[3] > 0.25
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.position[2] <= 0.03
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD
                && vertex.material_detail[2] > 0.45
                && vertex.material_detail[3] > 0.25
        }));
    }

    #[test]
    fn window_known_mesh_library_handles_camera_door_and_market_stall() {
        let mut geometry = WindowSceneGeometry::default();
        let camera_state = MaterialState {
            moisture: 0.28,
            soot: 0.18,
            corrosion: 0.22,
            electrical_charge: 64.0,
            ..MaterialState::default()
        };
        let door_state = MaterialState {
            moisture: 0.34,
            soot: 0.44,
            corrosion: 0.58,
            crack_density: 0.16,
            electrical_charge: 18.0,
            ..MaterialState::default()
        };
        let stall_state = MaterialState {
            moisture: 0.48,
            soot: 0.26,
            corrosion: 0.31,
            crack_density: 0.08,
            electrical_charge: 32.0,
            ..MaterialState::default()
        };

        assert!(
            geometry.add_known_procedural_mesh(
                WindowProceduralMeshInstance::new(
                    MeshAssetHandle(WINDOW_MESH_SECURITY_CAMERA),
                    [-4.72, -4.35],
                    2.36,
                    [0.55, 0.78, 0.9, 1.0],
                )
                .with_material_state(Some(&camera_state))
                .with_billboard_right([0.0, -1.0, 0.0]),
            )
        );
        assert!(
            geometry.add_known_procedural_mesh(
                WindowProceduralMeshInstance::new(
                    MeshAssetHandle(WINDOW_MESH_SERVICE_DOOR),
                    [-5.05, 6.65],
                    0.08,
                    [0.08, 0.09, 0.095, 1.0],
                )
                .with_material_state(Some(&door_state))
                .with_billboard_right([0.0, -1.0, 0.0]),
            )
        );
        assert!(
            geometry.add_known_procedural_mesh(
                WindowProceduralMeshInstance::new(
                    MeshAssetHandle(WINDOW_MESH_MARKET_STALL),
                    [3.62, -4.85],
                    0.06,
                    [1.0, 0.32, 0.66, 1.0],
                )
                .with_material_state(Some(&stall_state))
                .with_billboard_right([0.0, -1.0, 0.0]),
            )
        );

        assert!(geometry.vertices.len() > 900);
        assert!(
            geometry
                .vertices
                .iter()
                .any(
                    |vertex| vertex.surface_response == WINDOW_SURFACE_RESPONSE_GLASS
                        && vertex.color[2] > 0.5
                )
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(
                    |vertex| vertex.surface_response == WINDOW_SURFACE_RESPONSE_METAL
                        && vertex.position[2] > 1.5
                )
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(
                    |vertex| vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
                        && vertex.color[0] > vertex.color[1]
                )
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(
                    |vertex| vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD
                        && vertex.color[3] > 0.3
                )
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(
                    |vertex| vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
                        && vertex.color[3] > 0.1
                )
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(
                    |vertex| vertex.surface_response == WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE
                        && vertex.color[0] >= 0.78
                )
        );
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_METAL
                && vertex.position[2] > 2.0
                && [vertex.normal[0], vertex.normal[1], vertex.normal[2]]
                    .into_iter()
                    .filter(|component| component.abs() > 0.2)
                    .count()
                    >= 2
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[0] == 0.32
                && vertex.color[1] == 0.68
                && vertex.color[2] == 0.48
                && (0.13..0.15).contains(&vertex.color[3])
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
        }));
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.material_detail != WINDOW_SURFACE_DETAIL_DEFAULT)
        );
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_METAL
                && vertex.material_detail[0] > 2.0
                && vertex.material_detail[2] > 0.45
                && vertex.material_detail[3] > 0.4
        }));
    }

    #[test]
    fn stateful_hard_surface_boxes_emit_edge_wear_microgeometry() {
        let min = [-0.06, -0.8, 0.0];
        let max = [0.06, 0.8, 2.1];
        let color = [0.08, 0.09, 0.095, 1.0];
        let neutral_detail =
            window_material_state_detail_from_channels(WindowMaterialStateChannels::default());
        let damaged_detail =
            window_material_state_detail_from_channels(WindowMaterialStateChannels {
                crack_density: 0.62,
                moisture: 0.36,
                soot: 0.4,
                corrosion: 0.68,
                ..WindowMaterialStateChannels::default()
            });
        let recipe = WindowMicroDetailRecipe {
            seed: 71_900,
            density: 0.74,
        };

        let mut direct_neutral = WindowSceneGeometry::default();
        direct_neutral.add_world_box_state_edge_wear(
            min,
            max,
            color,
            WINDOW_SURFACE_RESPONSE_METAL,
            recipe.seed,
            neutral_detail,
        );
        assert!(direct_neutral.vertices.is_empty());

        let mut direct_damaged = WindowSceneGeometry::default();
        direct_damaged.add_world_box_state_edge_wear(
            min,
            max,
            color,
            WINDOW_SURFACE_RESPONSE_METAL,
            recipe.seed,
            damaged_detail,
        );
        assert!(direct_damaged.vertices.len() >= 48);
        assert!(direct_damaged.vertices.iter().any(|vertex| {
            (vertex.position[0] < min[0]
                || vertex.position[0] > max[0]
                || vertex.position[1] < min[1]
                || vertex.position[1] > max[1])
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_METAL
                && vertex.material_detail[2] > 0.85
                && vertex.material_detail[3] > 0.75
        }));
        assert!(direct_damaged.vertices.iter().any(|vertex| {
            vertex.color[3] < 0.82
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_METAL
                && vertex.material_detail[0] > 2.0
        }));

        let mut neutral_mesh = WindowSceneGeometry::default();
        neutral_mesh.world_micro_detailed_box_with_surface_state_detail(
            min,
            max,
            color,
            WINDOW_SURFACE_RESPONSE_METAL,
            recipe,
            neutral_detail,
        );
        let mut damaged_mesh = WindowSceneGeometry::default();
        damaged_mesh.world_micro_detailed_box_with_surface_state_detail(
            min,
            max,
            color,
            WINDOW_SURFACE_RESPONSE_METAL,
            recipe,
            damaged_detail,
        );
        assert!(damaged_mesh.vertices.len() > neutral_mesh.vertices.len());
    }

    #[test]
    fn stateful_fractured_boxes_emit_cracks_and_exposed_interior_geometry() {
        let min = [-0.2, -0.7, 0.0];
        let max = [0.2, 0.7, 1.9];
        let color = [0.08, 0.09, 0.095, 1.0];
        let neutral_detail =
            window_material_state_detail_from_channels(WindowMaterialStateChannels::default());
        let fractured_detail =
            window_material_state_detail_from_channels(WindowMaterialStateChannels {
                crack_density: 0.82,
                moisture: 0.18,
                soot: 0.36,
                corrosion: 0.58,
                ..WindowMaterialStateChannels::default()
            });
        let recipe = WindowMicroDetailRecipe {
            seed: 72_400,
            density: 0.76,
        };

        let mut direct_neutral = WindowSceneGeometry::default();
        direct_neutral.add_world_box_state_fracture_detail(
            min,
            max,
            color,
            WINDOW_SURFACE_RESPONSE_METAL,
            recipe.seed,
            neutral_detail,
        );
        assert!(direct_neutral.vertices.is_empty());

        let mut direct_fractured = WindowSceneGeometry::default();
        direct_fractured.add_world_box_state_fracture_detail(
            min,
            max,
            color,
            WINDOW_SURFACE_RESPONSE_METAL,
            recipe.seed,
            fractured_detail,
        );
        assert!(direct_fractured.vertices.len() >= 96);
        assert!(direct_fractured.vertices.iter().any(|vertex| {
            (vertex.position[0] < min[0]
                || vertex.position[0] > max[0]
                || vertex.position[1] < min[1]
                || vertex.position[1] > max[1])
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
                && vertex.color[0] < color[0] * 2.4
                && vertex.color[3] > 0.3
                && vertex.material_detail[2] > 0.8
                && vertex.material_detail[3] > 0.8
        }));

        let mut neutral_mesh = WindowSceneGeometry::default();
        neutral_mesh.world_micro_detailed_box_with_surface_state_detail(
            min,
            max,
            color,
            WINDOW_SURFACE_RESPONSE_METAL,
            recipe,
            neutral_detail,
        );
        let mut fractured_mesh = WindowSceneGeometry::default();
        fractured_mesh.world_micro_detailed_box_with_surface_state_detail(
            min,
            max,
            color,
            WINDOW_SURFACE_RESPONSE_METAL,
            recipe,
            fractured_detail,
        );
        assert!(fractured_mesh.vertices.len() > neutral_mesh.vertices.len());
        assert!(fractured_mesh.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
                && vertex.material_detail[3] > 0.8
        }));
    }

    #[test]
    fn stateful_energetic_boxes_emit_heat_and_electrical_microgeometry() {
        let min = [-0.14, -0.72, 0.0];
        let max = [0.14, 0.72, 1.8];
        let color = [0.08, 0.09, 0.095, 1.0];
        let neutral_detail =
            window_material_state_detail_from_channels(WindowMaterialStateChannels::default());
        let energy_detail =
            window_material_state_detail_from_channels(WindowMaterialStateChannels {
                moisture: 0.05,
                soot: 0.28,
                corrosion: 0.18,
                heat: 0.72,
                electrical_charge: 0.68,
                ..WindowMaterialStateChannels::default()
            });
        let recipe = WindowMicroDetailRecipe {
            seed: 72_980,
            density: 0.7,
        };

        let mut direct_neutral = WindowSceneGeometry::default();
        direct_neutral.add_world_box_state_energy_detail(
            min,
            max,
            color,
            WINDOW_SURFACE_RESPONSE_METAL,
            recipe.seed,
            neutral_detail,
        );
        assert!(direct_neutral.vertices.is_empty());

        let mut direct_energy = WindowSceneGeometry::default();
        direct_energy.add_world_box_state_energy_detail(
            min,
            max,
            color,
            WINDOW_SURFACE_RESPONSE_METAL,
            recipe.seed,
            energy_detail,
        );
        assert!(direct_energy.vertices.len() >= 80);
        assert!(direct_energy.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE
                && vertex.color[0] >= 1.0
                && vertex.color[3] > 0.3
                && vertex.material_detail[2] > 0.65
        }));
        assert!(direct_energy.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
                && vertex.color[2] >= 1.0
                && vertex.color[3] > 0.5
                && vertex.material_detail[2] > 0.6
        }));

        let mut neutral_mesh = WindowSceneGeometry::default();
        neutral_mesh.world_micro_detailed_box_with_surface_state_detail(
            min,
            max,
            color,
            WINDOW_SURFACE_RESPONSE_METAL,
            recipe,
            neutral_detail,
        );
        let mut energy_mesh = WindowSceneGeometry::default();
        energy_mesh.world_micro_detailed_box_with_surface_state_detail(
            min,
            max,
            color,
            WINDOW_SURFACE_RESPONSE_METAL,
            recipe,
            energy_detail,
        );
        assert!(energy_mesh.vertices.len() > neutral_mesh.vertices.len());
        assert!(energy_mesh.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_EMISSIVE_NEON
                || vertex.surface_response == WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE
        }));
    }

    #[test]
    fn stateful_deformed_boxes_emit_silhouette_dents_and_contamination_layers() {
        let min = [-0.16, -0.64, 0.0];
        let max = [0.16, 0.64, 1.8];
        let color = [0.08, 0.09, 0.095, 1.0];
        let neutral_detail =
            window_material_state_detail_from_channels(WindowMaterialStateChannels::default());
        let deformed_detail =
            window_material_state_detail_from_channels(WindowMaterialStateChannels {
                crack_density: 0.08,
                moisture: 0.32,
                soot: 0.2,
                corrosion: 0.18,
                plastic_strain: 0.74,
                biological_contamination: 0.62,
                oil_contamination: 0.58,
                ..WindowMaterialStateChannels::default()
            });
        let recipe = WindowMicroDetailRecipe {
            seed: 73_040,
            density: 0.72,
        };

        let mut direct_neutral = WindowSceneGeometry::default();
        direct_neutral.add_world_box_state_deformation_layer_detail(
            min,
            max,
            color,
            WINDOW_SURFACE_RESPONSE_METAL,
            recipe.seed,
            neutral_detail,
        );
        assert!(direct_neutral.vertices.is_empty());

        let mut direct_deformed = WindowSceneGeometry::default();
        direct_deformed.add_world_box_state_deformation_layer_detail(
            min,
            max,
            color,
            WINDOW_SURFACE_RESPONSE_METAL,
            recipe.seed,
            deformed_detail,
        );
        assert!(direct_deformed.vertices.len() >= 96);
        assert!(direct_deformed.vertices.iter().any(|vertex| {
            (vertex.position[0] < min[0]
                || vertex.position[0] > max[0]
                || vertex.position[1] < min[1]
                || vertex.position[1] > max[1])
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_METAL
                && vertex.material_detail[2] > 0.7
                && vertex.material_detail[3] > 0.55
        }));
        assert!(direct_deformed.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD
                && vertex.color[0] > 0.12
                && vertex.color[0] > vertex.color[2] * 2.0
                && vertex.color[3] > 0.25
                && vertex.material_detail[2] > 0.7
        }));

        let mut neutral_mesh = WindowSceneGeometry::default();
        neutral_mesh.world_micro_detailed_box_with_surface_state_detail(
            min,
            max,
            color,
            WINDOW_SURFACE_RESPONSE_METAL,
            recipe,
            neutral_detail,
        );
        let mut deformed_mesh = WindowSceneGeometry::default();
        deformed_mesh.world_micro_detailed_box_with_surface_state_detail(
            min,
            max,
            color,
            WINDOW_SURFACE_RESPONSE_METAL,
            recipe,
            deformed_detail,
        );
        assert!(deformed_mesh.vertices.len() > neutral_mesh.vertices.len());
        assert!(
            deformed_mesh
                .vertices
                .iter()
                .any(|vertex| vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD)
        );
    }

    #[test]
    fn stateful_grounded_boxes_emit_contact_grime_wetness_and_debris() {
        let min = [-0.18, -0.62, 0.0];
        let max = [0.18, 0.62, 1.65];
        let color = [0.08, 0.09, 0.095, 1.0];
        let neutral_detail =
            window_material_state_detail_from_channels(WindowMaterialStateChannels::default());
        let wet_dirty_detail =
            window_material_state_detail_from_channels(WindowMaterialStateChannels {
                crack_density: 0.18,
                moisture: 0.72,
                soot: 0.54,
                corrosion: 0.42,
                ..WindowMaterialStateChannels::default()
            });
        let recipe = WindowMicroDetailRecipe {
            seed: 73_100,
            density: 0.72,
        };

        let mut direct_neutral = WindowSceneGeometry::default();
        direct_neutral.add_world_box_state_contact_detail(
            min,
            max,
            color,
            WINDOW_SURFACE_RESPONSE_METAL,
            recipe.seed,
            neutral_detail,
        );
        assert!(direct_neutral.vertices.is_empty());

        let mut direct_contact = WindowSceneGeometry::default();
        direct_contact.add_world_box_state_contact_detail(
            min,
            max,
            color,
            WINDOW_SURFACE_RESPONSE_METAL,
            recipe.seed,
            wet_dirty_detail,
        );
        assert!(direct_contact.vertices.len() >= 64);
        assert!(direct_contact.vertices.iter().any(|vertex| {
            vertex.position[2] <= 0.03
                && (vertex.position[0] < min[0]
                    || vertex.position[0] > max[0]
                    || vertex.position[1] < min[1]
                    || vertex.position[1] > max[1])
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
                && vertex.material_detail[2] > 0.7
        }));
        assert!(direct_contact.vertices.iter().any(|vertex| {
            vertex.position[2] <= 0.03
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD
                && vertex.color[2] > vertex.color[0]
                && vertex.material_detail[2] > 0.7
        }));

        let mut neutral_mesh = WindowSceneGeometry::default();
        neutral_mesh.world_micro_detailed_box_with_surface_state_detail(
            min,
            max,
            color,
            WINDOW_SURFACE_RESPONSE_METAL,
            recipe,
            neutral_detail,
        );
        let mut wet_dirty_mesh = WindowSceneGeometry::default();
        wet_dirty_mesh.world_micro_detailed_box_with_surface_state_detail(
            min,
            max,
            color,
            WINDOW_SURFACE_RESPONSE_METAL,
            recipe,
            wet_dirty_detail,
        );
        assert!(wet_dirty_mesh.vertices.len() > neutral_mesh.vertices.len());
        assert!(wet_dirty_mesh.vertices.iter().any(|vertex| {
            vertex.position[2] <= 0.03
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD
        }));
    }

    #[test]
    fn window_known_mesh_library_handles_human_bundle() {
        let mut geometry = WindowSceneGeometry::default();
        let color = [0.96, 0.72, 0.28, 1.0];

        assert!(
            geometry.add_known_procedural_mesh(
                WindowProceduralMeshInstance::new(
                    MeshAssetHandle(WINDOW_MESH_MARA_HUMAN_BUNDLE),
                    [5.5, 0.0],
                    0.0,
                    color,
                )
                .with_billboard_right([1.0, 0.0, 0.0]),
            )
        );

        let normal_proxy = human_proxy_geometry_for_quality(QualityTier::NormalRuntime);
        assert_eq!(normal_proxy.body_parts.len(), 18);
        assert!(
            normal_proxy
                .body_parts
                .iter()
                .any(|part| part.part == HumanProxyPart::LeftHand)
        );
        assert!(
            normal_proxy
                .body_parts
                .iter()
                .any(|part| part.part == HumanProxyPart::RightFoot)
        );
        assert!(
            !normal_proxy
                .body_parts
                .iter()
                .any(|part| part.part == HumanProxyPart::Torso)
        );

        assert!(geometry.vertices.len() > 1_200);
        assert_eq!(geometry.vertices.len() % 4, 0);
        assert_eq!(geometry.indices.len(), geometry.vertices.len() / 4 * 6);
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_default_skin_color(color)
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_hair_color(color, None)
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_clothing_color(color)
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_trim_color()
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
        for surface_response in [
            WINDOW_SURFACE_RESPONSE_HUMAN_SKIN,
            WINDOW_SURFACE_RESPONSE_HUMAN_EYE,
            WINDOW_SURFACE_RESPONSE_HUMAN_HAIR,
            WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH,
            WINDOW_SURFACE_RESPONSE_HUMAN_CYBERNETIC,
        ] {
            assert!(
                geometry
                    .vertices
                    .iter()
                    .any(|vertex| vertex.surface_response == surface_response),
                "{surface_response:?} should be present in the known human bundle"
            );
        }
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_SKIN
                && vertex.material_detail != WINDOW_SURFACE_DETAIL_DEFAULT
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_skin_microdetail_color(color, None)
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_SKIN
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_eye_tearline_color(None)
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_EYE
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_skin_soft_shadow_color(color, None)
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_SKIN
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_clothing_fold_shadow_color(color, None)
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_boot_sole_color(color, None)
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH
        }));
    }

    #[test]
    fn window_background_humanoid_lod_stays_human_shaped_instead_of_box_impostor() {
        let mut geometry = WindowSceneGeometry::default();
        let human = HumanState {
            human_id: 404,
            quality_tier: QualityTier::BackgroundApproximation,
        };
        let color = [0.62, 0.48, 0.36, 1.0];

        geometry.add_humanoid_proxy(
            WindowHumanoidProxyInstance::new([1.25, -0.75], 0.0, color)
                .with_human(Some(&human))
                .with_facing_yaw_radians(0.35),
        );

        let proxy = human_proxy_geometry_for_state(&human);
        assert_eq!(proxy.quality_tier, QualityTier::BackgroundApproximation);
        assert_eq!(proxy.body_parts.len(), 18);
        assert!(proxy.face_features.is_empty());
        assert!(!proxy.detail_profile.supports_photoreal_near_proxy());
        assert!(
            !proxy
                .body_parts
                .iter()
                .any(|part| part.part == HumanProxyPart::Impostor)
        );
        for expected in [
            HumanProxyPart::LeftFoot,
            HumanProxyPart::RightFoot,
            HumanProxyPart::Pelvis,
            HumanProxyPart::Chest,
            HumanProxyPart::LeftHand,
            HumanProxyPart::RightHand,
            HumanProxyPart::Head,
            HumanProxyPart::HairCap,
        ] {
            assert!(
                proxy.body_parts.iter().any(|part| part.part == expected),
                "{expected:?} should remain in the background human silhouette"
            );
        }

        let old_box_impostor_vertex_count = 6 * 4;
        assert!(
            geometry.vertices.len() > old_box_impostor_vertex_count * 20,
            "background humans should render as a body silhouette, not one box"
        );
        assert_eq!(geometry.vertices.len() % 4, 0);
        assert_eq!(geometry.indices.len(), geometry.vertices.len() / 4 * 6);
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_SKIN
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_HAIR
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.position[2] > proxy.height_meters * 0.90)
        );
    }

    #[test]
    fn window_humanoid_proxy_uses_human_lod_and_emotion_for_faces() {
        let mut geometry = WindowSceneGeometry::default();
        let human = HumanState {
            human_id: 300,
            quality_tier: QualityTier::HeroHighFidelityRuntime,
        };
        let emotion = EmotionState::alarmed();
        let color = [0.96, 0.72, 0.28, 1.0];

        geometry.add_humanoid_proxy(
            WindowHumanoidProxyInstance::new([5.5, 0.0], 0.0, color)
                .with_human(Some(&human))
                .with_emotion(Some(&emotion))
                .with_viewer_position([5.5, -1.25, 1.25]),
        );

        let proxy = human_proxy_geometry_for_state(&human);
        assert!(proxy.detail_profile.supports_photoreal_near_proxy());
        assert!(proxy.detail_profile.face_anatomy_feature_count >= 12);
        assert!(proxy.detail_profile.anatomical_joint_count >= 18);
        assert!(proxy.detail_profile.pose_deformation_zone_count >= 8);
        assert!(proxy.detail_profile.garment_layer_count >= 3);
        assert!(proxy.detail_profile.footwear_detail_count >= 6);
        assert!(proxy.detail_profile.hairline_detail_count >= 12);
        assert!(proxy.detail_profile.limb_volume_layer_count >= 12);
        assert!(proxy.detail_profile.soft_tissue_form_count >= 8);
        for expected in [
            HumanProxyFaceFeatureKind::NoseBridge,
            HumanProxyFaceFeatureKind::NoseTip,
            HumanProxyFaceFeatureKind::LeftCheek,
            HumanProxyFaceFeatureKind::RightCheek,
            HumanProxyFaceFeatureKind::LeftBrow,
            HumanProxyFaceFeatureKind::RightBrow,
            HumanProxyFaceFeatureKind::LeftEar,
            HumanProxyFaceFeatureKind::RightEar,
            HumanProxyFaceFeatureKind::Chin,
            HumanProxyFaceFeatureKind::JawShadow,
        ] {
            assert!(
                proxy
                    .face_features
                    .iter()
                    .any(|feature| feature.kind == expected),
                "{expected:?} should be present in the hero face proxy"
            );
        }
        let old_box_proxy_vertex_count = 6 * 6 * 4 + 3 * 4;
        assert!(geometry.vertices.len() > old_box_proxy_vertex_count);
        assert!(
            geometry.vertices.len() > 3_000,
            "hero humanoid proxy should emit dense layered photoreal geometry"
        );
        assert_eq!(geometry.vertices.len() % 4, 0);
        assert_eq!(geometry.indices.len(), geometry.vertices.len() / 4 * 6);
        assert!(geometry.vertices.iter().all(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && (dot3(vertex.normal, vertex.normal).sqrt() - 1.0).abs() < 0.001
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.normal[0].abs() > 0.2
                && vertex.normal[1].abs() > 0.2
                && vertex.normal[2].abs() > 0.2
        }));
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.color == humanoid_default_skin_color(color))
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.color == humanoid_hair_color(color, None))
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.color == humanoid_clothing_color(color))
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.color == humanoid_trim_color())
        );
        let nose_tip = proxy
            .face_features
            .iter()
            .find(|feature| feature.kind == HumanProxyFaceFeatureKind::NoseTip)
            .copied()
            .expect("hero proxy should include a nose tip");
        let cheek = proxy
            .face_features
            .iter()
            .find(|feature| feature.kind == HumanProxyFaceFeatureKind::LeftCheek)
            .copied()
            .expect("hero proxy should include cheeks");
        let brow = proxy
            .face_features
            .iter()
            .find(|feature| feature.kind == HumanProxyFaceFeatureKind::LeftBrow)
            .copied()
            .expect("hero proxy should include brows");
        for expected_color in [
            humanoid_face_feature_color(color, nose_tip, Some(&emotion), None),
            humanoid_face_feature_color(color, cheek, Some(&emotion), None),
            humanoid_face_feature_color(color, brow, Some(&emotion), None),
            humanoid_boot_sole_color(color, None),
        ] {
            assert!(
                geometry
                    .vertices
                    .iter()
                    .any(|vertex| vertex.color == expected_color),
                "{expected_color:?} should be emitted by refined hero human geometry"
            );
        }

        let alert =
            (emotion.fear * 0.45 + emotion.anger * 0.2 + emotion.urgency * 0.35).clamp(0.0, 1.0);
        let alert_eye_color = [
            0.12 + alert * 0.82,
            0.86 - alert * 0.42,
            1.0 - alert * 0.72,
            1.0,
        ];
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.color == alert_eye_color)
        );
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == alert_eye_color
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_EYE
        }));
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_HAIR)
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH)
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_CYBERNETIC)
        );
        let skin_vertex_count = geometry
            .vertices
            .iter()
            .filter(|vertex| vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_SKIN)
            .count();
        let hair_vertex_count = geometry
            .vertices
            .iter()
            .filter(|vertex| vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_HAIR)
            .count();
        let cloth_vertex_count = geometry
            .vertices
            .iter()
            .filter(|vertex| vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH)
            .count();
        assert!(skin_vertex_count > 450);
        assert!(hair_vertex_count > proxy.detail_profile.hair_card_count as usize * 4);
        assert!(cloth_vertex_count > 450);
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_eye_tearline_color(None)
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_EYE
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_fingernail_color(color, None)
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_SKIN
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_tooth_enamel_color(color, None)
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_SKIN
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_inner_mouth_color(color, None)
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_SKIN
        }));
        assert!(
            geometry.vertices.iter().all(|vertex| {
                vertex.color != humanoid_limb_volume_color(HumanProxyPart::LeftForearm, color, None)
                    && vertex.color
                        != humanoid_limb_volume_color(HumanProxyPart::LeftThigh, color, None)
                    && vertex.color
                        != humanoid_limb_shadow_color(HumanProxyPart::LeftCalf, color, None)
            }),
            "runtime hero humans should avoid the separated limb-volume stack that reads as fractured geometry"
        );
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_skin_cool_shadow_color(color, None)
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_SKIN
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_soft_tissue_subsurface_color(color, None)
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_SKIN
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_soft_tissue_blush_color(color, None)
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_SKIN
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_clothing_inner_layer_color(color, None)
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_clothing_pressure_color(color, None)
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_hairline_shadow_color(color, None)
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_HAIR
        }));
        assert!(humanoid_trim_color()[1] < 0.7);
        assert!(humanoid_trim_color()[3] < 0.7);
    }

    #[test]
    fn window_mid_distance_humanoid_uses_cohesive_shell_without_fragmented_part_stack() {
        let human = HumanState {
            human_id: 305,
            quality_tier: QualityTier::HeroHighFidelityRuntime,
        };
        let color = [0.88, 0.66, 0.50, 1.0];
        let proxy = human_proxy_geometry_for_state(&human);
        let instance = WindowHumanoidProxyInstance::new([6.0, 0.0], 0.0, color)
            .with_human(Some(&human))
            .with_viewer_position([0.0, 0.0, 1.65])
            .with_facing_yaw_radians(0.25)
            .with_pose_weights(0.45, 0.0);

        assert!(humanoid_allows_mid_detail(&instance, &proxy));
        assert!(!humanoid_allows_near_detail(&instance, &proxy));

        let mut geometry = WindowSceneGeometry::default();
        geometry.add_humanoid_proxy(instance);

        for surface_response in [
            WINDOW_SURFACE_RESPONSE_HUMAN_SKIN,
            WINDOW_SURFACE_RESPONSE_HUMAN_HAIR,
            WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH,
        ] {
            assert!(
                geometry
                    .vertices
                    .iter()
                    .any(|vertex| vertex.surface_response == surface_response),
                "{surface_response:?} should remain visible in the cohesive human shell"
            );
        }
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_default_skin_color(color)
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_SKIN
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_hair_color(color, None)
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_HAIR
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_boot_color(color)
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH
        }));
        assert!(
            geometry.vertices.iter().all(|vertex| {
                vertex.color != humanoid_limb_volume_color(HumanProxyPart::LeftForearm, color, None)
                    && vertex.color
                        != humanoid_limb_volume_color(HumanProxyPart::LeftThigh, color, None)
                    && vertex.color
                        != humanoid_limb_shadow_color(HumanProxyPart::LeftCalf, color, None)
                    && vertex.color != humanoid_fingernail_color(color, None)
            }),
            "mid-distance humans should be a cohesive body shell without close-up limb fragments"
        );

        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for vertex in geometry
            .vertices
            .iter()
            .filter(|vertex| vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE)
        {
            min[0] = min[0].min(vertex.position[0]);
            min[1] = min[1].min(vertex.position[1]);
            min[2] = min[2].min(vertex.position[2]);
            max[0] = max[0].max(vertex.position[0]);
            max[1] = max[1].max(vertex.position[1]);
            max[2] = max[2].max(vertex.position[2]);
        }
        let horizontal_span = (max[0] - min[0]).max(max[1] - min[1]);
        assert!(
            max[2] - min[2] > proxy.height_meters * 0.88,
            "cohesive shell should preserve head-to-foot human height"
        );
        assert!(
            horizontal_span > proxy.height_meters * 0.24,
            "cohesive shell should preserve shoulders, arms, and feet"
        );

        let mut near = WindowSceneGeometry::default();
        near.add_humanoid_proxy(
            WindowHumanoidProxyInstance::new([6.0, 0.0], 0.0, color)
                .with_human(Some(&human))
                .with_viewer_position([6.0, -1.0, 1.25])
                .with_facing_yaw_radians(0.25),
        );
        assert!(
            near.vertices.len() > geometry.vertices.len(),
            "only near humans should add the layered anatomical detail stack"
        );
    }

    #[test]
    fn window_far_hero_humanoid_suppresses_closeup_fragment_detail() {
        let human = HumanState {
            human_id: 303,
            quality_tier: QualityTier::HeroHighFidelityRuntime,
        };
        let color = [0.86, 0.64, 0.46, 1.0];

        let mut far = WindowSceneGeometry::default();
        far.add_humanoid_proxy(
            WindowHumanoidProxyInstance::new([12.0, 0.0], 0.0, color)
                .with_human(Some(&human))
                .with_viewer_position([0.0, 0.0, 1.65]),
        );

        assert!(far.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_SKIN
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
        assert!(far.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
        assert!(
            far.vertices.iter().all(|vertex| {
                vertex.color != humanoid_soft_tissue_subsurface_color(color, None)
                    && vertex.color != humanoid_soft_tissue_blush_color(color, None)
                    && vertex.color != humanoid_clothing_pressure_color(color, None)
                    && vertex.color != humanoid_eye_tearline_color(None)
                    && vertex.color != humanoid_fingernail_color(color, None)
            }),
            "far hero humans should keep a cohesive silhouette instead of close-up fragment detail"
        );

        let mut near = WindowSceneGeometry::default();
        near.add_humanoid_proxy(
            WindowHumanoidProxyInstance::new([1.0, 0.0], 0.0, color)
                .with_human(Some(&human))
                .with_viewer_position([1.0, -1.0, 1.25]),
        );

        assert!(
            near.vertices.len() > far.vertices.len(),
            "near humans should be allowed to add controlled close-up detail"
        );
        assert!(near.vertices.iter().any(|vertex| {
            vertex.color == humanoid_soft_tissue_subsurface_color(color, None)
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_SKIN
        }));
    }

    #[test]
    fn window_humanoid_proxy_uses_explicit_facing_for_body_orientation() {
        let human = HumanState {
            human_id: 301,
            quality_tier: QualityTier::HeroHighFidelityRuntime,
        };
        let color = [0.80, 0.62, 0.46, 1.0];

        let mut facing_x = WindowSceneGeometry::default();
        facing_x.add_humanoid_proxy(
            WindowHumanoidProxyInstance::new([0.0, 0.0], 0.0, color)
                .with_human(Some(&human))
                .with_viewer_position([0.0, -3.0, 1.65])
                .with_facing_yaw_radians(0.0),
        );
        let mut facing_y = WindowSceneGeometry::default();
        facing_y.add_humanoid_proxy(
            WindowHumanoidProxyInstance::new([0.0, 0.0], 0.0, color)
                .with_human(Some(&human))
                .with_viewer_position([0.0, -3.0, 1.65])
                .with_facing_yaw_radians(std::f32::consts::FRAC_PI_2),
        );

        let world_span = |geometry: &WindowSceneGeometry| {
            let mut min = [f32::INFINITY; 2];
            let mut max = [f32::NEG_INFINITY; 2];
            for vertex in geometry
                .vertices
                .iter()
                .filter(|vertex| vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE)
            {
                min[0] = min[0].min(vertex.position[0]);
                min[1] = min[1].min(vertex.position[1]);
                max[0] = max[0].max(vertex.position[0]);
                max[1] = max[1].max(vertex.position[1]);
            }
            [max[0] - min[0], max[1] - min[1]]
        };
        let x_span = world_span(&facing_x);
        let y_span = world_span(&facing_y);

        assert!(
            x_span[1] > x_span[0] * 1.04,
            "facing +X should rotate shoulder/body width onto world Y: {x_span:?}"
        );
        assert!(
            y_span[0] > y_span[1] * 1.04,
            "facing +Y should rotate shoulder/body width onto world X: {y_span:?}"
        );
    }

    #[test]
    fn window_humanoid_proxy_uses_pose_weights_for_non_rigid_limbs() {
        let human = HumanState {
            human_id: 302,
            quality_tier: QualityTier::HeroHighFidelityRuntime,
        };
        let color = [0.82, 0.62, 0.48, 1.0];

        let mut idle = WindowSceneGeometry::default();
        idle.add_humanoid_proxy(
            WindowHumanoidProxyInstance::new([0.0, 0.0], 0.0, color)
                .with_human(Some(&human))
                .with_facing_yaw_radians(0.0),
        );
        let mut moving = WindowSceneGeometry::default();
        moving.add_humanoid_proxy(
            WindowHumanoidProxyInstance::new([0.0, 0.0], 0.0, color)
                .with_human(Some(&human))
                .with_facing_yaw_radians(0.0)
                .with_pose_weights(1.0, 0.0),
        );
        let mut crouched = WindowSceneGeometry::default();
        crouched.add_humanoid_proxy(
            WindowHumanoidProxyInstance::new([0.0, 0.0], 0.0, color)
                .with_human(Some(&human))
                .with_facing_yaw_radians(0.0)
                .with_pose_weights(0.0, 1.0),
        );

        assert_eq!(idle.vertices.len(), moving.vertices.len());
        assert_eq!(idle.vertices.len(), crouched.vertices.len());
        let position_delta = |left: &WindowSceneGeometry, right: &WindowSceneGeometry| {
            left.vertices
                .iter()
                .zip(right.vertices.iter())
                .map(|(left, right)| {
                    (left.position[0] - right.position[0]).abs()
                        + (left.position[1] - right.position[1]).abs()
                        + (left.position[2] - right.position[2]).abs()
                })
                .sum::<f32>()
        };
        let moving_delta = position_delta(&idle, &moving);
        let crouched_delta = position_delta(&idle, &crouched);
        assert!(
            moving_delta > 0.5,
            "locomotion should swing limbs instead of reusing idle rods: delta={moving_delta}"
        );
        assert!(
            crouched_delta > 0.5,
            "crouch should push knees forward instead of reusing idle rods: delta={crouched_delta}"
        );
    }

    #[test]
    fn window_humanoid_proxy_uses_surface_state_for_skin_clothing_hair_and_eyes() {
        let mut geometry = WindowSceneGeometry::default();
        let human = HumanState {
            human_id: 300,
            quality_tier: QualityTier::HeroHighFidelityRuntime,
        };
        let surface = HumanSurfaceState {
            skin_wetness: 0.84,
            sweat_sheen: 0.38,
            oil_sheen: 0.26,
            bruising: 0.46,
            injury_overlay: 0.62,
            dirt: 0.51,
            hair_wetness: 0.73,
            clothing_wetness: 0.76,
            clothing_damage: 0.58,
            eye_redness: 0.66,
            material_layers: Vec::new(),
        };

        geometry.add_humanoid_proxy(
            WindowHumanoidProxyInstance::new([5.5, 0.0], 0.0, [0.96, 0.72, 0.28, 1.0])
                .with_human(Some(&human))
                .with_surface_state(Some(surface.clone()))
                .with_viewer_position([5.5, -1.25, 1.25]),
        );

        for expected in [
            humanoid_skin_wet_sheen_color(&surface).expect("wet skin should draw a sheen"),
            humanoid_injury_overlay_color(&surface).expect("injury should draw a skin overlay"),
            humanoid_bruise_overlay_color(&surface).expect("bruising should draw a skin overlay"),
            humanoid_dirt_overlay_color(&surface).expect("dirt should draw an overlay"),
            humanoid_clothing_wet_color(&surface).expect("wet clothing should draw an overlay"),
            humanoid_clothing_damage_color(&surface)
                .expect("damaged clothing should draw an overlay"),
            humanoid_hair_wet_color(&surface).expect("wet hair should draw an overlay"),
        ] {
            assert!(
                geometry
                    .vertices
                    .iter()
                    .any(|vertex| vertex.color == expected
                        && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE),
                "{expected:?} should be present in humanoid surface geometry"
            );
        }

        let proxy = human_proxy_geometry_for_state(&human);
        let eye = proxy
            .face_features
            .iter()
            .find(|feature| feature.kind == HumanProxyFaceFeatureKind::LeftEye)
            .copied()
            .expect("hero proxy should include eyes");
        let eye_color =
            humanoid_face_feature_color([0.96, 0.72, 0.28, 1.0], eye, None, Some(&surface));
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.color == eye_color)
        );

        let skin_response = humanoid_skin_surface_response(Some(&surface));
        let wet_skin_response = humanoid_wet_skin_surface_response(Some(&surface));
        let eye_response = humanoid_eye_surface_response(Some(&surface));
        let hair_response = humanoid_hair_surface_response(Some(&surface));
        let clothing_response = humanoid_clothing_surface_response(Some(&surface));
        assert!(skin_response[2] > WINDOW_SURFACE_RESPONSE_HUMAN_SKIN[2]);
        assert!(wet_skin_response[2] >= 0.62);
        assert!(eye_response[0] > WINDOW_SURFACE_RESPONSE_HUMAN_EYE[0]);
        assert!(hair_response[2] > WINDOW_SURFACE_RESPONSE_HUMAN_HAIR[2]);
        assert!(clothing_response[2] > WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH[2]);

        for surface_response in [
            skin_response,
            wet_skin_response,
            eye_response,
            hair_response,
            clothing_response,
            WINDOW_SURFACE_RESPONSE_HUMAN_CYBERNETIC,
        ] {
            assert!(
                geometry
                    .vertices
                    .iter()
                    .any(|vertex| vertex.surface_response == surface_response),
                "{surface_response:?} should be emitted by surface-aware human geometry"
            );
        }
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_eye_tearline_color(Some(&surface))
                && vertex.surface_response == eye_response
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == wet_skin_response
                && vertex.material_detail[2] > 0.34
                && vertex.material_detail[3] > 0.22
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_skin_soft_shadow_color([0.96, 0.72, 0.28, 1.0], Some(&surface))
                && vertex.surface_response == skin_response
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color
                == humanoid_clothing_fold_shadow_color([0.96, 0.72, 0.28, 1.0], Some(&surface))
                && vertex.surface_response == clothing_response
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == humanoid_boot_sole_color([0.96, 0.72, 0.28, 1.0], Some(&surface))
                && vertex.surface_response == clothing_response
        }));
    }

    #[test]
    fn default_frame_uses_identity_world_camera_matrix() {
        let frame = WindowFrameState::default();

        assert_eq!(frame.world_to_clip, identity_matrix4());
        assert!(!frame.dynamic_light.is_enabled());
        assert_eq!(frame.atmosphere, WindowAtmosphere::wet_alley());
        assert!(frame.indices.is_empty());
    }
}
