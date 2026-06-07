//! V18 photoreal lighting contract.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NaturalLightingRigV18 {
    pub sky_visible: bool,
    pub sun_direction_world: [f32; 3],
    pub sun_intensity_lux: f32,
    pub moon_direction_world: [f32; 3],
    pub moon_intensity_lux: f32,
    pub sky_fill_luminance: f32,
    pub cloud_coverage_0_to_1: f32,
    pub fog_density_0_to_1: f32,
    pub exposure_value: f32,
    pub white_balance_kelvin: f32,
    pub neon_accent_weight_0_to_1: f32,
    pub bloom_strength_0_to_1: f32,
}

impl Default for NaturalLightingRigV18 {
    fn default() -> Self {
        Self::rainy_day()
    }
}

impl NaturalLightingRigV18 {
    pub fn rainy_day() -> Self {
        Self {
            sky_visible: true,
            sun_direction_world: normalize3([-0.36, -0.28, 0.89]),
            sun_intensity_lux: 32_000.0,
            moon_direction_world: normalize3([0.22, 0.28, -0.93]),
            moon_intensity_lux: 0.02,
            sky_fill_luminance: 0.32,
            cloud_coverage_0_to_1: 0.72,
            fog_density_0_to_1: 0.055,
            exposure_value: 10.8,
            white_balance_kelvin: 6200.0,
            neon_accent_weight_0_to_1: 0.18,
            bloom_strength_0_to_1: 0.10,
        }
    }

    pub fn readable_without_neon(&self) -> bool {
        self.sky_visible
            && (self.sun_intensity_lux > 10.0
                || self.moon_intensity_lux > 0.01
                || self.sky_fill_luminance > 0.05)
    }

    pub fn neon_is_accent(&self) -> bool {
        self.neon_accent_weight_0_to_1 <= 0.35
    }

    pub fn bloom_is_not_fake_glare(&self) -> bool {
        self.bloom_strength_0_to_1 <= 0.35
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
