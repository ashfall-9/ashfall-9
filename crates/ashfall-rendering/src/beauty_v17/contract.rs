//! Top-level V17 Beauty Scene contract.

use super::detail_budget::{DetailDecisionV17, DetailOrchestratorV17, FrameBudgetConfigV17};
use super::environment::EnvironmentStateV17;
use super::geometry::{
    BeautyBoundsV17, BeautyCellIdV17, CurbSegmentV17, CurveObjectV17, FacadeModuleV17,
    GroundedPuddleV17, HumanProxyV17, RoadPatchV17, ScatterFieldV17, VehicleProxyV17,
};
use super::material_pages::MaterialPageRequestV17;
use super::validation::{BeautyDebugLeakFlagsV17, BeautyValidationReportV17};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeautySceneModeV17 {
    Beauty,
    Debug,
    Mixed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BeautyCellPackageV17 {
    pub cell_id: BeautyCellIdV17,
    pub bounds: BeautyBoundsV17,
    pub roads: Vec<RoadPatchV17>,
    pub curbs: Vec<CurbSegmentV17>,
    pub facades: Vec<FacadeModuleV17>,
    pub pipes_and_cables: Vec<CurveObjectV17>,
    pub scatter_fields: Vec<ScatterFieldV17>,
    pub puddles: Vec<GroundedPuddleV17>,
    pub material_page_requests: Vec<MaterialPageRequestV17>,
    pub retained_cache_key: u128,
    pub dirty: bool,
}

impl BeautyCellPackageV17 {
    pub fn is_valid_beauty_cell(&self) -> bool {
        self.bounds.non_degenerate()
            && self.roads.iter().any(RoadPatchV17::is_real_road)
            && self
                .curbs
                .iter()
                .any(CurbSegmentV17::is_rounded_and_chipped)
            && self.facades.iter().any(FacadeModuleV17::is_not_lego_block)
            && self
                .pipes_and_cables
                .iter()
                .any(CurveObjectV17::is_pipe_or_cable)
            && self.scatter_fields.iter().any(ScatterFieldV17::is_lived_in)
            && self
                .puddles
                .iter()
                .all(GroundedPuddleV17::is_ground_anchored)
            && self
                .material_page_requests
                .iter()
                .any(MaterialPageRequestV17::has_microdetail)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BeautySceneV17 {
    pub frame_index: u64,
    pub mode: BeautySceneModeV17,
    pub environment: EnvironmentStateV17,
    pub cells: Vec<BeautyCellPackageV17>,
    pub humans: Vec<HumanProxyV17>,
    pub vehicles: Vec<VehicleProxyV17>,
    pub material_page_requests: Vec<MaterialPageRequestV17>,
    pub detail_orchestrator: DetailOrchestratorV17,
    pub debug_leaks: BeautyDebugLeakFlagsV17,
    pub global_detail_decision: DetailDecisionV17,
}

impl BeautySceneV17 {
    pub fn new(frame_index: u64) -> Self {
        Self {
            frame_index,
            mode: BeautySceneModeV17::Beauty,
            environment: EnvironmentStateV17::default(),
            cells: Vec::new(),
            humans: Vec::new(),
            vehicles: Vec::new(),
            material_page_requests: Vec::new(),
            detail_orchestrator: DetailOrchestratorV17::default(),
            debug_leaks: BeautyDebugLeakFlagsV17::default(),
            global_detail_decision: DetailDecisionV17::hero(),
        }
    }

    pub fn with_budget(mut self, budget: FrameBudgetConfigV17) -> Self {
        self.detail_orchestrator = DetailOrchestratorV17::new(budget);
        self
    }

    pub fn collect_cell_material_pages(&mut self) {
        self.material_page_requests.clear();
        for cell in &self.cells {
            self.material_page_requests
                .extend(cell.material_page_requests.iter().cloned());
        }
    }

    pub fn validate_for_visual_realism(&self) -> BeautyValidationReportV17 {
        let valid_cell_count = self
            .cells
            .iter()
            .filter(|cell| cell.is_valid_beauty_cell())
            .count();
        let roads = self.cells.iter().flat_map(|cell| cell.roads.iter());
        let curbs = self.cells.iter().flat_map(|cell| cell.curbs.iter());
        let facades = self.cells.iter().flat_map(|cell| cell.facades.iter());
        let scatter = self
            .cells
            .iter()
            .flat_map(|cell| cell.scatter_fields.iter());

        let road_count = roads.clone().count();
        let valid_road_count = roads.filter(|road| road.is_real_road()).count();
        let curb_count = curbs.clone().count();
        let valid_curb_count = curbs.filter(|curb| curb.is_rounded_and_chipped()).count();
        let facade_count = facades.clone().count();
        let valid_facade_count = facades.filter(|facade| facade.is_not_lego_block()).count();
        let scatter_field_count = scatter.clone().count();
        let valid_scatter_field_count = scatter.filter(|field| field.is_lived_in()).count();

        let valid_human_count = self
            .humans
            .iter()
            .filter(|human| human.is_proportionate_non_rod())
            .count();
        let valid_vehicle_count = self
            .vehicles
            .iter()
            .filter(|vehicle| vehicle.is_proportionate_non_box())
            .count();
        let floating_puddle_count = self
            .cells
            .iter()
            .flat_map(|cell| cell.puddles.iter())
            .filter(|puddle| !puddle.is_ground_anchored())
            .count();
        let material_page_count = self.material_page_requests.len();
        let microdetail_page_count = self
            .material_page_requests
            .iter()
            .filter(|page| page.has_microdetail())
            .count();

        let mut report = BeautyValidationReportV17 {
            pass: false,
            has_proper_sky: self.environment.has_proper_sky(),
            has_sun_or_moon: self.environment.has_sun_or_moon(),
            has_natural_light: self.environment.has_natural_light(),
            readable_without_neon: self.environment.readable_without_neon(),
            neon_is_accent: self.environment.neon_is_accent(),
            cell_count: self.cells.len(),
            valid_cell_count,
            road_count,
            valid_road_count,
            curb_count,
            valid_curb_count,
            facade_count,
            valid_facade_count,
            scatter_field_count,
            valid_scatter_field_count,
            human_proxy_count: self.humans.len(),
            valid_human_proxy_count: valid_human_count,
            vehicle_proxy_count: self.vehicles.len(),
            valid_vehicle_proxy_count: valid_vehicle_count,
            material_page_request_count: material_page_count,
            microdetail_page_count,
            floating_puddle_count,
            debug_leaks: self.debug_leaks,
            failure_reasons: Vec::new(),
        };
        report.compute_pass();
        report
    }
}
