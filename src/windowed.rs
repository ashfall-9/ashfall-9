use std::error::Error;

use ashfall_materials::{
    MATERIAL_GLASS, MATERIAL_HUMAN_SKIN, MATERIAL_NEON_TUBE, MATERIAL_WATER, MATERIAL_WET_ASPHALT,
};
use ashfall_physics::{PlanarMotionSettings, constrain_planar_motion};
use ashfall_rendering::windowed::{
    WINDOW_CITY_INFRASTRUCTURE_DATA, WINDOW_CITY_INFRASTRUCTURE_DRAINAGE,
    WINDOW_CITY_INFRASTRUCTURE_POWER, WINDOW_CITY_INFRASTRUCTURE_SURVEILLANCE,
    WINDOW_CITY_INFRASTRUCTURE_TRANSIT, WINDOW_CITY_INFRASTRUCTURE_WATER,
    WINDOW_HUD_EVENT_STRIP_MAX_PULSES, WINDOW_WORLD_EVENT_MARKER_MAX_COUNT,
    WindowCameraControlSettings, WindowCityDangerFieldVisual, WindowCityDangerVisualKind,
    WindowCityDistrictVisualKind, WindowCityMaterialPlacementKind,
    WindowCityMaterialPlacementVisual, WindowCityPersistenceCounts, WindowCityPersistentCellVisual,
    WindowCityRouteConsequenceStatus, WindowCityRouteConsequenceVisual,
    WindowCityStreamingCellState, WindowCityStreamingCellVisual, WindowCityTraversalVisualKind,
    WindowDynamicLight, WindowFrameState, WindowGasVolumeVisual, WindowGeneratedCityChunkVisual,
    WindowGeneratedCityNavigationEdgeVisual, WindowGeneratedCityNavigationNodeVisual,
    WindowHudEventPulse, WindowInfrastructureVisualState, WindowInputState,
    WindowPerspectiveCamera, WindowScene, WindowSceneGeometry, WindowSnapshotSceneOptions,
    WindowWorldEventMarker, WindowWorldMarkerVisual, WindowedRendererConfig, run_windowed_scene,
    window_atmosphere_for_alley, window_dynamic_light_from_city_material_placements,
    window_dynamic_light_from_snapshot, window_hud_event_pulse_from_event,
    window_world_marker_from_event,
};
#[cfg(test)]
use ashfall_rendering::windowed::{
    WINDOW_SURFACE_RESPONSE_GLASS, WINDOW_SURFACE_RESPONSE_HUMAN_CLOTH,
    WINDOW_SURFACE_RESPONSE_HUMAN_SKIN, WINDOW_SURFACE_RESPONSE_METAL,
    WINDOW_SURFACE_RESPONSE_ROUGH_DIRT, WindowSceneVertex,
};
use ashfall_voice::RodioEventAudioSink;
use ashfall_worldgen::{
    CityCellStreamingDecision, CityCellStreamingState, CityRendererStreamingFeedback,
    CityStreamingRequest, DistrictKind, InfrastructureSystem, NavigationDangerKind,
    NavigationRouteStatus, PLAYER_ID, TraversalKind, WorldChunk, WorldTemplate,
    alley_player_runtime_seed, generate_world_template, navigation_cell_states_from_persistence,
    persistent_city_state_from_events, plan_city_cell_streaming,
};

#[cfg(test)]
use crate::core::QualityTier;
use crate::core::{MaterialDescriptor, MaterialId, MaterialState, Vec3};
use crate::runtime::EngineRuntime;
use crate::scenarios::{alley_city_generation_request, build_alley_runtime};
use crate::world::{
    EntityTemplate, MoveEntityStepCommand, MoveEntityStepMode, PrimaryInteraction,
    PrimaryInteractionSettings, WorldCommand, WorldEvent, WorldEventKind, WorldSnapshot,
    primary_interaction_from_snapshot,
};

#[cfg(target_os = "windows")]
const WINDOWS_WINDOW_THREAD_STACK_BYTES: usize = 32 * 1024 * 1024;
const WINDOW_CITY_PERSISTENT_EVENT_HISTORY_MAX: usize = 192;

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
    let scene = AlleyWindowScene::new(runtime);
    run_windowed_scene(
        scene,
        WindowedRendererConfig {
            title: "Ashfall - Vulkan Runtime".to_string(),
            ..Default::default()
        },
    )
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
        let (event_audio_sink, audio_status) = match RodioEventAudioSink::open_default() {
            Ok(sink) => (Some(sink), WindowAudioStatus::Ready),
            Err(error) => (None, WindowAudioStatus::Unavailable(error)),
        };
        Self::new_with_audio(runtime, event_audio_sink, audio_status)
    }

    fn new_with_audio(
        mut runtime: EngineRuntime,
        event_audio_sink: Option<RodioEventAudioSink>,
        audio_status: WindowAudioStatus,
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

        let x_wave = (self.camera.position[0] * 0.15 + self.frame_index as f32 * 0.01)
            .sin()
            .mul_add(0.5, 0.5);
        let y_wave = (self.camera.position[1] * 0.11).cos().mul_add(0.5, 0.5);
        let z_glow = (self.camera.position[2] / 12.0).clamp(0.0, 1.0);
        let event_glow = (self.last_event_count as f32 / 42.0).clamp(0.0, 1.0);

        [
            0.31 + x_wave * 0.05 + event_glow * 0.025,
            0.38 + y_wave * 0.06 + z_glow * 0.02,
            0.47 + z_glow * 0.07 + event_glow * 0.045,
            1.0,
        ]
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

    fn city_streaming_visuals(&self) -> Vec<WindowCityStreamingCellVisual> {
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
        let placements = self.city_material_placement_visuals();
        window_dynamic_light_from_city_material_placements(
            &placements,
            self.camera.position,
            self.frame_index,
        )
    }

    fn city_consequence_visuals(
        &self,
    ) -> (
        Vec<WindowCityPersistentCellVisual>,
        Vec<WindowCityDangerFieldVisual>,
        Vec<WindowCityRouteConsequenceVisual>,
    ) {
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
        let (generated_city_chunks, generated_city_nodes, generated_city_edges) =
            self.generated_city_visuals();
        geometry.add_window_generated_city(
            &generated_city_chunks,
            &generated_city_nodes,
            &generated_city_edges,
            self.frame_index,
        );
        let city_material_placements = self.city_material_placement_visuals();
        geometry.add_window_city_material_placements(&city_material_placements, self.frame_index);
        let city_streaming_cells = self.city_streaming_visuals();
        geometry.add_window_city_streaming_cells(&city_streaming_cells, self.frame_index);
        let (persistent_cells, danger_fields, route_consequences) = self.city_consequence_visuals();
        geometry.add_window_city_consequences(
            &persistent_cells,
            &danger_fields,
            &route_consequences,
            self.frame_index,
        );
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
        self.add_gas_volumes(&mut geometry);
        self.add_interaction_target_preview(&mut geometry);
        self.add_event_markers(&mut geometry);
        self.add_hud(&mut geometry);

        geometry
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
        assert!(frame.atmosphere.bloom_strength > 0.12);
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
        assert!(frame.atmosphere.bloom_strength > 0.1);
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
