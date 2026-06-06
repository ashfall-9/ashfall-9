//! Application-level bridge from runtime/world state into Beauty Mode.

use ashfall_rendering::beauty_contract::{
    FrameBudgetConfigV1, HumanProxyV1, NaturalEnvironmentStateV1, VehicleProxyV1,
};

#[derive(Clone, Debug, PartialEq)]
pub struct BeautySceneV1 {
    pub environment: NaturalEnvironmentStateV1,
    pub frame_budget: FrameBudgetConfigV1,
    pub human_proxies: Vec<HumanProxyV1>,
    pub vehicle_proxies: Vec<VehicleProxyV1>,
    pub debug_geometry_allowed: bool,
}

impl BeautySceneV1 {
    pub fn rainy_alley_minimum() -> Self {
        Self {
            environment: NaturalEnvironmentStateV1::rainy_alley_default(),
            frame_budget: FrameBudgetConfigV1::default(),
            human_proxies: vec![HumanProxyV1::adult_default(1, [1.5, -2.0, 0.0])],
            vehicle_proxies: vec![VehicleProxyV1::compact_car_default(2, [-3.2, 4.4, 0.0])],
            debug_geometry_allowed: false,
        }
    }
}

pub fn build_beauty_scene_v1() -> BeautySceneV1 {
    BeautySceneV1::rainy_alley_minimum()
}
