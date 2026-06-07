//! V21 city + nature + landfill cell package.

use super::geometry::*;
use super::human::HumanProxyV21;
use super::vehicle::VehicleProxyV21;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorldBiomeV21 {
    City,
    NatureReserve,
    Landfill,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BeautyCellPackageV21 {
    pub cell_id: u64,
    pub biome: WorldBiomeV21,
    pub bounds: Bounds3V21,
    pub roads: Vec<RoadPatchV21>,
    pub terrain: Vec<TerrainPatchV21>,
    pub curbs: Vec<CurbSegmentV21>,
    pub facades: Vec<FacadeModuleV21>,
    pub curves: Vec<CurveObjectV21>,
    pub water_films: Vec<GroundedWaterFilmV21>,
    pub plants: Vec<PlantInstanceV21>,
    pub stones: Vec<StoneInstanceV21>,
    pub landfill_props: Vec<LandfillPropV21>,
    pub humans: Vec<HumanProxyV21>,
    pub vehicles: Vec<VehicleProxyV21>,
    pub retained_cache_key: u64,
    pub dirty: bool,
}

impl BeautyCellPackageV21 {
    pub fn empty(cell_id: u64, biome: WorldBiomeV21, bounds: Bounds3V21) -> Self {
        Self {
            cell_id,
            biome,
            bounds,
            roads: Vec::new(),
            terrain: Vec::new(),
            curbs: Vec::new(),
            facades: Vec::new(),
            curves: Vec::new(),
            water_films: Vec::new(),
            plants: Vec::new(),
            stones: Vec::new(),
            landfill_props: Vec::new(),
            humans: Vec::new(),
            vehicles: Vec::new(),
            retained_cache_key: cell_id ^ ((biome as u64) << 32),
            dirty: true,
        }
    }

    pub fn visible_content_count(&self) -> usize {
        self.roads.len()
            + self.terrain.len()
            + self.curbs.len()
            + self.facades.len()
            + self.curves.len()
            + self.water_films.len()
            + self.plants.len()
            + self.stones.len()
            + self.landfill_props.len()
            + self.humans.len()
            + self.vehicles.len()
    }

    pub fn has_grounded_water_only(&self) -> bool {
        self.water_films
            .iter()
            .all(GroundedWaterFilmV21::visually_valid)
    }
    pub fn has_valid_humans(&self) -> bool {
        self.humans.iter().all(HumanProxyV21::visually_valid)
    }
    pub fn has_valid_vehicles(&self) -> bool {
        self.vehicles.iter().all(VehicleProxyV21::visually_valid)
    }
    pub fn has_valid_geometry(&self) -> bool {
        self.bounds.is_plausible()
            && self.roads.iter().all(RoadPatchV21::visually_valid)
            && self.curbs.iter().all(CurbSegmentV21::visually_valid)
            && self.facades.iter().all(FacadeModuleV21::visually_valid)
            && self.curves.iter().all(CurveObjectV21::visually_valid)
            && self.plants.iter().all(PlantInstanceV21::visually_valid)
            && self.stones.iter().all(StoneInstanceV21::visually_valid)
            && self
                .landfill_props
                .iter()
                .all(LandfillPropV21::visually_valid)
    }
}
