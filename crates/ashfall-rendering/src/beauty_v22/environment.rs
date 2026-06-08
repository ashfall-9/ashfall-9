//! Stable natural environment state for sky, sun, moon, clouds, fog, rain,
//! exposure, and restrained bloom.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NaturalEnvironmentV22 {
    pub time_of_day_hours: f32,
    pub sun_direction_world: [f32; 3],
    pub sun_intensity_lux: f32,
    pub moon_direction_world: [f32; 3],
    pub moon_intensity_lux: f32,
    pub sky_turbidity_0_to_1: f32,
    pub cloud_coverage_0_to_1: f32,
    pub cloud_density_0_to_1: f32,
    pub fog_density_0_to_1: f32,
    pub rain_intensity_0_to_1: f32,
    pub exposure_value: f32,
    pub white_balance_kelvin: f32,
    pub bloom_threshold_nits: f32,
    pub bloom_strength_0_to_1: f32,
    pub lens_dirt_strength_0_to_1: f32,
    pub neon_accent_multiplier: f32,
    pub allow_fake_glare_sprites: bool,
    pub allow_camera_wave_background: bool,
    pub allow_screen_weather_stripes: bool,
}

impl NaturalEnvironmentV22 {
    pub fn city_nature_landfill_overcast_day() -> Self {
        Self {
            time_of_day_hours: 15.2,
            sun_direction_world: normalize3([-0.38, -0.27, 0.88]),
            sun_intensity_lux: 38_000.0,
            moon_direction_world: normalize3([0.28, 0.24, -0.93]),
            moon_intensity_lux: 0.03,
            sky_turbidity_0_to_1: 0.48,
            cloud_coverage_0_to_1: 0.58,
            cloud_density_0_to_1: 0.46,
            fog_density_0_to_1: 0.08,
            rain_intensity_0_to_1: 0.08,
            exposure_value: 10.2,
            white_balance_kelvin: 6500.0,
            bloom_threshold_nits: 1200.0,
            bloom_strength_0_to_1: 0.06,
            lens_dirt_strength_0_to_1: 0.02,
            neon_accent_multiplier: 0.12,
            allow_fake_glare_sprites: false,
            allow_camera_wave_background: false,
            allow_screen_weather_stripes: false,
        }
    }

    pub fn moonlit_landfill_rain() -> Self {
        Self {
            time_of_day_hours: 22.6,
            sun_direction_world: normalize3([0.12, -0.30, -0.95]),
            sun_intensity_lux: 0.0,
            moon_direction_world: normalize3([-0.26, -0.20, 0.94]),
            moon_intensity_lux: 0.28,
            sky_turbidity_0_to_1: 0.34,
            cloud_coverage_0_to_1: 0.64,
            cloud_density_0_to_1: 0.58,
            fog_density_0_to_1: 0.16,
            rain_intensity_0_to_1: 0.18,
            exposure_value: 4.7,
            white_balance_kelvin: 7200.0,
            bloom_threshold_nits: 950.0,
            bloom_strength_0_to_1: 0.08,
            lens_dirt_strength_0_to_1: 0.03,
            neon_accent_multiplier: 0.28,
            allow_fake_glare_sprites: false,
            allow_camera_wave_background: false,
            allow_screen_weather_stripes: false,
        }
    }

    pub fn clear_color_rgba(self) -> [f32; 4] {
        let sun_up = self.sun_direction_world[2].clamp(0.0, 1.0);
        let moon_up = self.moon_direction_world[2].clamp(0.0, 1.0);
        let cloud = self.cloud_coverage_0_to_1.clamp(0.0, 1.0);
        let fog = self.fog_density_0_to_1.clamp(0.0, 1.0);
        let turbidity = self.sky_turbidity_0_to_1.clamp(0.0, 1.0);

        let day = sun_up * (1.0 - cloud * 0.42);
        let night = moon_up * (1.0 - sun_up);

        [
            0.050 + 0.31 * day + 0.035 * night + 0.10 * fog + 0.025 * turbidity,
            0.065 + 0.39 * day + 0.050 * night + 0.12 * fog + 0.022 * turbidity,
            0.090 + 0.50 * day + 0.130 * night + 0.15 * fog,
            1.0,
        ]
    }

    pub fn has_natural_readability(self) -> bool {
        self.sun_intensity_lux > 1_000.0 || self.moon_intensity_lux >= 0.05
    }

    pub fn bloom_is_restrained(self) -> bool {
        self.bloom_strength_0_to_1 <= 0.14
            && self.bloom_threshold_nits >= 750.0
            && self.lens_dirt_strength_0_to_1 <= 0.06
            && !self.allow_fake_glare_sprites
    }

    pub fn neon_is_accent(self) -> bool {
        self.neon_accent_multiplier <= 0.30
    }

    pub fn no_camera_attached_environment_effects(self) -> bool {
        !self.allow_camera_wave_background && !self.allow_screen_weather_stripes
    }
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len <= f32::EPSILON {
        [0.0, 0.0, 1.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}
