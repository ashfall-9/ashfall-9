#![forbid(unsafe_code)]

use engine_core::{CameraExposure, CameraState, InterfaceVersion, RenderQualityProfile};
use serde::{Deserialize, Serialize};

pub const SCENE_SCHEMA_VERSION: InterfaceVersion = InterfaceVersion::new("scene_schema", 0, 1, 0);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SunParams {
    pub direction: [f32; 3],
    pub angular_radius_degrees: f32,
    pub illuminance_lux: f32,
}

impl Default for SunParams {
    fn default() -> Self {
        Self {
            direction: [0.14, 0.32, 0.94],
            angular_radius_degrees: 0.266,
            illuminance_lux: 110_000.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AtmosphereParams {
    pub rayleigh_scale_height_m: f32,
    pub mie_scale_height_m: f32,
    pub ozone_strength: f32,
    pub ground_albedo: [f32; 3],
}

impl Default for AtmosphereParams {
    fn default() -> Self {
        Self {
            rayleigh_scale_height_m: 8_000.0,
            mie_scale_height_m: 1_200.0,
            ozone_strength: 1.0,
            ground_albedo: [0.18, 0.18, 0.18],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CloudLayer {
    pub seed: u64,
    pub coverage: f32,
    pub base_altitude_m: f32,
    pub thickness_m: f32,
    pub density: f32,
}

impl CloudLayer {
    pub fn smoke(seed: u64) -> Self {
        Self {
            seed,
            coverage: 0.35,
            base_altitude_m: 1_500.0,
            thickness_m: 900.0,
            density: 0.45,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkySceneRequest {
    pub seed: u64,
    pub time_seconds: f32,
    pub camera: CameraState,
    pub exposure: CameraExposure,
    pub sun: SunParams,
    pub atmosphere: AtmosphereParams,
    pub clouds: Vec<CloudLayer>,
    pub quality: RenderQualityProfile,
}

impl SkySceneRequest {
    pub fn clear_sky_smoke(seed: u64) -> Self {
        Self {
            seed,
            time_seconds: 0.0,
            camera: CameraState::default(),
            exposure: CameraExposure::default(),
            sun: SunParams::default(),
            atmosphere: AtmosphereParams::default(),
            clouds: Vec::new(),
            quality: RenderQualityProfile::Smoke,
        }
    }

    pub fn cloudy_smoke(seed: u64) -> Self {
        Self {
            clouds: vec![CloudLayer::smoke(seed)],
            ..Self::clear_sky_smoke(seed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_scene_is_sky_only() {
        let scene = SkySceneRequest::cloudy_smoke(7);

        assert_eq!(scene.seed, 7);
        assert_eq!(scene.clouds.len(), 1);
        assert!(scene.sun.angular_radius_degrees > 0.0);
    }
}
