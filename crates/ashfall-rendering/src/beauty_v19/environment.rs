//! Natural outdoor lighting and weather contract.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorldBiomeV19 {
    City,
    NatureReserve,
    Landfill,
    IndustrialEdge,
    Wetland,
    RockySoil,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NaturalEnvironmentV19 {
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
    pub neon_accent_multiplier: f32,
}

impl NaturalEnvironmentV19 {
    pub fn overcast_city_nature_landfill() -> Self {
        Self {
            time_of_day_hours: 16.2,
            sun_direction_world: normalize3([-0.42, -0.30, 0.86]),
            sun_intensity_lux: 22_000.0,
            moon_direction_world: normalize3([0.22, 0.34, -0.91]),
            moon_intensity_lux: 0.05,
            sky_turbidity_0_to_1: 0.58,
            cloud_coverage_0_to_1: 0.74,
            cloud_density_0_to_1: 0.62,
            fog_density_0_to_1: 0.12,
            rain_intensity_0_to_1: 0.22,
            exposure_value: 10.5,
            white_balance_kelvin: 6500.0,
            bloom_threshold_nits: 900.0,
            bloom_strength_0_to_1: 0.12,
            neon_accent_multiplier: 0.35,
        }
    }

    pub fn night_with_moon_and_low_neon() -> Self {
        Self {
            time_of_day_hours: 22.0,
            sun_direction_world: normalize3([0.10, -0.24, -0.96]),
            sun_intensity_lux: 0.0,
            moon_direction_world: normalize3([-0.26, -0.18, 0.95]),
            moon_intensity_lux: 0.22,
            sky_turbidity_0_to_1: 0.25,
            cloud_coverage_0_to_1: 0.38,
            cloud_density_0_to_1: 0.35,
            fog_density_0_to_1: 0.16,
            rain_intensity_0_to_1: 0.18,
            exposure_value: 4.4,
            white_balance_kelvin: 7200.0,
            bloom_threshold_nits: 700.0,
            bloom_strength_0_to_1: 0.16,
            neon_accent_multiplier: 0.50,
        }
    }

    pub fn clear_color_rgba(self) -> [f32; 4] {
        let sun_up = self.sun_direction_world[2].clamp(0.0, 1.0);
        let moon_up = self.moon_direction_world[2].clamp(0.0, 1.0);
        let cloud = self.cloud_coverage_0_to_1.clamp(0.0, 1.0);
        let fog = self.fog_density_0_to_1.clamp(0.0, 1.0);
        let day = sun_up * (1.0 - cloud * 0.45);
        let night = moon_up * (1.0 - sun_up);
        [
            0.045 + 0.26 * day + 0.035 * night + 0.10 * fog,
            0.060 + 0.34 * day + 0.050 * night + 0.12 * fog,
            0.082 + 0.46 * day + 0.130 * night + 0.14 * fog,
            1.0,
        ]
    }

    pub fn has_natural_readability(self) -> bool {
        self.sun_intensity_lux > 1000.0 || self.moon_intensity_lux > 0.05
    }

    pub fn bloom_is_restrained(self) -> bool {
        self.bloom_strength_0_to_1 <= 0.25 && self.bloom_threshold_nits >= 400.0
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
