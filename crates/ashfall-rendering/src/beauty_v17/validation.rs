//! V17 Beauty Mode validation gates.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BeautyDebugLeakFlagsV17 {
    pub chunk_bounds_visible: bool,
    pub streaming_cell_boxes_visible: bool,
    pub material_placement_boxes_visible: bool,
    pub nav_graph_visible: bool,
    pub route_lines_visible: bool,
    pub event_markers_visible: bool,
    pub gas_debug_boxes_visible: bool,
    pub rod_humans_visible: bool,
    pub box_vehicles_visible: bool,
    pub camera_static_human_visible: bool,
}

impl BeautyDebugLeakFlagsV17 {
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
            || self.camera_static_human_visible
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BeautyFailureReasonV17 {
    MissingProperSky,
    MissingSunOrMoon,
    MissingNaturalLight,
    NeonIsOnlyReadableLighting,
    NeonDominatesNaturalLight,
    NoValidBeautyCell,
    NoRealRoads,
    NoRoundedCurbs,
    NoNonLegoFacades,
    NoLivedInScatter,
    NoHumanProxy,
    HumanRenderedAsRod,
    HumanCameraLockedOrFloating,
    NoVehicleProxy,
    VehicleRenderedAsBox,
    NoGeneratedMaterialPages,
    NoGeneratedMicrodetailPages,
    FloatingPuddles,
    DebugPrimitiveLeak,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BeautyValidationReportV17 {
    pub pass: bool,
    pub has_proper_sky: bool,
    pub has_sun_or_moon: bool,
    pub has_natural_light: bool,
    pub readable_without_neon: bool,
    pub neon_is_accent: bool,
    pub cell_count: usize,
    pub valid_cell_count: usize,
    pub road_count: usize,
    pub valid_road_count: usize,
    pub curb_count: usize,
    pub valid_curb_count: usize,
    pub facade_count: usize,
    pub valid_facade_count: usize,
    pub scatter_field_count: usize,
    pub valid_scatter_field_count: usize,
    pub human_proxy_count: usize,
    pub valid_human_proxy_count: usize,
    pub vehicle_proxy_count: usize,
    pub valid_vehicle_proxy_count: usize,
    pub material_page_request_count: usize,
    pub microdetail_page_count: usize,
    pub floating_puddle_count: usize,
    pub debug_leaks: BeautyDebugLeakFlagsV17,
    pub failure_reasons: Vec<BeautyFailureReasonV17>,
}

impl BeautyValidationReportV17 {
    pub fn compute_pass(&mut self) {
        self.failure_reasons.clear();
        if !self.has_proper_sky {
            self.failure_reasons
                .push(BeautyFailureReasonV17::MissingProperSky);
        }
        if !self.has_sun_or_moon {
            self.failure_reasons
                .push(BeautyFailureReasonV17::MissingSunOrMoon);
        }
        if !self.has_natural_light {
            self.failure_reasons
                .push(BeautyFailureReasonV17::MissingNaturalLight);
        }
        if !self.readable_without_neon {
            self.failure_reasons
                .push(BeautyFailureReasonV17::NeonIsOnlyReadableLighting);
        }
        if !self.neon_is_accent {
            self.failure_reasons
                .push(BeautyFailureReasonV17::NeonDominatesNaturalLight);
        }
        if self.valid_cell_count == 0 {
            self.failure_reasons
                .push(BeautyFailureReasonV17::NoValidBeautyCell);
        }
        if self.valid_road_count == 0 {
            self.failure_reasons
                .push(BeautyFailureReasonV17::NoRealRoads);
        }
        if self.valid_curb_count == 0 {
            self.failure_reasons
                .push(BeautyFailureReasonV17::NoRoundedCurbs);
        }
        if self.valid_facade_count == 0 {
            self.failure_reasons
                .push(BeautyFailureReasonV17::NoNonLegoFacades);
        }
        if self.valid_scatter_field_count == 0 {
            self.failure_reasons
                .push(BeautyFailureReasonV17::NoLivedInScatter);
        }
        if self.human_proxy_count == 0 {
            self.failure_reasons
                .push(BeautyFailureReasonV17::NoHumanProxy);
        }
        if self.human_proxy_count > 0 && self.valid_human_proxy_count == 0 {
            self.failure_reasons
                .push(BeautyFailureReasonV17::HumanRenderedAsRod);
        }
        if self.vehicle_proxy_count == 0 {
            self.failure_reasons
                .push(BeautyFailureReasonV17::NoVehicleProxy);
        }
        if self.vehicle_proxy_count > 0 && self.valid_vehicle_proxy_count == 0 {
            self.failure_reasons
                .push(BeautyFailureReasonV17::VehicleRenderedAsBox);
        }
        if self.material_page_request_count == 0 {
            self.failure_reasons
                .push(BeautyFailureReasonV17::NoGeneratedMaterialPages);
        }
        if self.microdetail_page_count == 0 {
            self.failure_reasons
                .push(BeautyFailureReasonV17::NoGeneratedMicrodetailPages);
        }
        if self.floating_puddle_count > 0 {
            self.failure_reasons
                .push(BeautyFailureReasonV17::FloatingPuddles);
        }
        if self.debug_leaks.camera_static_human_visible {
            self.failure_reasons
                .push(BeautyFailureReasonV17::HumanCameraLockedOrFloating);
        }
        if self.debug_leaks.any() {
            self.failure_reasons
                .push(BeautyFailureReasonV17::DebugPrimitiveLeak);
        }
        self.pass = self.failure_reasons.is_empty();
    }
}
