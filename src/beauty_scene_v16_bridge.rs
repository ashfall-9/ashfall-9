//! Bridge from the current Ashfall world template to `BeautySceneV16`.
//!
//! This bridge is intentionally deterministic and small. It converts nearby world
//! chunks into real player-facing beauty concepts instead of drawing chunk bounds
//! or material placement boxes.

use ashfall_rendering::beauty_v16::{
    AnchoredPuddleV16, BeautyBoundsV16, BeautyCellPackageV16, BeautyMaterialIdV16,
    BeautyObjectIdV16, BeautySceneV16, BeautySurfaceIdV16, CurbSegmentV16, CurveObjectV16,
    EnvironmentStateV16, FacadeModuleV16, HumanProxyV16, IrregularityRecipeV16,
    MaterialPageRequestV16, RoadSplineV16, ScatterFieldV16, VehicleProxyV16,
};
use ashfall_worldgen::WorldTemplate;

const MAT_WET_ASPHALT: BeautyMaterialIdV16 = BeautyMaterialIdV16(0xA5F_A17);
const MAT_DIRTY_CONCRETE: BeautyMaterialIdV16 = BeautyMaterialIdV16(0xC0A1_C0A1);
const MAT_CURB_CONCRETE: BeautyMaterialIdV16 = BeautyMaterialIdV16(0xC0B_C0B);
const MAT_PIPE_DARK_METAL: BeautyMaterialIdV16 = BeautyMaterialIdV16(0x000D_A110_DA11);
const MAT_WATER: BeautyMaterialIdV16 = BeautyMaterialIdV16(0xA11EA);

/// Build a small, high-quality Beauty scene around the camera.
///
/// Codex integration note: call this from the windowed Beauty Mode path, then
/// translate the returned scene to the current window/render-packet objects.
/// Keep debug visual generators available only in Debug or Mixed overlays.
pub fn build_beauty_scene_v16(
    city_template: &WorldTemplate,
    camera_xy: [f32; 2],
    frame_index: u64,
) -> BeautySceneV16 {
    let mut scene = BeautySceneV16::new(frame_index);
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

    for (_, chunk) in nearest_chunks.into_iter().take(5) {
        scene.cells.push(build_cell_package_from_chunk(
            chunk.chunk_id,
            chunk.bounds.min.x,
            chunk.bounds.min.y,
            chunk.bounds.max.x,
            chunk.bounds.max.y,
            city_template.seed,
        ));
    }

    scene.humans.push(default_human_proxy(
        city_template.seed,
        camera_xy,
        frame_index,
    ));
    scene
        .vehicles
        .push(default_vehicle_proxy(city_template.seed, camera_xy));
    scene.collect_cell_material_pages();
    scene
}

fn environment_from_frame(frame_index: u64) -> EnvironmentStateV16 {
    if (frame_index / 9_000).is_multiple_of(2) {
        EnvironmentStateV16::rainy_alley_day()
    } else {
        EnvironmentStateV16::rainy_alley_night()
    }
}

fn build_cell_package_from_chunk(
    cell_id: u64,
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
    world_seed: u64,
) -> BeautyCellPackageV16 {
    let seed = world_seed ^ cell_id;
    let z_ground = 0.0;
    let center_x = (min_x + max_x) * 0.5;
    let center_y = (min_y + max_y) * 0.5;
    let width = (max_x - min_x).abs().max(12.0);
    let depth = (max_y - min_y).abs().max(12.0);

    let road_surface = BeautySurfaceIdV16(cell_id.wrapping_mul(100).wrapping_add(1));
    let facade_left_surface = BeautySurfaceIdV16(cell_id.wrapping_mul(100).wrapping_add(2));
    let facade_right_surface = BeautySurfaceIdV16(cell_id.wrapping_mul(100).wrapping_add(3));

    let bounds = BeautyBoundsV16 {
        min: [min_x, min_y, z_ground - 0.10],
        max: [max_x, max_y, 9.0],
    };

    let road = RoadSplineV16 {
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
        width_meters: (width * 0.42).clamp(4.0, 9.0),
        crown_height_meters: 0.038,
        edge_noise_meters: 0.22,
        material_id: MAT_WET_ASPHALT,
        irregularity: IrregularityRecipeV16::road(seed),
    };

    let curb_left = CurbSegmentV16 {
        object_id: BeautyObjectIdV16(cell_id.wrapping_mul(1000).wrapping_add(10)),
        start_world: [
            center_x - road.width_meters * 0.5,
            min_y + 0.8,
            z_ground + 0.02,
        ],
        end_world: [
            center_x - road.width_meters * 0.5 + seeded_signed(seed, 3) * 0.35,
            max_y - 0.8,
            z_ground + 0.02,
        ],
        height_meters: 0.16,
        bevel_radius_meters: 0.045,
        chip_density_0_to_1: 0.32,
        material_id: MAT_CURB_CONCRETE,
        irregularity: IrregularityRecipeV16::curb(seed ^ 0xC0B1),
    };

    let curb_right = CurbSegmentV16 {
        object_id: BeautyObjectIdV16(cell_id.wrapping_mul(1000).wrapping_add(11)),
        start_world: [
            center_x + road.width_meters * 0.5,
            min_y + 0.8,
            z_ground + 0.02,
        ],
        end_world: [
            center_x + road.width_meters * 0.5 + seeded_signed(seed, 4) * 0.35,
            max_y - 0.8,
            z_ground + 0.02,
        ],
        height_meters: 0.16,
        bevel_radius_meters: 0.045,
        chip_density_0_to_1: 0.32,
        material_id: MAT_CURB_CONCRETE,
        irregularity: IrregularityRecipeV16::curb(seed ^ 0xC0B2),
    };

    let facade_left = FacadeModuleV16 {
        object_id: BeautyObjectIdV16(cell_id.wrapping_mul(1000).wrapping_add(20)),
        bounds: BeautyBoundsV16 {
            min: [min_x + 0.4, min_y + 0.5, z_ground],
            max: [
                center_x - road.width_meters * 0.5 - 0.8,
                max_y - 0.5,
                6.0 + seeded01(seed, 10) * 2.5,
            ],
        },
        material_id: MAT_DIRTY_CONCRETE,
        window_count: 8,
        door_count: 1,
        vent_count: 3,
        inset_depth_meters: 0.16,
        bevel_radius_meters: 0.055,
        dirt_0_to_1: 0.62,
        irregularity: IrregularityRecipeV16::dirty_facade(seed ^ 0x00FA_CADE_0001),
    };

    let facade_right = FacadeModuleV16 {
        object_id: BeautyObjectIdV16(cell_id.wrapping_mul(1000).wrapping_add(21)),
        bounds: BeautyBoundsV16 {
            min: [
                center_x + road.width_meters * 0.5 + 0.8,
                min_y + 0.5,
                z_ground,
            ],
            max: [max_x - 0.4, max_y - 0.5, 5.5 + seeded01(seed, 11) * 3.0],
        },
        material_id: MAT_DIRTY_CONCRETE,
        window_count: 7,
        door_count: 1,
        vent_count: 4,
        inset_depth_meters: 0.14,
        bevel_radius_meters: 0.055,
        dirt_0_to_1: 0.68,
        irregularity: IrregularityRecipeV16::dirty_facade(seed ^ 0x00FA_CADE_0002),
    };

    let pipe = CurveObjectV16 {
        object_id: BeautyObjectIdV16(cell_id.wrapping_mul(1000).wrapping_add(30)),
        points_world: vec![
            [center_x - road.width_meters * 0.55, min_y + 1.0, 2.2],
            [
                center_x - road.width_meters * 0.55 + seeded_signed(seed, 20) * 0.4,
                center_y,
                2.35,
            ],
            [center_x - road.width_meters * 0.55, max_y - 1.0, 2.1],
        ],
        radius_meters: 0.055,
        material_id: MAT_PIPE_DARK_METAL,
        sag_meters: 0.0,
    };

    let cable = CurveObjectV16 {
        object_id: BeautyObjectIdV16(cell_id.wrapping_mul(1000).wrapping_add(31)),
        points_world: vec![
            [center_x - road.width_meters * 0.70, min_y + 1.2, 4.4],
            [center_x + seeded_signed(seed, 21) * 0.35, center_y, 4.1],
            [center_x + road.width_meters * 0.70, max_y - 1.2, 4.35],
        ],
        radius_meters: 0.018,
        material_id: MAT_PIPE_DARK_METAL,
        sag_meters: 0.28,
    };

    let puddle = AnchoredPuddleV16 {
        object_id: BeautyObjectIdV16(cell_id.wrapping_mul(1000).wrapping_add(40)),
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

    let scatter = ScatterFieldV16 {
        object_id: BeautyObjectIdV16(cell_id.wrapping_mul(1000).wrapping_add(50)),
        bounds: BeautyBoundsV16 {
            min: [min_x + 0.5, min_y + 0.5, z_ground],
            max: [max_x - 0.5, max_y - 0.5, z_ground + 0.8],
        },
        density_0_to_1: 0.34,
        item_count_budget: 96,
        seed: seed ^ 0x5CA77E,
    };

    let material_page_requests = vec![
        MaterialPageRequestV16::wet_asphalt(road_surface, MAT_WET_ASPHALT, seed ^ 0xA5FA17),
        MaterialPageRequestV16::dirty_concrete(
            facade_left_surface,
            MAT_DIRTY_CONCRETE,
            seed ^ 0xC011,
        ),
        MaterialPageRequestV16::dirty_concrete(
            facade_right_surface,
            MAT_DIRTY_CONCRETE,
            seed ^ 0xC012,
        ),
    ];

    BeautyCellPackageV16 {
        cell_id,
        bounds,
        roads: vec![road],
        curbs: vec![curb_left, curb_right],
        facades: vec![facade_left, facade_right],
        pipes_and_cables: vec![pipe, cable],
        scatter_fields: vec![scatter],
        puddles: vec![puddle],
        material_page_requests,
        retained_cache_key: retained_cache_key(world_seed, cell_id),
        dirty: true,
    }
}

fn default_human_proxy(world_seed: u64, camera_xy: [f32; 2], frame_index: u64) -> HumanProxyV16 {
    let mut human = HumanProxyV16::default_adult(
        0x484D_0001,
        [camera_xy[0] + 2.2, camera_xy[1] - 3.4, 0.02],
        world_seed ^ 0x484D,
    );
    human.animation_phase_0_to_1 = ((frame_index % 180) as f32 / 180.0).clamp(0.0, 1.0);
    human
}

fn default_vehicle_proxy(world_seed: u64, camera_xy: [f32; 2]) -> VehicleProxyV16 {
    VehicleProxyV16::compact_car_default(
        0xCA9_0001,
        [camera_xy[0] - 3.7, camera_xy[1] + 4.8, 0.02],
        world_seed ^ 0xCA9,
    )
}

fn retained_cache_key(world_seed: u64, cell_id: u64) -> u128 {
    ((world_seed as u128) << 64) ^ cell_id as u128 ^ ASHFALL_V16_CACHE_SALT
}

const ASHFALL_V16_CACHE_SALT: u128 = 0x0A5F_A110_0000_0016;

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
