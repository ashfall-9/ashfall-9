//! Beauty-mode contract for the Ashfall visual realism pivot.
//!
//! This module is plain Rust data. It deliberately avoids Vulkano, Vulkan,
//! command buffers, descriptors, queues, and GPU memory so the renderer keeps GPU
//! ownership inside its windowed/GPU-services layers.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BeautyRenderModeV1 {
    #[default]
    Beauty,
    Debug,
    Mixed,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeautyDebugOverlayFlagsV1 {
    pub city_chunks: bool,
    pub streaming_cells: bool,
    pub navigation_graph: bool,
    pub material_placements: bool,
    pub event_markers: bool,
    pub gas_volumes: bool,
    pub route_consequences: bool,
    pub performance_hud: bool,
}

impl BeautyDebugOverlayFlagsV1 {
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

impl Default for BeautyDebugOverlayFlagsV1 {
    fn default() -> Self {
        Self::none()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BeautyPrimitiveClassV1 {
    TerrainPatch,
    RoadSurface,
    CurbProfile,
    BuildingShell,
    BuildingFacadeModule,
    Door,
    WindowFrame,
    RoofDetail,
    PipeCurve,
    CableCurve,
    Prop,
    Decal,
    ScatterField,
    WaterSurface,
    FogVolume,
    CloudLayer,
    HumanProxy,
    VehicleProxy,
    ProductionLight,
    SkyDome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebugPrimitiveClassV1 {
    CityChunkBox,
    StreamingCellBox,
    NavigationNode,
    NavigationEdge,
    RouteLine,
    MaterialPlacementBox,
    EventMarker,
    GasVolumeDebugBox,
    SkeletonRod,
    PlaceholderVehicleBox,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BeautyGateViolationV1 {
    DebugPrimitiveInBeautyMode(DebugPrimitiveClassV1),
    MissingNaturalSky,
    MissingSunAndMoon,
    NeonUsedAsOnlyLighting,
    HumanRenderedAsRods,
    VehicleRenderedAsBox,
    ForegroundObjectLostRoundedSilhouette,
    ForegroundMaterialRenderedFlat,
    NoGeneratedMaterialMicrodetail,
    FrameBudgetExceededWithoutGracefulDegrade,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NaturalEnvironmentStateV1 {
    pub time_of_day_seconds: f32,
    pub sun_direction_world: [f32; 3],
    pub sun_color_linear: [f32; 3],
    pub sun_intensity_lux: f32,
    pub moon_direction_world: [f32; 3],
    pub moon_color_linear: [f32; 3],
    pub moon_intensity_lux: f32,
    pub sky_turbidity: f32,
    pub cloud_coverage: f32,
    pub cloud_density: f32,
    pub fog_density: f32,
    pub rain_intensity: f32,
    pub exposure_value: f32,
    pub white_balance_kelvin: f32,
}

impl NaturalEnvironmentStateV1 {
    pub const fn rainy_alley_default() -> Self {
        Self {
            time_of_day_seconds: 18.5 * 60.0 * 60.0,
            sun_direction_world: [0.24, -0.78, 0.58],
            sun_color_linear: [1.0, 0.88, 0.72],
            sun_intensity_lux: 22_000.0,
            moon_direction_world: [-0.36, 0.54, 0.76],
            moon_color_linear: [0.62, 0.70, 1.0],
            moon_intensity_lux: 0.25,
            sky_turbidity: 3.2,
            cloud_coverage: 0.62,
            cloud_density: 0.52,
            fog_density: 0.012,
            rain_intensity: 0.38,
            exposure_value: 1.18,
            white_balance_kelvin: 6200.0,
        }
    }
}

impl Default for NaturalEnvironmentStateV1 {
    fn default() -> Self {
        Self::rainy_alley_default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameBudgetConfigV1 {
    pub target_frame_ms: f32,
    pub target_cpu_ms: f32,
    pub target_gpu_ms: f32,
    pub max_geometry_upload_mb_per_frame: f32,
    pub max_material_pages_generated_per_frame: u32,
    pub max_shadow_pages_updated_per_frame: u32,
    pub max_new_scatter_instances_per_frame: u32,
    pub max_animation_updates_per_frame: u32,
}

impl Default for FrameBudgetConfigV1 {
    fn default() -> Self {
        Self {
            target_frame_ms: 16.67,
            target_cpu_ms: 6.0,
            target_gpu_ms: 9.5,
            max_geometry_upload_mb_per_frame: 8.0,
            max_material_pages_generated_per_frame: 12,
            max_shadow_pages_updated_per_frame: 16,
            max_new_scatter_instances_per_frame: 256,
            max_animation_updates_per_frame: 64,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameBudgetPressureV1 {
    pub cpu_ms: f32,
    pub gpu_ms: f32,
    pub upload_mb_this_frame: f32,
    pub material_pages_generated: u32,
    pub shadow_pages_updated: u32,
    pub memory_pressure_0_to_1: f32,
}

impl FrameBudgetPressureV1 {
    pub fn normalized_pressure(self, budget: FrameBudgetConfigV1) -> f32 {
        let cpu = if budget.target_cpu_ms > 0.0 {
            self.cpu_ms / budget.target_cpu_ms
        } else {
            1.0
        };
        let gpu = if budget.target_gpu_ms > 0.0 {
            self.gpu_ms / budget.target_gpu_ms
        } else {
            1.0
        };
        let upload = if budget.max_geometry_upload_mb_per_frame > 0.0 {
            self.upload_mb_this_frame / budget.max_geometry_upload_mb_per_frame
        } else {
            1.0
        };
        cpu.max(gpu)
            .max(upload)
            .max(self.memory_pressure_0_to_1)
            .clamp(0.0, 2.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailImportanceInputV1 {
    pub screen_coverage_0_to_1: f32,
    pub distance_meters: f32,
    pub visible_this_frame: bool,
    pub player_interaction_likelihood_0_to_1: f32,
    pub story_importance_0_to_1: f32,
    pub material_importance_0_to_1: f32,
    pub is_foreground: bool,
    pub is_human: bool,
    pub is_vehicle: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailDecisionV1 {
    pub geometry_lod_bias: f32,
    pub material_page_resolution_scale: f32,
    pub scatter_density_scale: f32,
    pub animation_update_rate_scale: f32,
    pub shadow_update_rate_scale: f32,
    pub reflection_update_rate_scale: f32,
    pub preserve_rounded_silhouette: bool,
    pub preserve_human_or_vehicle_identity: bool,
}

pub fn choose_detail_decision_v1(
    input: DetailImportanceInputV1,
    pressure: FrameBudgetPressureV1,
    budget: FrameBudgetConfigV1,
) -> DetailDecisionV1 {
    let pressure = pressure.normalized_pressure(budget);
    let importance = (input.screen_coverage_0_to_1 * 0.34
        + input.player_interaction_likelihood_0_to_1 * 0.18
        + input.story_importance_0_to_1 * 0.24
        + input.material_importance_0_to_1 * 0.14
        + if input.is_foreground { 0.18 } else { 0.0 })
    .clamp(0.0, 1.0);

    let distance_fade = if input.distance_meters <= 8.0 {
        1.0
    } else if input.distance_meters <= 45.0 {
        1.0 - ((input.distance_meters - 8.0) / 37.0) * 0.45
    } else {
        0.42
    };

    let degrade = (pressure - 0.88).clamp(0.0, 0.75);
    let base = if input.visible_this_frame {
        distance_fade.max(importance)
    } else {
        0.25
    };

    DetailDecisionV1 {
        geometry_lod_bias: (base - degrade * 0.35).clamp(0.28, 1.0),
        material_page_resolution_scale: (base - degrade * 0.50).clamp(0.25, 1.0),
        scatter_density_scale: (base - degrade * 0.80).clamp(0.0, 1.0),
        animation_update_rate_scale: (base - degrade * 0.45).clamp(0.20, 1.0),
        shadow_update_rate_scale: (base - degrade * 0.70).clamp(0.20, 1.0),
        reflection_update_rate_scale: (base - degrade * 0.75).clamp(0.16, 1.0),
        preserve_rounded_silhouette: input.is_foreground || input.screen_coverage_0_to_1 > 0.025,
        preserve_human_or_vehicle_identity: input.is_human || input.is_vehicle,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectClassV1 {
    Road,
    Curb,
    Building,
    Pipe,
    Cable,
    Prop,
    Human,
    Vehicle,
    Terrain,
    Water,
    Glass,
    MetalPanel,
    ConcreteWall,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IrregularityRecipeV1 {
    pub seed: u64,
    pub object_class: ObjectClassV1,
    pub silhouette_variation: f32,
    pub bevel_radius_meters_min: f32,
    pub bevel_radius_meters_max: f32,
    pub surface_warp_strength: f32,
    pub dirt_density: f32,
    pub chip_density: f32,
    pub crack_density: f32,
    pub decal_density: f32,
    pub scatter_density: f32,
    pub wetness_bias: f32,
}

impl IrregularityRecipeV1 {
    pub const fn road(seed: u64) -> Self {
        Self {
            seed,
            object_class: ObjectClassV1::Road,
            silhouette_variation: 0.18,
            bevel_radius_meters_min: 0.015,
            bevel_radius_meters_max: 0.060,
            surface_warp_strength: 0.045,
            dirt_density: 0.72,
            chip_density: 0.28,
            crack_density: 0.20,
            decal_density: 0.44,
            scatter_density: 0.36,
            wetness_bias: 0.74,
        }
    }

    pub const fn human_proxy(seed: u64) -> Self {
        Self {
            seed,
            object_class: ObjectClassV1::Human,
            silhouette_variation: 0.08,
            bevel_radius_meters_min: 0.005,
            bevel_radius_meters_max: 0.020,
            surface_warp_strength: 0.012,
            dirt_density: 0.16,
            chip_density: 0.0,
            crack_density: 0.0,
            decal_density: 0.10,
            scatter_density: 0.0,
            wetness_bias: 0.28,
        }
    }

    pub const fn vehicle(seed: u64) -> Self {
        Self {
            seed,
            object_class: ObjectClassV1::Vehicle,
            silhouette_variation: 0.10,
            bevel_radius_meters_min: 0.020,
            bevel_radius_meters_max: 0.100,
            surface_warp_strength: 0.018,
            dirt_density: 0.42,
            chip_density: 0.18,
            crack_density: 0.05,
            decal_density: 0.24,
            scatter_density: 0.0,
            wetness_bias: 0.46,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanProxyV1 {
    pub entity_id: u64,
    pub position_world_meters: [f32; 3],
    pub height_meters: f32,
    pub shoulder_width_meters: f32,
    pub hip_width_meters: f32,
    pub head_radius_meters: f32,
    pub clothing_material_id: u64,
    pub skin_material_id: u64,
    pub hair_material_id: u64,
    pub animation_phase_0_to_1: f32,
    pub lod_identity_locked: bool,
}

impl HumanProxyV1 {
    pub fn adult_default(entity_id: u64, position_world_meters: [f32; 3]) -> Self {
        Self {
            entity_id,
            position_world_meters,
            height_meters: 1.74,
            shoulder_width_meters: 0.46,
            hip_width_meters: 0.34,
            head_radius_meters: 0.105,
            clothing_material_id: 0,
            skin_material_id: 0,
            hair_material_id: 0,
            animation_phase_0_to_1: 0.0,
            lod_identity_locked: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleProxyV1 {
    pub entity_id: u64,
    pub position_world_meters: [f32; 3],
    pub length_meters: f32,
    pub width_meters: f32,
    pub height_meters: f32,
    pub wheel_radius_meters: f32,
    pub body_material_id: u64,
    pub glass_material_id: u64,
    pub tire_material_id: u64,
    pub wetness_0_to_1: f32,
    pub dirt_0_to_1: f32,
    pub lod_identity_locked: bool,
}

impl VehicleProxyV1 {
    pub fn compact_car_default(entity_id: u64, position_world_meters: [f32; 3]) -> Self {
        Self {
            entity_id,
            position_world_meters,
            length_meters: 4.35,
            width_meters: 1.82,
            height_meters: 1.42,
            wheel_radius_meters: 0.32,
            body_material_id: 0,
            glass_material_id: 0,
            tire_material_id: 0,
            wetness_0_to_1: 0.35,
            dirt_0_to_1: 0.24,
            lod_identity_locked: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GoldenSceneReportV1 {
    pub scene_name: String,
    pub frame_ms: f32,
    pub cpu_ms: f32,
    pub gpu_ms: f32,
    pub debug_primitive_count_in_beauty: u32,
    pub visible_human_proxy_count: u32,
    pub visible_vehicle_proxy_count: u32,
    pub generated_material_page_count: u32,
    pub natural_light_present: bool,
    pub sky_present: bool,
    pub violations: Vec<BeautyGateViolationV1>,
}

impl GoldenSceneReportV1 {
    pub fn passes_minimum_gate(&self) -> bool {
        self.debug_primitive_count_in_beauty == 0
            && self.visible_human_proxy_count >= 1
            && self.visible_vehicle_proxy_count >= 1
            && self.generated_material_page_count >= 8
            && self.natural_light_present
            && self.sky_present
            && self.frame_ms <= 20.0
            && self.violations.is_empty()
    }
}
