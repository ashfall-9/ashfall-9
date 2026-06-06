//! Natural environment state for Beauty Mode V16.
//!
//! Cyberpunk can have neon, but neon must be an accent. The scene must still be
//! readable with neon disabled.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvironmentStateV16 {
    pub time_of_day_seconds: f32,
    pub sun_direction_world: [f32; 3],
    pub sun_color_linear: [f32; 3],
    pub sun_intensity_lux: f32,
    pub moon_direction_world: [f32; 3],
    pub moon_color_linear: [f32; 3],
    pub moon_intensity_lux: f32,
    pub sky_albedo_linear: [f32; 3],
    pub sky_turbidity_0_to_1: f32,
    pub sky_visible: bool,
    pub cloud_coverage_0_to_1: f32,
    pub cloud_density_0_to_1: f32,
    pub cloud_layer_height_meters: f32,
    pub fog_density_0_to_1: f32,
    pub rain_intensity_0_to_1: f32,
    pub exposure_value: f32,
    pub white_balance_kelvin: f32,
    pub neon_accent_scale_0_to_1: f32,
}

impl Default for EnvironmentStateV16 {
    fn default() -> Self {
        Self::rainy_alley_day()
    }
}

impl EnvironmentStateV16 {
    pub fn rainy_alley_day() -> Self {
        Self {
            time_of_day_seconds: 15.0 * 60.0 * 60.0,
            sun_direction_world: normalize3([-0.33, -0.24, 0.91]),
            sun_color_linear: [1.00, 0.94, 0.86],
            sun_intensity_lux: 42_000.0,
            moon_direction_world: normalize3([0.20, 0.35, -0.92]),
            moon_color_linear: [0.56, 0.62, 0.82],
            moon_intensity_lux: 0.03,
            sky_albedo_linear: [0.50, 0.59, 0.72],
            sky_turbidity_0_to_1: 0.62,
            sky_visible: true,
            cloud_coverage_0_to_1: 0.66,
            cloud_density_0_to_1: 0.58,
            cloud_layer_height_meters: 1200.0,
            fog_density_0_to_1: 0.055,
            rain_intensity_0_to_1: 0.52,
            exposure_value: 11.3,
            white_balance_kelvin: 6400.0,
            neon_accent_scale_0_to_1: 0.28,
        }
    }

    pub fn rainy_alley_night() -> Self {
        Self {
            time_of_day_seconds: 22.0 * 60.0 * 60.0,
            sun_direction_world: normalize3([0.10, 0.20, -0.98]),
            sun_color_linear: [1.00, 0.90, 0.78],
            sun_intensity_lux: 0.0,
            moon_direction_world: normalize3([-0.38, -0.18, 0.91]),
            moon_color_linear: [0.56, 0.63, 0.86],
            moon_intensity_lux: 0.24,
            sky_albedo_linear: [0.08, 0.10, 0.16],
            sky_turbidity_0_to_1: 0.72,
            sky_visible: true,
            cloud_coverage_0_to_1: 0.58,
            cloud_density_0_to_1: 0.52,
            cloud_layer_height_meters: 950.0,
            fog_density_0_to_1: 0.085,
            rain_intensity_0_to_1: 0.60,
            exposure_value: 6.7,
            white_balance_kelvin: 5200.0,
            neon_accent_scale_0_to_1: 0.38,
        }
    }

    pub fn with_neon_disabled(mut self) -> Self {
        self.neon_accent_scale_0_to_1 = 0.0;
        self
    }

    pub fn has_visible_sky(&self) -> bool {
        self.sky_visible && luminance(self.sky_albedo_linear) > 0.005
    }

    pub fn has_natural_light(&self) -> bool {
        self.sun_intensity_lux > 0.1
            || self.moon_intensity_lux > 0.01
            || luminance(self.sky_albedo_linear) > 0.02
    }

    pub fn readable_without_neon(&self) -> bool {
        self.with_neon_disabled().has_visible_sky() && self.with_neon_disabled().has_natural_light()
    }
}

fn luminance(rgb: [f32; 3]) -> f32 {
    rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len <= f32::EPSILON {
        [0.0, 0.0, 1.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}
