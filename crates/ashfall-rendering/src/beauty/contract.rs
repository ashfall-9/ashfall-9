//! Beauty Mode top-level scene contract.
//!
//! Beauty Mode is the player-facing scene. It may be overlaid with debug data in
//! Mixed mode, but debug boxes, markers, rods, and route lines are not beauty
//! content.

use super::detail_budget::{DetailDecisionV15, DetailOrchestratorV15, FrameBudgetConfigV15};
use super::environment::EnvironmentStateV15;
use super::geometry_primitives::{
    AnchoredPuddleV15, BeautyBoundsV15, CurbSegmentV15, CurveObjectV15, FacadeModuleV15,
    HumanProxyV15, RoadStripV15, ScatterFieldV15, VehicleProxyV15,
};
use super::material_pages::MaterialPageRequestV15;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeautySceneModeV15 {
    Beauty,
    Debug,
    Mixed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DebugLeakFlagsV15 {
    pub chunk_bounds_visible: bool,
    pub material_placement_boxes_visible: bool,
    pub nav_graph_visible: bool,
    pub route_lines_visible: bool,
    pub event_markers_visible: bool,
    pub gas_debug_boxes_visible: bool,
    pub rod_humans_visible: bool,
    pub box_vehicles_visible: bool,
}

impl DebugLeakFlagsV15 {
    pub fn any(&self) -> bool {
        self.chunk_bounds_visible
            || self.material_placement_boxes_visible
            || self.nav_graph_visible
            || self.route_lines_visible
            || self.event_markers_visible
            || self.gas_debug_boxes_visible
            || self.rod_humans_visible
            || self.box_vehicles_visible
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BeautyCellPackageV15 {
    pub cell_id: u64,
    pub bounds: BeautyBoundsV15,
    pub road_strips: Vec<RoadStripV15>,
    pub curbs: Vec<CurbSegmentV15>,
    pub facades: Vec<FacadeModuleV15>,
    pub pipes_and_cables: Vec<CurveObjectV15>,
    pub scatter_fields: Vec<ScatterFieldV15>,
    pub puddles: Vec<AnchoredPuddleV15>,
    pub material_page_requests: Vec<MaterialPageRequestV15>,
    pub retained_cache_key: u128,
    pub dirty: bool,
}

impl BeautyCellPackageV15 {
    pub fn is_valid_beauty_cell(&self) -> bool {
        !self.road_strips.is_empty()
            && !self.facades.is_empty()
            && self
                .puddles
                .iter()
                .all(AnchoredPuddleV15::is_ground_anchored)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BeautySceneV15 {
    pub frame_index: u64,
    pub mode: BeautySceneModeV15,
    pub environment: EnvironmentStateV15,
    pub cells: Vec<BeautyCellPackageV15>,
    pub humans: Vec<HumanProxyV15>,
    pub vehicles: Vec<VehicleProxyV15>,
    pub material_page_requests: Vec<MaterialPageRequestV15>,
    pub detail_orchestrator: DetailOrchestratorV15,
    pub debug_leaks: DebugLeakFlagsV15,
    pub global_detail_decision: DetailDecisionV15,
}

impl BeautySceneV15 {
    pub fn new(frame_index: u64) -> Self {
        let orchestrator = DetailOrchestratorV15::default();
        Self {
            frame_index,
            mode: BeautySceneModeV15::Beauty,
            environment: EnvironmentStateV15::default(),
            cells: Vec::new(),
            humans: Vec::new(),
            vehicles: Vec::new(),
            material_page_requests: Vec::new(),
            detail_orchestrator: orchestrator,
            debug_leaks: DebugLeakFlagsV15::default(),
            global_detail_decision: DetailDecisionV15::hero(),
        }
    }

    pub fn with_budget(mut self, budget: FrameBudgetConfigV15) -> Self {
        self.detail_orchestrator = DetailOrchestratorV15::new(budget);
        self
    }

    pub fn collect_cell_material_pages(&mut self) {
        self.material_page_requests.clear();
        for cell in &self.cells {
            self.material_page_requests
                .extend(cell.material_page_requests.iter().cloned());
        }
    }

    pub fn validate_for_beauty(&self) -> BeautyValidationReportV15 {
        let mut report = BeautyValidationReportV15 {
            has_environment: self.environment.has_visible_sky()
                && self.environment.has_natural_light(),
            cell_count: self.cells.len(),
            valid_cell_count: self
                .cells
                .iter()
                .filter(|cell| cell.is_valid_beauty_cell())
                .count(),
            human_proxy_count: self.humans.len(),
            valid_human_proxy_count: self
                .humans
                .iter()
                .filter(|human| human.is_proportionate_non_rod())
                .count(),
            vehicle_proxy_count: self.vehicles.len(),
            valid_vehicle_proxy_count: self
                .vehicles
                .iter()
                .filter(|vehicle| vehicle.is_proportionate_non_box())
                .count(),
            material_page_request_count: self.material_page_requests.len(),
            floating_puddle_count: self
                .cells
                .iter()
                .flat_map(|cell| cell.puddles.iter())
                .filter(|puddle| !puddle.is_ground_anchored())
                .count(),
            debug_leaks: self.debug_leaks,
            ..BeautyValidationReportV15::default()
        };
        report.pass = report.has_environment
            && report.valid_cell_count > 0
            && report.valid_human_proxy_count > 0
            && report.valid_vehicle_proxy_count > 0
            && report.material_page_request_count > 0
            && report.floating_puddle_count == 0
            && !report.debug_leaks.any();
        report
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BeautyValidationReportV15 {
    pub pass: bool,
    pub has_environment: bool,
    pub cell_count: usize,
    pub valid_cell_count: usize,
    pub human_proxy_count: usize,
    pub valid_human_proxy_count: usize,
    pub vehicle_proxy_count: usize,
    pub valid_vehicle_proxy_count: usize,
    pub material_page_request_count: usize,
    pub floating_puddle_count: usize,
    pub debug_leaks: DebugLeakFlagsV15,
}
