//! Physically plausible environment state for V17 Beauty Mode.
//!
//! The screenshot feedback shows a missing/weak sky and overly synthetic lighting.
//! This contract makes the sky, sun, moon, clouds, fog, rain, exposure, and white
//! balance part of the Beauty scene. Neon is an accent, not the scene's only light.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvironmentStateV17 {
    pub time_of_day_seconds: f32,
    pub latitude_degrees: f32,
    pub sun_direction_world: [f32; 3],
    pub sun_color_linear: [f32; 3],
    pub sun_intensity_lux: f32,
    pub moon_direction_world: [f32; 3],
    pub moon_color_linear: [f32; 3],
    pub moon_intensity_lux: f32,
    pub sky_visible: bool,
    pub sky_albedo_linear: [f32; 3],
    pub sky_turbidity_0_to_1: f32,
    pub aerial_perspective_0_to_1: f32,
    pub cloud_coverage_0_to_1: f32,
    pub cloud_density_0_to_1: f32,
    pub cloud_layer_height_meters: f32,
    pub fog_density_0_to_1: f32,
    pub rain_intensity_0_to_1: f32,
    pub exposure_value: f32,
    pub white_balance_kelvin: f32,
    pub neon_accent_scale_0_to_1: f32,
}

impl Default for EnvironmentStateV17 {
    fn default() -> Self {
        Self::rainy_overcast_day()
    }
}

impl EnvironmentStateV17 {
    pub fn rainy_overcast_day() -> Self {
        Self {
            time_of_day_seconds: 15.0 * 60.0 * 60.0,
            latitude_degrees: 59.3,
            sun_direction_world: normalize3([-0.38, -0.24, 0.89]),
            sun_color_linear: [1.0, 0.94, 0.86],
            sun_intensity_lux: 36_000.0,
            moon_direction_world: normalize3([0.24, 0.38, -0.89]),
            moon_color_linear: [0.56, 0.62, 0.84],
            moon_intensity_lux: 0.02,
            sky_visible: true,
            sky_albedo_linear: [0.52, 0.60, 0.72],
            sky_turbidity_0_to_1: 0.72,
            aerial_perspective_0_to_1: 0.28,
            cloud_coverage_0_to_1: 0.78,
            cloud_density_0_to_1: 0.66,
            cloud_layer_height_meters: 1100.0,
            fog_density_0_to_1: 0.055,
            rain_intensity_0_to_1: 0.54,
            exposure_value: 11.1,
            white_balance_kelvin: 6300.0,
            neon_accent_scale_0_to_1: 0.18,
        }
    }

    pub fn rainy_moonlit_night() -> Self {
        Self {
            time_of_day_seconds: 22.0 * 60.0 * 60.0,
            latitude_degrees: 59.3,
            sun_direction_world: normalize3([0.08, 0.22, -0.97]),
            sun_color_linear: [1.0, 0.90, 0.76],
            sun_intensity_lux: 0.0,
            moon_direction_world: normalize3([-0.42, -0.18, 0.89]),
            moon_color_linear: [0.56, 0.63, 0.86],
            moon_intensity_lux: 0.24,
            sky_visible: true,
            sky_albedo_linear: [0.08, 0.10, 0.16],
            sky_turbidity_0_to_1: 0.72,
            aerial_perspective_0_to_1: 0.36,
            cloud_coverage_0_to_1: 0.58,
            cloud_density_0_to_1: 0.52,
            cloud_layer_height_meters: 950.0,
            fog_density_0_to_1: 0.085,
            rain_intensity_0_to_1: 0.60,
            exposure_value: 6.8,
            white_balance_kelvin: 5200.0,
            neon_accent_scale_0_to_1: 0.34,
        }
    }

    pub fn with_neon_disabled(mut self) -> Self {
        self.neon_accent_scale_0_to_1 = 0.0;
        self
    }

    pub fn has_proper_sky(&self) -> bool {
        self.sky_visible
            && luminance(self.sky_albedo_linear) > 0.006
            && self.cloud_layer_height_meters > 50.0
            && self.cloud_coverage_0_to_1 >= 0.0
    }

    pub fn has_sun_or_moon(&self) -> bool {
        self.sun_intensity_lux > 10.0 || self.moon_intensity_lux > 0.01
    }

    pub fn has_natural_light(&self) -> bool {
        self.has_sun_or_moon() || luminance(self.sky_albedo_linear) > 0.02
    }

    pub fn readable_without_neon(&self) -> bool {
        let no_neon = self.with_neon_disabled();
        no_neon.has_proper_sky() && no_neon.has_natural_light()
    }

    pub fn neon_is_accent(&self) -> bool {
        self.neon_accent_scale_0_to_1 <= 0.45
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
