//! Beauty Mode validation gates.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BeautyDebugLeakFlagsV16 {
    pub chunk_bounds_visible: bool,
    pub streaming_cell_boxes_visible: bool,
    pub material_placement_boxes_visible: bool,
    pub nav_graph_visible: bool,
    pub route_lines_visible: bool,
    pub event_markers_visible: bool,
    pub gas_debug_boxes_visible: bool,
    pub rod_humans_visible: bool,
    pub box_vehicles_visible: bool,
}

impl BeautyDebugLeakFlagsV16 {
    pub fn any(&self) -> bool {
        self.chunk_bounds_visible
            || self.streaming_cell_boxes_visible
            || self.material_placement_boxes_visible
            || self.nav_graph_visible
            || self.route_lines_visible
            || self.event_markers_visible
            || self.gas_debug_boxes_visible
            || self.rod_humans_visible
            || self.box_vehicles_visible
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BeautyFailureReasonV16 {
    MissingVisibleSky,
    MissingNaturalLight,
    NeonIsOnlyReadableLighting,
    NoValidBeautyCell,
    NoHumanProxy,
    HumanRenderedAsRod,
    NoVehicleProxy,
    VehicleRenderedAsBox,
    NoGeneratedMaterialPages,
    NoGeneratedMicrodetailPages,
    FloatingPuddles,
    DebugPrimitiveLeak,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BeautyValidationReportV16 {
    pub pass: bool,
    pub has_visible_sky: bool,
    pub has_natural_light: bool,
    pub readable_without_neon: bool,
    pub cell_count: usize,
    pub valid_cell_count: usize,
    pub human_proxy_count: usize,
    pub valid_human_proxy_count: usize,
    pub vehicle_proxy_count: usize,
    pub valid_vehicle_proxy_count: usize,
    pub material_page_request_count: usize,
    pub microdetail_page_count: usize,
    pub floating_puddle_count: usize,
    pub debug_leaks: BeautyDebugLeakFlagsV16,
    pub failure_reasons: Vec<BeautyFailureReasonV16>,
}

impl BeautyValidationReportV16 {
    pub fn compute_pass(&mut self) {
        self.failure_reasons.clear();

        if !self.has_visible_sky {
            self.failure_reasons
                .push(BeautyFailureReasonV16::MissingVisibleSky);
        }
        if !self.has_natural_light {
            self.failure_reasons
                .push(BeautyFailureReasonV16::MissingNaturalLight);
        }
        if !self.readable_without_neon {
            self.failure_reasons
                .push(BeautyFailureReasonV16::NeonIsOnlyReadableLighting);
        }
        if self.valid_cell_count == 0 {
            self.failure_reasons
                .push(BeautyFailureReasonV16::NoValidBeautyCell);
        }
        if self.human_proxy_count == 0 {
            self.failure_reasons
                .push(BeautyFailureReasonV16::NoHumanProxy);
        }
        if self.human_proxy_count > 0 && self.valid_human_proxy_count == 0 {
            self.failure_reasons
                .push(BeautyFailureReasonV16::HumanRenderedAsRod);
        }
        if self.vehicle_proxy_count == 0 {
            self.failure_reasons
                .push(BeautyFailureReasonV16::NoVehicleProxy);
        }
        if self.vehicle_proxy_count > 0 && self.valid_vehicle_proxy_count == 0 {
            self.failure_reasons
                .push(BeautyFailureReasonV16::VehicleRenderedAsBox);
        }
        if self.material_page_request_count == 0 {
            self.failure_reasons
                .push(BeautyFailureReasonV16::NoGeneratedMaterialPages);
        }
        if self.microdetail_page_count == 0 {
            self.failure_reasons
                .push(BeautyFailureReasonV16::NoGeneratedMicrodetailPages);
        }
        if self.floating_puddle_count > 0 {
            self.failure_reasons
                .push(BeautyFailureReasonV16::FloatingPuddles);
        }
        if self.debug_leaks.any() {
            self.failure_reasons
                .push(BeautyFailureReasonV16::DebugPrimitiveLeak);
        }

        self.pass = self.failure_reasons.is_empty();
    }
}
