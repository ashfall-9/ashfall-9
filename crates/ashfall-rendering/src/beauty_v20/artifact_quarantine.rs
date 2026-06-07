//! Strict artifact quarantine for pure Beauty Mode.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VisualArtifactCountersV20 {
    pub horizontal_screen_stripes: u32,
    pub camera_relative_circles: u32,
    pub camera_relative_ellipses: u32,
    pub circular_glare_sprites: u32,
    pub monochrome_effect_blobs: u32,
    pub full_screen_weather_quads: u32,
    pub floating_billboard_puddles: u32,
    pub procedural_smear_surfaces: u32,
    pub debug_chunk_boxes: u32,
    pub debug_streaming_cell_boxes: u32,
    pub debug_material_boxes: u32,
    pub debug_navigation_lines: u32,
    pub debug_route_lines: u32,
    pub debug_event_markers: u32,
    pub debug_gas_boxes: u32,
    pub camera_relative_humans: u32,
    pub rod_humans: u32,
    pub box_vehicles: u32,
}

impl VisualArtifactCountersV20 {
    pub fn total(self) -> u32 {
        self.horizontal_screen_stripes
            + self.camera_relative_circles
            + self.camera_relative_ellipses
            + self.circular_glare_sprites
            + self.monochrome_effect_blobs
            + self.full_screen_weather_quads
            + self.floating_billboard_puddles
            + self.procedural_smear_surfaces
            + self.debug_chunk_boxes
            + self.debug_streaming_cell_boxes
            + self.debug_material_boxes
            + self.debug_navigation_lines
            + self.debug_route_lines
            + self.debug_event_markers
            + self.debug_gas_boxes
            + self.camera_relative_humans
            + self.rod_humans
            + self.box_vehicles
    }

    pub fn is_clean(self) -> bool {
        self.total() == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeautyModeQuarantineV20 {
    pub allow_debug_overlays: bool,
    pub allow_screen_space_overlays: bool,
    pub allow_camera_facing_effects: bool,
    pub allow_placeholder_geometry: bool,
    pub max_total_artifacts: u32,
}

impl BeautyModeQuarantineV20 {
    pub fn strict_beauty() -> Self {
        Self {
            allow_debug_overlays: false,
            allow_screen_space_overlays: false,
            allow_camera_facing_effects: false,
            allow_placeholder_geometry: false,
            max_total_artifacts: 0,
        }
    }

    pub fn debug_mode() -> Self {
        Self {
            allow_debug_overlays: true,
            allow_screen_space_overlays: true,
            allow_camera_facing_effects: true,
            allow_placeholder_geometry: true,
            max_total_artifacts: u32::MAX,
        }
    }

    pub fn accepts(self, counters: VisualArtifactCountersV20) -> bool {
        if counters.total() > self.max_total_artifacts {
            return false;
        }

        let debug_total = counters.debug_chunk_boxes
            + counters.debug_streaming_cell_boxes
            + counters.debug_material_boxes
            + counters.debug_navigation_lines
            + counters.debug_route_lines
            + counters.debug_event_markers
            + counters.debug_gas_boxes;
        if !self.allow_debug_overlays && debug_total > 0 {
            return false;
        }

        let screen_total = counters.horizontal_screen_stripes
            + counters.monochrome_effect_blobs
            + counters.full_screen_weather_quads
            + counters.floating_billboard_puddles;
        if !self.allow_screen_space_overlays && screen_total > 0 {
            return false;
        }

        let camera_facing_total = counters.camera_relative_circles
            + counters.camera_relative_ellipses
            + counters.circular_glare_sprites
            + counters.camera_relative_humans;
        if !self.allow_camera_facing_effects && camera_facing_total > 0 {
            return false;
        }

        let placeholder_total = counters.rod_humans + counters.box_vehicles;
        if !self.allow_placeholder_geometry && placeholder_total > 0 {
            return false;
        }

        counters.procedural_smear_surfaces == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BeautyPrimitiveClassV20 {
    WorldMesh,
    TerrainPatch,
    PlantCluster,
    Stone,
    LandfillProp,
    GroundedWaterFilm,
    HumanProxy,
    VehicleProxy,
    AtmospherePass,
    CloudLayer,
    DirectionalLight,
    LocalLight,
    DebugChunkBox,
    DebugMaterialBox,
    DebugNavLine,
    DebugEventMarker,
    ScreenSpaceStripe,
    CameraFacingCircle,
    CameraFacingEllipse,
    CircularGlareSprite,
    ProceduralSmearSurface,
}

impl BeautyPrimitiveClassV20 {
    pub fn allowed_in_strict_beauty(self) -> bool {
        matches!(
            self,
            Self::WorldMesh
                | Self::TerrainPatch
                | Self::PlantCluster
                | Self::Stone
                | Self::LandfillProp
                | Self::GroundedWaterFilm
                | Self::HumanProxy
                | Self::VehicleProxy
                | Self::AtmospherePass
                | Self::CloudLayer
                | Self::DirectionalLight
                | Self::LocalLight
        )
    }
}
