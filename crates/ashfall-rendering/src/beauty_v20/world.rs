//! Beauty world cell contracts for city + nature + landfill.

use super::geometry::*;
use super::human::HumanProxyV20;
use super::vehicle::VehicleProxyV20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorldBiomeV20 {
    City,
    NatureReserve,
    Landfill,
    IndustrialEdge,
    Wetland,
    RockySoil,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BeautyCellPackageV20 {
    pub cell_id: u64,
    pub biome: WorldBiomeV20,
    pub bounds: BoundsV20,
    pub roads: Vec<RoadPatchV20>,
    pub curbs: Vec<CurbSegmentV20>,
    pub facades: Vec<FacadeModuleV20>,
    pub curves: Vec<CurveObjectV20>,
    pub terrain: Vec<TerrainPatchV20>,
    pub plants: Vec<PlantInstanceV20>,
    pub stones: Vec<StoneInstanceV20>,
    pub landfill_props: Vec<LandfillPropV20>,
    pub water_films: Vec<GroundedWaterFilmV20>,
    pub humans: Vec<HumanProxyV20>,
    pub vehicles: Vec<VehicleProxyV20>,
}

impl BeautyCellPackageV20 {
    pub fn new(cell_id: u64, biome: WorldBiomeV20, bounds: BoundsV20) -> Self {
        Self {
            cell_id,
            biome,
            bounds,
            roads: Vec::new(),
            curbs: Vec::new(),
            facades: Vec::new(),
            curves: Vec::new(),
            terrain: Vec::new(),
            plants: Vec::new(),
            stones: Vec::new(),
            landfill_props: Vec::new(),
            water_films: Vec::new(),
            humans: Vec::new(),
            vehicles: Vec::new(),
        }
    }

    pub fn visible_content_count(&self) -> usize {
        self.roads.len()
            + self.curbs.len()
            + self.facades.len()
            + self.curves.len()
            + self.terrain.len()
            + self.plants.len()
            + self.stones.len()
            + self.landfill_props.len()
            + self.water_films.len()
            + self.humans.len()
            + self.vehicles.len()
    }

    pub fn has_grounded_water_only(&self) -> bool {
        self.water_films
            .iter()
            .all(GroundedWaterFilmV20::is_grounded)
    }

    pub fn has_valid_humans(&self) -> bool {
        self.humans.iter().all(HumanProxyV20::visually_valid)
    }

    pub fn has_valid_vehicles(&self) -> bool {
        self.vehicles.iter().all(VehicleProxyV20::visually_valid)
    }
}
