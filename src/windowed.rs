use std::error::Error;

use ashfall_human::HumanSurfaceState;
use ashfall_materials::{
    MATERIAL_GLASS, MATERIAL_HUMAN_SKIN, MATERIAL_NEON_TUBE, MATERIAL_WATER, MATERIAL_WET_ASPHALT,
};
use ashfall_physics::{PlanarMotionSettings, constrain_planar_motion};
use ashfall_rendering::beauty_v16::BeautySceneV16;
#[cfg(test)]
use ashfall_rendering::beauty_v16::{
    AnchoredPuddleV16, BeautyBoundsV16, CurbSegmentV16, CurveObjectV16, FacadeModuleV16,
    HumanProxyV16, RoadSplineV16, ScatterFieldV16, VehicleProxyV16,
};
use ashfall_rendering::beauty_v17::{
    BeautyBoundsV17, BeautyDrawListV17, BeautySceneV17, CurbSegmentV17, CurveObjectV17,
    FacadeModuleV17, GroundedPuddleV17, HumanProxyV17, RoadPatchV17, ScatterFieldV17,
    VehicleProxyV17,
};
use ashfall_rendering::beauty_v18::{
    BeautyArtifactCountersV18, BeautyArtifactPolicyV18, BeautyMaterialIdV18, BeautySurfaceIdV18,
    BeautyValidationInputV18, NaturalLightingRigV18, SurfaceTextureRecipeV18,
    TexturePageResolutionV18,
};
use ashfall_rendering::beauty_v19::{
    BeautySceneV19, CurbSegmentV19, FacadeModuleV19, GroundedWaterFilmV19, HumanProxyV19,
    LandfillPropV19, NaturalEnvironmentV19, PlantInstanceV19, RoadPatchV19, StoneInstanceV19,
    TerrainPatchV19, Vec3V19, VehicleProxyV19, validate_scene_v19,
};
use ashfall_rendering::windowed::{
    WINDOW_CITY_INFRASTRUCTURE_DATA, WINDOW_CITY_INFRASTRUCTURE_DRAINAGE,
    WINDOW_CITY_INFRASTRUCTURE_POWER, WINDOW_CITY_INFRASTRUCTURE_SURVEILLANCE,
    WINDOW_CITY_INFRASTRUCTURE_TRANSIT, WINDOW_CITY_INFRASTRUCTURE_WATER,
    WINDOW_HUD_EVENT_STRIP_MAX_PULSES, WINDOW_SURFACE_RESPONSE_GLASS,
    WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE, WINDOW_SURFACE_RESPONSE_METAL,
    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT, WINDOW_SURFACE_RESPONSE_WET_ROAD,
    WINDOW_WORLD_EVENT_MARKER_MAX_COUNT, WindowCameraControlSettings, WindowCityDangerFieldVisual,
    WindowCityDangerVisualKind, WindowCityDistrictVisualKind, WindowCityMaterialPlacementKind,
    WindowCityMaterialPlacementVisual, WindowCityPersistenceCounts, WindowCityPersistentCellVisual,
    WindowCityRouteConsequenceStatus, WindowCityRouteConsequenceVisual,
    WindowCityStreamingCellState, WindowCityStreamingCellVisual, WindowCityTraversalVisualKind,
    WindowDebugOverlayFlags, WindowDynamicLight, WindowFrameState, WindowGasVolumeVisual,
    WindowGeneratedCityChunkVisual, WindowGeneratedCityNavigationEdgeVisual,
    WindowGeneratedCityNavigationNodeVisual, WindowHudEventPulse, WindowHumanoidProxyInstance,
    WindowInfrastructureVisualState, WindowInputState, WindowPerspectiveCamera, WindowRenderMode,
    WindowScene, WindowSceneGeometry, WindowSnapshotSceneOptions, WindowWorldEventMarker,
    WindowWorldMarkerVisual, WindowedRendererConfig, run_windowed_scene,
    window_atmosphere_for_alley, window_dynamic_light_from_city_material_placements,
    window_dynamic_light_from_snapshot, window_hud_event_pulse_from_event,
    window_world_marker_from_event,
};
#[cfg(test)]
use ashfall_rendering::windowed::{
    WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH, WINDOW_SURFACE_RESPONSE_HUMAN_CYBERNETIC,
    WINDOW_SURFACE_RESPONSE_HUMAN_EYE, WINDOW_SURFACE_RESPONSE_HUMAN_HAIR,
    WINDOW_SURFACE_RESPONSE_HUMAN_SKIN, WindowSceneVertex,
};
use ashfall_voice::RodioEventAudioSink;
use ashfall_worldgen::{
    CityCellStreamingDecision, CityCellStreamingState, CityRendererStreamingFeedback,
    CityStreamingRequest, DistrictKind, InfrastructureSystem, NavigationDangerKind,
    NavigationRouteStatus, PLAYER_ID, TraversalKind, WorldChunk, WorldTemplate,
    alley_player_runtime_seed, generate_world_template, navigation_cell_states_from_persistence,
    persistent_city_state_from_events, plan_city_cell_streaming,
};

use crate::beauty_scene_v15_bridge::build_beauty_scene_v15;
use crate::beauty_scene_v16_bridge::build_beauty_scene_v16;
use crate::beauty_scene_v17_bridge::build_beauty_scene_v17;
use crate::beauty_scene_v19_bridge::{BeautySceneBuildContextV19, build_beauty_scene_v19};
use crate::core::{MaterialDescriptor, MaterialId, MaterialState, QualityTier, Vec3};
use crate::runtime::EngineRuntime;
use crate::scenarios::{alley_city_generation_request, build_alley_runtime};
use crate::world::{
    EntityTemplate, HumanState, MoveEntityStepCommand, MoveEntityStepMode, PrimaryInteraction,
    PrimaryInteractionSettings, WorldCommand, WorldEvent, WorldEventKind, WorldSnapshot,
    primary_interaction_from_snapshot,
};

#[cfg(target_os = "windows")]
const WINDOWS_WINDOW_THREAD_STACK_BYTES: usize = 32 * 1024 * 1024;
const WINDOW_CITY_PERSISTENT_EVENT_HISTORY_MAX: usize = 192;
const WINDOW_SURFACE_RESPONSE_SOIL_V18: [f32; 4] = [0.88, 0.0, 0.22, 0.0];
const WINDOW_SURFACE_RESPONSE_PLANT_V18: [f32; 4] = [0.70, 0.0, 0.18, 0.0];
const WINDOW_SURFACE_RESPONSE_STONE_V18: [f32; 4] = [0.72, 0.04, 0.04, 0.0];
const WINDOW_SURFACE_RESPONSE_LANDFILL_V18: [f32; 4] = [0.82, 0.12, 0.20, 0.0];

pub fn run_windowed_game() -> Result<(), Box<dyn Error>> {
    #[cfg(target_os = "windows")]
    {
        run_windowed_game_on_windows_stack_thread()
    }

    #[cfg(not(target_os = "windows"))]
    {
        run_windowed_game_on_current_thread()
    }
}

fn run_windowed_game_on_current_thread() -> Result<(), Box<dyn Error>> {
    let runtime = build_alley_runtime()?;
    let config = WindowedRendererConfig::beauty_default("Ashfall - Beauty Mode");
    let scene = AlleyWindowScene::new(runtime);
    run_windowed_scene(scene, config)
}

#[cfg(target_os = "windows")]
fn run_windowed_game_on_windows_stack_thread() -> Result<(), Box<dyn Error>> {
    let stack_size = std::env::var("ASHFALL_WINDOW_STACK_MB")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map(|megabytes| megabytes.saturating_mul(1024 * 1024))
        .filter(|bytes| *bytes >= 4 * 1024 * 1024)
        .unwrap_or(WINDOWS_WINDOW_THREAD_STACK_BYTES);

    let handle = std::thread::Builder::new()
        .name("ashfall-window".to_string())
        .stack_size(stack_size)
        .spawn(|| run_windowed_game_on_current_thread().map_err(|error| error.to_string()))?;

    match handle.join() {
        Ok(result) => result.map_err(|error| std::io::Error::other(error).into()),
        Err(_) => Err(std::io::Error::other("Ashfall window thread panicked").into()),
    }
}

struct AlleyWindowScene {
    runtime: EngineRuntime,
    city_template: WorldTemplate,
    last_snapshot: Option<WorldSnapshot>,
    event_markers: Vec<WindowWorldEventMarker>,
    gas_volumes: Vec<WindowGasVolumeRuntime>,
    hud_event_pulses: Vec<WindowHudEventPulse>,
    persistent_city_events: Vec<WorldEvent>,
    infrastructure_state: WindowInfrastructureRuntimeState,
    event_audio_sink: Option<RodioEventAudioSink>,
    audio_cues_played: u64,
    audio_status: WindowAudioStatus,
    queued_action_count: u64,
    interaction_notice: Option<WindowInteractionNotice>,
    sim_accumulator_seconds: f32,
    player_position_meters: [f32; 2],
    player_z_meters: f32,
    camera: WindowPerspectiveCamera,
    frame_index: u64,
    last_tick: u64,
    last_event_count: usize,
    last_gpu_pass_count: usize,
    last_event_summary: Option<String>,
    last_error: Option<String>,
    render_mode: WindowRenderMode,
    debug_overlays: WindowDebugOverlayFlags,
}

#[derive(Clone, Debug, PartialEq)]
struct WindowInteractionNotice {
    message: String,
    remaining_seconds: f32,
}

#[derive(Clone, Debug, PartialEq)]
struct WindowGasVolumeRuntime {
    visual: WindowGasVolumeVisual,
    remaining_seconds: f32,
    total_seconds: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct WindowInfrastructureRuntimeState {
    surveillance: f32,
    power: f32,
    drainage: f32,
    data: f32,
}

impl WindowGasVolumeRuntime {
    fn new(visual: WindowGasVolumeVisual) -> Self {
        Self {
            visual,
            remaining_seconds: 6.0,
            total_seconds: 6.0,
        }
    }

    fn normalized_lifetime(&self) -> f32 {
        if self.total_seconds <= f32::EPSILON {
            return 0.0;
        }

        (self.remaining_seconds / self.total_seconds).clamp(0.0, 1.0)
    }

    fn visible_visual(&self) -> WindowGasVolumeVisual {
        self.visual
            .with_alpha_scale(self.normalized_lifetime().clamp(0.0, 1.0))
    }

    fn tick(&mut self, dt_seconds: f32) -> bool {
        self.remaining_seconds = (self.remaining_seconds - dt_seconds.max(0.0)).max(0.0);
        self.remaining_seconds > 0.0
    }
}

impl WindowInfrastructureRuntimeState {
    fn capture_events(&mut self, events: &[WorldEvent]) {
        for event in events {
            match &event.kind {
                WorldEventKind::SecurityAlertRaised { severity, .. } => {
                    self.surveillance = self
                        .surveillance
                        .max(0.72 + severity.clamp(0.0, 1.0) * 0.28);
                    self.data = self.data.max(0.62);
                }
                WorldEventKind::SurveillanceIncreased { amount, .. } => {
                    self.surveillance = self.surveillance.max(0.45 + amount.clamp(0.0, 1.0) * 0.5);
                    self.data = self.data.max(0.54);
                }
                WorldEventKind::NpcWitnessedCrime { .. }
                | WorldEventKind::PlayerIdentityExposed => {
                    self.surveillance = self.surveillance.max(0.9);
                    self.data = self.data.max(0.58);
                }
                WorldEventKind::PowerTransformerOverheated { .. } => {
                    self.power = self.power.max(1.0);
                }
                WorldEventKind::StreetFlooded => {
                    self.drainage = self.drainage.max(1.0);
                }
                WorldEventKind::ToxicGasReleased => {
                    self.drainage = self.drainage.max(0.45);
                    self.power = self.power.max(0.28);
                }
                WorldEventKind::AssetStreamingRequested { .. }
                | WorldEventKind::AssetBecameResident { .. } => {
                    self.data = self.data.max(0.72);
                }
                _ => {}
            }
        }
    }

    fn visible_state(&self) -> WindowInfrastructureVisualState {
        let baseline = WindowInfrastructureVisualState::default();
        WindowInfrastructureVisualState::new(
            baseline.surveillance_coverage.max(self.surveillance),
            baseline.power_instability.max(self.power),
            baseline.drainage_overflow.max(self.drainage),
            baseline.data_activity.max(self.data),
        )
    }

    fn tick(&mut self, dt_seconds: f32) {
        let decay = (dt_seconds.max(0.0) / 3.0).clamp(0.0, 1.0);
        self.surveillance = (self.surveillance - decay).max(0.0);
        self.power = (self.power - decay).max(0.0);
        self.drainage = (self.drainage - decay).max(0.0);
        self.data = (self.data - decay).max(0.0);
    }
}

impl WindowInteractionNotice {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            remaining_seconds: 1.35,
        }
    }

    fn tick(&mut self, dt_seconds: f32) -> bool {
        self.remaining_seconds = (self.remaining_seconds - dt_seconds.max(0.0)).max(0.0);
        self.remaining_seconds > 0.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum WindowAudioStatus {
    Ready,
    Unavailable(String),
}

impl WindowAudioStatus {
    fn title_label(&self, cues_played: u64) -> String {
        match self {
            Self::Ready => format!("audio cues {cues_played}"),
            Self::Unavailable(reason) => format!("audio off: {}", truncate_summary(reason, 34)),
        }
    }
}

impl AlleyWindowScene {
    fn new(runtime: EngineRuntime) -> Self {
        let config = WindowedRendererConfig::beauty_default("Ashfall - Beauty Mode");
        Self::new_with_render_config(runtime, &config)
    }

    fn new_with_render_config(runtime: EngineRuntime, config: &WindowedRendererConfig) -> Self {
        let (event_audio_sink, audio_status) = match RodioEventAudioSink::open_default() {
            Ok(sink) => (Some(sink), WindowAudioStatus::Ready),
            Err(error) => (None, WindowAudioStatus::Unavailable(error)),
        };
        Self::new_with_audio_and_policy(
            runtime,
            event_audio_sink,
            audio_status,
            config.render_mode,
            config.debug_overlays,
        )
    }

    #[cfg(test)]
    fn new_with_audio(
        runtime: EngineRuntime,
        event_audio_sink: Option<RodioEventAudioSink>,
        audio_status: WindowAudioStatus,
    ) -> Self {
        Self::new_with_audio_and_policy(
            runtime,
            event_audio_sink,
            audio_status,
            WindowRenderMode::Debug,
            WindowDebugOverlayFlags::all(),
        )
    }

    fn new_with_audio_and_policy(
        mut runtime: EngineRuntime,
        event_audio_sink: Option<RodioEventAudioSink>,
        audio_status: WindowAudioStatus,
        render_mode: WindowRenderMode,
        debug_overlays: WindowDebugOverlayFlags,
    ) -> Self {
        let initial_frame = runtime.render_frame(0.0);
        let last_tick = initial_frame.sim_time.tick;
        let (player_position_meters, player_z_meters) =
            player_position_from_snapshot(&initial_frame.snapshot).unwrap_or(([0.0, 0.0], 0.0));
        let player_seed = alley_player_runtime_seed();
        let city_template = generate_world_template(&alley_city_generation_request());

        Self {
            runtime,
            city_template,
            last_snapshot: Some(initial_frame.snapshot),
            event_markers: Vec::new(),
            gas_volumes: Vec::new(),
            hud_event_pulses: Vec::new(),
            persistent_city_events: Vec::new(),
            infrastructure_state: WindowInfrastructureRuntimeState::default(),
            event_audio_sink,
            audio_cues_played: 0,
            audio_status,
            queued_action_count: 0,
            interaction_notice: None,
            sim_accumulator_seconds: 0.0,
            player_position_meters,
            player_z_meters,
            camera: WindowPerspectiveCamera::new(
                [
                    player_position_meters[0],
                    player_position_meters[1],
                    player_seed.camera_eye_height_meters,
                ],
                player_seed.initial_camera_yaw_radians,
                0.0,
            ),
            frame_index: 0,
            last_tick,
            last_event_count: 0,
            last_gpu_pass_count: 0,
            last_event_summary: None,
            last_error: None,
            render_mode,
            debug_overlays,
        }
    }

    fn debug_overlay_enabled(&self, overlay: impl FnOnce(WindowDebugOverlayFlags) -> bool) -> bool {
        match self.render_mode {
            WindowRenderMode::Beauty => false,
            WindowRenderMode::Debug => true,
            WindowRenderMode::Mixed => overlay(self.debug_overlays),
        }
    }

    fn update_view_controls(&mut self, input: &WindowInputState, dt_seconds: f32) {
        self.camera
            .apply_controls(input, dt_seconds, &WindowCameraControlSettings::default());
    }

    fn queue_player_motion(&mut self, input: &WindowInputState, dt_seconds: f32) -> bool {
        let Some(movement) = player_motion_delta(input, self.camera.yaw_radians, dt_seconds) else {
            return false;
        };

        let current_position = self.player_position_meters;
        let proposed_position = [
            current_position[0] + movement[0],
            current_position[1] + movement[1],
        ];
        let motion_settings = alley_player_motion_settings();
        let bounded_position = motion_settings.clamp_to_bounds(proposed_position);
        self.player_position_meters = self
            .last_snapshot
            .as_ref()
            .map(|snapshot| {
                constrain_planar_motion(
                    snapshot,
                    PLAYER_ID,
                    current_position,
                    proposed_position,
                    motion_settings,
                )
            })
            .unwrap_or(bounded_position);
        let step_meters = distance2(current_position, bounded_position);
        if step_meters > f32::EPSILON {
            self.runtime
                .queue_command(WorldCommand::MoveEntityStep(MoveEntityStepCommand {
                    entity: PLAYER_ID,
                    mode: MoveEntityStepMode::Toward,
                    reference_meters: Vec3::new(
                        bounded_position[0],
                        bounded_position[1],
                        self.player_z_meters,
                    ),
                    max_step_meters: step_meters,
                    stop_radius_meters: 0.0,
                }));
        }
        self.follow_player_camera();
        true
    }

    fn queue_primary_action(&mut self) -> bool {
        let Some(snapshot) = &self.last_snapshot else {
            self.interaction_notice = Some(WindowInteractionNotice::new("no world snapshot yet"));
            return false;
        };
        let Some(interaction) = primary_interaction_from_snapshot(
            snapshot,
            self.camera.yaw_radians,
            alley_primary_interaction_settings(),
        ) else {
            self.interaction_notice =
                Some(WindowInteractionNotice::new("nothing to interact with"));
            return false;
        };

        if !interaction.in_reach {
            self.interaction_notice = Some(WindowInteractionNotice::new(format!(
                "too far from {} ({:.1} m)",
                interaction.label, interaction.distance_meters
            )));
            self.event_markers.push(WindowWorldEventMarker::new(
                interaction.target_position,
                0.36,
                [1.0, 0.74, 0.24, 1.0],
                0.9,
            ));
            return false;
        }

        if interaction.already_fractured {
            self.interaction_notice = Some(WindowInteractionNotice::new(format!(
                "already fractured: {}",
                interaction.label
            )));
            self.event_markers.push(WindowWorldEventMarker::new(
                interaction.target_position,
                0.42,
                [0.72, 0.9, 1.0, 1.0],
                0.9,
            ));
            return false;
        }

        self.runtime
            .queue_command(WorldCommand::ApplyForce(interaction.force));
        self.queued_action_count = self.queued_action_count.saturating_add(1);
        self.interaction_notice = Some(WindowInteractionNotice::new(format!(
            "interacting with {}",
            interaction.label
        )));
        true
    }

    fn step_runtime(&mut self) {
        match self.runtime.step() {
            Ok(report) => {
                self.last_tick = report.sim_time.tick;
                self.last_event_count = report.events.len();
                self.last_gpu_pass_count = report.gpu_passes.len();
                self.capture_frame_events(&report.events);
                self.last_error = None;
            }
            Err(error) => {
                self.last_error = Some(error.to_string());
            }
        }
    }

    fn capture_frame_events(&mut self, events: &[WorldEvent]) {
        self.audio_cues_played = self
            .audio_cues_played
            .saturating_add(self.play_event_audio(events) as u64);

        let previous_len = self.event_markers.len();
        self.event_markers
            .extend(events.iter().filter_map(window_world_marker_from_event));
        if self.event_markers.len() > WINDOW_WORLD_EVENT_MARKER_MAX_COUNT {
            let marker_count_to_drop =
                self.event_markers.len() - WINDOW_WORLD_EVENT_MARKER_MAX_COUNT;
            self.event_markers.drain(0..marker_count_to_drop);
        }

        self.hud_event_pulses
            .extend(events.iter().map(window_hud_event_pulse_from_event));
        if self.hud_event_pulses.len() > WINDOW_HUD_EVENT_STRIP_MAX_PULSES {
            let pulse_count_to_drop =
                self.hud_event_pulses.len() - WINDOW_HUD_EVENT_STRIP_MAX_PULSES;
            self.hud_event_pulses.drain(0..pulse_count_to_drop);
        }

        self.persistent_city_events.extend(events.iter().cloned());
        if self.persistent_city_events.len() > WINDOW_CITY_PERSISTENT_EVENT_HISTORY_MAX {
            let event_count_to_drop =
                self.persistent_city_events.len() - WINDOW_CITY_PERSISTENT_EVENT_HISTORY_MAX;
            self.persistent_city_events.drain(0..event_count_to_drop);
        }

        self.last_event_summary = events.iter().rev().find_map(event_summary);
        self.infrastructure_state.capture_events(events);
        self.capture_gas_volumes(events);
        if self.event_markers.len() == previous_len && !events.is_empty() {
            self.event_markers.push(WindowWorldEventMarker::new(
                [0.0, 0.0],
                0.24,
                [0.45, 0.54, 0.72, 1.0],
                0.8,
            ));
        }
    }

    fn capture_gas_volumes(&mut self, events: &[WorldEvent]) {
        let new_volumes = events
            .iter()
            .filter_map(|event| self.gas_volume_from_event(event))
            .collect::<Vec<_>>();
        self.gas_volumes.extend(new_volumes);
        if self.gas_volumes.len() > 8 {
            let volume_count_to_drop = self.gas_volumes.len() - 8;
            self.gas_volumes.drain(0..volume_count_to_drop);
        }
    }

    fn gas_volume_from_event(&self, event: &WorldEvent) -> Option<WindowGasVolumeRuntime> {
        if !matches!(event.kind, WorldEventKind::ToxicGasReleased) {
            return None;
        }

        let source_tags = event.actors.first().and_then(|entity| {
            self.last_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.tags.find(*entity))
        });
        let gas_kind = evidence_value(&event.physical_evidence, "gas_kind");
        let steam = gas_kind == Some("steam")
            || (gas_kind.is_none()
                && source_tags.is_some_and(|tags| {
                    tags.iter().any(|tag| tag == "steam_leak")
                        && !tags.iter().any(|tag| tag == "toxic_gas_source")
                }));
        let color = if steam {
            [0.58, 0.72, 0.75, 0.36]
        } else {
            [0.32, 0.95, 0.28, 0.34]
        };
        let visibility_blocking = evidence_f32(&event.physical_evidence, "visibility_blocking")
            .unwrap_or(if steam { 0.45 } else { 0.72 });
        let hazard_level = evidence_f32(&event.physical_evidence, "hazard_level")
            .unwrap_or(if steam { 0.28 } else { 0.86 });
        let radius_meters = evidence_f32(&event.physical_evidence, "volume_radius_meters")
            .unwrap_or(if steam { 1.65 } else { 2.0 });
        let height_meters = evidence_f32(&event.physical_evidence, "volume_height_meters")
            .unwrap_or(if steam { 2.45 } else { 3.0 });
        let center = [
            event.location_meters.x,
            event.location_meters.y,
            event.location_meters.z.max(0.05),
        ];
        Some(WindowGasVolumeRuntime::new(
            WindowGasVolumeVisual::new(center, color)
                .with_radius(radius_meters)
                .with_height(height_meters)
                .with_visibility_blocking(visibility_blocking)
                .with_hazard_level(hazard_level)
                .with_phase_seed((event.event_id as u64 % 997) as f32 / 997.0),
        ))
    }

    fn play_event_audio(&self, events: &[WorldEvent]) -> usize {
        self.event_audio_sink
            .as_ref()
            .map(|sink| sink.play_world_events(events, 6))
            .unwrap_or(0)
    }

    fn decay_event_markers(&mut self, dt_seconds: f32) {
        self.event_markers
            .retain_mut(|marker| marker.tick(dt_seconds));
    }

    fn decay_gas_volumes(&mut self, dt_seconds: f32) {
        self.gas_volumes
            .retain_mut(|volume| volume.tick(dt_seconds));
    }

    fn decay_hud_event_pulses(&mut self, dt_seconds: f32) {
        self.hud_event_pulses
            .retain_mut(|pulse| pulse.tick(dt_seconds));
    }

    fn decay_infrastructure_state(&mut self, dt_seconds: f32) {
        self.infrastructure_state.tick(dt_seconds);
    }

    fn decay_interaction_notice(&mut self, dt_seconds: f32) {
        if self
            .interaction_notice
            .as_mut()
            .is_some_and(|notice| !notice.tick(dt_seconds))
        {
            self.interaction_notice = None;
        }
    }

    fn sync_render_snapshot(&mut self, dt_seconds: f32) {
        let render_frame = self.runtime.render_frame(dt_seconds);
        if let Some((position, z_meters)) = player_position_from_snapshot(&render_frame.snapshot) {
            self.player_position_meters = position;
            self.player_z_meters = z_meters;
        }
        self.follow_player_camera();
        self.last_snapshot = Some(render_frame.snapshot);
    }

    fn follow_player_camera(&mut self) {
        self.camera.position[0] = self.player_position_meters[0];
        self.camera.position[1] = self.player_position_meters[1];
    }

    fn clear_color(&self) -> [f32; 4] {
        if self.last_error.is_some() {
            return [0.26, 0.02, 0.025, 1.0];
        }

        NaturalEnvironmentV19::overcast_city_nature_landfill().clear_color_rgba()
    }

    fn title(&self) -> String {
        if let Some(error) = &self.last_error {
            return format!("Ashfall render runtime error: {error}");
        }

        let entity_count = self
            .last_snapshot
            .as_ref()
            .map(|snapshot| snapshot.transforms.len())
            .unwrap_or(0);

        format!(
            "Ashfall Vulkan | tick {} | entities {} | events {} | actions {} | gpu passes {} | player {:.1},{:.1} cam z {:.1} | {} | {} | WASD move, R/F height, mouse/arrows look, LMB/Enter/B interact, Esc quit",
            self.last_tick,
            entity_count,
            self.last_event_count,
            self.queued_action_count,
            self.last_gpu_pass_count,
            self.player_position_meters[0],
            self.player_position_meters[1],
            self.camera.position[2],
            self.audio_status.title_label(self.audio_cues_played),
            self.status_summary()
        )
    }

    fn status_summary(&self) -> String {
        if let Some(notice) = &self.interaction_notice {
            return notice.message.clone();
        }

        if let Some(interaction) = self.interaction_preview() {
            return interaction.status_summary();
        }

        self.last_event_summary
            .clone()
            .unwrap_or_else(|| "quiet".to_string())
    }

    #[cfg(test)]
    fn scene_vertices(&self) -> Vec<WindowSceneVertex> {
        self.scene_geometry().vertices
    }

    fn beauty_scene_v15(&self) -> ashfall_rendering::beauty::BeautySceneV15 {
        build_beauty_scene_v15(
            &self.city_template,
            [self.camera.position[0], self.camera.position[1]],
            self.frame_index,
        )
    }

    fn beauty_scene_v16(&self) -> BeautySceneV16 {
        build_beauty_scene_v16(
            &self.city_template,
            [self.camera.position[0], self.camera.position[1]],
            self.frame_index,
        )
    }

    fn beauty_scene_v17(&self) -> BeautySceneV17 {
        build_beauty_scene_v17(
            &self.city_template,
            [self.camera.position[0], self.camera.position[1]],
            self.frame_index,
        )
    }

    fn beauty_validation_v18(&self) -> BeautyValidationInputV18 {
        BeautyValidationInputV18 {
            policy: BeautyArtifactPolicyV18::strict_beauty(),
            counters: BeautyArtifactCountersV18::default(),
            lighting: NaturalLightingRigV18::rainy_day(),
            material_recipes: vec![
                SurfaceTextureRecipeV18::wet_asphalt(
                    BeautySurfaceIdV18(10_001),
                    BeautyMaterialIdV18(0xA5FA_5011),
                ),
                SurfaceTextureRecipeV18::dirty_concrete(
                    BeautySurfaceIdV18(10_002),
                    BeautyMaterialIdV18(0xC0A1_C0A1),
                ),
                SurfaceTextureRecipeV18::dirty_concrete(
                    BeautySurfaceIdV18(10_003),
                    BeautyMaterialIdV18(0x5011_5011),
                ),
                SurfaceTextureRecipeV18::car_paint(
                    BeautySurfaceIdV18(10_004),
                    BeautyMaterialIdV18(0xCA9_B0D7),
                ),
                SurfaceTextureRecipeV18 {
                    material_id: BeautyMaterialIdV18(0x0050_1118),
                    surface_id: BeautySurfaceIdV18(10_005),
                    meters_per_tile: 1.6,
                    normal_strength_0_to_1: 0.74,
                    roughness_variation_0_to_1: 0.84,
                    dirt_0_to_1: 0.92,
                    wetness_response_0_to_1: 0.28,
                    crack_chip_0_to_1: 0.22,
                    procedural_warp_0_to_1: 0.08,
                    resolution: TexturePageResolutionV18::Near512,
                },
                SurfaceTextureRecipeV18 {
                    material_id: BeautyMaterialIdV18(0x0007_1A97),
                    surface_id: BeautySurfaceIdV18(10_006),
                    meters_per_tile: 0.72,
                    normal_strength_0_to_1: 0.50,
                    roughness_variation_0_to_1: 0.62,
                    dirt_0_to_1: 0.36,
                    wetness_response_0_to_1: 0.42,
                    crack_chip_0_to_1: 0.08,
                    procedural_warp_0_to_1: 0.05,
                    resolution: TexturePageResolutionV18::Near512,
                },
                SurfaceTextureRecipeV18 {
                    material_id: BeautyMaterialIdV18(0x0005_700E),
                    surface_id: BeautySurfaceIdV18(10_007),
                    meters_per_tile: 0.95,
                    normal_strength_0_to_1: 0.88,
                    roughness_variation_0_to_1: 0.70,
                    dirt_0_to_1: 0.48,
                    wetness_response_0_to_1: 0.18,
                    crack_chip_0_to_1: 0.46,
                    procedural_warp_0_to_1: 0.06,
                    resolution: TexturePageResolutionV18::Near512,
                },
                SurfaceTextureRecipeV18 {
                    material_id: BeautyMaterialIdV18(0x1A9D_F111),
                    surface_id: BeautySurfaceIdV18(10_008),
                    meters_per_tile: 1.2,
                    normal_strength_0_to_1: 0.68,
                    roughness_variation_0_to_1: 0.86,
                    dirt_0_to_1: 0.82,
                    wetness_response_0_to_1: 0.44,
                    crack_chip_0_to_1: 0.52,
                    procedural_warp_0_to_1: 0.09,
                    resolution: TexturePageResolutionV18::Near512,
                },
            ],
        }
    }

    fn beauty_scene_v19(&self) -> BeautySceneV19 {
        build_beauty_scene_v19(BeautySceneBuildContextV19 {
            world_seed: self.city_template.seed,
            frame_index: self.frame_index,
            camera_xy_for_visibility_only: [self.camera.position[0], self.camera.position[1]],
            include_city: true,
            include_nature: true,
            include_landfill: true,
        })
    }

    fn city_streaming_visuals(&self) -> Vec<WindowCityStreamingCellVisual> {
        if self.render_mode == WindowRenderMode::Beauty {
            return Vec::new();
        }

        let request = self.city_streaming_request();
        let plan = plan_city_cell_streaming(&self.city_template, &request);
        plan.cells
            .iter()
            .filter_map(|decision| self.city_streaming_cell_visual_from_decision(decision))
            .collect()
    }

    fn generated_city_visuals(
        &self,
    ) -> (
        Vec<WindowGeneratedCityChunkVisual>,
        Vec<WindowGeneratedCityNavigationNodeVisual>,
        Vec<WindowGeneratedCityNavigationEdgeVisual>,
    ) {
        if self.render_mode == WindowRenderMode::Beauty {
            return (Vec::new(), Vec::new(), Vec::new());
        }

        let request = self.city_streaming_request();
        let plan = plan_city_cell_streaming(&self.city_template, &request);
        let chunks = self
            .city_template
            .chunks
            .iter()
            .filter_map(|chunk| {
                let district = self
                    .city_template
                    .districts
                    .iter()
                    .find(|district| district.id == chunk.district_id)?;
                let state = plan
                    .cells
                    .iter()
                    .find(|decision| decision.cell_id == chunk.chunk_id)
                    .map(|decision| window_city_streaming_state(decision.state))
                    .unwrap_or(WindowCityStreamingCellState::Unloaded);
                let center = [
                    (chunk.bounds.min.x + chunk.bounds.max.x) * 0.5,
                    (chunk.bounds.min.y + chunk.bounds.max.y) * 0.5,
                ];
                let half_extents = [
                    (chunk.bounds.max.x - chunk.bounds.min.x) * 0.5,
                    (chunk.bounds.max.y - chunk.bounds.min.y) * 0.5,
                ];
                let faction = self.strongest_faction_for_district(chunk.district_id);
                let npc_count = chunk
                    .entities
                    .iter()
                    .filter(|entity| {
                        entity.agent.is_some()
                            || entity.human.is_some()
                            || entity_has_tag(entity.tags.as_slice(), "npc")
                    })
                    .count();
                let destructible_count = chunk
                    .entities
                    .iter()
                    .filter(|entity| entity_has_tag(entity.tags.as_slice(), "destructible"))
                    .count();
                let light_count = chunk
                    .entities
                    .iter()
                    .filter(|entity| {
                        entity_has_tag(entity.tags.as_slice(), "light")
                            || entity_has_tag(entity.tags.as_slice(), "neon")
                    })
                    .count();
                let hazard_count = chunk
                    .entities
                    .iter()
                    .filter(|entity| {
                        entity_has_tag(entity.tags.as_slice(), "hazard")
                            || entity_has_tag(entity.tags.as_slice(), "water_leak")
                            || entity_has_tag(entity.tags.as_slice(), "steam_leak")
                    })
                    .count();

                Some(
                    WindowGeneratedCityChunkVisual::new(
                        chunk.chunk_id,
                        center,
                        half_extents,
                        window_city_district_visual_kind(district.kind),
                        state,
                    )
                    .with_district_pressure(
                        district.surveillance_level,
                        district.crime_pressure,
                        district.pollution,
                    )
                    .with_faction(faction.0, faction.1)
                    .with_activity_counts(
                        npc_count,
                        chunk.local_story_hooks.len(),
                        destructible_count,
                        light_count,
                        hazard_count,
                        chunk.streaming_dependencies.len(),
                    )
                    .with_population_counts(
                        chunk.population.crowd_spawn_rules.len(),
                        chunk.population.traffic_rules.len(),
                        chunk.population.expected_background_crowd,
                        chunk.population.expected_vehicle_or_transit_count,
                        chunk.population.validation_report.passed,
                    )
                    .with_detail_seed(self.city_template.seed ^ chunk.chunk_id),
                )
            })
            .collect::<Vec<_>>();

        let story_locations = self
            .city_template
            .story_seeds
            .iter()
            .map(|seed| seed.location)
            .collect::<Vec<_>>();
        let predicted_route = request.predicted_route;
        let mut navigation_nodes = Vec::new();
        for chunk in &self.city_template.chunks {
            let district = self
                .city_template
                .districts
                .iter()
                .find(|district| district.id == chunk.district_id);
            for node in &chunk.local_navigation.nodes {
                let position = [node.position.x, node.position.y, node.position.z.max(0.0)];
                let important = chunk
                    .local_navigation
                    .important_locations
                    .contains(&node.location);
                let story_focus = story_locations.contains(&node.location)
                    || predicted_route.contains(&node.location);
                let surveillance = district
                    .map(|district| district.surveillance_level)
                    .unwrap_or_default();
                let danger = district
                    .map(|district| {
                        (district.pollution * 0.55 + district.crime_pressure * 0.45).clamp(0.0, 1.0)
                    })
                    .unwrap_or_default();
                navigation_nodes.push(
                    WindowGeneratedCityNavigationNodeVisual::new(position)
                        .with_importance(important, story_focus)
                        .with_pressure(surveillance, danger),
                );
            }
        }

        let mut navigation_edges = Vec::new();
        for chunk in &self.city_template.chunks {
            for edge in &chunk.local_navigation.edges {
                let Some(start) = chunk
                    .local_navigation
                    .nodes
                    .iter()
                    .find(|node| node.location == edge.from)
                else {
                    continue;
                };
                let Some(end) = chunk
                    .local_navigation
                    .nodes
                    .iter()
                    .find(|node| node.location == edge.to)
                else {
                    continue;
                };
                let route_priority =
                    if predicted_route.contains(&edge.from) || predicted_route.contains(&edge.to) {
                        1.0
                    } else {
                        0.35
                    };
                navigation_edges.push(
                    WindowGeneratedCityNavigationEdgeVisual::new(
                        [
                            start.position.x,
                            start.position.y,
                            start.position.z.max(0.0),
                        ],
                        [end.position.x, end.position.y, end.position.z.max(0.0)],
                        window_city_traversal_visual_kind(edge.traversal),
                    )
                    .with_route_priority(route_priority),
                );
            }
        }

        (chunks, navigation_nodes, navigation_edges)
    }

    fn city_material_placement_visuals(&self) -> Vec<WindowCityMaterialPlacementVisual> {
        if self.render_mode == WindowRenderMode::Beauty {
            return Vec::new();
        }

        let mut placements = Vec::new();

        for chunk in &self.city_template.chunks {
            let Some(district) = self
                .city_template
                .districts
                .iter()
                .find(|district| district.id == chunk.district_id)
            else {
                continue;
            };
            let faction = self.strongest_faction_for_district(chunk.district_id);

            for entity in &chunk.entities {
                let Some(material_id) = city_entity_primary_material(entity) else {
                    continue;
                };
                let kind = window_city_material_placement_kind(material_id, entity);
                let material = self
                    .city_template
                    .materials
                    .iter()
                    .find(|material| material.id == material_id);
                let center = city_material_placement_center(entity, kind);
                let half_extents = city_material_placement_half_extents(chunk, entity, kind);
                let base_color = material
                    .map(|material| material.visual.base_color_linear)
                    .unwrap_or_else(|| window_city_material_fallback_color(kind));
                let surface_response = material
                    .map(window_city_material_surface_response_from_descriptor)
                    .unwrap_or_else(|| window_city_material_fallback_surface_response(kind));
                let (wetness, damage, pollution, traffic_wear) = city_material_surface_state(
                    district.kind,
                    district.pollution,
                    district.crime_pressure,
                    entity.material_state.as_ref(),
                    kind,
                    entity,
                );

                placements.push(
                    WindowCityMaterialPlacementVisual::new(center, half_extents, kind)
                        .with_base_color(base_color)
                        .with_surface_response(surface_response)
                        .with_surface_state(wetness, damage, pollution, traffic_wear)
                        .with_faction_accent(faction.0, faction.1)
                        .with_importance(city_material_importance(kind, entity))
                        .with_detail_seed(
                            self.city_template.seed
                                ^ chunk.chunk_id
                                ^ entity.entity_id.unwrap_or(chunk.chunk_id),
                        ),
                );
            }
        }

        placements
    }

    fn city_dynamic_light(&self) -> WindowDynamicLight {
        if !self.debug_overlay_enabled(|overlays| overlays.material_placements) {
            return self.beauty_production_light();
        }

        let placements = self.city_material_placement_visuals();
        window_dynamic_light_from_city_material_placements(
            &placements,
            self.camera.position,
            self.frame_index,
        )
    }

    fn beauty_production_light(&self) -> WindowDynamicLight {
        WindowDynamicLight::new([-1.8, -4.2, 4.4], 18.0, [0.94, 0.9, 0.8], 0.74)
    }

    fn city_consequence_visuals(
        &self,
    ) -> (
        Vec<WindowCityPersistentCellVisual>,
        Vec<WindowCityDangerFieldVisual>,
        Vec<WindowCityRouteConsequenceVisual>,
    ) {
        if self.render_mode == WindowRenderMode::Beauty {
            return (Vec::new(), Vec::new(), Vec::new());
        }

        if self.persistent_city_events.is_empty() {
            return (Vec::new(), Vec::new(), Vec::new());
        }

        let persistence =
            persistent_city_state_from_events(&self.city_template, &self.persistent_city_events);
        let navigation_states =
            navigation_cell_states_from_persistence(&self.city_template, &persistence);

        let mut cell_visuals = Vec::new();
        for cell in &persistence.cell_states {
            let Some(chunk) = self
                .city_template
                .chunks
                .iter()
                .find(|chunk| chunk.chunk_id == cell.cell_id)
            else {
                continue;
            };
            let navigation_state = navigation_states
                .iter()
                .find(|state| state.cell_id == cell.cell_id);
            let (center, half_extents) = city_chunk_center_and_half_extents(chunk);
            let danger_severity = navigation_state
                .map(|state| {
                    state
                        .danger_fields
                        .iter()
                        .map(|field| field.severity)
                        .fold(0.0_f32, f32::max)
                })
                .unwrap_or_default();
            let blocked_route_count = navigation_state
                .map(|state| {
                    state
                        .route_updates
                        .iter()
                        .filter(|route| route.status == NavigationRouteStatus::Blocked)
                        .count()
                })
                .unwrap_or_default();
            let dangerous_route_count = navigation_state
                .map(|state| {
                    state
                        .route_updates
                        .iter()
                        .filter(|route| route.status == NavigationRouteStatus::Dangerous)
                        .count()
                })
                .unwrap_or_default();
            let severity = cell
                .infrastructure_delta
                .severity
                .max(danger_severity)
                .max((cell.change_count() as f32 / 18.0).clamp(0.0, 1.0))
                .max(if blocked_route_count > 0 { 0.86 } else { 0.0 });

            cell_visuals.push(
                WindowCityPersistentCellVisual::new(cell.cell_id, center, half_extents)
                    .with_severity(severity)
                    .with_persistence_counts(WindowCityPersistenceCounts {
                        damaged_count: cell.damaged_entities.len() + cell.mesh_replacements.len(),
                        material_override_count: cell.material_state_overrides.len(),
                        infrastructure_delta_count: cell.infrastructure_delta.affected_nodes.len(),
                        faction_delta_count: cell.faction_state_delta.len(),
                        story_thread_count: cell.story_thread_refs.len(),
                        ai_memory_count: cell.ai_memory_refs.len(),
                        resident_asset_count: cell.resident_assets.len(),
                    })
                    .with_navigation_counts(
                        navigation_state
                            .map(|state| state.danger_fields.len())
                            .unwrap_or_default(),
                        navigation_state
                            .map(|state| state.restricted_zones.len())
                            .unwrap_or_default(),
                        blocked_route_count,
                        dangerous_route_count,
                    )
                    .with_infrastructure_system_mask(window_city_infrastructure_system_mask(
                        &cell.infrastructure_delta.systems,
                    ))
                    .with_save_required(persistence.save_required())
                    .with_detail_seed(self.city_template.seed ^ cell.cell_id ^ 0xC0DE),
            );
        }

        let mut danger_visuals = Vec::new();
        let mut route_visuals = Vec::new();
        for state in &navigation_states {
            let Some(chunk) = self
                .city_template
                .chunks
                .iter()
                .find(|chunk| chunk.chunk_id == state.cell_id)
            else {
                continue;
            };

            for danger in &state.danger_fields {
                let Some(center) =
                    city_average_location_position(chunk, &danger.affected_locations)
                else {
                    continue;
                };
                let radius =
                    1.1 + danger.affected_locations.len() as f32 * 0.34 + danger.severity * 1.45;
                danger_visuals.push(
                    WindowCityDangerFieldVisual::new(
                        center,
                        window_city_danger_visual_kind(danger.kind),
                    )
                    .with_radius(radius)
                    .with_severity(danger.severity),
                );
            }

            for route in &state.route_updates {
                let Some(start) = city_location_position(chunk, route.from) else {
                    continue;
                };
                let Some(end) = city_location_position(chunk, route.to) else {
                    continue;
                };
                let severity = match route.status {
                    NavigationRouteStatus::Blocked => 1.0,
                    NavigationRouteStatus::Dangerous => state
                        .danger_fields
                        .iter()
                        .map(|field| field.severity)
                        .fold(0.62_f32, f32::max),
                    NavigationRouteStatus::Restricted => state
                        .restricted_zones
                        .iter()
                        .map(|zone| zone.severity)
                        .fold(0.58_f32, f32::max),
                };
                route_visuals.push(
                    WindowCityRouteConsequenceVisual::new(
                        start,
                        end,
                        window_city_route_consequence_status(route.status),
                    )
                    .with_severity(severity),
                );
            }
        }

        (cell_visuals, danger_visuals, route_visuals)
    }

    fn strongest_faction_for_district(&self, district_id: u64) -> ([f32; 3], f32) {
        self.city_template
            .factions
            .iter()
            .flat_map(|faction| {
                faction
                    .territory_claims
                    .iter()
                    .filter(move |claim| claim.district == district_id)
                    .map(move |claim| (faction.visual_identity.primary_color, claim.strength))
            })
            .max_by(|left, right| {
                left.1
                    .partial_cmp(&right.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(([0.34, 0.78, 1.0], 0.0))
    }

    fn city_streaming_request(&self) -> CityStreamingRequest {
        let basis = self.camera.basis();
        let active_event_locations = self
            .event_markers
            .iter()
            .map(|marker| Vec3::new(marker.position[0], marker.position[1], 0.0))
            .collect::<Vec<_>>();
        let story_focus_locations = self
            .city_template
            .chunks
            .first()
            .map(|chunk| chunk.local_navigation.important_locations.clone())
            .unwrap_or_default();
        let renderer_feedback = self.renderer_city_streaming_feedback();
        let renderer_visible_chunks = renderer_feedback
            .iter()
            .map(|feedback| feedback.chunk_id)
            .collect::<Vec<_>>();
        let predicted_route = self.predicted_city_route_locations();

        CityStreamingRequest {
            player_position: Vec3::new(
                self.camera.position[0],
                self.camera.position[1],
                self.camera.position[2],
            ),
            camera_forward: Vec3::new(basis.forward[0], basis.forward[1], basis.forward[2]),
            player_velocity: Vec3::new(
                basis.forward[0] * alley_player_runtime_seed().walk_speed_meters_per_second,
                basis.forward[1] * alley_player_runtime_seed().walk_speed_meters_per_second,
                0.0,
            ),
            story_focus_locations,
            active_event_locations,
            renderer_visible_chunks,
            renderer_feedback,
            predicted_route,
            max_hero_cells: 1,
            max_render_high_detail_cells: 2,
            max_gameplay_cells: 4,
            max_loaded_bytes: 256 * 1024 * 1024,
        }
    }

    fn renderer_city_streaming_feedback(&self) -> Vec<CityRendererStreamingFeedback> {
        let basis = self.camera.basis();
        self.city_template
            .chunks
            .iter()
            .filter_map(|chunk| {
                let center = [
                    (chunk.bounds.min.x + chunk.bounds.max.x) * 0.5,
                    (chunk.bounds.min.y + chunk.bounds.max.y) * 0.5,
                    (chunk.bounds.min.z + chunk.bounds.max.z) * 0.5,
                ];
                let dx = center[0] - self.camera.position[0];
                let dy = center[1] - self.camera.position[1];
                let distance = (dx * dx + dy * dy).sqrt();
                let alignment = if distance <= f32::EPSILON {
                    1.0
                } else {
                    (dx / distance) * basis.forward[0] + (dy / distance) * basis.forward[1]
                };
                let projected = self.camera.projected_ndc(center);
                if distance >= 112.0
                    || (alignment <= -0.2
                        && !projected
                            .is_some_and(|ndc| ndc[0].abs() <= 1.35 && ndc[1].abs() <= 1.35))
                {
                    return None;
                }

                let ndc_focus = projected
                    .map(|ndc| (1.0 - (ndc[0].abs() * 0.62 + ndc[1].abs() * 0.38)).clamp(0.0, 1.0))
                    .unwrap_or_else(|| ((alignment + 0.2) / 1.2).clamp(0.0, 1.0));
                let chunk_width = (chunk.bounds.max.x - chunk.bounds.min.x).abs().max(1.0);
                let chunk_depth = (chunk.bounds.max.y - chunk.bounds.min.y).abs().max(1.0);
                let screen_coverage = ((chunk_width * chunk_depth).sqrt() / (distance + 12.0)
                    * ndc_focus)
                    .clamp(0.0, 1.0);
                let visible_cluster_count =
                    city_renderer_visible_cluster_estimate(chunk, screen_coverage);
                let material_cache_pressure =
                    (chunk.streaming_dependencies.len() as f32 / 8.0).clamp(0.0, 1.0);
                let micro_geometry_pressure = city_renderer_micro_geometry_pressure(chunk);

                Some(
                    CityRendererStreamingFeedback::new(chunk.chunk_id)
                        .with_visibility(screen_coverage, visible_cluster_count)
                        .with_streaming_pressure(material_cache_pressure, micro_geometry_pressure),
                )
            })
            .collect()
    }

    fn predicted_city_route_locations(&self) -> Vec<u64> {
        let basis = self.camera.basis();
        let mut candidates = self
            .city_template
            .chunks
            .iter()
            .map(|chunk| {
                let center_x = (chunk.bounds.min.x + chunk.bounds.max.x) * 0.5;
                let center_y = (chunk.bounds.min.y + chunk.bounds.max.y) * 0.5;
                let dx = center_x - self.camera.position[0];
                let dy = center_y - self.camera.position[1];
                let distance = (dx * dx + dy * dy).sqrt();
                let alignment = if distance <= f32::EPSILON {
                    1.0
                } else {
                    (dx / distance) * basis.forward[0] + (dy / distance) * basis.forward[1]
                };
                (alignment, distance, chunk)
            })
            .filter(|(alignment, distance, _)| *alignment > 0.1 && *distance <= 160.0)
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            right
                .0
                .partial_cmp(&left.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    left.1
                        .partial_cmp(&right.1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        candidates
            .into_iter()
            .take(2)
            .flat_map(|(_, _, chunk)| chunk.local_navigation.important_locations.clone())
            .collect()
    }

    fn city_streaming_cell_visual_from_decision(
        &self,
        decision: &CityCellStreamingDecision,
    ) -> Option<WindowCityStreamingCellVisual> {
        let chunk = self
            .city_template
            .chunks
            .iter()
            .find(|chunk| chunk.chunk_id == decision.cell_id)?;
        let center = [
            (chunk.bounds.min.x + chunk.bounds.max.x) * 0.5,
            (chunk.bounds.min.y + chunk.bounds.max.y) * 0.5,
        ];
        let half_extents = [
            (chunk.bounds.max.x - chunk.bounds.min.x) * 0.5,
            (chunk.bounds.max.y - chunk.bounds.min.y) * 0.5,
        ];
        Some(
            WindowCityStreamingCellVisual::new(
                center,
                half_extents,
                window_city_streaming_state(decision.state),
            )
            .with_streaming_metrics(
                decision.priority_score,
                decision.camera_alignment,
                decision.dependency_count,
                decision.estimated_streaming_bytes,
            ),
        )
    }

    fn scene_geometry(&self) -> WindowSceneGeometry {
        let mut geometry = WindowSceneGeometry::default();
        geometry.add_window_natural_sky(self.frame_index);
        geometry.add_window_alley_environment();
        if self.render_mode == WindowRenderMode::Beauty {
            self.add_beauty_v19_geometry(&mut geometry);
        }
        self.add_beauty_v18_worldscape_geometry(&mut geometry);
        self.add_beauty_v17_geometry(&mut geometry);
        self.add_debug_city_geometry(&mut geometry);
        geometry.add_window_alley_infrastructure(
            self.frame_index,
            self.infrastructure_state.visible_state(),
        );
        if let Some(snapshot) = &self.last_snapshot {
            geometry.add_window_snapshot_entities(
                WindowSnapshotSceneOptions::new(snapshot, self.frame_index, self.camera)
                    .with_player_entity(PLAYER_ID),
            );
        }
        geometry.add_window_alley_weather(self.frame_index, self.camera);
        if self.debug_overlay_enabled(|overlays| overlays.gas_volumes) {
            self.add_gas_volumes(&mut geometry);
        }
        if self.debug_overlay_enabled(|overlays| overlays.event_markers) {
            self.add_interaction_target_preview(&mut geometry);
            self.add_event_markers(&mut geometry);
        }
        if self.render_mode != WindowRenderMode::Beauty {
            self.add_hud(&mut geometry);
        }

        geometry
    }

    #[cfg(test)]
    fn add_beauty_v16_geometry(&self, geometry: &mut WindowSceneGeometry) {
        let scene = self.beauty_scene_v16();

        for cell in &scene.cells {
            for road in &cell.roads {
                add_beauty_v16_road(geometry, road);
            }
            for curb in &cell.curbs {
                add_beauty_v16_curb(geometry, curb);
            }
            for facade in &cell.facades {
                add_beauty_v16_facade(geometry, facade);
            }
            for curve in &cell.pipes_and_cables {
                add_beauty_v16_curve_object(geometry, curve);
            }
            for puddle in &cell.puddles {
                add_beauty_v16_puddle(geometry, puddle);
            }
            for scatter in &cell.scatter_fields {
                add_beauty_v16_scatter(geometry, scatter);
            }
        }

        for human in &scene.humans {
            add_beauty_v16_human(geometry, human, self.camera.position);
        }
        for vehicle in &scene.vehicles {
            add_beauty_v16_vehicle(geometry, vehicle);
        }
    }

    fn add_beauty_v17_geometry(&self, geometry: &mut WindowSceneGeometry) {
        let scene = self.beauty_scene_v17();
        let draw_list = BeautyDrawListV17::from_scene(&scene);

        if draw_list.has_sky_commands() {
            add_beauty_v17_sky_detail(geometry, scene.frame_index);
        }

        for cell in &scene.cells {
            for road in &cell.roads {
                add_beauty_v17_road(geometry, road);
            }
            for curb in &cell.curbs {
                add_beauty_v17_curb(geometry, curb);
            }
            for facade in &cell.facades {
                add_beauty_v17_facade(geometry, facade);
            }
            for curve in &cell.pipes_and_cables {
                add_beauty_v17_curve_object(geometry, curve);
            }
            for puddle in &cell.puddles {
                add_beauty_v17_puddle(geometry, puddle);
            }
            for scatter in &cell.scatter_fields {
                add_beauty_v17_scatter(geometry, scatter);
            }
        }

        for human in &scene.humans {
            add_beauty_v17_human(geometry, human);
        }
        for vehicle in &scene.vehicles {
            add_beauty_v17_vehicle(geometry, vehicle);
        }
    }

    fn add_beauty_v18_worldscape_geometry(&self, geometry: &mut WindowSceneGeometry) {
        add_beauty_v18_city_edge(geometry, self.frame_index);
        add_beauty_v18_nature_edge(geometry, self.frame_index);
        add_beauty_v18_landfill_edge(geometry, self.frame_index);
    }

    fn add_beauty_v19_geometry(&self, geometry: &mut WindowSceneGeometry) {
        let scene = self.beauty_scene_v19();
        add_beauty_v19_geometry(geometry, &scene, self.camera.position);
    }

    fn add_debug_city_geometry(&self, geometry: &mut WindowSceneGeometry) {
        let include_city_chunks = self.debug_overlay_enabled(|overlays| overlays.city_chunks);
        let include_navigation_graph =
            self.debug_overlay_enabled(|overlays| overlays.navigation_graph);
        if include_city_chunks || include_navigation_graph {
            let (generated_city_chunks, generated_city_nodes, generated_city_edges) =
                self.generated_city_visuals();
            let chunks = if include_city_chunks {
                generated_city_chunks.as_slice()
            } else {
                &[]
            };
            let nodes = if include_navigation_graph {
                generated_city_nodes.as_slice()
            } else {
                &[]
            };
            let edges = if include_navigation_graph {
                generated_city_edges.as_slice()
            } else {
                &[]
            };
            geometry.add_window_generated_city(chunks, nodes, edges, self.frame_index);
        }

        if self.debug_overlay_enabled(|overlays| overlays.material_placements) {
            let city_material_placements = self.city_material_placement_visuals();
            geometry
                .add_window_city_material_placements(&city_material_placements, self.frame_index);
        }

        if self.debug_overlay_enabled(|overlays| overlays.streaming_cells) {
            let city_streaming_cells = self.city_streaming_visuals();
            geometry.add_window_city_streaming_cells(&city_streaming_cells, self.frame_index);
        }

        if self.debug_overlay_enabled(|overlays| overlays.route_consequences) {
            let (persistent_cells, danger_fields, route_consequences) =
                self.city_consequence_visuals();
            geometry.add_window_city_consequences(
                &persistent_cells,
                &danger_fields,
                &route_consequences,
                self.frame_index,
            );
        }
    }

    fn interaction_preview(&self) -> Option<PrimaryInteraction> {
        primary_interaction_from_snapshot(
            self.last_snapshot.as_ref()?,
            self.camera.yaw_radians,
            alley_primary_interaction_settings(),
        )
    }

    fn add_interaction_target_preview(&self, geometry: &mut WindowSceneGeometry) {
        let Some(interaction) = self.interaction_preview() else {
            return;
        };

        let (radius, color) = interaction_preview_style(&interaction, self.frame_index);
        geometry.add_world_marker(
            WindowWorldMarkerVisual::new(interaction.target_position, radius, color)
                .with_axis_half_extent_scale(1.15),
        );
    }

    fn add_hud(&self, geometry: &mut WindowSceneGeometry) {
        geometry.add_window_status_hud(
            self.frame_index,
            self.last_event_count,
            self.last_gpu_pass_count,
            self.queued_action_count,
        );
        let color = self
            .interaction_preview()
            .map(|interaction| interaction_reticle_color(&interaction))
            .unwrap_or([0.62, 0.68, 0.78, 0.34]);
        geometry.add_window_interaction_reticle(color);

        let mut visible_pulses = self
            .hud_event_pulses
            .iter()
            .rev()
            .take(WINDOW_HUD_EVENT_STRIP_MAX_PULSES)
            .map(WindowHudEventPulse::visible_pulse)
            .collect::<Vec<_>>();
        visible_pulses.reverse();
        geometry.add_window_hud_event_strip(&visible_pulses, WINDOW_HUD_EVENT_STRIP_MAX_PULSES);
    }

    fn add_event_markers(&self, geometry: &mut WindowSceneGeometry) {
        for marker in &self.event_markers {
            geometry.add_world_marker(marker.visible_marker());
        }
    }

    fn add_gas_volumes(&self, geometry: &mut WindowSceneGeometry) {
        for volume in &self.gas_volumes {
            geometry.add_window_gas_volume(volume.visible_visual(), self.frame_index, self.camera);
        }
    }
}

fn add_beauty_v19_geometry(
    geometry: &mut WindowSceneGeometry,
    scene: &BeautySceneV19,
    viewer_position: [f32; 3],
) {
    for cell in &scene.cells {
        for terrain in &cell.terrain {
            add_beauty_v19_terrain(geometry, terrain);
        }
        for road in &cell.roads {
            add_beauty_v19_road(geometry, road);
        }
        for curb in &cell.curbs {
            add_beauty_v19_curb(geometry, curb);
        }
        for facade in &cell.facades {
            add_beauty_v19_facade(geometry, facade);
        }
        for water in &cell.water_films {
            add_beauty_v19_water_film(geometry, water);
        }
        for plant in &cell.plants {
            add_beauty_v19_plant(geometry, plant);
        }
        for stone in &cell.stones {
            add_beauty_v19_stone(geometry, stone);
        }
        for prop in &cell.landfill_props {
            add_beauty_v19_landfill_prop(geometry, prop);
        }
        for human in &cell.humans {
            add_beauty_v19_human(geometry, human, viewer_position);
        }
        for vehicle in &cell.vehicles {
            add_beauty_v19_vehicle(geometry, vehicle);
        }
    }
}

fn add_beauty_v19_terrain(geometry: &mut WindowSceneGeometry, terrain: &TerrainPatchV19) {
    let seed = v19_seed(terrain.surface_id.0, terrain.material_id.0, 0x7E44);
    let half_x = terrain.size_meters[0] * 0.5;
    let half_y = terrain.size_meters[1] * 0.5;
    let columns = 5;
    let rows = 4;

    for ix in 0..columns {
        for iy in 0..rows {
            let x0 =
                terrain.center.x - half_x + terrain.size_meters[0] * ix as f32 / columns as f32;
            let x1 = terrain.center.x - half_x
                + terrain.size_meters[0] * (ix + 1) as f32 / columns as f32;
            let y0 = terrain.center.y - half_y + terrain.size_meters[1] * iy as f32 / rows as f32;
            let y1 =
                terrain.center.y - half_y + terrain.size_meters[1] * (iy + 1) as f32 / rows as f32;
            let cell_seed = seed ^ (ix as u64 * 0xA31) ^ (iy as u64 * 0xC91);
            let z = terrain.center.z + terrain.unevenness_0_to_1 * 0.018;
            let color = v18_color_jitter(
                [
                    0.115 + terrain.moisture_0_to_1 * 0.026,
                    0.086 + terrain.moisture_0_to_1 * 0.018,
                    0.054 + terrain.moisture_0_to_1 * 0.012,
                    1.0,
                ],
                cell_seed,
                0.026,
            );
            geometry.world_quad_with_surface_response(
                [
                    x0,
                    y0,
                    z + v16_signed(cell_seed, 1) * terrain.unevenness_0_to_1 * 0.045,
                ],
                [
                    x1,
                    y0,
                    z + v16_signed(cell_seed, 2) * terrain.unevenness_0_to_1 * 0.045,
                ],
                [
                    x1,
                    y1,
                    z + v16_signed(cell_seed, 3) * terrain.unevenness_0_to_1 * 0.045,
                ],
                [
                    x0,
                    y1,
                    z + v16_signed(cell_seed, 4) * terrain.unevenness_0_to_1 * 0.045,
                ],
                color,
                WINDOW_SURFACE_RESPONSE_SOIL_V18,
            );

            if (ix + iy) % 2 == 0 {
                let axis = v18_horizontal_axis(cell_seed, 5);
                geometry.world_oriented_rect_with_surface_response(
                    [
                        (x0 + x1) * 0.5 + v16_signed(cell_seed, 6) * 0.34,
                        (y0 + y1) * 0.5 + v16_signed(cell_seed, 7) * 0.34,
                        z + 0.018,
                    ],
                    axis,
                    v18_perp_axis(axis),
                    [
                        0.16 + v16_unit(cell_seed, 8) * 0.42,
                        0.018 + v16_unit(cell_seed, 9) * 0.052,
                    ],
                    [0.046, 0.034, 0.022, 0.30 + terrain.moisture_0_to_1 * 0.18],
                    WINDOW_SURFACE_RESPONSE_SOIL_V18,
                );
            }
        }
    }
}

fn add_beauty_v19_road(geometry: &mut WindowSceneGeometry, road: &RoadPatchV19) {
    for (segment_index, segment) in road.control_points.windows(2).enumerate() {
        let start = v19_vec3(segment[0]);
        let end = v19_vec3(segment[1]);
        let Some(right) = v16_segment_right(start, end) else {
            continue;
        };
        let forward = v16_segment_forward(start, end);
        let half_width = road.width_meters * 0.5;
        let crown = 0.014 + road.camber_0_to_1 * 0.032;
        let seed = v19_seed(road.surface_id.0, road.material_id.0, segment_index as u64);

        geometry.world_quad_with_surface_response(
            [
                start[0] + right[0] * half_width,
                start[1] + right[1] * half_width,
                start[2] + crown,
            ],
            [
                end[0] + right[0] * half_width,
                end[1] + right[1] * half_width,
                end[2] + crown * 0.86,
            ],
            [
                end[0] - right[0] * half_width,
                end[1] - right[1] * half_width,
                end[2] + crown * 0.86,
            ],
            [
                start[0] - right[0] * half_width,
                start[1] - right[1] * half_width,
                start[2] + crown,
            ],
            [0.046, 0.046, 0.041, 1.0],
            WINDOW_SURFACE_RESPONSE_WET_ROAD,
        );

        let crack_count = (road.crack_density_0_to_1 * 18.0).ceil().clamp(3.0, 10.0) as usize;
        for crack in 0..crack_count {
            let crack_seed = seed ^ (crack as u64 * 0xC4AC);
            let t = (crack as f32 + 0.5) / crack_count as f32;
            let lateral = v16_signed(crack_seed, 1) * half_width * 0.72;
            let center = [
                start[0] + (end[0] - start[0]) * t + right[0] * lateral,
                start[1] + (end[1] - start[1]) * t + right[1] * lateral,
                start[2] + crown + 0.004 + crack as f32 * 0.0003,
            ];
            geometry.world_oriented_rect_with_surface_response(
                center,
                [forward[0], forward[1], 0.0],
                [right[0], right[1], 0.0],
                [
                    0.10 + v16_unit(crack_seed, 2) * 0.28,
                    0.006 + v16_unit(crack_seed, 3) * 0.014,
                ],
                [0.010, 0.009, 0.007, 0.46],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        for grit in 0..14 {
            let grit_seed = seed ^ (grit as u64 * 0xA55A);
            let t = 0.04 + grit as f32 * 0.070 + v16_signed(grit_seed, 1) * 0.018;
            let lateral = v16_signed(grit_seed, 2) * half_width * 0.80;
            let center = [
                start[0] + (end[0] - start[0]) * t + right[0] * lateral,
                start[1] + (end[1] - start[1]) * t + right[1] * lateral,
                start[2] + crown + 0.006 + grit as f32 * 0.0002,
            ];
            geometry.world_oriented_rect_with_surface_response(
                center,
                [forward[0], forward[1], 0.0],
                [right[0], right[1], 0.0],
                [
                    0.024 + v16_unit(grit_seed, 3) * 0.072,
                    0.004 + v16_unit(grit_seed, 4) * 0.009,
                ],
                [0.077, 0.071, 0.060, 0.22],
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            );
        }
    }
}

fn add_beauty_v19_curb(geometry: &mut WindowSceneGeometry, curb: &CurbSegmentV19) {
    let start = v19_vec3(curb.start);
    let end = v19_vec3(curb.end);
    let radius = curb.radius_meters.clamp(0.035, 0.22);
    geometry.world_cylinder_between_with_surface_response(
        start,
        end,
        radius,
        18,
        [0.31, 0.30, 0.270, 1.0],
        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
    );

    let Some(right) = v16_segment_right(start, end) else {
        return;
    };
    let chip_count = (curb.chip_density_0_to_1 * 14.0).ceil().clamp(2.0, 8.0) as usize;
    let seed = v19_seed(curb.surface_id.0, curb.material_id.0, 0xC0B);
    for chip in 0..chip_count {
        let chip_seed = seed ^ (chip as u64 * 0x91);
        let t = (chip as f32 + 0.5) / chip_count as f32;
        let center = v19_lerp3(start, end, t);
        geometry.world_oriented_rect_with_surface_response(
            [
                center[0] + right[0] * v16_signed(chip_seed, 1) * radius * 0.80,
                center[1] + right[1] * v16_signed(chip_seed, 2) * radius * 0.80,
                center[2] + radius * 0.35,
            ],
            [right[0], right[1], 0.0],
            [0.0, 0.0, 1.0],
            [
                0.036 + v16_unit(chip_seed, 3) * 0.065,
                0.014 + v16_unit(chip_seed, 4) * 0.032,
            ],
            [0.092, 0.078, 0.058, 0.34],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
    }
}

fn add_beauty_v19_facade(geometry: &mut WindowSceneGeometry, facade: &FacadeModuleV19) {
    let min = v19_vec3(facade.origin);
    let max = [
        facade.origin.x + facade.width_meters,
        facade.origin.y + facade.depth_meters,
        facade.origin.z + facade.height_meters,
    ];
    let seed = v19_seed(facade.surface_id.0, facade.material_id.0, 0xFACA);
    let grime = facade.grime_0_to_1.clamp(0.0, 1.0);
    geometry.world_micro_detailed_box_with_surface_response(
        min,
        max,
        [0.235, 0.225, 0.198, 1.0],
        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        seed,
        0.34 + grime * 0.18,
    );

    let face_y = facade.origin.y - 0.016;
    let columns = (facade.width_meters / 0.82).floor().clamp(4.0, 9.0) as usize;
    let floors = (facade.height_meters / 0.78).floor().clamp(3.0, 7.0) as usize;
    for floor in 0..floors {
        for column in 0..columns {
            let slot_seed = seed ^ (floor as u64 * 0x79) ^ column as u64;
            if v16_unit(slot_seed, 1) < 0.16 {
                continue;
            }
            let x =
                facade.origin.x + facade.width_meters * ((column as f32 + 0.5) / columns as f32);
            let z = facade.origin.z + 0.72 + floor as f32 * 0.68;
            geometry.world_oriented_rect_with_surface_response(
                [x, face_y, z.min(max[2] - 0.25)],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.13, 0.18],
                [0.30, 0.42, 0.46, 0.26 + v16_unit(slot_seed, 2) * 0.16],
                WINDOW_SURFACE_RESPONSE_GLASS,
            );
            geometry.world_oriented_rect_with_surface_response(
                [x, face_y - 0.002, z.min(max[2] - 0.25)],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.165, 0.010],
                [0.045, 0.043, 0.038, 0.62],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        }
    }

    let stain_count = (grime * 12.0).ceil().clamp(3.0, 9.0) as usize;
    for stain in 0..stain_count {
        let stain_seed = seed ^ (stain as u64 * 0x51A1);
        geometry.world_oriented_rect_with_surface_response(
            [
                facade.origin.x + facade.width_meters * v16_unit(stain_seed, 1),
                face_y - 0.004,
                facade.origin.z + facade.height_meters * (0.18 + v16_unit(stain_seed, 2) * 0.70),
            ],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [
                0.09 + v16_unit(stain_seed, 3) * 0.22,
                0.035 + v16_unit(stain_seed, 4) * 0.14,
            ],
            [0.034, 0.028, 0.021, 0.20 + grime * 0.18],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
    }
}

fn add_beauty_v19_water_film(geometry: &mut WindowSceneGeometry, water: &GroundedWaterFilmV19) {
    if !water.is_grounded() {
        return;
    }

    let center = v19_vec3(water.center);
    let seed = v19_seed(water.surface_id.0, water.receiver_surface_id.0, 0xA7E2);
    let radius = water.radius_meters;
    let axis = v18_horizontal_axis(seed, 1);
    let side = v18_perp_axis(axis);
    let jag = water.edge_irregularity_0_to_1.clamp(0.0, 1.0);
    geometry.world_quad_with_surface_response(
        v19_add2(
            center,
            axis,
            side,
            -radius * (0.86 + jag * 0.16),
            -radius * 0.26,
            0.002,
        ),
        v19_add2(
            center,
            axis,
            side,
            -radius * 0.18,
            radius * (0.56 + jag * 0.10),
            0.003,
        ),
        v19_add2(
            center,
            axis,
            side,
            radius * (0.92 + jag * 0.14),
            radius * 0.34,
            0.002,
        ),
        v19_add2(
            center,
            axis,
            side,
            radius * 0.46,
            -radius * (0.58 + jag * 0.12),
            0.003,
        ),
        [0.044, 0.112, 0.132, 0.36],
        WINDOW_SURFACE_RESPONSE_WET_ROAD,
    );
    geometry.world_oriented_rect_with_surface_response(
        v19_add2(center, axis, side, radius * 0.12, -radius * 0.12, 0.007),
        axis,
        side,
        [radius * 0.28, radius * 0.020],
        [0.72, 0.84, 0.80, 0.15],
        WINDOW_SURFACE_RESPONSE_WET_ROAD,
    );
}

fn add_beauty_v19_plant(geometry: &mut WindowSceneGeometry, plant: &PlantInstanceV19) {
    let seed = v19_seed(plant.surface_id.0, plant.material_id.0, 0x71A9);
    let root = v19_vec3(plant.root_position);
    let axis = v18_horizontal_axis(seed, 1);
    let height = plant.height_meters.clamp(0.06, 1.2);
    let bend = plant.bend_0_to_1.clamp(0.0, 1.0);
    let tip = [
        root[0] + axis[0] * bend * height * 0.18,
        root[1] + axis[1] * bend * height * 0.18,
        root[2] + height,
    ];
    geometry.world_cylinder_between_with_surface_response(
        root,
        tip,
        (height * 0.035).clamp(0.010, 0.030),
        7,
        [0.105, 0.070, 0.040, 0.82],
        WINDOW_SURFACE_RESPONSE_PLANT_V18,
    );

    let leaf_count = (plant.leaf_density_0_to_1 * 8.0).ceil().clamp(2.0, 7.0) as usize;
    for leaf in 0..leaf_count {
        let leaf_seed = seed ^ (leaf as u64 * 0x45);
        let t = (leaf as f32 + 0.4) / leaf_count as f32;
        let leaf_axis = v18_horizontal_axis(leaf_seed, 2);
        let center = [
            root[0] + (tip[0] - root[0]) * t + leaf_axis[0] * height * 0.11,
            root[1] + (tip[1] - root[1]) * t + leaf_axis[1] * height * 0.11,
            root[2] + height * (0.26 + t * 0.62),
        ];
        geometry.world_oriented_rect_with_surface_response(
            center,
            leaf_axis,
            [
                leaf_axis[0] * (0.10 + bend * 0.18),
                leaf_axis[1] * (0.10 + bend * 0.18),
                1.0,
            ],
            [
                height * (0.08 + v16_unit(leaf_seed, 3) * 0.045),
                height * (0.018 + v16_unit(leaf_seed, 4) * 0.022),
            ],
            v18_color_jitter([0.095, 0.205, 0.075, 0.70], leaf_seed, 0.035),
            WINDOW_SURFACE_RESPONSE_PLANT_V18,
        );
    }
}

fn add_beauty_v19_stone(geometry: &mut WindowSceneGeometry, stone: &StoneInstanceV19) {
    let seed = v19_seed(stone.surface_id.0, stone.material_id.0, 0x5700);
    let center = v19_vec3(stone.center);
    let radius = stone.radius_meters.clamp(0.04, 0.55);
    geometry.world_ellipsoid_with_surface_response(
        [center[0], center[1], center[2] + radius * 0.38],
        [
            radius * (1.00 + stone.angularity_0_to_1 * 0.50),
            radius * (0.62 + v16_unit(seed, 1) * 0.28),
            radius * (0.34 + v16_unit(seed, 2) * 0.22),
        ],
        4,
        10,
        v18_color_jitter([0.265, 0.250, 0.218, 0.96], seed, 0.045),
        WINDOW_SURFACE_RESPONSE_STONE_V18,
    );

    let chip_count = (stone.chip_density_0_to_1 * 7.0).ceil().clamp(1.0, 4.0) as usize;
    for chip in 0..chip_count {
        let chip_seed = seed ^ (chip as u64 * 0xC17);
        let axis = v18_horizontal_axis(chip_seed, 3);
        geometry.world_oriented_rect_with_surface_response(
            [
                center[0] + axis[0] * radius * 0.28,
                center[1] + axis[1] * radius * 0.28,
                center[2] + radius * (0.54 + v16_unit(chip_seed, 4) * 0.32),
            ],
            axis,
            v18_perp_axis(axis),
            [radius * 0.22, radius * 0.024],
            [0.090, 0.082, 0.068, 0.26],
            WINDOW_SURFACE_RESPONSE_STONE_V18,
        );
    }
}

fn add_beauty_v19_landfill_prop(geometry: &mut WindowSceneGeometry, prop: &LandfillPropV19) {
    let seed = v19_seed(prop.surface_id.0, prop.material_id.0, 0x1A9D);
    let center = v19_vec3(prop.center);
    let half = [
        prop.size_meters[0] * 0.5,
        prop.size_meters[1] * 0.5,
        prop.size_meters[2] * 0.5,
    ];
    let base_color = v18_color_jitter(
        [
            0.18 + prop.rust_or_stain_0_to_1 * 0.12,
            0.115 + prop.dirt_0_to_1 * 0.050,
            0.070,
            0.76,
        ],
        seed,
        0.055,
    );

    if prop.deformation_0_to_1 > 0.70 {
        geometry.world_ellipsoid_with_surface_response(
            [center[0], center[1], center[2] + half[2]],
            [
                half[0] * (1.05 + prop.deformation_0_to_1 * 0.35),
                half[1] * (0.82 + v16_unit(seed, 1) * 0.32),
                half[2] * (0.72 + v16_unit(seed, 2) * 0.20),
            ],
            4,
            9,
            base_color,
            WINDOW_SURFACE_RESPONSE_LANDFILL_V18,
        );
    } else {
        geometry.world_micro_detailed_box_with_surface_response(
            [
                center[0] - half[0],
                center[1] - half[1],
                center[2] - half[2],
            ],
            [
                center[0] + half[0],
                center[1] + half[1],
                center[2] + half[2],
            ],
            base_color,
            WINDOW_SURFACE_RESPONSE_LANDFILL_V18,
            seed,
            0.28 + prop.deformation_0_to_1 * 0.24,
        );
    }

    let axis = v18_horizontal_axis(seed, 3);
    geometry.world_oriented_rect_with_surface_response(
        [center[0], center[1], center[2] + half[2] + 0.006],
        axis,
        v18_perp_axis(axis),
        [half[0] * 0.44, half[1] * 0.18],
        [0.56, 0.52, 0.38, 0.22],
        WINDOW_SURFACE_RESPONSE_LANDFILL_V18,
    );
}

fn add_beauty_v19_human(
    geometry: &mut WindowSceneGeometry,
    human: &HumanProxyV19,
    viewer_position: [f32; 3],
) {
    if !human.is_believable_proxy() {
        return;
    }

    let base = v19_vec3(human.world_position);
    geometry.world_flat_ellipse_with_surface_response(
        [base[0], base[1], base[2] + 0.006],
        [human.shoulder_width_meters * 0.74, 0.16],
        18,
        [0.018, 0.015, 0.012, 0.24],
        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
    );

    let render_human = HumanState {
        human_id: human.entity_id,
        quality_tier: QualityTier::HeroHighFidelityRuntime,
    };
    let seed = human.entity_id ^ 0x1900_0019;
    let color = [
        0.66 + v16_unit(seed, 1) * 0.12,
        0.46 + v16_unit(seed, 2) * 0.10,
        0.34 + v16_unit(seed, 3) * 0.08,
        1.0,
    ];
    geometry.add_humanoid_proxy(
        WindowHumanoidProxyInstance::new([base[0], base[1]], base[2], color)
            .with_human(Some(&render_human))
            .with_surface_state(Some(v19_human_surface_state(seed)))
            .with_viewer_position(viewer_position),
    );
}

fn add_beauty_v19_vehicle(geometry: &mut WindowSceneGeometry, vehicle: &VehicleProxyV19) {
    if !vehicle.is_believable_proxy() {
        return;
    }

    let base = v19_vec3(vehicle.world_position);
    let forward = [1.0, 0.0, 0.0];
    let right = [0.0, 1.0, 0.0];
    let length = vehicle.length_meters;
    let half_width = vehicle.width_meters * 0.5;
    let wheel_radius = (vehicle.height_meters * 0.18).clamp(0.18, 0.34);
    let body_z = base[2] + wheel_radius + vehicle.height_meters * 0.21;
    let seed = vehicle.entity_id ^ 0x0CA9_B0D7;

    geometry.world_oriented_rect_with_surface_response(
        [base[0], base[1], base[2] + 0.026],
        forward,
        right,
        [length * 0.62, half_width * 0.74],
        [0.022, 0.019, 0.016, 0.34],
        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
    );
    geometry.world_ellipsoid_with_surface_response(
        v19_offset(base, forward, right, 0.0, 0.0, body_z - base[2]),
        [
            length * 0.48,
            half_width * 0.86,
            vehicle.height_meters * 0.22,
        ],
        6,
        16,
        v18_color_jitter([0.17, 0.21, 0.225, 1.0], seed, 0.035),
        WINDOW_SURFACE_RESPONSE_METAL,
    );
    geometry.world_ellipsoid_with_surface_response(
        v19_offset(
            base,
            forward,
            right,
            -length * 0.07,
            0.0,
            body_z - base[2] + vehicle.height_meters * 0.24,
        ),
        [
            length * 0.24,
            half_width * 0.56,
            vehicle.height_meters * 0.15,
        ],
        5,
        12,
        [0.31, 0.45, 0.50, 0.60],
        WINDOW_SURFACE_RESPONSE_GLASS,
    );

    for seam in [-0.28_f32, -0.04, 0.22] {
        geometry.world_oriented_rect_with_surface_response(
            v19_offset(
                base,
                forward,
                right,
                length * seam,
                0.0,
                body_z - base[2] + vehicle.height_meters * 0.12,
            ),
            right,
            [0.0, 0.0, 1.0],
            [half_width * 0.78, 0.010],
            [0.022, 0.023, 0.022, 0.48],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
    }

    for axle in [-0.34_f32, 0.34] {
        for side in [-1.0_f32, 1.0] {
            let wheel_center = v19_offset(
                base,
                forward,
                right,
                axle * length,
                side * half_width * 0.86,
                wheel_radius,
            );
            geometry.world_ellipsoid_with_surface_response(
                wheel_center,
                [wheel_radius * 0.96, wheel_radius * 0.30, wheel_radius],
                5,
                12,
                [0.022, 0.022, 0.020, 0.92],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            geometry.world_ellipse_ring_with_surface_response(
                wheel_center,
                [wheel_radius * 0.35, wheel_radius * 0.08],
                [wheel_radius * 0.76, wheel_radius * 0.17],
                14,
                [0.13, 0.13, 0.12, 0.52],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        }
    }

    for side in [-1.0_f32, 1.0] {
        geometry.world_oriented_rect_with_surface_response(
            v19_offset(
                base,
                forward,
                right,
                length * 0.46,
                side * half_width * 0.38,
                body_z - base[2] + 0.06,
            ),
            right,
            [0.0, 0.0, 1.0],
            [0.055, 0.028],
            [0.90, 0.78, 0.46, 0.72],
            WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
        );
        geometry.world_oriented_rect_with_surface_response(
            v19_offset(
                base,
                forward,
                right,
                -length * 0.48,
                side * half_width * 0.34,
                body_z - base[2] + 0.04,
            ),
            right,
            [0.0, 0.0, 1.0],
            [0.050, 0.025],
            [0.70, 0.05, 0.035, 0.42],
            WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
        );
    }
}

fn v19_vec3(point: Vec3V19) -> [f32; 3] {
    [point.x, point.y, point.z]
}

fn v19_seed(surface_id: u64, material_id: u64, salt: u64) -> u64 {
    surface_id.wrapping_mul(0x9E37_79B9).rotate_left(17)
        ^ material_id.rotate_left(31)
        ^ salt.wrapping_mul(0xBF58_476D)
}

fn v19_lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn v19_add2(
    center: [f32; 3],
    axis: [f32; 3],
    side: [f32; 3],
    along: f32,
    lateral: f32,
    z_offset: f32,
) -> [f32; 3] {
    [
        center[0] + axis[0] * along + side[0] * lateral,
        center[1] + axis[1] * along + side[1] * lateral,
        center[2] + z_offset,
    ]
}

fn v19_offset(
    base: [f32; 3],
    forward: [f32; 3],
    right: [f32; 3],
    forward_offset: f32,
    right_offset: f32,
    z_offset: f32,
) -> [f32; 3] {
    [
        base[0] + forward[0] * forward_offset + right[0] * right_offset,
        base[1] + forward[1] * forward_offset + right[1] * right_offset,
        base[2] + z_offset,
    ]
}

fn v19_human_surface_state(seed: u64) -> HumanSurfaceState {
    HumanSurfaceState {
        skin_wetness: 0.24 + v16_unit(seed, 10) * 0.18,
        sweat_sheen: 0.12 + v16_unit(seed, 11) * 0.12,
        oil_sheen: 0.16 + v16_unit(seed, 12) * 0.08,
        bruising: v16_unit(seed, 13) * 0.05,
        injury_overlay: 0.0,
        dirt: 0.18 + v16_unit(seed, 14) * 0.18,
        hair_wetness: 0.22 + v16_unit(seed, 15) * 0.18,
        clothing_wetness: 0.28 + v16_unit(seed, 16) * 0.22,
        clothing_damage: 0.06 + v16_unit(seed, 17) * 0.12,
        eye_redness: 0.03 + v16_unit(seed, 18) * 0.07,
        material_layers: Vec::new(),
    }
}

fn add_beauty_v18_city_edge(geometry: &mut WindowSceneGeometry, _frame_index: u64) {
    let seed = 0xC17E_1800;
    let buildings = [
        (-6.4, -3.7, 16.7, 4.1, [0.245, 0.238, 0.216, 1.0]),
        (-3.3, -0.8, 17.0, 5.2, [0.220, 0.226, 0.218, 1.0]),
        (-0.3, 2.6, 16.5, 3.7, [0.270, 0.260, 0.232, 1.0]),
        (3.0, 5.9, 16.9, 4.6, [0.200, 0.208, 0.202, 1.0]),
    ];

    for (index, (x0, x1, y, height, color)) in buildings.into_iter().enumerate() {
        let building_seed = seed ^ (index as u64 * 0x1001);
        geometry.world_micro_detailed_box_with_surface_response(
            [x0, y, 0.0],
            [x1, y + 0.54, height],
            color,
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            building_seed,
            0.42,
        );

        let front_y = y - 0.018;
        let width = (x1 - x0).abs();
        let columns = (width / 0.58).floor().clamp(2.0, 6.0) as usize;
        let floors = (height / 0.78).floor().clamp(3.0, 7.0) as usize;
        for floor in 0..floors {
            for column in 0..columns {
                let slot_seed = building_seed ^ (floor as u64 * 0x71) ^ column as u64;
                if v16_unit(slot_seed, 3) < 0.20 {
                    continue;
                }
                let x = x0 + width * ((column as f32 + 0.5) / columns as f32);
                let z = 0.72 + floor as f32 * 0.70;
                geometry.world_oriented_rect_with_surface_response(
                    [x, front_y, z.min(height - 0.28)],
                    [1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0],
                    [0.12, 0.17],
                    [0.34, 0.44, 0.47, 0.26],
                    WINDOW_SURFACE_RESPONSE_GLASS,
                );
                if v16_unit(slot_seed, 4) > 0.72 {
                    geometry.world_oriented_rect_with_surface_response(
                        [x, front_y - 0.002, z.min(height - 0.28) - 0.01],
                        [1.0, 0.0, 0.0],
                        [0.0, 0.0, 1.0],
                        [0.075, 0.12],
                        [0.78, 0.56, 0.34, 0.16],
                        WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
                    );
                }
            }
        }

        for stain in 0..7 {
            let stain_seed = building_seed ^ (stain as u64 * 0x51A1);
            let x = x0 + width * v16_unit(stain_seed, 1);
            let z = 0.65 + (height - 1.0) * v16_unit(stain_seed, 2);
            geometry.world_oriented_rect_with_surface_response(
                [x, front_y - 0.004, z],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [
                    0.10 + v16_unit(stain_seed, 3) * 0.20,
                    0.035 + v16_unit(stain_seed, 4) * 0.16,
                ],
                [0.040, 0.034, 0.026, 0.24],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        let roof_z = height + 0.06;
        geometry.world_cylinder_between_with_surface_response(
            [x0 + width * 0.22, y + 0.25, roof_z],
            [x0 + width * 0.22, y + 0.25, roof_z + 0.42],
            0.11,
            12,
            [0.12, 0.13, 0.12, 0.92],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        geometry.world_cylinder_between_with_surface_response(
            [x0 + width * 0.22, y + 0.25, roof_z + 0.40],
            [x0 + width * 0.47, y + 0.25, roof_z + 0.42],
            0.026,
            8,
            [0.10, 0.10, 0.095, 0.86],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
    }

    for rail in 0..5 {
        let x = -6.4 + rail as f32 * 3.1;
        geometry.world_cylinder_between_with_surface_response(
            [x, 16.30, 0.10],
            [x + 1.55, 16.42, 0.42],
            0.018,
            7,
            [0.060, 0.064, 0.060, 0.78],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
    }
}

fn add_beauty_v18_nature_edge(geometry: &mut WindowSceneGeometry, frame_index: u64) {
    let seed = 0x5011_1800;
    geometry.world_quad_with_surface_response(
        [-11.35, -12.25, 0.012],
        [-5.35, -11.70, 0.018],
        [-5.62, 15.90, 0.032],
        [-10.95, 16.55, 0.026],
        [0.145, 0.100, 0.060, 1.0],
        WINDOW_SURFACE_RESPONSE_SOIL_V18,
    );

    for patch in 0..18 {
        let patch_seed = seed ^ (patch as u64 * 0xA71);
        let center = [
            -10.75 + v16_unit(patch_seed, 1) * 4.85,
            -11.35 + v16_unit(patch_seed, 2) * 26.5,
            0.034 + patch as f32 * 0.0003,
        ];
        let axis = v18_horizontal_axis(patch_seed, 3);
        geometry.world_oriented_rect_with_surface_response(
            center,
            axis,
            v18_perp_axis(axis),
            [
                0.20 + v16_unit(patch_seed, 4) * 0.46,
                0.05 + v16_unit(patch_seed, 5) * 0.16,
            ],
            [0.070, 0.052, 0.032, 0.38],
            WINDOW_SURFACE_RESPONSE_SOIL_V18,
        );
    }

    for tuft in 0..46 {
        let tuft_seed = seed ^ (tuft as u64 * 0xB1A9);
        let base = [
            -10.85 + v16_unit(tuft_seed, 1) * 4.95,
            -11.15 + v16_unit(tuft_seed, 2) * 26.1,
            0.050,
        ];
        let blade_count = 2 + (v16_unit(tuft_seed, 3) * 3.0) as usize;
        for blade in 0..blade_count {
            let blade_seed = tuft_seed ^ (blade as u64 * 0x61);
            let axis = v18_horizontal_axis(blade_seed, 4);
            let sideways = v18_perp_axis(axis);
            let height = 0.16 + v16_unit(blade_seed, 5) * 0.34;
            let wind = ((frame_index as f32 * 0.025)
                + v16_unit(blade_seed, 6) * std::f32::consts::TAU)
                .sin()
                * 0.032;
            let lean = 0.08 + v16_unit(blade_seed, 7) * 0.17 + wind;
            let blade_base = [
                base[0] + v16_signed(blade_seed, 8) * 0.12,
                base[1] + v16_signed(blade_seed, 9) * 0.12,
                base[2],
            ];
            geometry.world_oriented_rect_with_surface_response(
                [
                    blade_base[0] + axis[0] * lean * height * 0.35,
                    blade_base[1] + axis[1] * lean * height * 0.35,
                    blade_base[2] + height * 0.50,
                ],
                sideways,
                [axis[0] * lean, axis[1] * lean, 1.0],
                [0.006 + v16_unit(blade_seed, 10) * 0.008, height * 0.50],
                v18_color_jitter([0.125, 0.245, 0.095, 0.78], blade_seed, 0.045),
                WINDOW_SURFACE_RESPONSE_PLANT_V18,
            );
        }
    }

    for shrub in 0..8 {
        let shrub_seed = seed ^ (shrub as u64 * 0x5157);
        let center = [
            -10.45 + v16_unit(shrub_seed, 1) * 4.25,
            -10.40 + v16_unit(shrub_seed, 2) * 24.4,
            0.070,
        ];
        geometry.world_oriented_rect_with_surface_response(
            [center[0], center[1], center[2] + 0.004],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.42, 0.30],
            [0.018, 0.016, 0.010, 0.20],
            WINDOW_SURFACE_RESPONSE_SOIL_V18,
        );
        geometry.world_cylinder_between_with_surface_response(
            [center[0], center[1], center[2]],
            [
                center[0] + v16_signed(shrub_seed, 3) * 0.045,
                center[1] + v16_signed(shrub_seed, 4) * 0.045,
                center[2] + 0.38,
            ],
            0.026,
            7,
            [0.115, 0.070, 0.040, 0.84],
            WINDOW_SURFACE_RESPONSE_SOIL_V18,
        );
        for leaf in 0..4 {
            let leaf_seed = shrub_seed ^ (leaf as u64 * 0x91);
            let leaf_axis = v18_horizontal_axis(leaf_seed, 5);
            geometry.world_ellipsoid_with_surface_response(
                [
                    center[0] + leaf_axis[0] * (0.12 + v16_unit(leaf_seed, 6) * 0.22),
                    center[1] + leaf_axis[1] * (0.12 + v16_unit(leaf_seed, 7) * 0.22),
                    center[2] + 0.28 + v16_unit(leaf_seed, 8) * 0.26,
                ],
                [
                    0.22 + v16_unit(leaf_seed, 9) * 0.14,
                    0.12 + v16_unit(leaf_seed, 10) * 0.11,
                    0.12 + v16_unit(leaf_seed, 11) * 0.12,
                ],
                4,
                10,
                v18_color_jitter([0.105, 0.210, 0.085, 0.72], leaf_seed, 0.035),
                WINDOW_SURFACE_RESPONSE_PLANT_V18,
            );
        }
    }

    for stone in 0..16 {
        let stone_seed = seed ^ (stone as u64 * 0x5700);
        let radius = 0.08 + v16_unit(stone_seed, 1) * 0.22;
        let center = [
            -10.85 + v16_unit(stone_seed, 2) * 4.95,
            -11.50 + v16_unit(stone_seed, 3) * 26.7,
            0.035 + radius * 0.42,
        ];
        geometry.world_ellipsoid_with_surface_response(
            center,
            [
                radius * (0.95 + v16_unit(stone_seed, 4) * 0.55),
                radius * (0.58 + v16_unit(stone_seed, 5) * 0.40),
                radius * (0.30 + v16_unit(stone_seed, 6) * 0.34),
            ],
            4,
            9,
            v18_color_jitter([0.27, 0.255, 0.220, 0.96], stone_seed, 0.050),
            WINDOW_SURFACE_RESPONSE_STONE_V18,
        );
        let axis = v18_horizontal_axis(stone_seed, 7);
        geometry.world_oriented_rect_with_surface_response(
            [center[0], center[1], center[2] + radius * 0.22],
            axis,
            v18_perp_axis(axis),
            [radius * 0.35, radius * 0.022],
            [0.08, 0.075, 0.064, 0.24],
            WINDOW_SURFACE_RESPONSE_STONE_V18,
        );
    }
}

fn add_beauty_v18_landfill_edge(geometry: &mut WindowSceneGeometry, _frame_index: u64) {
    let seed = 0x1A9D_F111;
    geometry.world_quad_with_surface_response(
        [6.15, -12.25, 0.010],
        [11.85, -12.70, 0.025],
        [11.45, 14.65, 0.052],
        [6.35, 13.95, 0.024],
        [0.125, 0.095, 0.062, 1.0],
        WINDOW_SURFACE_RESPONSE_LANDFILL_V18,
    );

    for mound in 0..6 {
        let mound_seed = seed ^ (mound as u64 * 0xA9D);
        let center = [
            6.85 + v16_unit(mound_seed, 1) * 4.25,
            -10.80 + v16_unit(mound_seed, 2) * 23.1,
            0.16 + v16_unit(mound_seed, 3) * 0.08,
        ];
        geometry.world_ellipsoid_with_surface_response(
            center,
            [
                0.62 + v16_unit(mound_seed, 4) * 0.70,
                0.42 + v16_unit(mound_seed, 5) * 0.84,
                0.18 + v16_unit(mound_seed, 6) * 0.16,
            ],
            4,
            10,
            v18_color_jitter([0.145, 0.105, 0.068, 0.88], mound_seed, 0.045),
            WINDOW_SURFACE_RESPONSE_LANDFILL_V18,
        );
        let axis = v18_horizontal_axis(mound_seed, 7);
        geometry.world_oriented_rect_with_surface_response(
            [center[0], center[1], center[2] + 0.10],
            axis,
            v18_perp_axis(axis),
            [
                0.38 + v16_unit(mound_seed, 8) * 0.36,
                0.045 + v16_unit(mound_seed, 9) * 0.10,
            ],
            [0.055, 0.043, 0.032, 0.34],
            WINDOW_SURFACE_RESPONSE_SOIL_V18,
        );
    }

    for scrap in 0..34 {
        let scrap_seed = seed ^ (scrap as u64 * 0x51);
        let x = 6.65 + v16_unit(scrap_seed, 1) * 4.65;
        let y = -11.35 + v16_unit(scrap_seed, 2) * 25.0;
        let z = 0.060 + v16_unit(scrap_seed, 3) * 0.18;
        let axis = v18_horizontal_axis(scrap_seed, 4);
        let kind = scrap % 7;
        match kind {
            0 => geometry.world_oriented_rect_with_surface_response(
                [x, y, z + 0.030],
                axis,
                v18_perp_axis(axis),
                [
                    0.16 + v16_unit(scrap_seed, 5) * 0.24,
                    0.04 + v16_unit(scrap_seed, 6) * 0.12,
                ],
                [0.42, 0.44, 0.41, 0.50],
                WINDOW_SURFACE_RESPONSE_METAL,
            ),
            1 => geometry.world_micro_detailed_box_with_surface_response(
                [x - 0.14, y - 0.09, z - 0.035],
                [x + 0.16, y + 0.10, z + 0.105],
                [0.24, 0.16, 0.08, 0.80],
                WINDOW_SURFACE_RESPONSE_LANDFILL_V18,
                scrap_seed,
                0.46,
            ),
            2 => geometry.world_cylinder_between_with_surface_response(
                [x - axis[0] * 0.22, y - axis[1] * 0.22, z + 0.020],
                [x + axis[0] * 0.22, y + axis[1] * 0.22, z + 0.035],
                0.030 + v16_unit(scrap_seed, 7) * 0.024,
                8,
                [0.050, 0.052, 0.050, 0.78],
                WINDOW_SURFACE_RESPONSE_METAL,
            ),
            3 => geometry.world_oriented_rect_with_surface_response(
                [x, y, z + 0.018],
                axis,
                v18_perp_axis(axis),
                [
                    0.08 + v16_unit(scrap_seed, 8) * 0.12,
                    0.035 + v16_unit(scrap_seed, 9) * 0.07,
                ],
                [0.58, 0.70, 0.68, 0.22],
                WINDOW_SURFACE_RESPONSE_GLASS,
            ),
            4 => geometry.world_ellipsoid_with_surface_response(
                [x, y, z + 0.050],
                [
                    0.16 + v16_unit(scrap_seed, 10) * 0.08,
                    0.06 + v16_unit(scrap_seed, 11) * 0.04,
                    0.14 + v16_unit(scrap_seed, 12) * 0.05,
                ],
                4,
                10,
                [0.020, 0.020, 0.018, 0.82],
                WINDOW_SURFACE_RESPONSE_LANDFILL_V18,
            ),
            5 => geometry.world_quad_with_surface_response(
                [x - 0.20, y - 0.08, z],
                [x + 0.17, y - 0.11, z + 0.014],
                [x + 0.22, y + 0.10, z + 0.028],
                [x - 0.16, y + 0.14, z + 0.006],
                [0.12, 0.15, 0.18, 0.42],
                WINDOW_SURFACE_RESPONSE_LANDFILL_V18,
            ),
            _ => geometry.world_oriented_rect_with_surface_response(
                [x, y, z + 0.012],
                axis,
                [0.0, 0.0, 1.0],
                [
                    0.06 + v16_unit(scrap_seed, 13) * 0.12,
                    0.035 + v16_unit(scrap_seed, 14) * 0.08,
                ],
                [0.30, 0.12, 0.08, 0.46],
                WINDOW_SURFACE_RESPONSE_LANDFILL_V18,
            ),
        }
    }

    for cable in 0..7 {
        let cable_seed = seed ^ (cable as u64 * 0xCABA);
        let start = [
            6.55 + v16_unit(cable_seed, 1) * 4.80,
            -11.40 + v16_unit(cable_seed, 2) * 24.8,
            0.070,
        ];
        let axis = v18_horizontal_axis(cable_seed, 3);
        let perp = v18_perp_axis(axis);
        let mid = [
            start[0] + axis[0] * (0.36 + v16_unit(cable_seed, 4) * 0.36),
            start[1] + axis[1] * (0.36 + v16_unit(cable_seed, 5) * 0.36),
            start[2] + 0.018,
        ];
        let end = [
            mid[0] + perp[0] * (0.18 + v16_unit(cable_seed, 6) * 0.24),
            mid[1] + perp[1] * (0.18 + v16_unit(cable_seed, 7) * 0.24),
            start[2] + 0.012,
        ];
        geometry.world_cylinder_between_with_surface_response(
            start,
            mid,
            0.014,
            6,
            [0.025, 0.025, 0.023, 0.72],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        geometry.world_cylinder_between_with_surface_response(
            mid,
            end,
            0.014,
            6,
            [0.025, 0.025, 0.023, 0.72],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
    }
}

fn v18_horizontal_axis(seed: u64, salt: u64) -> [f32; 3] {
    let angle = v16_unit(seed, salt) * std::f32::consts::TAU;
    [angle.cos(), angle.sin(), 0.0]
}

fn v18_perp_axis(axis: [f32; 3]) -> [f32; 3] {
    [-axis[1], axis[0], 0.0]
}

fn v18_color_jitter(base: [f32; 4], seed: u64, amount: f32) -> [f32; 4] {
    [
        (base[0] + v16_signed(seed, 21) * amount).clamp(0.0, 1.0),
        (base[1] + v16_signed(seed, 22) * amount).clamp(0.0, 1.0),
        (base[2] + v16_signed(seed, 23) * amount).clamp(0.0, 1.0),
        base[3],
    ]
}

#[cfg(test)]
fn add_beauty_v16_sky_detail(geometry: &mut WindowSceneGeometry, frame_index: u64) {
    let pulse = (frame_index as f32 * 0.004).sin().mul_add(0.5, 0.5);
    let far_depth = 0.985;

    geometry.screen_rect(
        [-1.0, -1.0],
        [1.0, 1.0],
        far_depth,
        [0.44, 0.55, 0.66, 0.26],
    );
    geometry.screen_rect(
        [-1.0, -0.20],
        [1.0, 0.34],
        far_depth - 0.004,
        [0.70, 0.67, 0.58, 0.16],
    );

    let sun = [0.52, -0.56];
    for (radius, alpha) in [(0.24, 0.08), (0.13, 0.16), (0.058, 0.72)] {
        geometry.screen_ellipse(
            sun,
            [radius, radius],
            36,
            far_depth - 0.012,
            [1.0, 0.86, 0.58, alpha + pulse * 0.04],
        );
    }

    let moon = [-0.58, 0.55];
    geometry.screen_ellipse(
        moon,
        [0.066, 0.066],
        28,
        far_depth - 0.011,
        [0.76, 0.82, 0.86, 0.38],
    );
    geometry.screen_ellipse(
        [moon[0] + 0.032, moon[1] - 0.006],
        [0.056, 0.060],
        28,
        far_depth - 0.012,
        [0.33, 0.42, 0.52, 0.45],
    );

    for layer in 0..3 {
        let y = -0.12 + layer as f32 * 0.25;
        let drift = (frame_index as f32 * (0.0009 + layer as f32 * 0.0004)).fract();
        for cluster in 0..7 {
            let seed = 0xC10D_0000 + layer * 101 + cluster;
            let x = -1.05 + cluster as f32 * 0.36 + drift * 0.18;
            let rough = v16_unit(seed, frame_index / 64 + 1);
            geometry.screen_ellipse(
                [x, y + (rough - 0.5) * 0.055],
                [0.18 + rough * 0.09, 0.034 + layer as f32 * 0.012],
                20,
                far_depth - 0.016 - layer as f32 * 0.002,
                [
                    0.74 - layer as f32 * 0.08,
                    0.78 - layer as f32 * 0.07,
                    0.78 - layer as f32 * 0.05,
                    0.11 + layer as f32 * 0.035,
                ],
            );
        }
    }
}

#[cfg(test)]
fn add_beauty_v16_road(geometry: &mut WindowSceneGeometry, road: &RoadSplineV16) {
    for (segment_index, segment) in road.centerline_world.windows(2).enumerate() {
        let start = segment[0];
        let end = segment[1];
        let Some(right) = v16_segment_right(start, end) else {
            continue;
        };
        let forward = v16_segment_forward(start, end);
        let half_width = road.width_meters * 0.5;
        let crown = road.crown_height_meters.max(0.006);
        let left_start = [
            start[0] + right[0] * half_width,
            start[1] + right[1] * half_width,
            start[2] + crown * 0.25,
        ];
        let left_end = [
            end[0] + right[0] * half_width,
            end[1] + right[1] * half_width,
            end[2] + crown * 0.25,
        ];
        let right_end = [
            end[0] - right[0] * half_width,
            end[1] - right[1] * half_width,
            end[2] + crown * 0.25,
        ];
        let right_start = [
            start[0] - right[0] * half_width,
            start[1] - right[1] * half_width,
            start[2] + crown * 0.25,
        ];

        geometry.world_quad_with_surface_response(
            left_start,
            left_end,
            right_end,
            right_start,
            [0.055, 0.052, 0.047, 1.0],
            WINDOW_SURFACE_RESPONSE_WET_ROAD,
        );

        let center = [
            (start[0] + end[0]) * 0.5
                + v16_signed(road.irregularity.seed, segment_index as u64) * 0.16,
            (start[1] + end[1]) * 0.5,
            (start[2] + end[2]) * 0.5 + 0.014,
        ];
        geometry.world_flat_ellipse_with_surface_response(
            center,
            [
                (road.width_meters * 0.18).max(0.35),
                0.08 + road.edge_noise_meters * 0.22,
            ],
            16,
            [0.024, 0.021, 0.017, 0.32],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );

        for crack in 0..5 {
            let seed = road.irregularity.seed ^ segment_index as u64 ^ (crack as u64 * 0xC4AC);
            let t = 0.12 + crack as f32 * 0.18 + v16_signed(seed, 1) * 0.035;
            let lateral = v16_signed(seed, 2) * half_width * 0.62;
            let center = [
                start[0] + (end[0] - start[0]) * t + right[0] * lateral,
                start[1] + (end[1] - start[1]) * t + right[1] * lateral,
                start[2] + 0.019 + crack as f32 * 0.0004,
            ];
            geometry.world_oriented_rect_with_surface_response(
                center,
                [forward[0], forward[1], 0.0],
                [right[0], right[1], 0.0],
                [
                    0.18 + v16_unit(seed, 3) * 0.22,
                    0.009 + v16_unit(seed, 4) * 0.014,
                ],
                [0.014, 0.012, 0.010, 0.34],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }
    }
}

#[cfg(test)]
fn add_beauty_v16_curb(geometry: &mut WindowSceneGeometry, curb: &CurbSegmentV16) {
    geometry.world_cylinder_between_with_surface_response(
        curb.start_world,
        curb.end_world,
        curb.height_meters.max(0.08) * 0.34,
        12,
        [0.34, 0.33, 0.30, 1.0],
        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
    );
    let center = [
        (curb.start_world[0] + curb.end_world[0]) * 0.5,
        (curb.start_world[1] + curb.end_world[1]) * 0.5,
        curb.start_world[2] + curb.height_meters * 0.68,
    ];
    geometry.world_ellipse_ring_with_surface_response(
        center,
        [0.16, 0.035],
        [0.34, 0.07],
        12,
        [0.09, 0.074, 0.055, 0.18 + curb.chip_density_0_to_1 * 0.22],
        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
    );
}

#[cfg(test)]
fn add_beauty_v16_facade(geometry: &mut WindowSceneGeometry, facade: &FacadeModuleV16) {
    let seed = facade.object_id.0;
    let bounds = &facade.bounds;
    let face_x = if (bounds.max[0] + bounds.min[0]).abs() > bounds.max[0].abs() {
        bounds.min[0] - 0.012
    } else {
        bounds.max[0] + 0.012
    };
    let y_span = (bounds.max[1] - bounds.min[1]).abs().max(1.0);
    let z_span = (bounds.max[2] - bounds.min[2]).abs().max(1.0);
    geometry.world_quad_with_surface_response(
        [face_x, bounds.min[1], bounds.min[2]],
        [face_x, bounds.max[1], bounds.min[2] + 0.06],
        [face_x, bounds.max[1], bounds.max[2]],
        [face_x, bounds.min[1], bounds.max[2] - 0.08],
        [0.30, 0.285, 0.255, 1.0],
        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
    );

    let panel_count = (y_span / 1.4).ceil().clamp(4.0, 12.0) as usize;
    for panel in 0..panel_count {
        let y = bounds.min[1] + y_span * ((panel as f32 + 0.5) / panel_count as f32);
        let z = bounds.min[2] + z_span * (0.20 + v16_unit(seed, panel as u64) * 0.68);
        geometry.world_oriented_rect_with_surface_response(
            [face_x + 0.004, y, z],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.24 + v16_unit(seed, 101 + panel as u64) * 0.18, 0.018],
            [0.075, 0.068, 0.055, 0.22 + facade.dirt_0_to_1 * 0.18],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
    }

    let window_count = facade.window_count.clamp(1, 10) as usize;
    for index in 0..window_count {
        let y_t = (index as f32 + 0.5) / window_count as f32;
        let floor = (index % 4) as f32;
        let y = bounds.min[1] + y_span * y_t;
        let z = bounds.min[2] + 1.15 + (floor + 0.2) * (z_span / 5.0).clamp(0.55, 1.25);
        let flicker = 0.12 + v16_unit(seed, index as u64) * 0.08;
        geometry.world_oriented_rect_with_surface_response(
            [face_x, y, z.min(bounds.max[2] - 0.45)],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.16, 0.24],
            [0.36, 0.50, 0.54, 0.34 + flicker],
            WINDOW_SURFACE_RESPONSE_GLASS,
        );
        geometry.world_ellipse_ring_with_surface_response(
            [face_x + 0.002, y, z.min(bounds.max[2] - 0.45)],
            [0.17, 0.25],
            [0.22, 0.30],
            12,
            [0.048, 0.048, 0.044, 0.42],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
    }

    for index in 0..facade.vent_count.clamp(1, 5) {
        let y = bounds.min[1] + y_span * ((index as f32 + 0.7) / 5.7);
        let z = bounds.min[2] + 0.55 + v16_unit(seed, 101 + index as u64) * 1.4;
        geometry.world_oriented_rect_with_surface_response(
            [face_x, y, z],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.18, 0.055],
            [0.08, 0.082, 0.078, 0.74],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
    }

    for pipe in 0..2 {
        let y = bounds.min[1]
            + y_span * (0.22 + pipe as f32 * 0.54 + v16_signed(seed, 201 + pipe) * 0.04);
        geometry.world_cylinder_between_with_surface_response(
            [face_x + 0.018, y, bounds.min[2] + 0.10],
            [
                face_x + 0.018,
                y + v16_signed(seed, 211 + pipe) * 0.12,
                bounds.max[2] - 0.18,
            ],
            0.026 + v16_unit(seed, 221 + pipe) * 0.014,
            8,
            [0.055, 0.056, 0.052, 0.90],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
    }

    geometry.world_cylinder_between_with_surface_response(
        [face_x, bounds.min[1], bounds.max[2] + 0.02],
        [face_x, bounds.max[1], bounds.max[2] + 0.04],
        0.045,
        10,
        [0.20, 0.19, 0.17, 1.0],
        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
    );
}

#[cfg(test)]
fn add_beauty_v16_curve_object(geometry: &mut WindowSceneGeometry, curve: &CurveObjectV16) {
    let segments = if curve.radius_meters < 0.03 { 7 } else { 10 };
    let color = if curve.radius_meters < 0.03 {
        [0.045, 0.047, 0.046, 0.92]
    } else {
        [0.075, 0.076, 0.072, 1.0]
    };
    for segment in curve.points_world.windows(2) {
        geometry.world_cylinder_between_with_surface_response(
            segment[0],
            segment[1],
            curve.radius_meters,
            segments,
            color,
            WINDOW_SURFACE_RESPONSE_METAL,
        );
    }
}

#[cfg(test)]
fn add_beauty_v16_puddle(geometry: &mut WindowSceneGeometry, puddle: &AnchoredPuddleV16) {
    if !puddle.is_ground_anchored() {
        return;
    }

    geometry.world_flat_ellipse_with_surface_response(
        [
            puddle.center_world[0],
            puddle.center_world[1],
            puddle.center_world[2] + 0.002,
        ],
        [puddle.radius_meters, puddle.radius_meters * 0.55],
        22,
        [0.055, 0.15, 0.18, 0.42],
        WINDOW_SURFACE_RESPONSE_WET_ROAD,
    );
    geometry.world_ellipse_ring_with_surface_response(
        [
            puddle.center_world[0],
            puddle.center_world[1],
            puddle.center_world[2] + 0.004,
        ],
        [puddle.radius_meters * 0.72, puddle.radius_meters * 0.35],
        [puddle.radius_meters, puddle.radius_meters * 0.55],
        22,
        [0.72, 0.80, 0.74, 0.18],
        WINDOW_SURFACE_RESPONSE_WET_ROAD,
    );
}

#[cfg(test)]
fn add_beauty_v16_scatter(geometry: &mut WindowSceneGeometry, scatter: &ScatterFieldV16) {
    let count = ((scatter.item_count_budget as f32 * scatter.density_0_to_1)
        .ceil()
        .clamp(4.0, 14.0)) as usize;
    let span = v16_bounds_span(&scatter.bounds);
    for index in 0..count {
        let seed = scatter.seed ^ index as u64;
        let x = scatter.bounds.min[0] + span[0] * v16_unit(seed, 1);
        let y = scatter.bounds.min[1] + span[1] * v16_unit(seed, 2);
        let z = scatter.bounds.min[2] + 0.035;
        let radius = 0.025 + v16_unit(seed, 3) * 0.055;
        geometry.world_ellipsoid_with_surface_response(
            [x, y, z],
            [radius * 1.8, radius, radius * 0.45],
            3,
            6,
            [0.08, 0.066, 0.048, 0.72],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
    }
}

#[cfg(test)]
fn add_beauty_v16_human(
    geometry: &mut WindowSceneGeometry,
    human: &HumanProxyV16,
    viewer_position: [f32; 3],
) {
    geometry.world_flat_ellipse_with_surface_response(
        [
            human.position_world[0],
            human.position_world[1],
            human.position_world[2] + 0.006,
        ],
        [0.34, 0.16],
        16,
        [0.018, 0.015, 0.012, 0.22],
        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
    );
    geometry.add_humanoid_proxy(
        WindowHumanoidProxyInstance::new(
            [human.position_world[0], human.position_world[1]],
            human.position_world[2],
            [0.47, 0.52, 0.47, 1.0],
        )
        .with_viewer_position(viewer_position),
    );
    geometry.world_ellipsoid_with_surface_response(
        [
            human.position_world[0],
            human.position_world[1],
            human.position_world[2] + human.height_meters * 0.84,
        ],
        [
            human.head_radius_meters * 0.78,
            human.head_radius_meters * 0.70,
            human.head_radius_meters * 0.92,
        ],
        4,
        8,
        [0.52, 0.37, 0.27, 0.66],
        WINDOW_SURFACE_RESPONSE_HUMAN_SKIN,
    );
    geometry.world_ellipsoid_with_surface_response(
        [
            human.position_world[0],
            human.position_world[1],
            human.position_world[2] + human.height_meters * 0.48,
        ],
        [
            human.shoulder_width_meters * 0.34,
            human.shoulder_width_meters * 0.18,
            human.height_meters * 0.14,
        ],
        4,
        8,
        [0.16, 0.27, 0.29, 0.62],
        WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH,
    );
    geometry.world_ellipsoid_with_surface_response(
        [
            human.position_world[0],
            human.position_world[1] - 0.018,
            human.position_world[2] + human.height_meters * 0.91,
        ],
        [
            human.head_radius_meters * 0.86,
            human.head_radius_meters * 0.74,
            human.head_radius_meters * 0.52,
        ],
        4,
        8,
        [0.045, 0.035, 0.028, 0.74],
        WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH,
    );
}

#[cfg(test)]
fn add_beauty_v16_vehicle(geometry: &mut WindowSceneGeometry, vehicle: &VehicleProxyV16) {
    let [x, y, z] = vehicle.position_world;
    let length = vehicle.length_meters;
    let half_width = vehicle.width_meters * 0.5;
    let body_z = z + vehicle.wheel_radius_meters + vehicle.height_meters * 0.22;
    let dirt_shadow = (0.18 + vehicle.dirt_0_to_1 * 0.22).clamp(0.18, 0.48);

    geometry.world_ellipse_ring_with_surface_response(
        [x, y, z + 0.035],
        [length * 0.26, half_width * 0.44],
        [length * 0.62, half_width * 0.78],
        24,
        [0.026, 0.022, 0.018, dirt_shadow],
        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
    );
    geometry.world_ellipsoid_with_surface_response(
        [x, y, body_z],
        [
            length * 0.48,
            half_width * 0.86,
            vehicle.height_meters * 0.22,
        ],
        5,
        14,
        [0.18, 0.22, 0.24, 1.0],
        WINDOW_SURFACE_RESPONSE_METAL,
    );
    geometry.world_ellipsoid_with_surface_response(
        [x - length * 0.06, y, body_z + vehicle.height_meters * 0.23],
        [
            length * 0.24,
            half_width * 0.55,
            vehicle.height_meters * 0.16,
        ],
        4,
        10,
        [0.34, 0.48, 0.52, 0.58],
        WINDOW_SURFACE_RESPONSE_GLASS,
    );

    for seam in [-0.22_f32, 0.0, 0.22] {
        geometry.world_oriented_rect_with_surface_response(
            [x + length * seam, y, body_z + vehicle.height_meters * 0.11],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [half_width * 0.78, 0.012],
            [0.025, 0.026, 0.025, 0.46],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
    }

    for axle in [-0.34_f32, 0.34] {
        for side in [-1.0_f32, 1.0] {
            geometry.world_ellipsoid_with_surface_response(
                [
                    x + axle * length,
                    y + side * half_width * 0.86,
                    z + vehicle.wheel_radius_meters,
                ],
                [
                    vehicle.wheel_radius_meters * 0.95,
                    vehicle.wheel_radius_meters * 0.28,
                    vehicle.wheel_radius_meters,
                ],
                4,
                10,
                [0.026, 0.025, 0.023, 0.9],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            geometry.world_ellipse_ring_with_surface_response(
                [
                    x + axle * length,
                    y + side * half_width * 0.86,
                    z + vehicle.wheel_radius_meters,
                ],
                [
                    vehicle.wheel_radius_meters * 0.34,
                    vehicle.wheel_radius_meters * 0.08,
                ],
                [
                    vehicle.wheel_radius_meters * 0.74,
                    vehicle.wheel_radius_meters * 0.16,
                ],
                12,
                [0.13, 0.13, 0.12, 0.50],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        }
    }

    for (offset, color) in [
        (length * 0.52, [1.0, 0.86, 0.48, 0.34]),
        (-length * 0.52, [0.85, 0.10, 0.055, 0.30]),
    ] {
        geometry.world_oriented_rect_with_surface_response(
            [x + offset, y, body_z + vehicle.height_meters * 0.02],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [half_width * 0.20, 0.055],
            color,
            WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
        );
    }
}

fn add_beauty_v17_sky_detail(geometry: &mut WindowSceneGeometry, frame_index: u64) {
    let far_depth = 0.986;

    geometry.screen_rect(
        [-1.0, -1.0],
        [1.0, 1.0],
        far_depth,
        [0.42, 0.53, 0.65, 0.30],
    );
    geometry.screen_rect(
        [-1.0, -0.24],
        [1.0, 0.32],
        far_depth - 0.003,
        [0.70, 0.66, 0.56, 0.17],
    );
    geometry.screen_rect(
        [-1.0, 0.32],
        [1.0, 1.0],
        far_depth - 0.002,
        [0.22, 0.30, 0.38, 0.10],
    );

    geometry.screen_rect(
        [0.18, -0.58],
        [0.78, -0.51],
        far_depth - 0.010,
        [0.96, 0.80, 0.55, 0.16],
    );
    geometry.screen_rect(
        [0.42, -0.512],
        [0.68, -0.497],
        far_depth - 0.012,
        [1.0, 0.90, 0.70, 0.34],
    );
    geometry.screen_rect(
        [-0.72, 0.52],
        [-0.42, 0.55],
        far_depth - 0.011,
        [0.72, 0.80, 0.86, 0.18],
    );

    for layer in 0..4 {
        let y = -0.18 + layer as f32 * 0.20;
        let drift = (frame_index as f32 * (0.0007 + layer as f32 * 0.00028)).fract();
        for cluster in 0..9 {
            let seed = 0xC10D_1700 + layer * 131 + cluster;
            let x = -1.10 + cluster as f32 * 0.28 + drift * 0.17;
            let rough = v16_unit(seed, frame_index / 64 + 1);
            let band_y = y + (rough - 0.5) * 0.060;
            geometry.screen_rect(
                [
                    x - 0.16 - rough * 0.07,
                    band_y - 0.016 - layer as f32 * 0.004,
                ],
                [
                    x + 0.18 + rough * 0.09,
                    band_y + 0.012 + layer as f32 * 0.003,
                ],
                far_depth - 0.017 - layer as f32 * 0.002,
                [
                    0.76 - layer as f32 * 0.070,
                    0.79 - layer as f32 * 0.060,
                    0.78 - layer as f32 * 0.050,
                    0.095 + layer as f32 * 0.030,
                ],
            );
        }
    }
}

fn add_beauty_v17_road(geometry: &mut WindowSceneGeometry, road: &RoadPatchV17) {
    for (segment_index, segment) in road.centerline_world.windows(2).enumerate() {
        let start = segment[0];
        let end = segment[1];
        let Some(right) = v16_segment_right(start, end) else {
            continue;
        };
        let forward = v16_segment_forward(start, end);
        let half_width = road.width_meters * 0.5;
        let crown = road.crown_height_meters.max(0.008);
        let seed = road.irregularity.seed ^ (segment_index as u64 * 0x1700);

        let left_start = [
            start[0] + right[0] * half_width,
            start[1] + right[1] * half_width,
            start[2] + crown * 0.30,
        ];
        let left_end = [
            end[0] + right[0] * half_width,
            end[1] + right[1] * half_width,
            end[2] + crown * 0.30,
        ];
        let right_end = [
            end[0] - right[0] * half_width,
            end[1] - right[1] * half_width,
            end[2] + crown * 0.30,
        ];
        let right_start = [
            start[0] - right[0] * half_width,
            start[1] - right[1] * half_width,
            start[2] + crown * 0.30,
        ];

        geometry.world_quad_with_surface_response(
            left_start,
            left_end,
            right_end,
            right_start,
            [0.052, 0.050, 0.046, 1.0],
            WINDOW_SURFACE_RESPONSE_WET_ROAD,
        );

        for side in [-1.0_f32, 1.0] {
            let edge_center = [
                (start[0] + end[0]) * 0.5 + right[0] * half_width * side,
                (start[1] + end[1]) * 0.5 + right[1] * half_width * side,
                (start[2] + end[2]) * 0.5 + 0.019,
            ];
            geometry.world_oriented_rect_with_surface_response(
                edge_center,
                [forward[0], forward[1], 0.0],
                [right[0] * side, right[1] * side, 0.0],
                [
                    0.95 + v16_unit(seed, 40 + side.to_bits() as u64) * 0.38,
                    0.030 + road.edge_noise_meters * 0.18,
                ],
                [0.020, 0.017, 0.012, 0.26],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        for crack in 0..7 {
            let crack_seed = seed ^ (crack as u64 * 0xC4AC);
            let t = 0.08 + crack as f32 * 0.14 + v16_signed(crack_seed, 1) * 0.032;
            let lateral = v16_signed(crack_seed, 2) * half_width * 0.66;
            let center = [
                start[0] + (end[0] - start[0]) * t + right[0] * lateral,
                start[1] + (end[1] - start[1]) * t + right[1] * lateral,
                start[2] + 0.021 + crack as f32 * 0.0004,
            ];
            geometry.world_oriented_rect_with_surface_response(
                center,
                [forward[0], forward[1], 0.0],
                [right[0], right[1], 0.0],
                [
                    0.14 + v16_unit(crack_seed, 3) * 0.26,
                    0.007 + v16_unit(crack_seed, 4) * 0.017,
                ],
                [0.012, 0.010, 0.008, 0.42],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }

        for fleck in 0..10 {
            let fleck_seed = seed ^ (fleck as u64 * 0xA55A);
            let t = 0.05 + fleck as f32 * 0.095 + v16_signed(fleck_seed, 1) * 0.025;
            let lateral = v16_signed(fleck_seed, 2) * half_width * 0.74;
            let center = [
                start[0] + (end[0] - start[0]) * t + right[0] * lateral,
                start[1] + (end[1] - start[1]) * t + right[1] * lateral,
                start[2] + 0.020 + fleck as f32 * 0.0002,
            ];
            let brighter = fleck % 4 == 0;
            geometry.world_oriented_rect_with_surface_response(
                center,
                [forward[0], forward[1], 0.0],
                [right[0], right[1], 0.0],
                [
                    0.030 + v16_unit(fleck_seed, 3) * 0.075,
                    0.004 + v16_unit(fleck_seed, 4) * 0.008,
                ],
                if brighter {
                    [0.082, 0.078, 0.068, 0.28]
                } else {
                    [0.022, 0.020, 0.017, 0.24]
                },
                WINDOW_SURFACE_RESPONSE_WET_ROAD,
            );
        }

        let pothole_count = road.pothole_count.clamp(1, 4) as usize;
        for pothole in 0..pothole_count {
            let pothole_seed = seed ^ (pothole as u64 * 0xF07E);
            let t = 0.18 + pothole as f32 * 0.20 + v16_signed(pothole_seed, 1) * 0.04;
            let lateral = v16_signed(pothole_seed, 2) * half_width * 0.45;
            let center = [
                start[0] + (end[0] - start[0]) * t + right[0] * lateral,
                start[1] + (end[1] - start[1]) * t + right[1] * lateral,
                start[2] + 0.023,
            ];
            geometry.world_oriented_rect_with_surface_response(
                center,
                [forward[0], forward[1], 0.0],
                [right[0], right[1], 0.0],
                [
                    0.18 + v16_unit(pothole_seed, 3) * 0.24,
                    0.045 + v16_unit(pothole_seed, 4) * 0.075,
                ],
                [0.016, 0.014, 0.011, 0.40],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
            geometry.world_oriented_rect_with_surface_response(
                [
                    center[0] + right[0] * 0.06,
                    center[1] + right[1] * 0.06,
                    center[2] + 0.002,
                ],
                [right[0], right[1], 0.0],
                [forward[0], forward[1], 0.0],
                [
                    0.055 + v16_unit(pothole_seed, 5) * 0.080,
                    0.020 + v16_unit(pothole_seed, 6) * 0.040,
                ],
                [0.035, 0.028, 0.020, 0.34],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }
    }
}

fn add_beauty_v17_curb(geometry: &mut WindowSceneGeometry, curb: &CurbSegmentV17) {
    let radius = curb
        .bevel_radius_meters
        .max(curb.height_meters * 0.28)
        .min(curb.width_meters * 0.55);
    geometry.world_cylinder_between_with_surface_response(
        curb.start_world,
        curb.end_world,
        radius,
        16,
        [0.32, 0.31, 0.285, 1.0],
        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
    );

    let Some(right) = v16_segment_right(curb.start_world, curb.end_world) else {
        return;
    };
    let chip_count = (curb.chip_density_0_to_1 * 13.0).ceil().clamp(2.0, 8.0) as usize;
    for chip in 0..chip_count {
        let t = (chip as f32 + 0.5) / chip_count as f32;
        let center = v17_lerp3(curb.start_world, curb.end_world, t);
        let seed = curb.irregularity.seed ^ (chip as u64 * 0xC0B);
        geometry.world_oriented_rect_with_surface_response(
            [
                center[0] + right[0] * v16_signed(seed, 1) * curb.width_meters * 0.35,
                center[1] + right[1] * v16_signed(seed, 1) * curb.width_meters * 0.35,
                center[2] + curb.height_meters * 0.45,
            ],
            [right[0], right[1], 0.0],
            [0.0, 0.0, 1.0],
            [
                0.045 + v16_unit(seed, 2) * 0.070,
                0.018 + v16_unit(seed, 3) * 0.030,
            ],
            [0.10, 0.086, 0.064, 0.34],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
    }
}

fn add_beauty_v17_facade(geometry: &mut WindowSceneGeometry, facade: &FacadeModuleV17) {
    let seed = facade.object_id.0 ^ facade.irregularity.seed;
    let bounds = &facade.bounds;
    let y_span = (bounds.max[1] - bounds.min[1]).abs().max(1.0);
    let z_span = (bounds.max[2] - bounds.min[2]).abs().max(1.0);
    let face_x = if bounds.max[0].abs() > bounds.min[0].abs() {
        bounds.min[0] - 0.012
    } else {
        bounds.max[0] + 0.012
    };

    geometry.world_quad_with_surface_response(
        [face_x, bounds.min[1], bounds.min[2]],
        [
            face_x + v16_signed(seed, 1) * facade.facade_warp_meters,
            bounds.max[1],
            bounds.min[2] + 0.055,
        ],
        [face_x, bounds.max[1], bounds.max[2]],
        [
            face_x + v16_signed(seed, 2) * facade.facade_warp_meters,
            bounds.min[1],
            bounds.max[2] - 0.070,
        ],
        [0.29, 0.274, 0.244, 1.0],
        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
    );

    let panel_count = (y_span / 1.15).ceil().clamp(5.0, 16.0) as usize;
    for panel in 0..panel_count {
        let y = bounds.min[1] + y_span * ((panel as f32 + 0.5) / panel_count as f32);
        let z = bounds.min[2] + z_span * (0.14 + v16_unit(seed, panel as u64) * 0.76);
        geometry.world_oriented_rect_with_surface_response(
            [face_x + 0.004, y, z],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.18 + v16_unit(seed, 101 + panel as u64) * 0.22, 0.014],
            [0.066, 0.058, 0.045, 0.22 + facade.dirt_0_to_1 * 0.20],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
    }

    let window_count = facade.window_count.clamp(2, 14) as usize;
    for index in 0..window_count {
        let y_t = (index as f32 + 0.5) / window_count as f32;
        let floor = (index % 5) as f32;
        let y = bounds.min[1] + y_span * y_t;
        let z = (bounds.min[2] + 1.10 + (floor + 0.20) * (z_span / 6.0).clamp(0.46, 1.10))
            .min(bounds.max[2] - 0.38);
        let glass_alpha = 0.30 + v16_unit(seed, index as u64) * 0.16;
        geometry.world_oriented_rect_with_surface_response(
            [face_x, y, z],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.145, 0.220],
            [0.30, 0.43, 0.48, glass_alpha],
            WINDOW_SURFACE_RESPONSE_GLASS,
        );
        if index % 4 == 1 {
            geometry.world_oriented_rect_with_surface_response(
                [face_x + 0.001, y, z - 0.012],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.092, 0.155],
                [0.74, 0.52, 0.30, 0.20],
                WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
            );
        }
        geometry.world_oriented_rect_with_surface_response(
            [face_x + 0.003, y, z],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.175, 0.012],
            [0.045, 0.045, 0.041, 0.60],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        geometry.world_oriented_rect_with_surface_response(
            [face_x + 0.004, y, z],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0],
            [0.235, 0.010],
            [0.045, 0.045, 0.041, 0.58],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
    }

    for door in 0..facade.door_count.clamp(1, 2) {
        let y = bounds.min[1] + y_span * (0.24 + door as f32 * 0.34);
        geometry.world_oriented_rect_with_surface_response(
            [
                face_x + facade.inset_depth_meters * 0.035,
                y,
                bounds.min[2] + 0.86,
            ],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.31, 0.78],
            [0.075, 0.066, 0.056, 0.92],
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
    }

    for vent in 0..facade.vent_count.clamp(1, 6) {
        let y = bounds.min[1] + y_span * ((vent as f32 + 0.7) / 6.7);
        let z = bounds.min[2] + 0.55 + v16_unit(seed, 201 + vent as u64) * z_span * 0.50;
        geometry.world_oriented_rect_with_surface_response(
            [face_x + 0.006, y, z],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.18, 0.055],
            [0.070, 0.074, 0.071, 0.74],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
    }

    for stain in 0..facade.poster_or_stain_count.clamp(2, 8) {
        let y = bounds.min[1] + y_span * v16_unit(seed, 301 + stain as u64);
        let z = bounds.min[2] + z_span * (0.20 + v16_unit(seed, 401 + stain as u64) * 0.68);
        let paper = stain % 3 == 0;
        geometry.world_oriented_rect_with_surface_response(
            [face_x + 0.008, y, z],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [
                0.10 + v16_unit(seed, 501 + stain as u64) * 0.16,
                0.045 + v16_unit(seed, 601 + stain as u64) * 0.12,
            ],
            if paper {
                [0.42, 0.39, 0.30, 0.28]
            } else {
                [0.034, 0.028, 0.021, 0.30 + facade.dirt_0_to_1 * 0.16]
            },
            WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
        );
    }

    for pipe in 0..facade.pipe_mount_count.clamp(1, 4) {
        let y = bounds.min[1]
            + y_span * (0.18 + pipe as f32 * 0.23 + v16_signed(seed, 701 + pipe as u64) * 0.035);
        geometry.world_cylinder_between_with_surface_response(
            [face_x + 0.020, y, bounds.min[2] + 0.12],
            [
                face_x + 0.020,
                y + v16_signed(seed, 801 + pipe as u64) * 0.10,
                bounds.max[2] - 0.16,
            ],
            0.022 + v16_unit(seed, 901 + pipe as u64) * 0.018,
            9,
            [0.050, 0.052, 0.049, 0.92],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
    }

    geometry.world_cylinder_between_with_surface_response(
        [face_x, bounds.min[1], bounds.max[2] + 0.020],
        [face_x, bounds.max[1], bounds.max[2] + 0.035],
        facade.bevel_radius_meters.clamp(0.026, 0.070),
        12,
        [0.18, 0.17, 0.15, 1.0],
        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
    );
}

fn add_beauty_v17_curve_object(geometry: &mut WindowSceneGeometry, curve: &CurveObjectV17) {
    let segments = if curve.radius_meters < 0.03 { 8 } else { 12 };
    let dirt = curve.surface_dirt_0_to_1.clamp(0.0, 1.0);
    let color = [
        0.040 + dirt * 0.040,
        0.042 + dirt * 0.034,
        0.040 + dirt * 0.026,
        0.88 + (1.0 - dirt) * 0.10,
    ];
    for segment in curve.points_world.windows(2) {
        if curve.sag_meters > 0.01 {
            let mid = [
                (segment[0][0] + segment[1][0]) * 0.5,
                (segment[0][1] + segment[1][1]) * 0.5,
                (segment[0][2] + segment[1][2]) * 0.5 - curve.sag_meters,
            ];
            geometry.world_cylinder_between_with_surface_response(
                segment[0],
                mid,
                curve.radius_meters,
                segments,
                color,
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            geometry.world_cylinder_between_with_surface_response(
                mid,
                segment[1],
                curve.radius_meters,
                segments,
                color,
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        } else {
            geometry.world_cylinder_between_with_surface_response(
                segment[0],
                segment[1],
                curve.radius_meters,
                segments,
                color,
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        }
    }
}

fn add_beauty_v17_puddle(geometry: &mut WindowSceneGeometry, puddle: &GroundedPuddleV17) {
    if !puddle.is_ground_anchored() {
        return;
    }

    let [x, y, z] = puddle.center_world;
    let rx = puddle.radius_meters;
    let ry = puddle.radius_meters * 0.54;
    geometry.world_quad_with_surface_response(
        [x - rx * 0.92, y - ry * 0.16, z + 0.002],
        [x - rx * 0.28, y + ry * 0.60, z + 0.002],
        [x + rx * 0.86, y + ry * 0.36, z + 0.002],
        [x + rx * 0.56, y - ry * 0.52, z + 0.002],
        [0.052, 0.135, 0.160, 0.38],
        WINDOW_SURFACE_RESPONSE_WET_ROAD,
    );
    geometry.world_quad_with_surface_response(
        [x - rx * 0.58, y - ry * 0.26, z + 0.003],
        [x - rx * 0.18, y + ry * 0.30, z + 0.003],
        [x + rx * 0.52, y + ry * 0.18, z + 0.003],
        [x + rx * 0.32, y - ry * 0.42, z + 0.003],
        [0.030, 0.095, 0.120, 0.30],
        WINDOW_SURFACE_RESPONSE_WET_ROAD,
    );
    geometry.world_oriented_rect_with_surface_response(
        [
            puddle.center_world[0] + puddle.radius_meters * 0.12,
            puddle.center_world[1] - puddle.radius_meters * 0.10,
            puddle.center_world[2] + 0.006,
        ],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [puddle.radius_meters * 0.24, 0.012],
        [0.78, 0.86, 0.82, 0.16],
        WINDOW_SURFACE_RESPONSE_WET_ROAD,
    );
}

fn add_beauty_v17_scatter(geometry: &mut WindowSceneGeometry, scatter: &ScatterFieldV17) {
    let count = ((scatter.item_count_budget as f32 * scatter.density_0_to_1)
        .ceil()
        .clamp(8.0, 22.0)) as usize;
    let span = v17_bounds_span(&scatter.bounds);

    for index in 0..count {
        let seed = scatter.seed ^ index as u64;
        let x = scatter.bounds.min[0] + span[0] * v16_unit(seed, 1);
        let y = scatter.bounds.min[1] + span[1] * v16_unit(seed, 2);
        let z = scatter.bounds.min[2] + 0.026 + v16_unit(seed, 3) * 0.018;
        let kind = index % 5;

        if scatter.has_paper && kind == 0 {
            geometry.world_oriented_rect_with_surface_response(
                [x, y, z],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [
                    0.055 + v16_unit(seed, 4) * 0.080,
                    0.030 + v16_unit(seed, 5) * 0.045,
                ],
                [0.34, 0.31, 0.22, 0.36],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        } else if scatter.has_cable_clutter && kind == 1 {
            let end = [
                x + v16_signed(seed, 4) * 0.34,
                y + v16_signed(seed, 5) * 0.34,
                z + 0.004,
            ];
            geometry.world_cylinder_between_with_surface_response(
                [x, y, z],
                end,
                0.010 + v16_unit(seed, 6) * 0.006,
                6,
                [0.028, 0.029, 0.027, 0.76],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        } else if scatter.has_broken_glass && kind == 2 {
            geometry.world_oriented_rect_with_surface_response(
                [x, y, z + 0.004],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [
                    0.022 + v16_unit(seed, 7) * 0.035,
                    0.012 + v16_unit(seed, 8) * 0.022,
                ],
                [0.62, 0.78, 0.80, 0.22],
                WINDOW_SURFACE_RESPONSE_GLASS,
            );
        } else {
            let radius = 0.022 + v16_unit(seed, 9) * 0.055;
            geometry.world_box_with_surface_response(
                [x - radius * 1.2, y - radius * 0.7, z - radius * 0.16],
                [x + radius * 1.1, y + radius * 0.65, z + radius * 0.32],
                [0.075, 0.061, 0.045, 0.72],
                WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
            );
        }
    }
}

fn add_beauty_v17_human(geometry: &mut WindowSceneGeometry, human: &HumanProxyV17) {
    if !human.is_proportionate_non_rod() {
        return;
    }

    let base = human.position_world;
    let (forward, right) = v17_yaw_vectors(human.facing_yaw_radians);
    let height = human.height_meters;
    geometry.world_oriented_rect_with_surface_response(
        [base[0], base[1], base[2] + 0.004],
        [forward[0], forward[1], 0.0],
        [right[0], right[1], 0.0],
        [
            human.shoulder_width_meters.max(human.hip_width_meters) * 0.92,
            0.19,
        ],
        [0.018, 0.015, 0.012, 0.24],
        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
    );

    let render_human = HumanState {
        human_id: human.entity_id.saturating_mul(3),
        quality_tier: QualityTier::HeroHighFidelityRuntime,
    };
    let viewer_position = [
        base[0] + forward[0] * 2.0,
        base[1] + forward[1] * 2.0,
        base[2] + height * 0.86,
    ];
    let render_color = [
        0.76 + v16_unit(human.entity_id, 301) * 0.08,
        0.55 + v16_unit(human.entity_id, 302) * 0.08,
        0.42 + v16_unit(human.entity_id, 303) * 0.06,
        1.0,
    ];

    geometry.add_humanoid_proxy(
        WindowHumanoidProxyInstance::new([base[0], base[1]], base[2], render_color)
            .with_human(Some(&render_human))
            .with_surface_state(Some(beauty_v17_human_surface_state(human)))
            .with_viewer_position(viewer_position),
    );
}

fn beauty_v17_human_surface_state(human: &HumanProxyV17) -> HumanSurfaceState {
    let seed = human.entity_id ^ human.irregularity.seed;
    let walk = human.walk_cycle_weight_0_to_1.clamp(0.0, 1.0);
    HumanSurfaceState {
        skin_wetness: 0.42 + v16_unit(seed, 410) * 0.18,
        sweat_sheen: 0.16 + walk * 0.22,
        oil_sheen: 0.18 + v16_unit(seed, 411) * 0.08,
        bruising: v16_unit(seed, 412) * 0.08,
        injury_overlay: 0.0,
        dirt: 0.16 + v16_unit(seed, 413) * 0.18,
        hair_wetness: 0.38 + v16_unit(seed, 414) * 0.22,
        clothing_wetness: 0.46 + v16_unit(seed, 415) * 0.24,
        clothing_damage: 0.08 + v16_unit(seed, 416) * 0.14,
        eye_redness: 0.04 + v16_unit(seed, 417) * 0.08,
        material_layers: Vec::new(),
    }
}

fn add_beauty_v17_vehicle(geometry: &mut WindowSceneGeometry, vehicle: &VehicleProxyV17) {
    if !vehicle.is_proportionate_non_box() {
        return;
    }

    let base = vehicle.position_world;
    let (forward, right) = v17_yaw_vectors(vehicle.facing_yaw_radians);
    let length = vehicle.length_meters;
    let half_width = vehicle.width_meters * 0.5;
    let body_z = vehicle.wheel_radius_meters + vehicle.height_meters * 0.22;
    let dirt_shadow = (0.18 + vehicle.dirt_0_to_1 * 0.24).clamp(0.18, 0.50);

    geometry.world_oriented_rect_with_surface_response(
        [base[0], base[1], base[2] + 0.030],
        [forward[0], forward[1], 0.0],
        [right[0], right[1], 0.0],
        [length * 0.64, half_width * 0.74],
        [0.024, 0.021, 0.018, dirt_shadow],
        WINDOW_SURFACE_RESPONSE_ROUGH_DIRT,
    );
    geometry.world_ellipsoid_with_surface_response(
        v17_offset(base, forward, right, 0.0, 0.0, body_z),
        [
            length * 0.48,
            half_width * 0.84,
            vehicle.height_meters * 0.22,
        ],
        6,
        16,
        [0.17, 0.21, 0.23, 1.0],
        WINDOW_SURFACE_RESPONSE_METAL,
    );
    geometry.world_ellipsoid_with_surface_response(
        v17_offset(
            base,
            forward,
            right,
            -length * 0.06,
            0.0,
            body_z + vehicle.height_meters * 0.23,
        ),
        [
            length * 0.24,
            half_width * 0.54,
            vehicle.height_meters * 0.16,
        ],
        5,
        12,
        [0.31, 0.45, 0.50, 0.60],
        WINDOW_SURFACE_RESPONSE_GLASS,
    );

    for seam in [-0.24_f32, -0.02, 0.22] {
        geometry.world_oriented_rect_with_surface_response(
            v17_offset(
                base,
                forward,
                right,
                length * seam,
                0.0,
                body_z + vehicle.height_meters * 0.12,
            ),
            [right[0], right[1], 0.0],
            [0.0, 0.0, 1.0],
            [half_width * 0.74, 0.011],
            [0.022, 0.023, 0.022, 0.48],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
    }

    for axle in [-0.34_f32, 0.34] {
        let axle_center = v17_offset(
            base,
            forward,
            right,
            axle * length,
            0.0,
            vehicle.wheel_radius_meters,
        );
        geometry.world_cylinder_between_with_surface_response(
            v17_offset(axle_center, forward, right, 0.0, -half_width * 0.72, 0.0),
            v17_offset(axle_center, forward, right, 0.0, half_width * 0.72, 0.0),
            0.026,
            8,
            [0.040, 0.040, 0.038, 0.80],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
        for side in [-1.0_f32, 1.0] {
            let wheel_center = v17_offset(
                base,
                forward,
                right,
                axle * length,
                side * half_width * 0.86,
                vehicle.wheel_radius_meters,
            );
            geometry.world_ellipsoid_with_surface_response(
                wheel_center,
                [
                    vehicle.wheel_radius_meters * 0.95,
                    vehicle.wheel_radius_meters * 0.28,
                    vehicle.wheel_radius_meters,
                ],
                5,
                12,
                [0.022, 0.022, 0.020, 0.92],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
            geometry.world_ellipse_ring_with_surface_response(
                wheel_center,
                [
                    vehicle.wheel_radius_meters * 0.34,
                    vehicle.wheel_radius_meters * 0.08,
                ],
                [
                    vehicle.wheel_radius_meters * 0.74,
                    vehicle.wheel_radius_meters * 0.16,
                ],
                14,
                [0.13, 0.13, 0.12, 0.52],
                WINDOW_SURFACE_RESPONSE_METAL,
            );
        }
    }

    for (offset, color) in [
        (length * 0.52, [0.95, 0.82, 0.52, 0.30]),
        (-length * 0.52, [0.82, 0.085, 0.050, 0.30]),
    ] {
        geometry.world_oriented_rect_with_surface_response(
            v17_offset(
                base,
                forward,
                right,
                offset,
                0.0,
                body_z + vehicle.height_meters * 0.03,
            ),
            [right[0], right[1], 0.0],
            [0.0, 0.0, 1.0],
            [half_width * 0.20, 0.052],
            color,
            WINDOW_SURFACE_RESPONSE_HOT_EMISSIVE,
        );
    }

    for side in [-1.0_f32, 1.0] {
        geometry.world_ellipsoid_with_surface_response(
            v17_offset(
                base,
                forward,
                right,
                length * 0.12,
                side * half_width * 1.02,
                body_z + vehicle.height_meters * 0.17,
            ),
            [0.090, 0.025, 0.040],
            3,
            8,
            [0.030, 0.033, 0.034, 0.72],
            WINDOW_SURFACE_RESPONSE_METAL,
        );
    }
}

fn v17_bounds_span(bounds: &BeautyBoundsV17) -> [f32; 3] {
    [
        (bounds.max[0] - bounds.min[0]).abs().max(0.001),
        (bounds.max[1] - bounds.min[1]).abs().max(0.001),
        (bounds.max[2] - bounds.min[2]).abs().max(0.001),
    ]
}

fn v17_lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn v17_yaw_vectors(yaw: f32) -> ([f32; 2], [f32; 2]) {
    let forward = [yaw.cos(), yaw.sin()];
    let right = [-forward[1], forward[0]];
    (forward, right)
}

fn v17_offset(
    base: [f32; 3],
    forward: [f32; 2],
    right: [f32; 2],
    forward_meters: f32,
    right_meters: f32,
    z_meters: f32,
) -> [f32; 3] {
    [
        base[0] + forward[0] * forward_meters + right[0] * right_meters,
        base[1] + forward[1] * forward_meters + right[1] * right_meters,
        base[2] + z_meters,
    ]
}

#[cfg(test)]
fn v16_bounds_span(bounds: &BeautyBoundsV16) -> [f32; 3] {
    [
        (bounds.max[0] - bounds.min[0]).abs().max(0.001),
        (bounds.max[1] - bounds.min[1]).abs().max(0.001),
        (bounds.max[2] - bounds.min[2]).abs().max(0.001),
    ]
}

fn v16_segment_right(start: [f32; 3], end: [f32; 3]) -> Option<[f32; 2]> {
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len <= f32::EPSILON {
        None
    } else {
        Some([-dy / len, dx / len])
    }
}

fn v16_segment_forward(start: [f32; 3], end: [f32; 3]) -> [f32; 2] {
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len <= f32::EPSILON {
        [0.0, 1.0]
    } else {
        [dx / len, dy / len]
    }
}

fn v16_unit(seed: u64, salt: u64) -> f32 {
    let mut x = seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    ((x >> 40) as f32) / ((1u64 << 24) as f32)
}

fn v16_signed(seed: u64, salt: u64) -> f32 {
    v16_unit(seed, salt) * 2.0 - 1.0
}

impl WindowScene for AlleyWindowScene {
    fn update(&mut self, input: &WindowInputState, dt_seconds: f32) -> WindowFrameState {
        self.frame_index = self.frame_index.saturating_add(1);
        self.camera
            .set_viewport_size_pixels(input.viewport_size_pixels);
        self.update_view_controls(input, dt_seconds);
        self.decay_event_markers(dt_seconds);
        self.decay_gas_volumes(dt_seconds);
        self.decay_hud_event_pulses(dt_seconds);
        self.decay_infrastructure_state(dt_seconds);
        self.decay_interaction_notice(dt_seconds);

        let fixed_step = 1.0 / 30.0;
        if player_movement_requested(input) {
            self.sim_accumulator_seconds = self.sim_accumulator_seconds.max(fixed_step);
        }
        if input.action_primary && self.queue_primary_action() {
            self.sim_accumulator_seconds = self.sim_accumulator_seconds.max(fixed_step);
        }

        self.sim_accumulator_seconds += dt_seconds;
        let mut steps = 0;
        while self.sim_accumulator_seconds >= fixed_step && steps < 2 {
            self.queue_player_motion(input, fixed_step);
            self.step_runtime();
            self.sim_accumulator_seconds = (self.sim_accumulator_seconds - fixed_step).max(0.0);
            steps += 1;
        }
        self.sync_render_snapshot(dt_seconds);
        let beauty_v15_report = self.beauty_scene_v15().validate_for_beauty();
        if self.render_mode == WindowRenderMode::Beauty && !beauty_v15_report.pass {
            eprintln!("Beauty V15 validation failed: {beauty_v15_report:?}");
        }
        let beauty_v16_report = self.beauty_scene_v16().validate_for_visual_realism();
        if self.render_mode == WindowRenderMode::Beauty && !beauty_v16_report.pass {
            eprintln!("Beauty V16 validation failed: {beauty_v16_report:?}");
        }
        let beauty_v17_report = self.beauty_scene_v17().validate_for_visual_realism();
        if self.render_mode == WindowRenderMode::Beauty && !beauty_v17_report.pass {
            eprintln!("Beauty V17 validation failed: {beauty_v17_report:?}");
        }
        let beauty_v18_report = self.beauty_validation_v18().validate();
        if self.render_mode == WindowRenderMode::Beauty && !beauty_v18_report.pass {
            eprintln!("Beauty V18 validation failed: {beauty_v18_report:?}");
        }
        let beauty_v19_report = validate_scene_v19(&self.beauty_scene_v19());
        if self.render_mode == WindowRenderMode::Beauty && !beauty_v19_report.passed {
            eprintln!("Beauty V19 validation failed: {beauty_v19_report:?}");
        }
        let geometry = self.scene_geometry();
        let snapshot_light = self
            .last_snapshot
            .as_ref()
            .map(|snapshot| window_dynamic_light_from_snapshot(snapshot, self.frame_index))
            .unwrap_or_else(ashfall_rendering::windowed::WindowDynamicLight::disabled);
        let city_light = self.city_dynamic_light();
        let dynamic_light =
            stronger_window_dynamic_light(snapshot_light, city_light, self.camera.position);
        let atmosphere =
            window_atmosphere_for_alley(self.frame_index, self.last_event_count, dynamic_light);

        WindowFrameState {
            clear_color: self.clear_color(),
            vertices: geometry.vertices,
            indices: geometry.indices,
            world_to_clip: self.camera.world_to_clip_matrix(),
            dynamic_light,
            atmosphere,
            title: Some(self.title()),
        }
    }
}

fn player_movement_requested(input: &WindowInputState) -> bool {
    input.move_forward || input.move_backward || input.move_left || input.move_right
}

fn stronger_window_dynamic_light(
    left: WindowDynamicLight,
    right: WindowDynamicLight,
    camera_position: [f32; 3],
) -> WindowDynamicLight {
    if dynamic_light_camera_score(right, camera_position)
        > dynamic_light_camera_score(left, camera_position)
    {
        right
    } else {
        left
    }
}

fn dynamic_light_camera_score(light: WindowDynamicLight, camera_position: [f32; 3]) -> f32 {
    if !light.is_enabled() {
        return 0.0;
    }

    let dx = light.position_meters[0] - camera_position[0];
    let dy = light.position_meters[1] - camera_position[1];
    let dz = light.position_meters[2] - camera_position[2];
    let distance = (dx * dx + dy * dy + dz * dz).sqrt();
    let attenuation = 1.0 / (1.0 + distance / light.radius_meters.max(1.0));
    light.intensity * light.radius_meters * attenuation
}

fn city_entity_primary_material(entity: &EntityTemplate) -> Option<MaterialId> {
    entity
        .renderable
        .as_ref()
        .filter(|renderable| renderable.visible)
        .map(|renderable| renderable.material)
        .or_else(|| entity.physical_body.as_ref().map(|body| body.material_id))
}

fn window_city_material_placement_kind(
    material_id: MaterialId,
    entity: &EntityTemplate,
) -> WindowCityMaterialPlacementKind {
    if entity_has_tag(entity.tags.as_slice(), "neon")
        || entity_has_tag(entity.tags.as_slice(), "light")
        || entity_has_tag(entity.tags.as_slice(), "material:neon")
        || material_id == MATERIAL_NEON_TUBE
    {
        WindowCityMaterialPlacementKind::Neon
    } else if entity_has_tag(entity.tags.as_slice(), "water_leak")
        || entity_has_tag(entity.tags.as_slice(), "steam_leak")
        || entity_has_tag(entity.tags.as_slice(), "material:water")
        || material_id == MATERIAL_WATER
    {
        WindowCityMaterialPlacementKind::Water
    } else if entity_has_tag(entity.tags.as_slice(), "glass")
        || entity_has_tag(entity.tags.as_slice(), "material:glass")
        || material_id == MATERIAL_GLASS
    {
        WindowCityMaterialPlacementKind::Glass
    } else if entity.human.is_some()
        || entity.agent.is_some()
        || entity_has_tag(entity.tags.as_slice(), "material:human_skin")
        || material_id == MATERIAL_HUMAN_SKIN
    {
        WindowCityMaterialPlacementKind::HumanSkin
    } else if entity_has_tag(entity.tags.as_slice(), "ground")
        || entity_has_tag(entity.tags.as_slice(), "asphalt")
        || entity_has_tag(entity.tags.as_slice(), "wettable")
        || entity_has_tag(entity.tags.as_slice(), "material:wet_asphalt")
        || material_id == MATERIAL_WET_ASPHALT
    {
        WindowCityMaterialPlacementKind::WetRoad
    } else if entity
        .physical_body
        .as_ref()
        .is_some_and(|body| !body.dynamic && body.mass_kg > 100.0)
    {
        WindowCityMaterialPlacementKind::Metal
    } else {
        WindowCityMaterialPlacementKind::Generic
    }
}

fn city_material_placement_center(
    entity: &EntityTemplate,
    kind: WindowCityMaterialPlacementKind,
) -> [f32; 3] {
    let position = entity.transform.translation_meters;
    let z = match kind {
        WindowCityMaterialPlacementKind::WetRoad | WindowCityMaterialPlacementKind::Water => {
            position.z.max(0.0)
        }
        WindowCityMaterialPlacementKind::Glass => position.z.max(0.0),
        WindowCityMaterialPlacementKind::Neon => position.z.max(0.6),
        WindowCityMaterialPlacementKind::HumanSkin => position.z.max(0.0),
        WindowCityMaterialPlacementKind::Metal | WindowCityMaterialPlacementKind::Generic => {
            position.z.max(0.04)
        }
    };

    [position.x, position.y, z]
}

fn city_material_placement_half_extents(
    chunk: &WorldChunk,
    entity: &EntityTemplate,
    kind: WindowCityMaterialPlacementKind,
) -> [f32; 2] {
    let scale = entity.transform.scale;
    match kind {
        WindowCityMaterialPlacementKind::WetRoad => {
            let chunk_width = (chunk.bounds.max.x - chunk.bounds.min.x).abs();
            [
                (chunk_width * 0.18).clamp(1.8, 8.0),
                (scale.y.abs() * 1.2).clamp(0.7, 1.8),
            ]
        }
        WindowCityMaterialPlacementKind::Glass => [
            (scale.x.abs() * 1.2).clamp(0.8, 2.0),
            (scale.z.abs() * 0.95).clamp(0.75, 1.8),
        ],
        WindowCityMaterialPlacementKind::Neon => [
            (scale.x.abs() * 0.9).clamp(0.65, 1.8),
            (scale.z.abs() * 0.18).clamp(0.12, 0.45),
        ],
        WindowCityMaterialPlacementKind::Water => [
            (scale.x.abs() * 1.05).clamp(0.8, 2.6),
            (scale.y.abs() * 0.65).clamp(0.42, 1.4),
        ],
        WindowCityMaterialPlacementKind::HumanSkin => [
            (scale.x.abs() * 0.42).clamp(0.32, 0.62),
            (scale.y.abs() * 0.32).clamp(0.24, 0.48),
        ],
        WindowCityMaterialPlacementKind::Metal | WindowCityMaterialPlacementKind::Generic => [
            (scale.x.abs() * 0.55).clamp(0.3, 1.4),
            (scale.y.abs() * 0.45).clamp(0.25, 1.1),
        ],
    }
}

fn window_city_material_surface_response_from_descriptor(
    material: &MaterialDescriptor,
) -> [f32; 4] {
    let emission = material
        .visual
        .emission_linear
        .iter()
        .copied()
        .fold(0.0_f32, f32::max);
    [
        material.visual.roughness.clamp(0.0, 1.0),
        material.visual.metallic.clamp(0.0, 1.0),
        material.visual.transparency.clamp(0.0, 1.0),
        (emission / 8.0).clamp(0.0, 1.0),
    ]
}

fn window_city_material_fallback_color(kind: WindowCityMaterialPlacementKind) -> [f32; 4] {
    match kind {
        WindowCityMaterialPlacementKind::WetRoad => [0.018, 0.023, 0.029, 0.92],
        WindowCityMaterialPlacementKind::Glass => [0.62, 0.88, 1.0, 0.48],
        WindowCityMaterialPlacementKind::Neon => [0.94, 0.12, 0.68, 0.9],
        WindowCityMaterialPlacementKind::Water => [0.08, 0.22, 0.28, 0.44],
        WindowCityMaterialPlacementKind::HumanSkin => [0.72, 0.48, 0.36, 0.82],
        WindowCityMaterialPlacementKind::Metal => [0.24, 0.27, 0.3, 0.88],
        WindowCityMaterialPlacementKind::Generic => [0.28, 0.3, 0.32, 0.78],
    }
}

fn window_city_material_fallback_surface_response(
    kind: WindowCityMaterialPlacementKind,
) -> [f32; 4] {
    match kind {
        WindowCityMaterialPlacementKind::WetRoad | WindowCityMaterialPlacementKind::Water => {
            [0.18, 0.0, 0.92, 0.0]
        }
        WindowCityMaterialPlacementKind::Glass => [0.08, 0.0, 0.36, 0.02],
        WindowCityMaterialPlacementKind::Neon => [0.12, 0.0, 0.18, 1.0],
        WindowCityMaterialPlacementKind::HumanSkin | WindowCityMaterialPlacementKind::Generic => {
            [0.78, 0.0, 0.0, 0.0]
        }
        WindowCityMaterialPlacementKind::Metal => [0.34, 0.75, 0.02, 0.0],
    }
}

fn city_material_surface_state(
    district_kind: DistrictKind,
    district_pollution: f32,
    district_crime_pressure: f32,
    state: Option<&MaterialState>,
    kind: WindowCityMaterialPlacementKind,
    entity: &EntityTemplate,
) -> (f32, f32, f32, f32) {
    let state_wetness = state.map(|state| state.moisture).unwrap_or_default();
    let state_damage = state
        .map(|state| {
            state
                .crack_density
                .max((state.plastic_strain / 0.2).clamp(0.0, 1.0))
                .max(state.corrosion * 0.55)
        })
        .unwrap_or_default();
    let state_pollution = state
        .map(|state| (state.soot * 0.68 + state.corrosion * 0.32).clamp(0.0, 1.0))
        .unwrap_or_default();
    let rain_baseline = if matches!(district_kind, DistrictKind::RainAlleySlum) {
        0.36
    } else {
        0.12
    };
    let wetness = match kind {
        WindowCityMaterialPlacementKind::WetRoad => {
            state_wetness.max(0.46 + district_pollution * 0.2 + rain_baseline * 0.25)
        }
        WindowCityMaterialPlacementKind::Water => 1.0,
        WindowCityMaterialPlacementKind::Glass => state_wetness.max(rain_baseline * 0.35),
        WindowCityMaterialPlacementKind::HumanSkin => state_wetness.max(rain_baseline * 0.25),
        WindowCityMaterialPlacementKind::Neon
        | WindowCityMaterialPlacementKind::Metal
        | WindowCityMaterialPlacementKind::Generic => state_wetness.max(rain_baseline * 0.2),
    }
    .clamp(0.0, 1.0);
    let fragile_damage = entity
        .physical_body
        .as_ref()
        .filter(|body| body.fragile)
        .map(|_| district_crime_pressure * 0.18)
        .unwrap_or_default();
    let damage = state_damage.max(fragile_damage).clamp(0.0, 1.0);
    let pollution = (district_pollution * 0.72 + state_pollution * 0.48).clamp(0.0, 1.0);
    let traffic_wear = if matches!(
        kind,
        WindowCityMaterialPlacementKind::WetRoad | WindowCityMaterialPlacementKind::Metal
    ) {
        (0.32 + district_crime_pressure * 0.28 + pollution * 0.24).clamp(0.0, 1.0)
    } else {
        (district_crime_pressure * 0.18 + pollution * 0.12).clamp(0.0, 1.0)
    };

    (wetness, damage, pollution, traffic_wear)
}

fn city_material_importance(kind: WindowCityMaterialPlacementKind, entity: &EntityTemplate) -> f32 {
    let tag_importance = if entity_has_tag(entity.tags.as_slice(), "hazard")
        || entity_has_tag(entity.tags.as_slice(), "destructible")
    {
        0.24_f32
    } else {
        0.0_f32
    };
    let kind_importance = match kind {
        WindowCityMaterialPlacementKind::Neon => 0.84_f32,
        WindowCityMaterialPlacementKind::Water => 0.72_f32,
        WindowCityMaterialPlacementKind::Glass => 0.58_f32,
        WindowCityMaterialPlacementKind::HumanSkin => 0.48_f32,
        WindowCityMaterialPlacementKind::WetRoad => 0.38_f32,
        WindowCityMaterialPlacementKind::Metal | WindowCityMaterialPlacementKind::Generic => {
            0.28_f32
        }
    };

    (kind_importance + tag_importance).clamp(0.0, 1.0)
}

fn window_city_streaming_state(state: CityCellStreamingState) -> WindowCityStreamingCellState {
    match state {
        CityCellStreamingState::Unloaded => WindowCityStreamingCellState::Unloaded,
        CityCellStreamingState::SummaryLoaded => WindowCityStreamingCellState::SummaryLoaded,
        CityCellStreamingState::GameplayLoaded => WindowCityStreamingCellState::GameplayLoaded,
        CityCellStreamingState::RenderHighDetail => WindowCityStreamingCellState::RenderHighDetail,
        CityCellStreamingState::HeroLoaded => WindowCityStreamingCellState::HeroLoaded,
    }
}

fn window_city_district_visual_kind(kind: DistrictKind) -> WindowCityDistrictVisualKind {
    match kind {
        DistrictKind::CorporateCore => WindowCityDistrictVisualKind::CorporateCore,
        DistrictKind::RainAlleySlum => WindowCityDistrictVisualKind::RainAlleySlum,
        DistrictKind::IndustrialDock => WindowCityDistrictVisualKind::IndustrialDock,
        DistrictKind::BlackMarket => WindowCityDistrictVisualKind::BlackMarket,
        DistrictKind::ClinicDistrict => WindowCityDistrictVisualKind::ClinicDistrict,
    }
}

fn window_city_traversal_visual_kind(kind: TraversalKind) -> WindowCityTraversalVisualKind {
    match kind {
        TraversalKind::Walk => WindowCityTraversalVisualKind::Walk,
        TraversalKind::CoverDash => WindowCityTraversalVisualKind::CoverDash,
        TraversalKind::StealthPath => WindowCityTraversalVisualKind::StealthPath,
        TraversalKind::ServiceLadder => WindowCityTraversalVisualKind::ServiceLadder,
        TraversalKind::Transit => WindowCityTraversalVisualKind::Transit,
    }
}

fn window_city_danger_visual_kind(kind: NavigationDangerKind) -> WindowCityDangerVisualKind {
    match kind {
        NavigationDangerKind::PhysicalDamage => WindowCityDangerVisualKind::PhysicalDamage,
        NavigationDangerKind::Flooding => WindowCityDangerVisualKind::Flooding,
        NavigationDangerKind::SlipperyContamination => {
            WindowCityDangerVisualKind::SlipperyContamination
        }
        NavigationDangerKind::BiohazardContamination => {
            WindowCityDangerVisualKind::BiohazardContamination
        }
        NavigationDangerKind::ToxicGas => WindowCityDangerVisualKind::ToxicGas,
        NavigationDangerKind::Surveillance | NavigationDangerKind::SecurityAlert => {
            WindowCityDangerVisualKind::Surveillance
        }
        NavigationDangerKind::Infrastructure => WindowCityDangerVisualKind::PhysicalDamage,
    }
}

fn window_city_route_consequence_status(
    status: NavigationRouteStatus,
) -> WindowCityRouteConsequenceStatus {
    match status {
        NavigationRouteStatus::Blocked => WindowCityRouteConsequenceStatus::Blocked,
        NavigationRouteStatus::Dangerous => WindowCityRouteConsequenceStatus::Dangerous,
        NavigationRouteStatus::Restricted => WindowCityRouteConsequenceStatus::Restricted,
    }
}

fn window_city_infrastructure_system_mask(systems: &[InfrastructureSystem]) -> u32 {
    systems.iter().fold(0, |mask, system| {
        mask | match system {
            InfrastructureSystem::Power => WINDOW_CITY_INFRASTRUCTURE_POWER,
            InfrastructureSystem::Water => WINDOW_CITY_INFRASTRUCTURE_WATER,
            InfrastructureSystem::Data => WINDOW_CITY_INFRASTRUCTURE_DATA,
            InfrastructureSystem::Surveillance => WINDOW_CITY_INFRASTRUCTURE_SURVEILLANCE,
            InfrastructureSystem::Transit => WINDOW_CITY_INFRASTRUCTURE_TRANSIT,
            InfrastructureSystem::Drainage => WINDOW_CITY_INFRASTRUCTURE_DRAINAGE,
        }
    })
}

fn city_renderer_visible_cluster_estimate(chunk: &WorldChunk, screen_coverage: f32) -> usize {
    let entity_clusters = chunk
        .entities
        .iter()
        .map(|entity| {
            let mut clusters = 1_usize;
            if entity.renderable.is_some() {
                clusters += 3;
            }
            if entity.physical_body.is_some() {
                clusters += 1;
            }
            if entity.human.is_some() {
                clusters += 8;
            }
            if entity.agent.is_some() {
                clusters += 2;
            }
            if entity_has_tag(entity.tags.as_slice(), "destructible") {
                clusters += 5;
            }
            if entity_has_tag(entity.tags.as_slice(), "neon")
                || entity_has_tag(entity.tags.as_slice(), "glass")
            {
                clusters += 3;
            }
            clusters
        })
        .sum::<usize>();
    let city_clusters = chunk.local_navigation.nodes.len()
        + chunk.local_navigation.edges.len()
        + chunk.local_infrastructure.len()
        + chunk.streaming_dependencies.len() * 4
        + chunk.local_story_hooks.len() * 2;
    let coverage_scale = 0.35 + screen_coverage.clamp(0.0, 1.0) * 1.15;
    ((entity_clusters + city_clusters).max(1) as f32 * coverage_scale).ceil() as usize
}

fn city_renderer_micro_geometry_pressure(chunk: &WorldChunk) -> f32 {
    let mut pressure = (chunk.streaming_dependencies.len() as f32 * 0.08)
        + (chunk.local_story_hooks.len() as f32 * 0.05)
        + (chunk.local_infrastructure.len() as f32 * 0.025);
    for entity in &chunk.entities {
        if entity.human.is_some() {
            pressure += 0.16;
        }
        if entity_has_tag(entity.tags.as_slice(), "destructible") {
            pressure += 0.14;
        }
        if entity_has_tag(entity.tags.as_slice(), "glass") {
            pressure += 0.08;
        }
        if entity_has_tag(entity.tags.as_slice(), "neon") {
            pressure += 0.06;
        }
        if entity_has_tag(entity.tags.as_slice(), "water_leak") {
            pressure += 0.08;
        }
    }
    pressure.clamp(0.0, 1.0)
}

fn city_chunk_center_and_half_extents(chunk: &WorldChunk) -> ([f32; 2], [f32; 2]) {
    (
        [
            (chunk.bounds.min.x + chunk.bounds.max.x) * 0.5,
            (chunk.bounds.min.y + chunk.bounds.max.y) * 0.5,
        ],
        [
            (chunk.bounds.max.x - chunk.bounds.min.x) * 0.5,
            (chunk.bounds.max.y - chunk.bounds.min.y) * 0.5,
        ],
    )
}

fn city_location_position(chunk: &WorldChunk, location: u64) -> Option<[f32; 3]> {
    chunk
        .local_navigation
        .nodes
        .iter()
        .find(|node| node.location == location)
        .map(|node| [node.position.x, node.position.y, node.position.z.max(0.0)])
}

fn city_average_location_position(chunk: &WorldChunk, locations: &[u64]) -> Option<[f32; 3]> {
    let mut count = 0.0_f32;
    let mut sum = [0.0_f32; 3];
    for location in locations {
        let Some(position) = city_location_position(chunk, *location) else {
            continue;
        };
        sum[0] += position[0];
        sum[1] += position[1];
        sum[2] += position[2];
        count += 1.0;
    }

    (count > 0.0).then_some([sum[0] / count, sum[1] / count, sum[2] / count])
}

fn entity_has_tag(tags: &[String], expected: &str) -> bool {
    tags.iter().any(|tag| tag == expected)
}

fn player_motion_delta(
    input: &WindowInputState,
    yaw_radians: f32,
    dt_seconds: f32,
) -> Option<[f32; 2]> {
    if !player_movement_requested(input) {
        return None;
    }

    let forward = [yaw_radians.sin(), yaw_radians.cos()];
    let right = [forward[1], -forward[0]];
    let mut direction = [0.0, 0.0];

    if input.move_forward {
        direction[0] += forward[0];
        direction[1] += forward[1];
    }
    if input.move_backward {
        direction[0] -= forward[0];
        direction[1] -= forward[1];
    }
    if input.move_right {
        direction[0] += right[0];
        direction[1] += right[1];
    }
    if input.move_left {
        direction[0] -= right[0];
        direction[1] -= right[1];
    }

    let length = (direction[0] * direction[0] + direction[1] * direction[1]).sqrt();
    if length <= f32::EPSILON {
        return None;
    }

    let player_seed = alley_player_runtime_seed();
    let speed_meters_per_second = if input.fast_modifier {
        player_seed.fast_walk_speed_meters_per_second
    } else {
        player_seed.walk_speed_meters_per_second
    };
    let step = speed_meters_per_second * dt_seconds.max(0.0);
    Some([direction[0] / length * step, direction[1] / length * step])
}

fn alley_player_motion_settings() -> PlanarMotionSettings {
    let player_seed = alley_player_runtime_seed();
    PlanarMotionSettings {
        bounds_min: player_seed.walkable_min,
        bounds_max: player_seed.walkable_max,
        overhead_z_meters: player_seed.camera_eye_height_meters,
        fractured_crack_density_threshold: player_seed
            .primary_interaction
            .fractured_crack_density_threshold,
        ..PlanarMotionSettings::default()
    }
}

fn alley_primary_interaction_settings() -> PrimaryInteractionSettings {
    alley_player_runtime_seed().primary_interaction
}

fn player_position_from_snapshot(snapshot: &WorldSnapshot) -> Option<([f32; 2], f32)> {
    let transform = snapshot.transforms.find(PLAYER_ID)?;
    let translation = transform.translation_meters;
    Some(([translation.x, translation.y], translation.z))
}

fn interaction_preview_style(
    interaction: &PrimaryInteraction,
    frame_index: u64,
) -> (f32, [f32; 4]) {
    let pulse = (frame_index as f32 * 0.09).sin().mul_add(0.5, 0.5);
    let aim_alpha = if interaction.in_aim_cone { 0.16 } else { 0.0 };
    let lateral_focus = (1.0
        - interaction.lateral_meters / alley_primary_interaction_settings().reach_meters)
        .clamp(0.0, 1.0)
        * 0.04;
    if interaction.already_fractured {
        return (
            0.42 + pulse * 0.06 + lateral_focus,
            [0.72, 0.9, 1.0, 0.42 + aim_alpha],
        );
    }

    if interaction.in_reach {
        (
            0.5 + pulse * 0.08 + lateral_focus,
            [0.3, 1.0, 0.76, 0.48 + aim_alpha],
        )
    } else {
        (
            0.38 + pulse * 0.05 + lateral_focus,
            [1.0, 0.72, 0.24, 0.38 + aim_alpha],
        )
    }
}

fn interaction_reticle_color(interaction: &PrimaryInteraction) -> [f32; 4] {
    let aim_strength = ((interaction.aim_alignment + 1.0) * 0.5).clamp(0.0, 1.0);
    let alpha_boost = if interaction.in_aim_cone {
        0.16
    } else {
        aim_strength * 0.08
    };

    if interaction.already_fractured {
        return [0.72, 0.9, 1.0, 0.48 + alpha_boost];
    }

    if interaction.in_reach {
        [0.3, 1.0, 0.76, 0.56 + alpha_boost]
    } else {
        [1.0, 0.72, 0.24, 0.42 + alpha_boost]
    }
}

fn event_summary(event: &WorldEvent) -> Option<String> {
    match &event.kind {
        WorldEventKind::GlassWallFractured { entity } => {
            Some(format!("glass fractured on entity {entity}"))
        }
        WorldEventKind::NpcWitnessedCrime { witness, .. } => {
            Some(format!("witness {witness} saw the crime"))
        }
        WorldEventKind::NpcHeardSound {
            listener,
            confidence,
            ..
        } => Some(format!("listener {listener} heard sound {confidence:.2}")),
        WorldEventKind::AgentDecisionExplained { agent, goal, .. } => {
            Some(format!("agent {agent}: {}", truncate_summary(goal, 52)))
        }
        WorldEventKind::AgentIntentProposed { agent, action, .. } => {
            Some(format!("agent {agent} intends {action}"))
        }
        WorldEventKind::AgentStateChanged { entity } => {
            Some(format!("agent {entity} state changed"))
        }
        WorldEventKind::NavigationMoveBlocked { entity } => {
            Some(format!("entity {entity} navigation blocked"))
        }
        WorldEventKind::DialogueEmitted { speaker, text, .. } => {
            Some(format!("entity {speaker}: {}", truncate_summary(text, 52)))
        }
        WorldEventKind::SecurityAlertRaised {
            threat, severity, ..
        } => Some(format!("security alert on {threat} severity {severity:.2}")),
        WorldEventKind::FactionReputationChanged { delta, reason, .. } => Some(format!(
            "reputation {delta:+.2}: {}",
            truncate_summary(reason, 44)
        )),
        WorldEventKind::SurveillanceIncreased { amount, .. } => {
            Some(format!("surveillance increased {amount:.2}"))
        }
        WorldEventKind::VoiceLineSpoken { speaker } => Some(format!("voice line entity {speaker}")),
        WorldEventKind::SpeechSynthesized {
            speaker,
            duration_seconds,
            ..
        } => Some(format!("speech entity {speaker} {duration_seconds:.1}s")),
        WorldEventKind::AudioFrameMixed {
            active_sound_count,
            peak_intensity,
            ..
        } if *active_sound_count > 0 => Some(format!(
            "audio mix {active_sound_count} sounds peak {peak_intensity:.2}"
        )),
        WorldEventKind::AudioFrameMixed { .. } => None,
        WorldEventKind::SoundEmitted {
            kind, intensity, ..
        } => Some(format!("sound {kind:?} intensity {intensity:.2}")),
        WorldEventKind::PlayerIdentityExposed => Some("player identity exposed".to_string()),
        WorldEventKind::StreetFlooded => Some("street flooded".to_string()),
        WorldEventKind::ToxicGasReleased => Some("toxic gas released".to_string()),
        WorldEventKind::AssetStreamingRequested { asset_id, .. } => {
            Some(format!("streaming asset {asset_id}"))
        }
        WorldEventKind::AssetBecameResident { asset_id, .. } => {
            Some(format!("asset {asset_id} resident"))
        }
        WorldEventKind::StoryEventEmitted { label } => {
            Some(format!("story: {}", truncate_summary(label, 52)))
        }
        WorldEventKind::Custom(label) => Some(truncate_summary(label, 52)),
        _ => None,
    }
}

fn evidence_f32(evidence: &[String], key: &str) -> Option<f32> {
    evidence_value(evidence, key).and_then(|value| value.parse().ok())
}

fn evidence_value<'a>(evidence: &'a [String], key: &str) -> Option<&'a str> {
    evidence.iter().find_map(|entry| {
        entry
            .split_once(':')
            .and_then(|(entry_key, value)| (entry_key == key).then_some(value))
    })
}

fn truncate_summary(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn distance2(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
fn natural_skin_color(color: [f32; 4]) -> [f32; 4] {
    let warmth = (color[0] * 0.36 + color[1] * 0.24 + color[2] * 0.08).clamp(0.0, 1.0);
    [
        0.38 + warmth * 0.34,
        0.24 + warmth * 0.24,
        0.17 + warmth * 0.16,
        color[3],
    ]
}

#[cfg(test)]
fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{EmotionState, MaterialState, MeshAssetHandle, Transform, Vec3};
    use crate::world::primary_interaction_entity_is_fractured;
    use crate::world::{AgentState, HumanState};
    use ashfall_rendering::windowed::{
        WINDOW_HUD_EVENT_PULSE_SECONDS, WINDOW_MESH_ALLEY_GLASS_FRACTURED,
        WindowHumanoidProxyInstance, WindowProceduralMeshInstance,
    };
    use ashfall_worldgen::{GLASS_WALL_ID, NEON_SIGN_ID, NPC_MARA_ID};

    fn world_event(kind: WorldEventKind) -> WorldEvent {
        WorldEvent {
            event_id: 11,
            tick: 7,
            location_meters: Vec3::new(2.5, -1.25, 0.0),
            actors: vec![1, 2],
            kind,
            physical_evidence: Vec::new(),
            narrative_tags: Vec::new(),
        }
    }

    fn scene_with_render_config(config: WindowedRendererConfig) -> AlleyWindowScene {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        AlleyWindowScene::new_with_audio_and_policy(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
            config.render_mode,
            config.debug_overlays,
        )
    }

    #[test]
    fn beauty_scene_omits_debug_overlays_but_keeps_realism_baseline() {
        let beauty = scene_with_render_config(WindowedRendererConfig::beauty_default(
            "Ashfall - Beauty Mode",
        ));
        let debug = scene_with_render_config(WindowedRendererConfig::debug_default(
            "Ashfall - Debug Mode",
        ));

        let beauty_geometry = beauty.scene_geometry();
        let debug_geometry = debug.scene_geometry();
        let beauty_scene_v15 = beauty.beauty_scene_v15();
        let beauty_v15_report = beauty_scene_v15.validate_for_beauty();
        let beauty_scene_v16 = beauty.beauty_scene_v16();
        let beauty_v16_report = beauty_scene_v16.validate_for_visual_realism();
        let mut beauty_v16_geometry = WindowSceneGeometry::default();
        add_beauty_v16_sky_detail(&mut beauty_v16_geometry, beauty.frame_index);
        beauty.add_beauty_v16_geometry(&mut beauty_v16_geometry);
        let beauty_scene_v17 = beauty.beauty_scene_v17();
        let beauty_v17_report = beauty_scene_v17.validate_for_visual_realism();
        let beauty_v18_report = beauty.beauty_validation_v18().validate();
        let beauty_scene_v19 = beauty.beauty_scene_v19();
        let beauty_v19_report = validate_scene_v19(&beauty_scene_v19);
        let mut beauty_v19_geometry = WindowSceneGeometry::default();
        beauty.add_beauty_v19_geometry(&mut beauty_v19_geometry);
        let beauty_v17_draw_list = BeautyDrawListV17::from_scene(&beauty_scene_v17);
        let mut beauty_v17_geometry = WindowSceneGeometry::default();
        beauty.add_beauty_v17_geometry(&mut beauty_v17_geometry);
        let mut shifted_camera_beauty = scene_with_render_config(
            WindowedRendererConfig::beauty_default("Ashfall - Beauty Mode"),
        );
        let anchored_humans_before = shifted_camera_beauty
            .beauty_scene_v16()
            .humans
            .iter()
            .map(|human| human.position_world)
            .collect::<Vec<_>>();
        let anchored_humans_v17_before = shifted_camera_beauty
            .beauty_scene_v17()
            .humans
            .iter()
            .map(|human| human.position_world)
            .collect::<Vec<_>>();
        shifted_camera_beauty.camera.position[0] += 0.01;
        shifted_camera_beauty.camera.position[1] += 0.01;
        let anchored_humans_after = shifted_camera_beauty
            .beauty_scene_v16()
            .humans
            .iter()
            .map(|human| human.position_world)
            .collect::<Vec<_>>();
        let anchored_humans_v17_after = shifted_camera_beauty
            .beauty_scene_v17()
            .humans
            .iter()
            .map(|human| human.position_world)
            .collect::<Vec<_>>();

        assert!(!beauty.debug_overlay_enabled(|overlays| overlays.city_chunks));
        assert!(!beauty.debug_overlay_enabled(|overlays| overlays.streaming_cells));
        assert!(!beauty.debug_overlay_enabled(|overlays| overlays.navigation_graph));
        assert!(!beauty.debug_overlay_enabled(|overlays| overlays.material_placements));
        assert!(beauty.city_streaming_visuals().is_empty());
        let (beauty_chunks, beauty_nodes, beauty_edges) = beauty.generated_city_visuals();
        assert!(beauty_chunks.is_empty());
        assert!(beauty_nodes.is_empty());
        assert!(beauty_edges.is_empty());
        assert!(beauty.city_material_placement_visuals().is_empty());
        let (beauty_cells, beauty_dangers, beauty_routes) = beauty.city_consequence_visuals();
        assert!(beauty_cells.is_empty());
        assert!(beauty_dangers.is_empty());
        assert!(beauty_routes.is_empty());
        assert!(debug.debug_overlay_enabled(|overlays| overlays.city_chunks));
        assert!(
            debug_geometry.vertices.len() > beauty_geometry.vertices.len() + 1_000,
            "Debug mode should retain generated city/streaming overlays while Beauty omits them"
        );
        assert!(beauty_geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::SCREEN_SPACE
                && vertex.position[2] > 0.95
                && vertex.color[2] > vertex.color[0] * 0.8
        }));
        let far_screen_vertex_count = beauty_geometry
            .vertices
            .iter()
            .filter(|vertex| {
                vertex.coordinate_space == WindowSceneVertex::SCREEN_SPACE
                    && vertex.position[2] > 0.95
            })
            .count();
        assert!(
            far_screen_vertex_count < 520,
            "Beauty sky should avoid high-poly screen-space glare/cloud ellipses"
        );
        assert!(
            beauty_geometry.vertices.iter().all(|vertex| {
                vertex.coordinate_space != WindowSceneVertex::SCREEN_SPACE
                    || vertex.position[2] > 0.95
            }),
            "Pure Beauty geometry should omit near-depth HUD strips and reticles"
        );
        assert!(beauty_geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD
        }));
        assert!(beauty_geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
        }));
        assert!(beauty_geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.surface_response[2] >= WINDOW_SURFACE_RESPONSE_HUMAN_SKIN[2]
                && vertex.surface_response[0] <= WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH[0]
                && vertex.position[2] > 0.8
        }));
        assert!(beauty_geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_METAL
                && vertex.position[2] > 0.18
        }));
        assert!(beauty_geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_SOIL_V18
        }));
        assert!(beauty_geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_PLANT_V18
        }));
        assert!(beauty_geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_STONE_V18
        }));
        assert!(beauty_geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_LANDFILL_V18
        }));
        assert!(
            beauty_v15_report.pass,
            "Beauty V15 scene should pass the non-debug visual bar: {beauty_v15_report:?}"
        );
        assert!(beauty_scene_v15.environment.has_visible_sky());
        assert!(beauty_scene_v15.environment.has_natural_light());
        assert!(
            beauty_scene_v15
                .cells
                .iter()
                .flat_map(|cell| cell.puddles.iter())
                .all(ashfall_rendering::beauty::AnchoredPuddleV15::is_ground_anchored)
        );
        assert!(
            beauty_scene_v15
                .humans
                .iter()
                .all(ashfall_rendering::beauty::HumanProxyV15::is_proportionate_non_rod)
        );
        assert!(
            beauty_scene_v15
                .vehicles
                .iter()
                .all(ashfall_rendering::beauty::VehicleProxyV15::is_proportionate_non_box)
        );
        assert!(
            beauty_v16_report.pass,
            "Beauty V16 scene should pass the raised visual-realism bar: {beauty_v16_report:?}"
        );
        assert!(beauty_scene_v16.humans.len() >= 2);
        assert!(!beauty_scene_v16.vehicles.is_empty());
        assert_eq!(
            anchored_humans_before, anchored_humans_after,
            "Beauty V16 humans should be anchored to world chunks, not fixed camera offsets"
        );
        assert!(
            beauty_v16_geometry.vertices.len() > 1_000,
            "Beauty V16 bridge should produce visible high-detail geometry"
        );
        assert!(beauty_v16_geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_GLASS && vertex.position[2] > 1.0
        }));
        assert!(beauty_v16_geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD && vertex.position[2] < 0.08
        }));
        assert!(beauty_v16_geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_METAL && vertex.position[2] > 0.25
        }));
        assert!(beauty_scene_v16.environment.readable_without_neon());
        assert!(
            beauty_scene_v16
                .cells
                .iter()
                .all(|cell| cell.is_valid_beauty_cell())
        );
        assert!(
            beauty_scene_v16
                .humans
                .iter()
                .all(ashfall_rendering::beauty_v16::HumanProxyV16::is_proportionate_non_rod)
        );
        assert!(
            beauty_scene_v16
                .vehicles
                .iter()
                .all(ashfall_rendering::beauty_v16::VehicleProxyV16::is_proportionate_non_box)
        );
        assert!(
            beauty_v17_report.pass,
            "Beauty V17 scene should pass the pivot visual-realism contract: {beauty_v17_report:?}"
        );
        assert!(
            beauty_v18_report.pass,
            "Beauty V18 scene should pass the artifact-fix realism contract: {beauty_v18_report:?}"
        );
        assert!(
            beauty_v19_report.passed,
            "Beauty V19 scene should pass the nature/texture/glitch contract: {beauty_v19_report:?}"
        );
        assert!(
            beauty_v19_geometry.vertices.len() > 1_200,
            "Beauty V19 bridge should produce visible city/nature/landfill geometry"
        );
        assert!(beauty_v19_geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_SOIL_V18
        }));
        assert!(beauty_v19_geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_PLANT_V18
        }));
        assert!(beauty_v19_geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_STONE_V18
        }));
        assert!(beauty_v19_geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_LANDFILL_V18
        }));
        assert!(beauty_v19_geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.surface_response[2] >= WINDOW_SURFACE_RESPONSE_HUMAN_SKIN[2]
                && vertex.surface_response[0] <= WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH[0]
                && vertex.position[2] > 0.8
        }));
        assert!(beauty_v19_geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_GLASS
        }));
        assert_eq!(
            beauty_scene_v19.biome_count(ashfall_rendering::beauty_v19::WorldBiomeV19::City),
            1
        );
        assert_eq!(
            beauty_scene_v19
                .biome_count(ashfall_rendering::beauty_v19::WorldBiomeV19::NatureReserve),
            1
        );
        assert_eq!(
            beauty_scene_v19.biome_count(ashfall_rendering::beauty_v19::WorldBiomeV19::Landfill),
            1
        );
        assert!(beauty_v17_draw_list.has_sky_commands());
        assert!(beauty_v17_draw_list.has_world_anchored_human_and_vehicle());
        assert!(beauty_scene_v17.humans.len() >= 3);
        assert!(!beauty_scene_v17.vehicles.is_empty());
        assert_eq!(
            anchored_humans_v17_before, anchored_humans_v17_after,
            "Beauty V17 humans should be anchored to world chunks, not fixed camera offsets"
        );
        assert!(
            beauty_v17_geometry.vertices.len() > 1_400,
            "Beauty V17 bridge should produce visible high-detail geometry"
        );
        let v17_far_screen_vertex_count = beauty_v17_geometry
            .vertices
            .iter()
            .filter(|vertex| {
                vertex.coordinate_space == WindowSceneVertex::SCREEN_SPACE
                    && vertex.position[2] > 0.95
            })
            .count();
        assert!(
            v17_far_screen_vertex_count < 220,
            "Beauty V17 sky should avoid screen-space ellipse fans"
        );
        assert!(beauty_v17_geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_GLASS && vertex.position[2] > 1.0
        }));
        assert!(beauty_v17_geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_WET_ROAD && vertex.position[2] < 0.08
        }));
        assert!(beauty_v17_geometry.vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_METAL && vertex.position[2] > 0.18
        }));
        let is_skin_response = |response: [f32; 4]| {
            response[1] == WINDOW_SURFACE_RESPONSE_HUMAN_SKIN[1]
                && response[0] > 0.30
                && response[0] < 0.55
                && response[2] > WINDOW_SURFACE_RESPONSE_HUMAN_SKIN[2] + 0.12
                && response[2] < 0.65
                && response[3] == WINDOW_SURFACE_RESPONSE_HUMAN_SKIN[3]
        };
        let is_eye_response = |response: [f32; 4]| {
            response[1] == WINDOW_SURFACE_RESPONSE_HUMAN_EYE[1]
                && response[0] < 0.20
                && response[2] >= WINDOW_SURFACE_RESPONSE_HUMAN_EYE[2]
                && response[3] >= WINDOW_SURFACE_RESPONSE_HUMAN_EYE[3]
        };
        let is_hair_response = |response: [f32; 4]| {
            response[1] == WINDOW_SURFACE_RESPONSE_HUMAN_HAIR[1]
                && response[0] > 0.45
                && response[0] < WINDOW_SURFACE_RESPONSE_HUMAN_HAIR[0]
                && response[2] > WINDOW_SURFACE_RESPONSE_HUMAN_HAIR[2] + 0.10
                && response[2] < 0.55
        };
        let is_cloth_response = |response: [f32; 4]| {
            response[1] == WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH[1]
                && response[0] > 0.60
                && response[0] < WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH[0]
                && response[2] > WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH[2] + 0.12
                && response[2] < 0.60
        };
        let is_human_response = |response: [f32; 4]| {
            is_skin_response(response)
                || is_eye_response(response)
                || is_hair_response(response)
                || is_cloth_response(response)
                || response == WINDOW_SURFACE_RESPONSE_HUMAN_CYBERNETIC
        };
        assert!(beauty_v17_geometry.vertices.iter().any(|vertex| {
            is_skin_response(vertex.surface_response) && vertex.position[2] > 0.8
        }));
        assert!(
            beauty_v17_geometry
                .vertices
                .iter()
                .any(|vertex| is_eye_response(vertex.surface_response)),
            "photoreal human proxy geometry should emit wet eye/tearline surfaces"
        );
        assert!(
            beauty_v17_geometry
                .vertices
                .iter()
                .any(|vertex| is_hair_response(vertex.surface_response)),
            "photoreal human proxy geometry should emit hair surfaces"
        );
        assert!(
            beauty_v17_geometry
                .vertices
                .iter()
                .any(|vertex| is_cloth_response(vertex.surface_response)),
            "photoreal human proxy geometry should emit cloth surfaces"
        );
        assert!(
            beauty_v17_geometry
                .vertices
                .iter()
                .any(|vertex| vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_CYBERNETIC),
            "photoreal human proxy geometry should emit cybernetic detail surfaces"
        );
        let human_surface_vertex_count = beauty_v17_geometry
            .vertices
            .iter()
            .filter(|vertex| is_human_response(vertex.surface_response))
            .count();
        assert!(
            human_surface_vertex_count > 700,
            "Beauty V17 human proxy should use layered photoreal body, face, hair, cloth, and cybernetic geometry"
        );
        assert!(beauty_scene_v17.environment.readable_without_neon());
        assert!(
            beauty_scene_v17
                .cells
                .iter()
                .all(|cell| cell.is_valid_beauty_cell())
        );
        assert!(
            beauty_scene_v17
                .humans
                .iter()
                .all(ashfall_rendering::beauty_v17::HumanProxyV17::is_proportionate_non_rod)
        );
        assert!(
            beauty_scene_v17
                .vehicles
                .iter()
                .all(ashfall_rendering::beauty_v17::VehicleProxyV17::is_proportionate_non_box)
        );
    }

    #[test]
    fn event_marker_tracks_world_location_and_fades() {
        let event = world_event(WorldEventKind::GlassWallFractured { entity: 2 });
        let mut marker =
            window_world_marker_from_event(&event).expect("fracture should be visible");

        assert_eq!(marker.position, [2.5, -1.25]);
        assert!(marker.base_radius > 0.7);

        let initial_radius = marker.visible_radius();
        let initial_color = marker.visible_color();
        assert!(marker.tick(0.5));

        assert!(marker.visible_radius() > initial_radius);
        assert!(marker.visible_color()[0] < initial_color[0]);
        assert!(!marker.tick(10.0));
    }

    #[test]
    fn hud_event_pulses_categorize_and_fade_world_events() {
        let fracture = world_event(WorldEventKind::GlassWallFractured { entity: 2 });
        let audio_mix = world_event(WorldEventKind::AudioFrameMixed {
            active_sound_count: 3,
            material_aware_sound_count: 2,
            peak_intensity: 0.9,
            mixed_audio_buffer: crate::core::AudioBufferHandle(77),
        });

        let mut fracture_pulse = window_hud_event_pulse_from_event(&fracture);
        let audio_pulse = window_hud_event_pulse_from_event(&audio_mix);
        let initial_color = fracture_pulse.visible_color();

        assert_eq!(fracture_pulse.weight, 1.0);
        assert!(audio_pulse.weight > 0.6);
        assert!(fracture_pulse.tick(0.5));
        assert!(fracture_pulse.visible_color()[3] < initial_color[3]);
        assert!(!fracture_pulse.tick(10.0));
    }

    #[test]
    fn infrastructure_state_tracks_ledger_events_and_scene_geometry() {
        let mut state = WindowInfrastructureRuntimeState::default();
        state.capture_events(&[
            world_event(WorldEventKind::SecurityAlertRaised {
                faction: 700,
                source_event: 99,
                threat: 1,
                severity: 0.86,
            }),
            world_event(WorldEventKind::PowerTransformerOverheated { entity: 44 }),
            world_event(WorldEventKind::StreetFlooded),
            world_event(WorldEventKind::AssetBecameResident {
                asset_id: 123,
                quality_tier: crate::core::QualityTier::NormalRuntime,
            }),
        ]);

        let visible = state.visible_state();
        assert!(visible.surveillance_coverage > 0.9);
        assert!(visible.power_instability > 0.9);
        assert!(visible.drainage_overflow > 0.9);
        assert!(visible.data_activity > 0.7);
        state.tick(1.5);
        assert!(state.surveillance < visible.surveillance_coverage);

        let runtime = build_alley_runtime().expect("alley runtime should build");
        let mut scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );
        scene.capture_frame_events(&[world_event(WorldEventKind::SecurityAlertRaised {
            faction: 700,
            source_event: 100,
            threat: 1,
            severity: 0.9,
        })]);
        let vertices = scene.scene_vertices();

        assert!(vertices.iter().any(|vertex| {
            vertex.color[0] == 1.0
                && vertex.color[1] == 0.12
                && vertex.color[2] == 0.18
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
        assert!(vertices.iter().any(|vertex| {
            vertex.color == [0.072, 0.084, 0.09, 1.0]
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
    }

    #[test]
    fn city_streaming_visuals_reflect_generated_world_plan() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let mut scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );
        scene.camera.yaw_radians = std::f32::consts::FRAC_PI_2;
        scene.capture_frame_events(&[world_event(WorldEventKind::GlassWallFractured {
            entity: GLASS_WALL_ID,
        })]);

        let request = scene.city_streaming_request();
        let visuals = scene.city_streaming_visuals();
        let vertices = scene.scene_vertices();

        assert!(!request.renderer_visible_chunks.is_empty());
        assert!(!request.renderer_feedback.is_empty());
        assert!(request.renderer_feedback.iter().any(|feedback| {
            feedback.screen_coverage > 0.0
                && feedback.visible_cluster_count > 0
                && feedback.importance_score() > 0.0
        }));
        assert!(!request.active_event_locations.is_empty());
        assert!(visuals.len() >= scene.city_template.chunks.len());
        assert!(visuals.iter().any(|cell| {
            cell.state == WindowCityStreamingCellState::HeroLoaded && cell.center_meters[0] < 32.0
        }));
        assert!(visuals.iter().any(|cell| {
            cell.state >= WindowCityStreamingCellState::GameplayLoaded
                && cell.requested_streaming_megabytes > 0.0
        }));
        assert!(vertices.iter().any(|vertex| {
            vertex.color[0] == 1.0
                && vertex.color[1] == 0.7
                && vertex.color[2] == 0.24
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
    }

    #[test]
    fn generated_city_visuals_reflect_world_template_districts_routes_and_population() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let mut scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );
        scene.camera.yaw_radians = std::f32::consts::FRAC_PI_2;

        let (chunks, nodes, edges) = scene.generated_city_visuals();
        let vertices = scene.scene_vertices();

        assert_eq!(chunks.len(), scene.city_template.chunks.len());
        assert!(nodes.len() >= scene.city_template.chunks.len() * 4);
        assert!(edges.len() >= scene.city_template.chunks.len() * 3);
        assert!(chunks.iter().any(|chunk| {
            chunk.district_kind == WindowCityDistrictVisualKind::BlackMarket
                && chunk.npc_count > 0
                && chunk.story_hook_count > 0
                && chunk.streaming_dependency_count > 0
                && chunk.faction_strength > 0.0
                && chunk.crowd_spawn_rule_count >= 3
                && chunk.traffic_rule_count > 0
                && chunk.expected_background_crowd_count >= 64
                && chunk.expected_vehicle_or_transit_count > 0
                && chunk.population_validation_passed
        }));
        assert!(chunks.iter().any(|chunk| {
            chunk.streaming_state >= WindowCityStreamingCellState::GameplayLoaded
        }));
        assert!(nodes.iter().any(|node| node.important && node.story_focus));
        assert!(edges.iter().any(|edge| {
            edge.traversal == WindowCityTraversalVisualKind::StealthPath
                && edge.route_priority > 0.0
        }));
        assert!(vertices.iter().any(|vertex| {
            vertex.color[0] == 0.82
                && vertex.color[1] == 0.24
                && vertex.color[2] == 0.56
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
        assert!(vertices.iter().any(|vertex| {
            vertex.color[0] == 0.34
                && vertex.color[1] == 1.0
                && vertex.color[2] == 0.62
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
        assert!(vertices.iter().any(|vertex| {
            vertex.color == [0.018, 0.017, 0.022, 1.0]
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_METAL
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
        assert!(vertices.iter().any(|vertex| {
            vertex.color[0] == 0.045
                && vertex.color[1] == 0.038
                && vertex.color[2] == 0.032
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
        assert!(vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
        assert!(vertices.iter().any(|vertex| {
            vertex.surface_response == WINDOW_SURFACE_RESPONSE_HUMAN_SKIN
                && vertex.position[2] > 0.8
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
        assert!(vertices.iter().any(|vertex| {
            vertex.color[0] == 0.68
                && vertex.color[1] == 0.66
                && vertex.color[2] == 0.5
                && vertex.color[3] > 0.2
                && vertex.surface_response[0] == 0.7
                && vertex.surface_response[3] == 0.0
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
    }

    #[test]
    fn city_material_placements_reflect_generated_material_assignments_and_district_state() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );

        let placements = scene.city_material_placement_visuals();
        let mut material_geometry = WindowSceneGeometry::default();
        material_geometry.add_window_city_material_placements(&placements, scene.frame_index);
        let scene_vertices = scene.scene_vertices();

        assert!(placements.len() >= scene.city_template.chunks.len() * 5);
        assert!(!scene.city_template.material_assignments.is_empty());
        assert_eq!(
            scene.city_template.materials.len(),
            scene.city_template.districts.len() * 5
        );
        let generated_material_ids = scene
            .city_template
            .materials
            .iter()
            .map(|material| material.id)
            .collect::<Vec<_>>();
        assert!(
            scene
                .city_template
                .material_assignments
                .iter()
                .all(|(_, material)| generated_material_ids.contains(material))
        );
        assert!(
            scene
                .city_template
                .material_assignments
                .iter()
                .all(|(_, material)| !matches!(
                    *material,
                    MATERIAL_GLASS
                        | MATERIAL_HUMAN_SKIN
                        | MATERIAL_NEON_TUBE
                        | MATERIAL_WATER
                        | MATERIAL_WET_ASPHALT
                ))
        );
        assert!(placements.iter().any(|placement| {
            placement.kind == WindowCityMaterialPlacementKind::Glass
                && placement.surface_response[2] > 0.3
                && placement.importance > 0.5
        }));
        assert!(placements.iter().any(|placement| {
            placement.kind == WindowCityMaterialPlacementKind::Neon
                && placement.surface_response[3] > 0.5
                && placement.importance > 0.8
        }));
        assert!(placements.iter().any(|placement| {
            placement.kind == WindowCityMaterialPlacementKind::Water && placement.wetness >= 1.0
        }));
        assert!(placements.iter().any(|placement| {
            placement.kind == WindowCityMaterialPlacementKind::WetRoad
                && placement.wetness > 0.45
                && placement.traffic_wear > 0.3
        }));
        assert!(placements.iter().any(|placement| {
            placement.kind == WindowCityMaterialPlacementKind::WetRoad
                && placement.base_color != [0.015, 0.016, 0.017, 1.0]
                && placement.surface_response[0] != 0.18
        }));
        assert!(placements.iter().any(|placement| {
            placement.kind == WindowCityMaterialPlacementKind::HumanSkin && placement.wetness > 0.1
        }));
        assert!(
            placements.iter().any(|placement| {
                placement.pollution > 0.1 && placement.faction_influence > 0.0
            })
        );
        assert!(material_geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.surface_response[2] > 0.6
                && vertex.color[2] > vertex.color[0]
                && vertex.position[2] <= 0.1
        }));
        assert!(material_geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.surface_response[3] > 0.7
                && vertex.color[0] > 0.5
        }));
        assert!(material_geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_ROUGH_DIRT
                && vertex.color[0] == 0.018
                && vertex.color[1] == 0.024
                && vertex.color[2] == 0.028
        }));
        assert!(material_geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.surface_response == WINDOW_SURFACE_RESPONSE_GLASS
                && vertex.color[0] == 0.72
                && vertex.color[1] == 0.92
                && vertex.color[2] == 1.0
        }));
        assert!(scene_vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.surface_response[2] > 0.6
                && vertex.position[2] <= 0.1
        }));
    }

    #[test]
    fn generated_city_materials_contribute_dynamic_light_to_window_frame() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let mut scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );
        scene.runtime.queue_command(WorldCommand::SetTransform(
            PLAYER_ID,
            Transform::at(Vec3::new(64.0, 0.0, 0.0)),
        ));
        scene.step_runtime();
        scene.sync_render_snapshot(0.0);

        let city_light = scene.city_dynamic_light();
        let frame = scene.update(&WindowInputState::default(), 0.0);

        assert!(city_light.is_enabled());
        assert!(city_light.position_meters[0] > 60.0);
        assert!(city_light.radius_meters > 4.0);
        assert!(city_light.intensity > 0.5);
        assert!(frame.dynamic_light.is_enabled());
        assert!(
            frame.dynamic_light.position_meters[0] > 55.0,
            "camera-near generated-city light should beat distant snapshot light"
        );
        assert!(frame.atmosphere.bloom_strength > 0.03);
        assert!(frame.atmosphere.bloom_strength < 0.09);
    }

    #[test]
    fn city_consequence_visuals_reflect_persistent_events_and_navigation_updates() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let mut scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );
        let (chunk_id, location_id, location, blocked_entity, damaged_entity) = {
            let chunk = scene
                .city_template
                .chunks
                .first()
                .expect("generated city should have a chunk");
            (
                chunk.chunk_id,
                chunk.local_navigation.nodes[0].location,
                chunk.local_navigation.nodes[0].position,
                chunk
                    .entities
                    .iter()
                    .find(|entity| entity.agent.is_some())
                    .and_then(|entity| entity.entity_id)
                    .expect("generated chunk should have an agent"),
                chunk
                    .entities
                    .iter()
                    .find(|entity| entity.tags.iter().any(|tag| tag == "destructible"))
                    .and_then(|entity| entity.entity_id)
                    .expect("generated chunk should have destructible geometry"),
            )
        };
        let mut events = Vec::new();
        for (event_id, kind) in [
            (201, WorldEventKind::StreetFlooded),
            (202, WorldEventKind::ToxicGasReleased),
            (
                203,
                WorldEventKind::NavigationMoveBlocked {
                    entity: blocked_entity,
                },
            ),
            (
                204,
                WorldEventKind::SecurityAlertRaised {
                    faction: 700,
                    source_event: 201,
                    threat: blocked_entity,
                    severity: 0.84,
                },
            ),
            (
                205,
                WorldEventKind::GlassWallFractured {
                    entity: damaged_entity,
                },
            ),
            (
                206,
                WorldEventKind::StoryEventEmitted {
                    label: "persistent_city_pressure".to_string(),
                },
            ),
            (
                207,
                WorldEventKind::PowerTransformerOverheated {
                    entity: damaged_entity,
                },
            ),
            (
                208,
                WorldEventKind::SurveillanceIncreased {
                    location: location_id,
                    source_event: 204,
                    amount: 0.86,
                },
            ),
        ] {
            let mut event = world_event(kind);
            event.event_id = event_id;
            event.location_meters = location;
            events.push(event);
        }

        scene.capture_frame_events(&events);
        let (cells, dangers, routes) = scene.city_consequence_visuals();
        let vertices = scene.scene_vertices();

        assert!(scene.persistent_city_events.len() >= events.len());
        assert!(cells.iter().any(|cell| {
            cell.cell_id == chunk_id
                && cell.save_required
                && cell.damaged_count > 0
                && cell.infrastructure_delta_count > 0
                && cell.has_infrastructure_system(WINDOW_CITY_INFRASTRUCTURE_POWER)
                && cell.has_infrastructure_system(WINDOW_CITY_INFRASTRUCTURE_WATER)
                && cell.has_infrastructure_system(WINDOW_CITY_INFRASTRUCTURE_DRAINAGE)
                && cell.has_infrastructure_system(WINDOW_CITY_INFRASTRUCTURE_SURVEILLANCE)
                && cell.faction_delta_count > 0
                && cell.story_thread_count > 0
                && cell.restricted_zone_count > 0
                && cell.blocked_route_count > 0
        }));
        assert!(dangers.iter().any(|danger| {
            danger.kind == WindowCityDangerVisualKind::Flooding && danger.severity >= 0.7
        }));
        assert!(dangers.iter().any(|danger| {
            danger.kind == WindowCityDangerVisualKind::ToxicGas && danger.severity >= 0.65
        }));
        assert!(routes.iter().any(|route| {
            route.status == WindowCityRouteConsequenceStatus::Blocked && route.severity >= 1.0
        }));
        assert!(vertices.iter().any(|vertex| {
            vertex.color[0] == 1.0
                && vertex.color[1] == 0.16
                && vertex.color[2] == 0.12
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
        assert!(vertices.iter().any(|vertex| {
            vertex.color[0] == 1.0
                && vertex.color[1] == 0.82
                && vertex.color[2] == 0.18
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
        assert!(vertices.iter().any(|vertex| {
            vertex.color[0] == 0.34
                && vertex.color[1] == 0.74
                && vertex.color[2] == 1.0
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
        assert!(vertices.iter().any(|vertex| {
            vertex.color[0] == 1.0
                && vertex.color[1] == 0.12
                && vertex.color[2] == 0.18
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
        assert!(vertices.iter().any(|vertex| {
            vertex.color[0] == 0.28
                && vertex.color[1] == 1.0
                && vertex.color[2] == 0.24
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
    }

    #[test]
    fn steam_gas_events_persist_as_visible_window_volumes() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let mut scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );

        scene.step_runtime();

        assert!(
            scene
                .runtime
                .world
                .event_ledger
                .contains_kind(|kind| { matches!(kind, WorldEventKind::ToxicGasReleased) })
        );
        assert!(scene.gas_volumes.iter().any(|volume| {
            volume.visual.color == [0.58, 0.72, 0.75, 0.36]
                && volume.visual.radius_meters == 2.0
                && volume.visual.height_meters == 3.0
                && volume.visual.visibility_blocking >= 0.4
                && volume.visual.hazard_level < 0.4
        }));

        let geometry = scene.scene_geometry();
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
                && vertex.color[0] >= 0.5
                && vertex.color[1] >= 0.6
                && vertex.color[2] >= 0.7
                && vertex.color[3] > 0.1
        }));

        scene.decay_gas_volumes(8.0);
        assert!(scene.gas_volumes.is_empty());
    }

    #[test]
    fn event_summary_surfaces_player_facing_ai_and_dialogue_events() {
        let ai_event = world_event(WorldEventKind::AgentDecisionExplained {
            agent: 3,
            source_event: Some(11),
            goal: "preserve evidence and escalate to security".to_string(),
            selected_lod: "normal".to_string(),
            available_actions: vec!["Investigate".to_string(), "ReportCrime".to_string()],
            accepted_actions: vec!["Investigate".to_string()],
            rejected_actions: vec!["Ignore".to_string()],
            reasons: vec!["nearby fracture event".to_string()],
        });
        let dialogue_event = world_event(WorldEventKind::DialogueEmitted {
            speaker: 3,
            target: Some(1),
            text: "I saw that. Security will hear about this.".to_string(),
            emotion: EmotionState::default(),
            voice_persona: None,
        });
        let blocked_event = world_event(WorldEventKind::NavigationMoveBlocked { entity: 3 });

        assert_eq!(
            event_summary(&ai_event).as_deref(),
            Some("agent 3: preserve evidence and escalate to security")
        );
        assert_eq!(
            event_summary(&dialogue_event).as_deref(),
            Some("entity 3: I saw that. Security will hear about this.")
        );
        assert_eq!(
            event_summary(&blocked_event).as_deref(),
            Some("entity 3 navigation blocked")
        );
    }

    #[test]
    fn player_motion_delta_is_camera_relative_and_normalized() {
        let input = WindowInputState {
            move_forward: true,
            move_right: true,
            ..Default::default()
        };

        let delta = player_motion_delta(&input, 0.0, 1.0).expect("movement should be requested");
        let length = (delta[0] * delta[0] + delta[1] * delta[1]).sqrt();

        assert!((length - alley_player_runtime_seed().walk_speed_meters_per_second).abs() < 0.0001);
        assert!(delta[0] > 0.0);
        assert!(delta[1] > 0.0);
    }

    #[test]
    fn window_movement_queues_core_player_movement_step() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let mut scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );
        let input = WindowInputState {
            move_forward: true,
            ..Default::default()
        };

        assert!(scene.queue_player_motion(&input, 1.0));
        scene.step_runtime();
        scene.sync_render_snapshot(0.0);

        let replay_frame = scene
            .runtime
            .replay_log()
            .frame(0)
            .expect("movement frame should be captured");
        assert!(
            replay_frame.queued_commands.iter().any(|command| {
                matches!(
                    command,
                    WorldCommand::MoveEntityStep(MoveEntityStepCommand {
                        entity: PLAYER_ID,
                        mode: MoveEntityStepMode::Toward,
                        ..
                    })
                )
            }),
            "player movement should be committed through the core movement-step command"
        );
        let snapshot = scene.last_snapshot.as_ref().expect("snapshot should sync");
        let player = snapshot
            .transforms
            .find(PLAYER_ID)
            .expect("player transform should exist");

        assert!(player.translation_meters.x < 0.55);
        assert!(player.translation_meters.x > 0.45);
        assert!(player.translation_meters.y.abs() < 0.001);
        assert_eq!(scene.camera.position[0], scene.player_position_meters[0]);
        assert_eq!(scene.camera.position[1], scene.player_position_meters[1]);
        assert!(scene.event_markers.iter().any(|marker| {
            (marker.position[0] - player.translation_meters.x).abs() < 0.001
                && (marker.position[1] - player.translation_meters.y).abs() < 0.001
        }));
    }

    #[test]
    fn repeated_player_motion_into_solid_surface_emits_blocked_navigation_event() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let mut scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );
        let input = WindowInputState {
            move_forward: true,
            ..Default::default()
        };

        assert!(scene.queue_player_motion(&input, 1.0));
        scene.step_runtime();
        scene.sync_render_snapshot(0.0);
        assert!(scene.queue_player_motion(&input, 1.0));
        scene.step_runtime();

        let replay_frame = scene
            .runtime
            .replay_log()
            .frame(1)
            .expect("second movement frame should be captured");
        assert!(replay_frame.events.iter().any(|event| {
            matches!(
                event.kind,
                WorldEventKind::NavigationMoveBlocked { entity: PLAYER_ID }
            )
        }));
        assert_eq!(
            scene.last_event_summary.as_deref(),
            Some("entity 1 navigation blocked")
        );
    }

    #[test]
    fn player_motion_is_constrained_to_alley_collision_bounds() {
        let mut runtime = build_alley_runtime().expect("alley runtime should build");
        let snapshot = runtime.render_frame(0.0).snapshot;
        let settings = alley_player_motion_settings();

        let blocked_by_glass =
            constrain_planar_motion(&snapshot, PLAYER_ID, [0.0, 0.0], [3.0, 0.0], settings);
        assert!(blocked_by_glass[0] < 0.55);
        assert!(blocked_by_glass[0] > 0.45);
        assert!(blocked_by_glass[1].abs() < 0.001);
        assert_eq!(
            constrain_planar_motion(&snapshot, PLAYER_ID, [0.0, 0.0], [4.0, 0.0], settings),
            blocked_by_glass
        );

        let clamped_to_corridor =
            constrain_planar_motion(&snapshot, PLAYER_ID, [4.8, 13.0], [9.0, 19.0], settings);
        assert_eq!(
            clamped_to_corridor,
            alley_player_runtime_seed().walkable_max
        );
    }

    #[test]
    fn fractured_glass_no_longer_blocks_player_motion() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let mut scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );

        let intact_snapshot = scene.last_snapshot.as_ref().expect("snapshot should sync");
        let blocked_by_intact_glass = constrain_planar_motion(
            intact_snapshot,
            PLAYER_ID,
            [0.0, 0.0],
            [3.0, 0.0],
            alley_player_motion_settings(),
        );
        assert!(blocked_by_intact_glass[0] < 0.55);

        assert!(scene.queue_primary_action());
        scene.step_runtime();
        scene.sync_render_snapshot(0.0);

        let fractured_snapshot = scene.last_snapshot.as_ref().expect("snapshot should sync");
        assert!(primary_interaction_entity_is_fractured(
            fractured_snapshot,
            GLASS_WALL_ID,
            alley_primary_interaction_settings(),
        ));
        assert_eq!(
            constrain_planar_motion(
                fractured_snapshot,
                PLAYER_ID,
                [0.0, 0.0],
                [4.0, 0.0],
                alley_player_motion_settings(),
            ),
            [4.0, 0.0]
        );
    }

    #[test]
    fn npc_agent_state_becomes_alarmed_after_witnessing_fracture() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let mut scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );

        let initial_snapshot = scene.last_snapshot.as_ref().expect("snapshot should sync");
        assert_eq!(
            initial_snapshot
                .agents
                .find(NPC_MARA_ID)
                .expect("Mara agent state should exist")
                .emotional_state,
            EmotionState::default()
        );

        assert!(scene.queue_primary_action());
        scene.step_runtime();
        scene.sync_render_snapshot(0.0);

        let snapshot = scene.last_snapshot.as_ref().expect("snapshot should sync");
        assert_eq!(
            snapshot
                .agents
                .find(NPC_MARA_ID)
                .expect("Mara agent state should exist")
                .emotional_state,
            EmotionState::alarmed()
        );
    }

    #[test]
    fn perspective_camera_projects_forward_points_and_culls_behind() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );

        let forward_point = [
            scene.camera.position[0] + 3.0,
            scene.camera.position[1],
            scene.camera.position[2],
        ];
        let behind_point = [
            scene.camera.position[0] - 3.0,
            scene.camera.position[1],
            scene.camera.position[2],
        ];
        let farther_point = [
            scene.camera.position[0] + 8.0,
            scene.camera.position[1],
            scene.camera.position[2],
        ];
        let above_point = [
            scene.camera.position[0] + 3.0,
            scene.camera.position[1],
            scene.camera.position[2] + 1.0,
        ];
        let below_point = [
            scene.camera.position[0] + 3.0,
            scene.camera.position[1],
            scene.camera.position[2] - 1.0,
        ];

        let projected = scene
            .camera
            .projected_ndc(forward_point)
            .expect("point in front should project");
        let farther_projected = scene
            .camera
            .projected_ndc(farther_point)
            .expect("farther point should project");
        let above_projected = scene
            .camera
            .projected_ndc(above_point)
            .expect("above point should project");
        let below_projected = scene
            .camera
            .projected_ndc(below_point)
            .expect("below point should project");

        assert!(projected[0].abs() < 0.001);
        assert!(projected[1].abs() < 0.001);
        assert!(projected[2] > 0.0);
        assert!(projected[2] < 1.0);
        assert!(farther_projected[2] > projected[2]);
        assert!(above_projected[1] < projected[1]);
        assert!(below_projected[1] > projected[1]);
        assert!(scene.camera.projected_ndc(behind_point).is_none());
    }

    #[test]
    fn window_camera_aspect_tracks_input_viewport_size() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let mut scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );
        let input = WindowInputState {
            viewport_size_pixels: [800.0, 800.0],
            ..WindowInputState::default()
        };

        scene.update(&input, 0.0);

        assert!((scene.camera.aspect_ratio - 1.0).abs() < 0.0001);
    }

    #[test]
    fn scene_geometry_uses_indexed_quads_for_vulkan_draws() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );
        let geometry = scene.scene_geometry();

        assert!(!geometry.vertices.is_empty());
        assert!(!geometry.indices.is_empty());
        assert_eq!(geometry.vertices.len() % 4, 0);
        assert_eq!(geometry.indices.len() % 6, 0);
        assert_eq!(geometry.vertices.len() / 4, geometry.indices.len() / 6);
        assert!(
            geometry
                .indices
                .iter()
                .all(|index| (*index as usize) < geometry.vertices.len())
        );
    }

    #[test]
    fn scene_geometry_assigns_world_surface_normals_for_lighting() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );
        let geometry = scene.scene_geometry();
        let world_vertices = geometry
            .vertices
            .iter()
            .filter(|vertex| vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE)
            .collect::<Vec<_>>();

        assert!(!world_vertices.is_empty());
        assert!(world_vertices.iter().all(|vertex| {
            let normal_length = dot3(vertex.normal, vertex.normal).sqrt();
            (normal_length - 1.0).abs() < 0.001
        }));
        assert!(world_vertices.iter().any(|vertex| vertex.normal[2] > 0.9));
        assert!(
            world_vertices
                .iter()
                .any(|vertex| vertex.normal[0].abs() > 0.85)
        );
    }

    #[test]
    fn scene_geometry_contains_solid_box_faces_on_all_axes() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );
        let geometry = scene.scene_geometry();
        let world_vertices = geometry
            .vertices
            .iter()
            .filter(|vertex| vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE)
            .collect::<Vec<_>>();

        assert!(world_vertices.iter().any(|vertex| vertex.normal[0] > 0.9));
        assert!(world_vertices.iter().any(|vertex| vertex.normal[0] < -0.9));
        assert!(world_vertices.iter().any(|vertex| vertex.normal[1] > 0.9));
        assert!(world_vertices.iter().any(|vertex| vertex.normal[1] < -0.9));
        assert!(world_vertices.iter().any(|vertex| vertex.normal[2] > 0.9));
        assert!(world_vertices.iter().any(|vertex| vertex.normal[2] < -0.9));
    }

    #[test]
    fn known_mesh_assets_emit_faceted_procedural_geometry() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );
        let geometry = scene.scene_geometry();
        let world_vertices = geometry
            .vertices
            .iter()
            .filter(|vertex| vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE)
            .collect::<Vec<_>>();

        assert!(world_vertices.iter().any(|vertex| {
            [vertex.normal[0], vertex.normal[1], vertex.normal[2]]
                .into_iter()
                .filter(|component| component.abs() > 0.18)
                .count()
                >= 2
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color == [0.34, 0.54, 0.58, 0.46]
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.color[3] < 0.65
                && vertex.color[2] > vertex.color[0]
                && vertex.position[2] <= 0.06
                && vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE
        }));
    }

    #[test]
    fn fractured_glass_mesh_adds_grounded_shard_triangles() {
        let mut geometry = WindowSceneGeometry::default();
        let state = MaterialState {
            crack_density: 0.82,
            ..MaterialState::default()
        };

        assert!(
            geometry.add_known_procedural_mesh(
                WindowProceduralMeshInstance::new(
                    MeshAssetHandle(WINDOW_MESH_ALLEY_GLASS_FRACTURED),
                    [2.0, 0.0],
                    0.0,
                    [0.62, 0.88, 1.0, 0.86],
                )
                .with_material_state(Some(&state)),
            )
        );

        assert!(geometry.vertices.chunks_exact(4).any(|quad| {
            quad[2].position == quad[3].position
                && quad[0].position[2] <= 0.05
                && quad[1].position[2] <= 0.06
                && quad[2].position[2] <= 0.05
        }));
        assert!(
            geometry
                .vertices
                .iter()
                .all(|vertex| vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE)
        );
    }

    #[test]
    fn humanoid_proxy_uses_multiple_solid_body_parts() {
        let mut geometry = WindowSceneGeometry::default();
        let color = [0.96, 0.72, 0.28, 1.0];
        let human = HumanState {
            human_id: 300,
            quality_tier: QualityTier::HeroHighFidelityRuntime,
        };
        let agent = AgentState {
            persona: 300,
            emotional_state: EmotionState::alarmed(),
            ai_lod: QualityTier::NormalRuntime,
        };

        geometry.add_humanoid_proxy(
            WindowHumanoidProxyInstance::new([5.5, 0.0], 0.0, color)
                .with_human(Some(&human))
                .with_emotion(Some(&agent.emotional_state))
                .with_viewer_position([0.0, 0.0, 1.65]),
        );

        let old_box_proxy_vertex_count = 6 * 6 * 4 + 3 * 4;
        assert!(geometry.vertices.len() > old_box_proxy_vertex_count);
        assert_eq!(geometry.vertices.len() % 4, 0);
        assert_eq!(geometry.indices.len(), geometry.vertices.len() / 4 * 6);
        assert!(
            geometry
                .vertices
                .iter()
                .all(|vertex| vertex.coordinate_space == WindowSceneVertex::WORLD_SPACE)
        );
        assert!(geometry.vertices.iter().any(|vertex| {
            vertex.normal[0].abs() > 0.2
                && vertex.normal[1].abs() > 0.2
                && vertex.normal[2].abs() > 0.2
        }));
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.position[2] > 1.45)
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.position[0] < 5.25)
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.position[0] > 5.75)
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.color == natural_skin_color(color))
        );
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.color == [0.22, 0.62, 0.66, 0.62])
        );
        let emotion = &agent.emotional_state;
        let alert =
            (emotion.fear * 0.45 + emotion.anger * 0.2 + emotion.urgency * 0.35).clamp(0.0, 1.0);
        let alert_eye_color = [
            0.12 + alert * 0.82,
            0.86 - alert * 0.42,
            1.0 - alert * 0.72,
            1.0,
        ];
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| vertex.color == alert_eye_color)
        );
    }

    #[test]
    fn mouse_delta_uses_u61q_style_sensitivity_for_camera_yaw() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let mut scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );
        let initial_yaw = scene.camera.yaw_radians;
        let input = WindowInputState {
            mouse_delta: [400.0, 0.0],
            ..Default::default()
        };

        scene.update_view_controls(&input, 0.0);

        assert!((scene.camera.yaw_radians - initial_yaw - 1.0).abs() < 0.0001);
    }

    #[test]
    fn positive_mouse_y_lowers_camera_pitch() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let mut scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );
        let input = WindowInputState {
            mouse_delta: [0.0, 240.0],
            ..Default::default()
        };

        scene.update_view_controls(&input, 0.0);

        assert!(scene.camera.pitch_radians < 0.0);
    }

    #[test]
    fn negative_mouse_y_raises_camera_pitch() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let mut scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );
        let input = WindowInputState {
            mouse_delta: [0.0, -240.0],
            ..Default::default()
        };

        scene.update_view_controls(&input, 0.0);

        assert!(scene.camera.pitch_radians > 0.0);
    }

    #[test]
    fn window_status_surfaces_live_interaction_preview() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );

        let interaction = scene
            .interaction_preview()
            .expect("glass should be previewed");
        let (_, preview_color) = interaction_preview_style(&interaction, scene.frame_index);
        let vertices = scene.scene_vertices();

        assert_eq!(interaction.entity, GLASS_WALL_ID);
        assert_eq!(scene.status_summary(), "target alley glass wall");
        assert!(vertices.iter().any(|vertex| vertex.color == preview_color));
    }

    #[test]
    fn window_frame_carries_dynamic_light_from_visible_world_state() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let mut scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );

        let frame = scene.update(&WindowInputState::default(), 0.0);

        assert!(frame.dynamic_light.is_enabled());
        assert!(frame.dynamic_light.radius_meters > 3.0);
        assert!(frame.dynamic_light.intensity > 0.1);
        assert!(frame.atmosphere.fog_density_per_meter > 0.009);
        assert!(frame.atmosphere.exposure > 1.0);
        assert!(frame.atmosphere.bloom_strength > 0.025);
        assert!(frame.atmosphere.bloom_strength < 0.08);
    }

    #[test]
    fn window_hud_draws_reticle_and_recent_event_strip() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let mut scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );
        let fracture = world_event(WorldEventKind::GlassWallFractured { entity: 2 });
        let dialogue = world_event(WorldEventKind::DialogueEmitted {
            speaker: 3,
            target: Some(1),
            text: "That glass was loud.".to_string(),
            emotion: EmotionState::default(),
            voice_persona: None,
        });

        scene.capture_frame_events(&[fracture.clone(), dialogue]);
        let reticle_color = scene
            .interaction_preview()
            .map(|interaction| interaction_reticle_color(&interaction))
            .expect("interaction reticle should have a target");
        let fracture_hud_color = window_hud_event_pulse_from_event(&fracture).visible_color();
        let vertices = scene.scene_vertices();

        assert_eq!(scene.hud_event_pulses.len(), 2);
        assert!(vertices.iter().any(|vertex| vertex.color == reticle_color));
        assert!(
            vertices
                .iter()
                .any(|vertex| vertex.color == fracture_hud_color)
        );

        scene.decay_hud_event_pulses(WINDOW_HUD_EVENT_PULSE_SECONDS + 0.1);
        assert!(scene.hud_event_pulses.is_empty());
    }

    #[test]
    fn primary_interaction_selects_nearby_destructible_target() {
        let mut runtime = build_alley_runtime().expect("alley runtime should build");
        let snapshot = runtime.render_frame(0.0).snapshot;

        let interaction =
            primary_interaction_from_snapshot(&snapshot, 0.0, alley_primary_interaction_settings())
                .expect("glass should be targetable");

        assert_eq!(interaction.entity, GLASS_WALL_ID);
        assert!(interaction.in_reach);
        assert!(!interaction.in_aim_cone);
        assert!(!interaction.already_fractured);
        assert_eq!(interaction.force.entity, GLASS_WALL_ID);
        assert_eq!(interaction.force.source, Some(PLAYER_ID));
        assert!(interaction.force.vector_newtons.x > 1_900.0);
        assert!(interaction.force.vector_newtons.y.abs() < 0.001);
    }

    #[test]
    fn primary_interaction_prefers_aimed_reachable_target_over_nearer_side_target() {
        let mut runtime = build_alley_runtime().expect("alley runtime should build");
        runtime.queue_command(WorldCommand::SetTransform(
            PLAYER_ID,
            Transform::at(Vec3::new(1.8, 1.5, 0.0)),
        ));
        runtime.step().expect("player relocation should commit");
        let snapshot = runtime.render_frame(0.0).snapshot;

        let interaction =
            primary_interaction_from_snapshot(&snapshot, 0.0, alley_primary_interaction_settings())
                .expect("aimed neon should be targetable");

        assert_eq!(interaction.entity, NEON_SIGN_ID);
        assert!(interaction.in_reach);
        assert!(interaction.in_aim_cone);
        assert!(
            interaction.aim_alignment
                > alley_primary_interaction_settings().aim_alignment_threshold
        );
    }

    #[test]
    fn primary_interaction_surfaces_aimed_target_after_nearby_target_is_completed() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let mut scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );

        assert!(scene.queue_primary_action());
        scene.step_runtime();
        scene.sync_render_snapshot(0.0);
        scene.camera.yaw_radians = 0.0;

        let snapshot = scene.last_snapshot.as_ref().expect("snapshot should sync");
        let interaction =
            primary_interaction_from_snapshot(snapshot, 0.0, alley_primary_interaction_settings())
                .expect("aimed neon should become the next visible target");
        let queued_action_count = scene.queued_action_count;

        assert_eq!(interaction.entity, NEON_SIGN_ID);
        assert!(interaction.in_aim_cone);
        assert!(!interaction.in_reach);
        assert!(!interaction.already_fractured);
        assert_eq!(
            interaction.status_summary(),
            "aim faulty magenta neon sign 3.2 m"
        );
        assert!(!scene.queue_primary_action());
        assert_eq!(scene.queued_action_count, queued_action_count);
        assert_eq!(
            scene
                .interaction_notice
                .as_ref()
                .map(|notice| notice.message.as_str()),
            Some("too far from faulty magenta neon sign (3.2 m)")
        );
    }

    #[test]
    fn primary_interaction_blocks_already_fractured_target() {
        let runtime = build_alley_runtime().expect("alley runtime should build");
        let mut scene = AlleyWindowScene::new_with_audio(
            runtime,
            None,
            WindowAudioStatus::Unavailable("test audio disabled".to_string()),
        );

        assert!(scene.queue_primary_action());
        scene.step_runtime();
        scene.sync_render_snapshot(0.0);
        scene.camera.yaw_radians = std::f32::consts::FRAC_PI_2;

        let snapshot = scene.last_snapshot.as_ref().expect("snapshot should sync");
        let interaction = primary_interaction_from_snapshot(
            snapshot,
            scene.camera.yaw_radians,
            alley_primary_interaction_settings(),
        )
        .expect("glass should remain targetable");
        let queued_action_count = scene.queued_action_count;

        assert_eq!(interaction.entity, GLASS_WALL_ID);
        assert!(interaction.in_aim_cone);
        assert!(interaction.already_fractured);
        assert_eq!(interaction.status_summary(), "fractured alley glass wall");
        assert!(!scene.queue_primary_action());
        assert_eq!(scene.queued_action_count, queued_action_count);
        assert_eq!(
            scene
                .interaction_notice
                .as_ref()
                .map(|notice| notice.message.as_str()),
            Some("already fractured: alley glass wall")
        );
    }

    #[test]
    fn primary_interaction_reports_target_out_of_reach() {
        let mut runtime = build_alley_runtime().expect("alley runtime should build");
        runtime.queue_command(WorldCommand::SetTransform(
            PLAYER_ID,
            Transform::at(Vec3::new(10.0, 0.0, 0.0)),
        ));
        runtime.step().expect("player relocation should commit");
        let snapshot = runtime.render_frame(0.0).snapshot;

        let interaction =
            primary_interaction_from_snapshot(&snapshot, 0.0, alley_primary_interaction_settings())
                .expect("destructible should exist");

        assert_eq!(interaction.entity, GLASS_WALL_ID);
        assert!(!interaction.in_reach);
        assert!(interaction.distance_meters > alley_primary_interaction_settings().reach_meters);
    }
}
