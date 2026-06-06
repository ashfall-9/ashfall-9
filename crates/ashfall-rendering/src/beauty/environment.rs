//! Natural environment state for Beauty Mode.
//!
//! A cyberpunk scene may contain neon, but Beauty Mode must still look plausible
//! when neon is disabled. This record makes sky, sun, moon, clouds, fog, rain,
//! exposure, and white balance required scene data rather than optional effects.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvironmentStateV15 {
    pub time_of_day_seconds: f32,
    pub sun_direction_world: [f32; 3],
    pub sun_color_linear: [f32; 3],
    pub sun_intensity_lux: f32,
    pub moon_direction_world: [f32; 3],
    pub moon_color_linear: [f32; 3],
    pub moon_intensity_lux: f32,
    pub sky_turbidity: f32,
    pub sky_albedo: [f32; 3],
    pub cloud_coverage: f32,
    pub cloud_density: f32,
    pub cloud_speed_meters_per_second: f32,
    pub fog_density: f32,
    pub rain_intensity: f32,
    pub exposure_value: f32,
    pub white_balance_kelvin: f32,
    pub neon_accent_scale: f32,
}

impl Default for EnvironmentStateV15 {
    fn default() -> Self {
        Self::rainy_alley_day()
    }
}

impl EnvironmentStateV15 {
    pub fn rainy_alley_day() -> Self {
        Self {
            time_of_day_seconds: 15.0 * 60.0 * 60.0,
            sun_direction_world: normalize3([-0.35, -0.28, 0.89]),
            sun_color_linear: [1.0, 0.94, 0.86],
            sun_intensity_lux: 45_000.0,
            moon_direction_world: normalize3([0.25, 0.4, -0.88]),
            moon_color_linear: [0.58, 0.65, 0.82],
            moon_intensity_lux: 0.08,
            sky_turbidity: 0.62,
            sky_albedo: [0.52, 0.62, 0.76],
            cloud_coverage: 0.66,
            cloud_density: 0.58,
            cloud_speed_meters_per_second: 2.4,
            fog_density: 0.045,
            rain_intensity: 0.55,
            exposure_value: 11.5,
            white_balance_kelvin: 6400.0,
            neon_accent_scale: 0.35,
        }
    }

    pub fn rainy_alley_night() -> Self {
        Self {
            time_of_day_seconds: 22.0 * 60.0 * 60.0,
            sun_direction_world: normalize3([0.1, 0.2, -0.97]),
            sun_color_linear: [1.0, 0.9, 0.78],
            sun_intensity_lux: 0.0,
            moon_direction_world: normalize3([-0.4, -0.2, 0.89]),
            moon_color_linear: [0.58, 0.64, 0.82],
            moon_intensity_lux: 0.22,
            sky_turbidity: 0.75,
            sky_albedo: [0.09, 0.12, 0.17],
            cloud_coverage: 0.52,
            cloud_density: 0.48,
            cloud_speed_meters_per_second: 1.8,
            fog_density: 0.075,
            rain_intensity: 0.6,
            exposure_value: 6.5,
            white_balance_kelvin: 5200.0,
            neon_accent_scale: 0.42,
        }
    }

    pub fn with_neon_disabled(mut self) -> Self {
        self.neon_accent_scale = 0.0;
        self
    }

    pub fn has_natural_light(&self) -> bool {
        self.sun_intensity_lux > 0.1
            || self.moon_intensity_lux > 0.01
            || luminance(self.sky_albedo) > 0.01
    }

    pub fn has_visible_sky(&self) -> bool {
        luminance(self.sky_albedo) > 0.005 && self.sky_turbidity >= 0.0
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
