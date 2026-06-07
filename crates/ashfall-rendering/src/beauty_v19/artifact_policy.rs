//! Artifact policy for pure Beauty Mode.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BeautyArtifactCountersV19 {
    pub horizontal_screen_stripes: u32,
    pub camera_relative_circles: u32,
    pub camera_relative_ellipses: u32,
    pub circular_glare_sprites: u32,
    pub monochrome_effect_blobs: u32,
    pub floating_billboard_puddles: u32,
    pub procedural_smear_surfaces: u32,
    pub debug_chunk_boxes: u32,
    pub debug_material_boxes: u32,
    pub debug_navigation_lines: u32,
    pub debug_event_markers: u32,
    pub debug_gas_boxes: u32,
    pub camera_relative_humans: u32,
    pub rod_humans: u32,
    pub box_vehicles: u32,
}

impl BeautyArtifactCountersV19 {
    pub fn total(self) -> u32 {
        self.horizontal_screen_stripes
            + self.camera_relative_circles
            + self.camera_relative_ellipses
            + self.circular_glare_sprites
            + self.monochrome_effect_blobs
            + self.floating_billboard_puddles
            + self.procedural_smear_surfaces
            + self.debug_chunk_boxes
            + self.debug_material_boxes
            + self.debug_navigation_lines
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
pub struct BeautyArtifactPolicyV19 {
    pub allow_debug_overlays: bool,
    pub allow_screen_space_weather: bool,
    pub allow_camera_facing_effects: bool,
    pub allow_placeholder_geometry: bool,
    pub max_total_artifacts: u32,
}

impl BeautyArtifactPolicyV19 {
    pub fn strict_beauty() -> Self {
        Self {
            allow_debug_overlays: false,
            allow_screen_space_weather: false,
            allow_camera_facing_effects: false,
            allow_placeholder_geometry: false,
            max_total_artifacts: 0,
        }
    }

    pub fn debug_mode() -> Self {
        Self {
            allow_debug_overlays: true,
            allow_screen_space_weather: true,
            allow_camera_facing_effects: true,
            allow_placeholder_geometry: true,
            max_total_artifacts: u32::MAX,
        }
    }

    pub fn accepts(self, counters: BeautyArtifactCountersV19) -> bool {
        if counters.total() > self.max_total_artifacts {
            return false;
        }
        if !self.allow_debug_overlays
            && (counters.debug_chunk_boxes
                + counters.debug_material_boxes
                + counters.debug_navigation_lines
                + counters.debug_event_markers
                + counters.debug_gas_boxes)
                > 0
        {
            return false;
        }
        if !self.allow_screen_space_weather
            && (counters.horizontal_screen_stripes
                + counters.monochrome_effect_blobs
                + counters.floating_billboard_puddles)
                > 0
        {
            return false;
        }
        if !self.allow_camera_facing_effects
            && (counters.camera_relative_circles
                + counters.camera_relative_ellipses
                + counters.circular_glare_sprites
                + counters.camera_relative_humans)
                > 0
        {
            return false;
        }
        if !self.allow_placeholder_geometry && (counters.rod_humans + counters.box_vehicles) > 0 {
            return false;
        }
        counters.procedural_smear_surfaces == 0
    }
}
