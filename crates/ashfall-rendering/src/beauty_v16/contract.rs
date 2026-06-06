//! Top-level V16 Beauty Scene contract.

use super::detail_budget::{DetailDecisionV16, DetailOrchestratorV16, FrameBudgetConfigV16};
use super::environment::EnvironmentStateV16;
use super::geometry::{
    AnchoredPuddleV16, BeautyBoundsV16, CurbSegmentV16, CurveObjectV16, FacadeModuleV16,
    HumanProxyV16, RoadSplineV16, ScatterFieldV16, VehicleProxyV16,
};
use super::material_pages::MaterialPageRequestV16;
use super::validation::{BeautyDebugLeakFlagsV16, BeautyValidationReportV16};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeautySceneModeV16 {
    Beauty,
    Debug,
    Mixed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BeautyCellPackageV16 {
    pub cell_id: u64,
    pub bounds: BeautyBoundsV16,
    pub roads: Vec<RoadSplineV16>,
    pub curbs: Vec<CurbSegmentV16>,
    pub facades: Vec<FacadeModuleV16>,
    pub pipes_and_cables: Vec<CurveObjectV16>,
    pub scatter_fields: Vec<ScatterFieldV16>,
    pub puddles: Vec<AnchoredPuddleV16>,
    pub material_page_requests: Vec<MaterialPageRequestV16>,
    pub retained_cache_key: u128,
    pub dirty: bool,
}

impl BeautyCellPackageV16 {
    pub fn is_valid_beauty_cell(&self) -> bool {
        self.bounds.non_degenerate()
            && self.roads.iter().any(RoadSplineV16::is_real_road)
            && self.curbs.iter().any(CurbSegmentV16::is_rounded)
            && self.facades.iter().any(FacadeModuleV16::is_not_lego_block)
            && self
                .pipes_and_cables
                .iter()
                .any(CurveObjectV16::is_pipe_or_cable)
            && self
                .puddles
                .iter()
                .all(AnchoredPuddleV16::is_ground_anchored)
            && self
                .material_page_requests
                .iter()
                .any(MaterialPageRequestV16::has_microdetail)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BeautySceneV16 {
    pub frame_index: u64,
    pub mode: BeautySceneModeV16,
    pub environment: EnvironmentStateV16,
    pub cells: Vec<BeautyCellPackageV16>,
    pub humans: Vec<HumanProxyV16>,
    pub vehicles: Vec<VehicleProxyV16>,
    pub material_page_requests: Vec<MaterialPageRequestV16>,
    pub detail_orchestrator: DetailOrchestratorV16,
    pub debug_leaks: BeautyDebugLeakFlagsV16,
    pub global_detail_decision: DetailDecisionV16,
}

impl BeautySceneV16 {
    pub fn new(frame_index: u64) -> Self {
        Self {
            frame_index,
            mode: BeautySceneModeV16::Beauty,
            environment: EnvironmentStateV16::default(),
            cells: Vec::new(),
            humans: Vec::new(),
            vehicles: Vec::new(),
            material_page_requests: Vec::new(),
            detail_orchestrator: DetailOrchestratorV16::default(),
            debug_leaks: BeautyDebugLeakFlagsV16::default(),
            global_detail_decision: DetailDecisionV16::hero(),
        }
    }

    pub fn with_budget(mut self, budget: FrameBudgetConfigV16) -> Self {
        self.detail_orchestrator = DetailOrchestratorV16::new(budget);
        self
    }

    pub fn collect_cell_material_pages(&mut self) {
        self.material_page_requests.clear();
        for cell in &self.cells {
            self.material_page_requests
                .extend(cell.material_page_requests.iter().cloned());
        }
    }

    pub fn validate_for_visual_realism(&self) -> BeautyValidationReportV16 {
        let valid_cell_count = self
            .cells
            .iter()
            .filter(|cell| cell.is_valid_beauty_cell())
            .count();
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

        let mut report = BeautyValidationReportV16 {
            pass: false,
            has_visible_sky: self.environment.has_visible_sky(),
            has_natural_light: self.environment.has_natural_light(),
            readable_without_neon: self.environment.readable_without_neon(),
            cell_count: self.cells.len(),
            valid_cell_count,
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
