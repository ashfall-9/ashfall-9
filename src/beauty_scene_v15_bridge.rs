//! Bridge from current Ashfall world/window data to `BeautySceneV15`.
//!
//! This first V15 bridge intentionally keeps the beauty scene small and
//! deterministic. It turns nearby world chunks into player-facing beauty
//! concepts rather than exposing chunk bounds or material placement boxes as the
//! visible world.

use ashfall_rendering::beauty::{
    AnchoredPuddleV15, BeautyBoundsV15, BeautyCellPackageV15, BeautyMaterialIdV15,
    BeautyObjectClassV15, BeautyObjectIdV15, BeautySceneV15, BeautySurfaceIdV15, CurbSegmentV15,
    CurveObjectV15, EnvironmentStateV15, FacadeModuleV15, HumanProxyV15, IrregularityRecipeV15,
    MaterialPageRequestV15, RoadStripV15, ScatterFieldV15, VehicleProxyV15,
};
use ashfall_worldgen::WorldTemplate;

const MAT_WET_ASPHALT: BeautyMaterialIdV15 = BeautyMaterialIdV15(0xA5F_A17);
const MAT_DIRTY_CONCRETE: BeautyMaterialIdV15 = BeautyMaterialIdV15(0xC0A1_C0A1);
const MAT_CURB_CONCRETE: BeautyMaterialIdV15 = BeautyMaterialIdV15(0xC0B_C0B);
const MAT_PIPE_DARK_METAL: BeautyMaterialIdV15 = BeautyMaterialIdV15(0x000D_A110_DA11);
const MAT_HUMAN_SKIN: BeautyMaterialIdV15 = BeautyMaterialIdV15(0x5151_5151);
const MAT_HUMAN_CLOTH: BeautyMaterialIdV15 = BeautyMaterialIdV15(0x000C_107A);
const MAT_VEHICLE_BODY: BeautyMaterialIdV15 = BeautyMaterialIdV15(0xCA9_B0D7);
const MAT_VEHICLE_GLASS: BeautyMaterialIdV15 = BeautyMaterialIdV15(0x91A55);
const MAT_VEHICLE_RUBBER: BeautyMaterialIdV15 = BeautyMaterialIdV15(0x9B_B3B);
const MAT_WATER: BeautyMaterialIdV15 = BeautyMaterialIdV15(0xA11EA);

pub fn build_beauty_scene_v15(
    city_template: &WorldTemplate,
    camera_xy: [f32; 2],
    frame_index: u64,
) -> BeautySceneV15 {
    let mut scene = BeautySceneV15::new(frame_index);
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

    scene
        .humans
        .push(default_human_proxy(city_template.seed, camera_xy));
    scene
        .vehicles
        .push(default_vehicle_proxy(city_template.seed, camera_xy));
    scene.collect_cell_material_pages();
    scene
}

fn environment_from_frame(frame_index: u64) -> EnvironmentStateV15 {
    if (frame_index / 9_000).is_multiple_of(2) {
        EnvironmentStateV15::rainy_alley_day()
    } else {
        EnvironmentStateV15::rainy_alley_night()
    }
}

fn build_cell_package_from_chunk(
    cell_id: u64,
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
    world_seed: u64,
) -> BeautyCellPackageV15 {
    let seed = world_seed ^ cell_id;
    let z_ground = 0.0;
    let center_x = (min_x + max_x) * 0.5;
    let center_y = (min_y + max_y) * 0.5;
    let width = (max_x - min_x).abs().max(8.0);
    let depth = (max_y - min_y).abs().max(8.0);

    let road_surface_id = BeautySurfaceIdV15(cell_id.wrapping_mul(10).wrapping_add(1));
    let bounds = BeautyBoundsV15 {
        min: [min_x, min_y, z_ground - 0.05],
        max: [max_x, max_y, 7.0],
    };

    let road = RoadStripV15 {
        surface_id: road_surface_id,
        centerline: vec![
            [center_x, min_y + depth * 0.08, z_ground],
            [
                center_x + seeded_signed(seed, 1) * 0.8,
                center_y,
                z_ground + 0.015,
            ],
            [center_x, max_y - depth * 0.08, z_ground],
        ],
        width_meters: (width * 0.42).clamp(4.0, 9.0),
        crown_height_meters: 0.035,
        edge_noise_meters: 0.18,
        material_id: MAT_WET_ASPHALT,
        irregularity: IrregularityRecipeV15::road(seed),
    };

    let left_curb_x = center_x - road.width_meters * 0.55;
    let right_curb_x = center_x + road.width_meters * 0.55;
    let curbs = vec![
        CurbSegmentV15 {
            object_id: BeautyObjectIdV15(cell_id.wrapping_mul(10).wrapping_add(2)),
            path: vec![
                [left_curb_x, min_y, z_ground],
                [left_curb_x, max_y, z_ground],
            ],
            height_meters: 0.16,
            width_meters: 0.28,
            bevel_radius_meters: 0.06,
            material_id: MAT_CURB_CONCRETE,
            irregularity: IrregularityRecipeV15::curb(seed ^ 2),
        },
        CurbSegmentV15 {
            object_id: BeautyObjectIdV15(cell_id.wrapping_mul(10).wrapping_add(3)),
            path: vec![
                [right_curb_x, min_y, z_ground],
                [right_curb_x, max_y, z_ground],
            ],
            height_meters: 0.16,
            width_meters: 0.28,
            bevel_radius_meters: 0.06,
            material_id: MAT_CURB_CONCRETE,
            irregularity: IrregularityRecipeV15::curb(seed ^ 3),
        },
    ];

    let facades = vec![
        FacadeModuleV15 {
            object_id: BeautyObjectIdV15(cell_id.wrapping_mul(10).wrapping_add(4)),
            bounds: BeautyBoundsV15 {
                min: [min_x, min_y, z_ground],
                max: [left_curb_x - 0.35, max_y, 6.5 + seeded01(seed, 4) * 4.0],
            },
            floors: 3,
            facade_depth_meters: 0.45,
            bevel_radius_meters: 0.045,
            material_id: MAT_DIRTY_CONCRETE,
            irregularity: IrregularityRecipeV15::facade(seed ^ 4),
        },
        FacadeModuleV15 {
            object_id: BeautyObjectIdV15(cell_id.wrapping_mul(10).wrapping_add(5)),
            bounds: BeautyBoundsV15 {
                min: [right_curb_x + 0.35, min_y, z_ground],
                max: [max_x, max_y, 7.0 + seeded01(seed, 5) * 5.0],
            },
            floors: 4,
            facade_depth_meters: 0.55,
            bevel_radius_meters: 0.045,
            material_id: MAT_DIRTY_CONCRETE,
            irregularity: IrregularityRecipeV15::facade(seed ^ 5),
        },
    ];

    let pipes_and_cables = vec![
        CurveObjectV15 {
            object_id: BeautyObjectIdV15(cell_id.wrapping_mul(10).wrapping_add(6)),
            object_class: BeautyObjectClassV15::Pipe,
            points: vec![
                [left_curb_x - 0.55, min_y, 2.2],
                [left_curb_x - 0.45, center_y, 2.45],
                [left_curb_x - 0.62, max_y, 2.1],
            ],
            radius_meters: 0.055,
            material_id: MAT_PIPE_DARK_METAL,
            irregularity: IrregularityRecipeV15::facade(seed ^ 6),
        },
        CurveObjectV15 {
            object_id: BeautyObjectIdV15(cell_id.wrapping_mul(10).wrapping_add(7)),
            object_class: BeautyObjectClassV15::Cable,
            points: vec![
                [right_curb_x + 0.5, min_y, 3.6],
                [right_curb_x + seeded_signed(seed, 7) * 0.25, center_y, 3.25],
                [right_curb_x + 0.6, max_y, 3.5],
            ],
            radius_meters: 0.018,
            material_id: MAT_PIPE_DARK_METAL,
            irregularity: IrregularityRecipeV15::facade(seed ^ 7),
        },
    ];

    let scatter_fields = vec![ScatterFieldV15 {
        field_id: cell_id.wrapping_mul(10).wrapping_add(8),
        bounds: BeautyBoundsV15 {
            min: [left_curb_x, min_y, z_ground],
            max: [right_curb_x, max_y, z_ground + 0.35],
        },
        object_class: BeautyObjectClassV15::Trash,
        density_per_square_meter: 0.045,
        min_radius_meters: 0.025,
        max_radius_meters: 0.22,
        deterministic_seed: seed ^ 8,
    }];

    let puddles = vec![AnchoredPuddleV15 {
        object_id: BeautyObjectIdV15(cell_id.wrapping_mul(10).wrapping_add(9)),
        receiver_surface_id: road_surface_id,
        center_world: [
            center_x + seeded_signed(seed, 9) * road.width_meters * 0.25,
            center_y,
            z_ground + 0.006,
        ],
        normal_world: normalize3([
            0.02 * seeded_signed(seed, 10),
            0.03 * seeded_signed(seed, 11),
            1.0,
        ]),
        radius_x_meters: 0.8 + seeded01(seed, 12) * 1.1,
        radius_y_meters: 0.35 + seeded01(seed, 13) * 0.65,
        water_depth_meters: 0.006 + seeded01(seed, 14) * 0.025,
        edge_feather_meters: 0.18,
        z_bias_meters: 0.006,
        material_id: MAT_WATER,
    }];

    let material_page_requests = MaterialPageRequestV15::wet_asphalt(
        road_surface_id.0,
        [min_x, min_y, z_ground - 0.02],
        [max_x, max_y, z_ground + 0.03],
        seed,
    );

    BeautyCellPackageV15 {
        cell_id,
        bounds,
        road_strips: vec![road],
        curbs,
        facades,
        pipes_and_cables,
        scatter_fields,
        puddles,
        material_page_requests,
        retained_cache_key: ((world_seed as u128) << 64) ^ cell_id as u128,
        dirty: true,
    }
}

fn default_human_proxy(seed: u64, camera_xy: [f32; 2]) -> HumanProxyV15 {
    HumanProxyV15 {
        object_id: BeautyObjectIdV15(0xABCD_0010),
        root_position: [camera_xy[0] + 3.5, camera_xy[1] + 5.0, 0.0],
        height_meters: 1.74 + seeded_signed(seed, 20) * 0.08,
        shoulder_width_meters: 0.43,
        hip_width_meters: 0.34,
        head_radius_meters: 0.115,
        clothing_material_id: MAT_HUMAN_CLOTH,
        skin_material_id: MAT_HUMAN_SKIN,
        irregularity: IrregularityRecipeV15::human(seed ^ 20),
    }
}

fn default_vehicle_proxy(seed: u64, camera_xy: [f32; 2]) -> VehicleProxyV15 {
    VehicleProxyV15 {
        object_id: BeautyObjectIdV15(0xABCD_0020),
        bounds: BeautyBoundsV15 {
            min: [camera_xy[0] - 5.2, camera_xy[1] + 7.0, 0.0],
            max: [camera_xy[0] - 0.9, camera_xy[1] + 8.9, 1.55],
        },
        wheel_count: 4,
        cabin_glass_ratio: 0.22,
        panel_seam_density: 0.55,
        body_material_id: MAT_VEHICLE_BODY,
        glass_material_id: MAT_VEHICLE_GLASS,
        rubber_material_id: MAT_VEHICLE_RUBBER,
        irregularity: IrregularityRecipeV15::vehicle(seed ^ 30),
    }
}

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

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len <= f32::EPSILON {
        [0.0, 0.0, 1.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}
