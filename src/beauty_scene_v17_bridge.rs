//! Bridge from Ashfall world generation to V17 Beauty Scene.
//!
//! Important behavior changes compared with the current visual result:
//! - Do not render chunk bounds as visible city geometry.
//! - Do not render material placement boxes as final surfaces.
//! - Do not spawn a single camera-relative human.
//! - Generate retained world-anchored beauty cells with roads, curbs, facades,
//!   pipes, cables, scatter, grounded puddles, human proxies, vehicle proxies,
//!   natural sky, and material page requests.

use ashfall_rendering::beauty_v17::{
    BeautyBoundsV17, BeautyCellIdV17, BeautyCellPackageV17, BeautyMaterialIdV17, BeautyObjectIdV17,
    BeautySceneV17, BeautySurfaceIdV17, CurbSegmentV17, CurveObjectKindV17, CurveObjectV17,
    EnvironmentStateV17, FacadeModuleV17, FrameBudgetConfigV17, GroundedPuddleV17, HumanProxyV17,
    IrregularityRecipeV17, MaterialPageRequestV17, RoadPatchV17, ScatterFieldV17, VehicleProxyV17,
};
use ashfall_worldgen::WorldTemplate;

const MAT_WET_ASPHALT: BeautyMaterialIdV17 = BeautyMaterialIdV17(0xA5F_A17);
const MAT_DIRTY_CONCRETE: BeautyMaterialIdV17 = BeautyMaterialIdV17(0xC0A1_C0A1);
const MAT_CURB_CONCRETE: BeautyMaterialIdV17 = BeautyMaterialIdV17(0xC0B_C0B);
const MAT_PIPE_DARK_METAL: BeautyMaterialIdV17 = BeautyMaterialIdV17(0x000D_A110_DA11);
const MAT_WATER: BeautyMaterialIdV17 = BeautyMaterialIdV17(0xA11EA);

pub fn build_beauty_scene_v17(
    city_template: &WorldTemplate,
    camera_xy: [f32; 2],
    frame_index: u64,
) -> BeautySceneV17 {
    let budget = FrameBudgetConfigV17::default();
    let mut scene = BeautySceneV17::new(frame_index).with_budget(budget);
    scene.environment = environment_from_frame(frame_index);

    let mut nearest_chunks = city_template
        .chunks
        .iter()
        .map(|chunk| {
            let center_x = (chunk.bounds.min.x + chunk.bounds.max.x) * 0.5;
            let center_y = (chunk.bounds.min.y + chunk.bounds.max.y) * 0.5;
            let dx = center_x - camera_xy[0];
            let dy = center_y - camera_xy[1];
            (dx * dx + dy * dy, chunk)
        })
        .collect::<Vec<_>>();
    nearest_chunks.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut remaining_humans = scene.detail_orchestrator.budget.max_human_proxies;
    let mut remaining_vehicles = scene.detail_orchestrator.budget.max_vehicle_proxies;

    for (_, chunk) in nearest_chunks
        .into_iter()
        .take(scene.detail_orchestrator.budget.max_visible_beauty_cells)
    {
        let cell = build_cell_package_from_chunk(
            chunk.chunk_id,
            chunk.bounds.min.x,
            chunk.bounds.min.y,
            chunk.bounds.max.x,
            chunk.bounds.max.y,
            city_template.seed,
        );

        let human_target_count = chunk
            .population
            .expected_active_npcs
            .max(chunk.population.expected_background_crowd.min(3))
            .clamp(1, 4);
        let human_count = human_target_count.min(remaining_humans);
        remaining_humans = remaining_humans.saturating_sub(human_count);
        for index in 0..human_count {
            let seed = city_template.seed ^ chunk.chunk_id ^ ((index as u64 + 1) * 0x484D);
            scene.humans.push(human_for_cell(
                BeautyCellIdV17(chunk.chunk_id),
                chunk.chunk_id.wrapping_mul(100).wrapping_add(index as u64),
                cell.bounds,
                seed,
            ));
        }

        let vehicle_target_count = chunk
            .population
            .expected_vehicle_or_transit_count
            .clamp(1, 2);
        let vehicle_count = vehicle_target_count.min(remaining_vehicles);
        remaining_vehicles = remaining_vehicles.saturating_sub(vehicle_count);
        for index in 0..vehicle_count {
            let seed = city_template.seed ^ chunk.chunk_id ^ ((index as u64 + 1) * 0xCA9);
            scene.vehicles.push(vehicle_for_cell(
                BeautyCellIdV17(chunk.chunk_id),
                chunk
                    .chunk_id
                    .wrapping_mul(100)
                    .wrapping_add(50 + index as u64),
                cell.bounds,
                seed,
            ));
        }

        scene.cells.push(cell);
    }

    scene.collect_cell_material_pages();
    scene
}

fn environment_from_frame(frame_index: u64) -> EnvironmentStateV17 {
    // Keep day/night switch deterministic for testing; later replace with city time-of-day.
    if (frame_index / 9_000).is_multiple_of(2) {
        EnvironmentStateV17::rainy_overcast_day()
    } else {
        EnvironmentStateV17::rainy_moonlit_night()
    }
}

fn build_cell_package_from_chunk(
    cell_id_raw: u64,
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
    world_seed: u64,
) -> BeautyCellPackageV17 {
    let cell_id = BeautyCellIdV17(cell_id_raw);
    let seed = world_seed ^ cell_id_raw;
    let z_ground = 0.0;
    let center_x = (min_x + max_x) * 0.5;
    let center_y = (min_y + max_y) * 0.5;
    let width = (max_x - min_x).abs().max(12.0);
    let depth = (max_y - min_y).abs().max(12.0);
    let road_surface = BeautySurfaceIdV17(cell_id_raw.wrapping_mul(100).wrapping_add(1));
    let facade_left_surface = BeautySurfaceIdV17(cell_id_raw.wrapping_mul(100).wrapping_add(2));
    let facade_right_surface = BeautySurfaceIdV17(cell_id_raw.wrapping_mul(100).wrapping_add(3));

    let bounds = BeautyBoundsV17 {
        min: [min_x, min_y, z_ground - 0.10],
        max: [max_x, max_y, 9.0],
    };

    let road_width = (width * 0.42).clamp(4.0, 9.0);
    let road = RoadPatchV17 {
        object_id: BeautyObjectIdV17(cell_id_raw.wrapping_mul(1000).wrapping_add(1)),
        surface_id: road_surface,
        centerline_world: vec![
            [center_x, min_y + depth * 0.05, z_ground],
            [
                center_x + seeded_signed(seed, 1) * 0.9,
                center_y,
                z_ground + 0.018,
            ],
            [
                center_x + seeded_signed(seed, 2) * 0.5,
                max_y - depth * 0.05,
                z_ground,
            ],
        ],
        width_meters: road_width,
        crown_height_meters: 0.038,
        edge_noise_meters: 0.22,
        puddle_basin_count: 3,
        pothole_count: 2,
        material_id: MAT_WET_ASPHALT,
        irregularity: IrregularityRecipeV17::road(seed),
    };

    let curb_left = curb_segment(
        cell_id_raw,
        10,
        [center_x - road_width * 0.5, min_y + 0.8, z_ground + 0.02],
        [
            center_x - road_width * 0.5 + seeded_signed(seed, 3) * 0.35,
            max_y - 0.8,
            z_ground + 0.02,
        ],
        seed ^ 0xC0B1,
    );
    let curb_right = curb_segment(
        cell_id_raw,
        11,
        [center_x + road_width * 0.5, min_y + 0.8, z_ground + 0.02],
        [
            center_x + road_width * 0.5 + seeded_signed(seed, 4) * 0.35,
            max_y - 0.8,
            z_ground + 0.02,
        ],
        seed ^ 0xC0B2,
    );

    let facade_left = facade(
        cell_id_raw,
        20,
        BeautyBoundsV17 {
            min: [min_x + 0.4, min_y + 0.5, z_ground],
            max: [
                center_x - road_width * 0.5 - 0.8,
                max_y - 0.5,
                6.0 + seeded01(seed, 10) * 2.5,
            ],
        },
        9,
        seed ^ 0x00FA_CADE_0001,
    );
    let facade_right = facade(
        cell_id_raw,
        21,
        BeautyBoundsV17 {
            min: [center_x + road_width * 0.5 + 0.8, min_y + 0.5, z_ground],
            max: [max_x - 0.4, max_y - 0.5, 5.5 + seeded01(seed, 11) * 3.0],
        },
        8,
        seed ^ 0x00FA_CADE_0002,
    );

    let pipe = CurveObjectV17 {
        object_id: BeautyObjectIdV17(cell_id_raw.wrapping_mul(1000).wrapping_add(30)),
        kind: CurveObjectKindV17::Pipe,
        points_world: vec![
            [center_x - road_width * 0.55, min_y + 1.0, 2.2],
            [
                center_x - road_width * 0.55 + seeded_signed(seed, 20) * 0.4,
                center_y,
                2.35,
            ],
            [center_x - road_width * 0.55, max_y - 1.0, 2.1],
        ],
        radius_meters: 0.055,
        material_id: MAT_PIPE_DARK_METAL,
        sag_meters: 0.0,
        surface_dirt_0_to_1: 0.42,
    };
    let cable = CurveObjectV17 {
        object_id: BeautyObjectIdV17(cell_id_raw.wrapping_mul(1000).wrapping_add(31)),
        kind: CurveObjectKindV17::Cable,
        points_world: vec![
            [center_x - road_width * 0.70, min_y + 1.2, 4.4],
            [center_x + seeded_signed(seed, 21) * 0.35, center_y, 4.1],
            [center_x + road_width * 0.70, max_y - 1.2, 4.35],
        ],
        radius_meters: 0.018,
        material_id: MAT_PIPE_DARK_METAL,
        sag_meters: 0.28,
        surface_dirt_0_to_1: 0.36,
    };

    let puddle = GroundedPuddleV17 {
        object_id: BeautyObjectIdV17(cell_id_raw.wrapping_mul(1000).wrapping_add(40)),
        receiver_surface_id: road_surface,
        center_world: [
            center_x + seeded_signed(seed, 30) * 1.3,
            center_y + seeded_signed(seed, 31) * 2.0,
            z_ground + 0.004,
        ],
        receiver_normal_world: [0.0, 0.0, 1.0],
        radius_meters: 0.55 + seeded01(seed, 32) * 0.85,
        max_depth_meters: 0.012 + seeded01(seed, 33) * 0.014,
        edge_softness_meters: 0.18,
        material_id: MAT_WATER,
    };

    let scatter = ScatterFieldV17 {
        object_id: BeautyObjectIdV17(cell_id_raw.wrapping_mul(1000).wrapping_add(50)),
        bounds: BeautyBoundsV17 {
            min: [min_x + 0.5, min_y + 0.5, z_ground],
            max: [max_x - 0.5, max_y - 0.5, z_ground + 0.8],
        },
        density_0_to_1: 0.34,
        item_count_budget: 96,
        has_trash: true,
        has_stones: true,
        has_paper: true,
        has_broken_glass: true,
        has_cable_clutter: true,
        seed: seed ^ 0x5CA77E,
    };

    let mut material_page_requests = Vec::new();
    material_page_requests.extend(MaterialPageRequestV17::wet_asphalt_pack(
        road_surface,
        MAT_WET_ASPHALT,
        seed ^ 0xA5FA17,
    ));
    material_page_requests.extend(MaterialPageRequestV17::dirty_concrete_pack(
        facade_left_surface,
        MAT_DIRTY_CONCRETE,
        seed ^ 0xC011,
    ));
    material_page_requests.extend(MaterialPageRequestV17::dirty_concrete_pack(
        facade_right_surface,
        MAT_DIRTY_CONCRETE,
        seed ^ 0xC012,
    ));

    BeautyCellPackageV17 {
        cell_id,
        bounds,
        roads: vec![road],
        curbs: vec![curb_left, curb_right],
        facades: vec![facade_left, facade_right],
        pipes_and_cables: vec![pipe, cable],
        scatter_fields: vec![scatter],
        puddles: vec![puddle],
        material_page_requests,
        retained_cache_key: retained_cache_key(world_seed, cell_id_raw),
        dirty: true,
    }
}

fn curb_segment(
    cell_id: u64,
    local_id: u64,
    start_world: [f32; 3],
    end_world: [f32; 3],
    seed: u64,
) -> CurbSegmentV17 {
    CurbSegmentV17 {
        object_id: BeautyObjectIdV17(cell_id.wrapping_mul(1000).wrapping_add(local_id)),
        start_world,
        end_world,
        height_meters: 0.16,
        width_meters: 0.32,
        bevel_radius_meters: 0.045,
        chip_density_0_to_1: 0.32,
        material_id: MAT_CURB_CONCRETE,
        irregularity: IrregularityRecipeV17::curb(seed),
    }
}

fn facade(
    cell_id: u64,
    local_id: u64,
    bounds: BeautyBoundsV17,
    window_count: u16,
    seed: u64,
) -> FacadeModuleV17 {
    FacadeModuleV17 {
        object_id: BeautyObjectIdV17(cell_id.wrapping_mul(1000).wrapping_add(local_id)),
        bounds,
        material_id: MAT_DIRTY_CONCRETE,
        window_count,
        door_count: 1,
        vent_count: 3,
        pipe_mount_count: 2,
        sign_mount_count: 1,
        inset_depth_meters: 0.16,
        bevel_radius_meters: 0.055,
        facade_warp_meters: 0.018,
        dirt_0_to_1: 0.66,
        poster_or_stain_count: 5,
        irregularity: IrregularityRecipeV17::dirty_facade(seed),
    }
}

fn human_for_cell(
    cell_id: BeautyCellIdV17,
    entity_id: u64,
    bounds: BeautyBoundsV17,
    seed: u64,
) -> HumanProxyV17 {
    let x = lerp(
        bounds.min[0] + 1.4,
        bounds.max[0] - 1.4,
        seeded01(seed, 100),
    );
    let y = lerp(
        bounds.min[1] + 1.4,
        bounds.max[1] - 1.4,
        seeded01(seed, 101),
    );
    HumanProxyV17::adult_world_anchored(entity_id, cell_id, [x, y, 0.02], seed)
}

fn vehicle_for_cell(
    cell_id: BeautyCellIdV17,
    entity_id: u64,
    bounds: BeautyBoundsV17,
    seed: u64,
) -> VehicleProxyV17 {
    let center = bounds.center();
    let x = center[0] + seeded_signed(seed, 200) * 1.4;
    let y = center[1] + seeded_signed(seed, 201) * 2.2;
    VehicleProxyV17::compact_car_world_anchored(entity_id, cell_id, [x, y, 0.03], seed)
}

fn retained_cache_key(world_seed: u64, cell_id: u64) -> u128 {
    ((world_seed as u128) << 64) ^ cell_id as u128 ^ ASHFALL_V17_CACHE_SALT
}

const ASHFALL_V17_CACHE_SALT: u128 = 0x0A5F_A110_0000_0017;

fn seeded01(seed: u64, salt: u64) -> f32 {
    let mut x = seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    ((x >> 40) as f32) / ((1u64 << 24) as f32)
}

fn seeded_signed(seed: u64, salt: u64) -> f32 {
    seeded01(seed, salt) * 2.0 - 1.0
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}
