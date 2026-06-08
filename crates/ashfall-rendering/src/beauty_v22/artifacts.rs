//! Artifact quarantine for strict V22 Beauty Mode.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VisualArtifactCountersV22 {
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
    pub legacy_beauty_path_invocations: u32,
    pub camera_relative_humans: u32,
    pub rod_humans: u32,
    pub cartoon_humans: u32,
    pub fractured_humans: u32,
    pub disjoint_human_part_sets: u32,
    pub box_vehicles: u32,
    pub unstructured_texture_surfaces: u32,
}

impl VisualArtifactCountersV22 {
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
            + self.legacy_beauty_path_invocations
            + self.camera_relative_humans
            + self.rod_humans
            + self.cartoon_humans
            + self.fractured_humans
            + self.disjoint_human_part_sets
            + self.box_vehicles
            + self.unstructured_texture_surfaces
    }

    pub fn is_clean(self) -> bool {
        self.total() == 0
    }

    pub fn add_legacy_path(&mut self) {
        self.legacy_beauty_path_invocations = self.legacy_beauty_path_invocations.saturating_add(1);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeautyModeQuarantineV22 {
    pub allow_debug_overlays: bool,
    pub allow_screen_space_overlays: bool,
    pub allow_camera_facing_effects: bool,
    pub allow_placeholder_geometry: bool,
    pub allow_unstructured_textures: bool,
    pub allow_legacy_beauty_paths: bool,
    pub allow_fractured_humans: bool,
    pub max_total_artifacts: u32,
}

impl BeautyModeQuarantineV22 {
    pub fn strict_beauty() -> Self {
        Self {
            allow_debug_overlays: false,
            allow_screen_space_overlays: false,
            allow_camera_facing_effects: false,
            allow_placeholder_geometry: false,
            allow_unstructured_textures: false,
            allow_legacy_beauty_paths: false,
            allow_fractured_humans: false,
            max_total_artifacts: 0,
        }
    }

    pub fn debug_mode() -> Self {
        Self {
            allow_debug_overlays: true,
            allow_screen_space_overlays: true,
            allow_camera_facing_effects: true,
            allow_placeholder_geometry: true,
            allow_unstructured_textures: true,
            allow_legacy_beauty_paths: true,
            allow_fractured_humans: true,
            max_total_artifacts: u32::MAX,
        }
    }

    pub fn accepts(self, counters: VisualArtifactCountersV22) -> bool {
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

        let camera_total = counters.camera_relative_circles
            + counters.camera_relative_ellipses
            + counters.circular_glare_sprites
            + counters.camera_relative_humans;
        if !self.allow_camera_facing_effects && camera_total > 0 {
            return false;
        }

        let placeholder_total = counters.rod_humans
            + counters.cartoon_humans
            + counters.disjoint_human_part_sets
            + counters.box_vehicles;
        if !self.allow_placeholder_geometry && placeholder_total > 0 {
            return false;
        }

        if !self.allow_unstructured_textures
            && counters.procedural_smear_surfaces + counters.unstructured_texture_surfaces > 0
        {
            return false;
        }

        if !self.allow_legacy_beauty_paths && counters.legacy_beauty_path_invocations > 0 {
            return false;
        }

        if !self.allow_fractured_humans && counters.fractured_humans > 0 {
            return false;
        }

        true
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LegacyBeautyPathFlagsV22 {
    pub v15_active: bool,
    pub v16_active: bool,
    pub v17_active: bool,
    pub v18_active: bool,
    pub v19_active: bool,
    pub v20_active: bool,
    pub v21_active: bool,
}

impl LegacyBeautyPathFlagsV22 {
    pub fn any_active(self) -> bool {
        self.v15_active
            || self.v16_active
            || self.v17_active
            || self.v18_active
            || self.v19_active
            || self.v20_active
            || self.v21_active
    }

    pub fn to_counters(self) -> VisualArtifactCountersV22 {
        let mut counters = VisualArtifactCountersV22::default();
        if self.any_active() {
            counters.legacy_beauty_path_invocations = 1;
        }
        counters
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BeautyPrimitiveClassV22 {
    WorldMesh,
    TerrainPatch,
    PlantCluster,
    StoneCluster,
    LandfillProp,
    GroundedWaterFilm,
    CoherentHumanMesh,
    CoherentVehicleMesh,
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
    FracturedHumanPart,
}

impl BeautyPrimitiveClassV22 {
    pub fn allowed_in_strict_beauty(self) -> bool {
        matches!(
            self,
            Self::WorldMesh
                | Self::TerrainPatch
                | Self::PlantCluster
                | Self::StoneCluster
                | Self::LandfillProp
                | Self::GroundedWaterFilm
                | Self::CoherentHumanMesh
                | Self::CoherentVehicleMesh
                | Self::AtmospherePass
                | Self::CloudLayer
                | Self::DirectionalLight
                | Self::LocalLight
        )
    }
}
