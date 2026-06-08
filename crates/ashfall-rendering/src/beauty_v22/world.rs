//! V22 city + nature + landfill cell package.

use super::geometry::*;
use super::human::HumanProxyV22;
use super::vehicle::VehicleProxyV22;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorldBiomeV22 {
    City,
    NatureReserve,
    Landfill,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BeautyCellPackageV22 {
    pub cell_id: u64,
    pub biome: WorldBiomeV22,
    pub bounds: Bounds3V22,
    pub roads: Vec<RoadPatchV22>,
    pub terrain: Vec<TerrainPatchV22>,
    pub curbs: Vec<CurbSegmentV22>,
    pub facades: Vec<FacadeModuleV22>,
    pub curves: Vec<CurveObjectV22>,
    pub water_films: Vec<GroundedWaterFilmV22>,
    pub plants: Vec<PlantInstanceV22>,
    pub stones: Vec<StoneInstanceV22>,
    pub landfill_props: Vec<LandfillPropV22>,
    pub humans: Vec<HumanProxyV22>,
    pub vehicles: Vec<VehicleProxyV22>,
    pub retained_cache_key: u64,
    pub dirty: bool,
}

impl BeautyCellPackageV22 {
    pub fn empty(cell_id: u64, biome: WorldBiomeV22, bounds: Bounds3V22) -> Self {
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
            .all(GroundedWaterFilmV22::visually_valid)
    }

    pub fn has_valid_humans(&self) -> bool {
        self.humans.iter().all(HumanProxyV22::visually_valid)
    }

    pub fn has_valid_vehicles(&self) -> bool {
        self.vehicles.iter().all(VehicleProxyV22::visually_valid)
    }

    pub fn has_valid_geometry(&self) -> bool {
        self.bounds.is_plausible()
            && self.roads.iter().all(RoadPatchV22::visually_valid)
            && self.terrain.iter().all(TerrainPatchV22::visually_valid)
            && self.curbs.iter().all(CurbSegmentV22::visually_valid)
            && self.facades.iter().all(FacadeModuleV22::visually_valid)
            && self.curves.iter().all(CurveObjectV22::visually_valid)
            && self.plants.iter().all(PlantInstanceV22::visually_valid)
            && self.stones.iter().all(StoneInstanceV22::visually_valid)
            && self
                .landfill_props
                .iter()
                .all(LandfillPropV22::visually_valid)
    }
}
