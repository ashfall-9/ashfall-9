use std::fs::{File, create_dir_all};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use ashfall_rendering::beauty_v20::{
    BeautyCellPackageV20, BeautyMaterialIdV20, BeautySurfaceIdV20, BoundsV20, HumanPoseStateV20,
    HumanProxyV20, NaturalEnvironmentV20, SurfaceTextureRecipeV20, Vec3V20, WorldBiomeV20,
    sample_material_v20,
};

use crate::beauty_scene_v20_bridge::{BeautySceneBuildContextV20, build_beauty_scene_v20};

const GOLDEN_CAPTURE_WIDTH: u32 = 960;
const GOLDEN_CAPTURE_HEIGHT: u32 = 540;
const GROUND_TOP_FRACTION: f32 = 0.28;

pub fn capture_beauty_v20_golden_scenes(output_dir: impl AsRef<Path>) -> io::Result<Vec<PathBuf>> {
    let output_dir = output_dir.as_ref();
    create_dir_all(output_dir)?;

    let scene = build_beauty_scene_v20(BeautySceneBuildContextV20::default());
    let captures = [
        (WorldBiomeV20::City, "ashfall_v20_city.bmp"),
        (WorldBiomeV20::NatureReserve, "ashfall_v20_nature.bmp"),
        (WorldBiomeV20::Landfill, "ashfall_v20_landfill.bmp"),
    ];

    let mut paths = Vec::new();
    for (biome, file_name) in captures {
        let Some(cell) = scene.cells.iter().find(|cell| cell.biome == biome) else {
            continue;
        };
        let mut image = RgbImage::new(GOLDEN_CAPTURE_WIDTH, GOLDEN_CAPTURE_HEIGHT);
        render_v20_golden_cell(
            &mut image,
            cell,
            scene.scene_id,
            scene.environment,
            &scene.material_recipes,
        );
        let path = output_dir.join(file_name);
        image.write_bmp(&path)?;
        paths.push(path);
    }

    Ok(paths)
}

fn render_v20_golden_cell(
    image: &mut RgbImage,
    cell: &BeautyCellPackageV20,
    seed: u64,
    environment: NaturalEnvironmentV20,
    recipes: &[SurfaceTextureRecipeV20],
) {
    draw_sky(image, cell.biome, environment, seed ^ cell.cell_id);
    draw_ground(image, cell, recipes, seed ^ 0x9017);

    for terrain in &cell.terrain {
        let min = Vec3V20::new(
            terrain.center.x - terrain.size_meters[0] * 0.5,
            terrain.center.y - terrain.size_meters[1] * 0.5,
            0.0,
        );
        let max = Vec3V20::new(
            terrain.center.x + terrain.size_meters[0] * 0.5,
            terrain.center.y + terrain.size_meters[1] * 0.5,
            0.0,
        );
        let base = material_color(
            recipes,
            terrain.surface_id,
            terrain.material_id,
            [terrain.center.x, terrain.center.y],
            seed ^ terrain.surface_id.0,
            match cell.biome {
                WorldBiomeV20::NatureReserve => [62, 83, 48],
                WorldBiomeV20::Landfill => [85, 72, 55],
                _ => [91, 88, 79],
            },
        );
        image.fill_irregular_world_patch(
            cell.bounds,
            min,
            max,
            base,
            match cell.biome {
                WorldBiomeV20::NatureReserve => 0.50,
                WorldBiomeV20::Landfill => 0.56,
                _ => 0.42,
            },
            seed ^ terrain.material_id.0,
        );
    }

    for road in &cell.roads {
        for pair in road.control_points.windows(2) {
            let road_color = material_color(
                recipes,
                road.surface_id,
                road.material_id,
                [pair[0].x, pair[0].y],
                seed ^ road.surface_id.0,
                [45, 49, 46],
            );
            image.world_line(
                cell.bounds,
                pair[0],
                pair[1],
                road.width_meters * 9.5,
                road_color,
                0.95,
            );
            image.world_line(cell.bounds, pair[0], pair[1], 2.0, [133, 128, 105], 0.34);
        }
    }

    for curb in &cell.curbs {
        let curb_color = material_color(
            recipes,
            curb.surface_id,
            curb.material_id,
            [curb.start.x, curb.start.y],
            seed ^ curb.surface_id.0,
            [126, 118, 104],
        );
        image.world_line(
            cell.bounds,
            curb.start,
            curb.end,
            curb.radius_meters * 28.0,
            curb_color,
            0.86,
        );
    }

    for facade in &cell.facades {
        let min = facade.origin;
        let max = Vec3V20::new(
            facade.origin.x + facade.width_meters,
            facade.origin.y + facade.depth_meters,
            0.0,
        );
        let facade_color = material_color(
            recipes,
            facade.surface_id,
            facade.material_id,
            [facade.origin.x, facade.origin.y],
            seed ^ facade.surface_id.0,
            [95, 92, 82],
        );
        image.fill_world_rect(cell.bounds, min, max, facade_color, 0.92);
        for floor in 0..4 {
            let y = facade.origin.y + 0.08 + floor as f32 * 0.08;
            image.world_line(
                cell.bounds,
                Vec3V20::new(facade.origin.x + 0.3, y, 0.0),
                Vec3V20::new(facade.origin.x + facade.width_meters - 0.3, y, 0.0),
                1.4,
                [42, 53, 56],
                0.62,
            );
        }
    }

    for curve in &cell.curves {
        let curve_color = material_color(
            recipes,
            curve.surface_id,
            curve.material_id,
            [
                curve.points.first().map_or(0.0, |point| point.x),
                curve.points.first().map_or(0.0, |point| point.y),
            ],
            seed ^ curve.surface_id.0,
            [67, 70, 66],
        );
        for pair in curve.points.windows(2) {
            image.world_line(
                cell.bounds,
                pair[0],
                pair[1],
                (curve.radius_meters * 26.0).max(1.2),
                curve_color,
                0.78,
            );
        }
    }

    for water in &cell.water_films {
        let (x, y) = image.project(cell.bounds, water.center);
        let radius = (water.radius_meters * 12.0).max(5.0);
        image.fill_irregular_water(
            [x, y],
            [
                radius * (1.4 + water.edge_irregularity_0_to_1 * 0.22),
                radius * (0.55 + water.edge_irregularity_0_to_1 * 0.12),
            ],
            Brush::new([64, 96, 105], 0.42),
            seed ^ water.surface_id.0,
        );
        image.line(
            x - (radius * 0.42) as i32,
            y - 2,
            x + (radius * 0.30) as i32,
            y - 1,
            1.0,
            Brush::new([184, 203, 203], 0.16),
        );
    }

    for plant in &cell.plants {
        let (x, y) = image.project(cell.bounds, plant.root_position);
        let h = (plant.height_meters * 34.0).clamp(4.0, 20.0);
        let plant_color = material_color(
            recipes,
            plant.surface_id,
            plant.material_id,
            [plant.root_position.x, plant.root_position.y],
            seed ^ plant.variation_seed,
            [58, 94, 45],
        );
        let tip_x = x - (plant.bend_0_to_1 * 7.0) as i32;
        let tip_y = y - h as i32;
        image.line(x, y, tip_x, tip_y, 1.5, Brush::new([75, 58, 36], 0.82));
        let leaf_count = (plant.leaf_density_0_to_1 * 7.0).ceil().clamp(2.0, 7.0) as usize;
        for leaf in 0..leaf_count {
            let leaf_seed = seed ^ plant.variation_seed ^ (leaf as u64 * 0x4EAF);
            let t = (leaf as f32 + 0.35) / leaf_count as f32;
            let base_x = x as f32 + (tip_x - x) as f32 * t;
            let base_y = y as f32 + (tip_y - y) as f32 * t;
            let side = if leaf % 2 == 0 { -1.0 } else { 1.0 };
            let reach = 4.0 + stable_unit(leaf_seed, 1) * 8.0 + h * 0.10;
            let dy = -1.5 + stable_unit(leaf_seed, 2) * 4.0;
            image.line(
                base_x as i32,
                base_y as i32,
                (base_x + side * reach) as i32,
                (base_y + dy) as i32,
                1.1,
                Brush::new(plant_color, 0.64),
            );
            image.fill_irregular_blob(
                [
                    (base_x + side * reach * 0.78) as i32,
                    (base_y + dy * 0.7) as i32,
                ],
                [3.2 + h * 0.035, 1.6 + h * 0.020],
                Brush::new(plant_color, 0.44),
                leaf_seed,
            );
        }
        if plant.height_meters > 0.55 {
            for lobe in 0..3 {
                let lobe_seed = seed ^ plant.variation_seed ^ ((lobe as u64 + 17) * 0x891);
                image.fill_irregular_blob(
                    [
                        tip_x + (stable_unit(lobe_seed, 1) * 12.0 - 6.0) as i32,
                        tip_y + (stable_unit(lobe_seed, 2) * 8.0 - 2.0) as i32,
                    ],
                    [
                        6.0 + stable_unit(lobe_seed, 3) * 5.0,
                        3.0 + stable_unit(lobe_seed, 4) * 3.0,
                    ],
                    Brush::new(plant_color, 0.42),
                    lobe_seed,
                );
            }
        }
    }

    for stone in &cell.stones {
        let (x, y) = image.project(cell.bounds, stone.center);
        let radius = (stone.radius_meters * 30.0).max(3.0);
        let stone_color = material_color(
            recipes,
            stone.surface_id,
            stone.material_id,
            [stone.center.x, stone.center.y],
            seed ^ stone.surface_id.0,
            [103, 101, 91],
        );
        image.fill_ellipse(x, y, radius * 1.24, radius * 0.84, stone_color, 0.92);
        image.fill_ellipse(
            x - 2,
            y - 1,
            radius * 0.42,
            radius * 0.22,
            brighten(stone_color, 1.34),
            0.42,
        );
    }

    for prop in &cell.landfill_props {
        let (x, y) = image.project(cell.bounds, prop.center);
        let width = (prop.size_meters[0] * 28.0).max(5.0);
        let height = (prop.size_meters[1] * 28.0).max(4.0);
        let color = material_color(
            recipes,
            prop.surface_id,
            prop.material_id,
            [prop.center.x, prop.center.y],
            seed ^ prop.surface_id.0,
            if prop.rust_or_stain_0_to_1 > 0.45 {
                [113, 77, 54]
            } else {
                [77, 91, 99]
            },
        );
        let is_rubber = prop.material_id == BeautyMaterialIdV20(0x700E_2020);
        let is_fabric = prop.material_id == BeautyMaterialIdV20(0xFAB1_2020);
        let is_cardboard = prop.material_id == BeautyMaterialIdV20(0xCA2D_2020);
        let is_rusted_metal = prop.material_id == BeautyMaterialIdV20(0xB057_2020);
        let prop_seed =
            seed ^ prop.surface_id.0 ^ prop.material_id.0 ^ prop.center.x.to_bits() as u64;
        if is_rubber {
            image.fill_ellipse(x, y, width * 0.58, height * 0.58, darken(color, 0.48), 0.92);
            image.fill_ellipse(x, y, width * 0.28, height * 0.25, [35, 34, 29], 0.82);
            image.line(
                x - 5,
                y + 1,
                x + 6,
                y - 1,
                1.3,
                Brush::new([23, 23, 20], 0.74),
            );
        } else if is_fabric || is_cardboard || is_rusted_metal {
            let tilt = stable_unit(prop_seed, 1) * 4.0 - 2.0;
            let points = [
                (
                    (x as f32 - width * 0.54 + stable_unit(prop_seed, 2) * 4.0) as i32,
                    (y as f32 - height * 0.46 + tilt) as i32,
                ),
                (
                    (x as f32 + width * 0.50 + stable_unit(prop_seed, 3) * 4.0) as i32,
                    (y as f32 - height * 0.38 - tilt) as i32,
                ),
                (
                    (x as f32 + width * 0.45 + stable_unit(prop_seed, 4) * 5.0) as i32,
                    (y as f32 + height * 0.46 + tilt) as i32,
                ),
                (
                    (x as f32 - width * 0.48 + stable_unit(prop_seed, 5) * 5.0) as i32,
                    (y as f32 + height * 0.40 - tilt) as i32,
                ),
            ];
            image.fill_polygon(&points, color, 0.84);
            image.line(
                points[0].0,
                (points[0].1 + points[3].1) / 2,
                points[1].0,
                (points[1].1 + points[2].1) / 2,
                1.0,
                Brush::new(darken(color, 0.58), 0.44),
            );
            if is_rusted_metal {
                image.line(
                    points[3].0,
                    points[3].1,
                    points[1].0,
                    points[1].1,
                    1.2,
                    Brush::new([139, 70, 38], 0.50),
                );
            }
        } else {
            image.fill_irregular_blob(
                [x, y],
                [width * 0.55, height * 0.48],
                Brush::new(color, 0.86),
                prop_seed,
            );
            image.line(
                x - (width * 0.26) as i32,
                y - 1,
                x + (width * 0.22) as i32,
                y + 1,
                1.0,
                Brush::new(brighten(color, 1.32), 0.30),
            );
        }
    }

    for vehicle in &cell.vehicles {
        let (x, y) = image.project(cell.bounds, vehicle.world_position);
        let w = vehicle.width_meters * 18.0;
        let l = vehicle.length_meters * 13.0;
        let body = material_color(
            recipes,
            vehicle.materials.body_surface,
            vehicle.materials.body_material,
            [vehicle.world_position.x, vehicle.world_position.y],
            seed ^ vehicle.materials.body_surface.0,
            [60, 70, 72],
        );
        image.fill_ellipse(x, y, l * 0.55, w * 0.44, body, 0.96);
        image.fill_ellipse(x - 5, y - 1, l * 0.20, w * 0.30, [118, 143, 151], 0.76);
        image.fill_rect_centered(x + 9, y, l * 0.34, w * 0.42, darken(body, 0.74), 0.7);
        image.fill_rect_centered(x - 14, y - w as i32 / 2 - 2, 7.0, 3.0, [21, 22, 21], 0.95);
        image.fill_rect_centered(x + 14, y - w as i32 / 2 - 2, 7.0, 3.0, [21, 22, 21], 0.95);
        image.fill_rect_centered(x - 14, y + w as i32 / 2 + 2, 7.0, 3.0, [21, 22, 21], 0.95);
        image.fill_rect_centered(x + 14, y + w as i32 / 2 + 2, 7.0, 3.0, [21, 22, 21], 0.95);
        image.fill_rect_centered(x + l as i32 / 2 - 2, y - 6, 2.0, 3.0, [220, 204, 144], 0.82);
        image.fill_rect_centered(x + l as i32 / 2 - 2, y + 6, 2.0, 3.0, [220, 204, 144], 0.82);
    }

    for human in &cell.humans {
        draw_capture_human(image, cell.bounds, recipes, human, seed);
    }

    draw_texture_grain(image, seed ^ cell.cell_id);
}

fn draw_capture_human(
    image: &mut RgbImage,
    bounds: BoundsV20,
    recipes: &[SurfaceTextureRecipeV20],
    human: &HumanProxyV20,
    seed: u64,
) {
    let (x, foot_y) = image.project(bounds, human.world_position);
    let pose_seed = seed ^ human.entity_id ^ human.world_position.x.to_bits() as u64;
    let phase = if stable_unit(pose_seed, 1) < 0.5 {
        -1.0
    } else {
        1.0
    };
    let (stride, arm_swing, crouch, forward_lean) = match human.pose {
        HumanPoseStateV20::Idle => (0.10, 0.14, 0.0, 0.0),
        HumanPoseStateV20::Walking => (0.52, 0.56, 0.02, 0.02),
        HumanPoseStateV20::Running => (0.86, 0.82, 0.03, 0.07),
        HumanPoseStateV20::Crouched => (0.26, 0.34, 0.22, 0.05),
        HumanPoseStateV20::Sitting => (0.34, 0.18, 0.30, -0.01),
    };
    let height = (human.proportions.height_meters * 15.0 * (1.0 - crouch * 0.38)).clamp(18.0, 32.0);
    let scale = height / human.proportions.height_meters.max(1.0);
    let facing_side = human.facing_yaw_radians.sin();
    let frontness = human.facing_yaw_radians.cos().abs();
    let width_factor = lerp(0.72, 1.08, frontness);
    let shoulder_half =
        (human.proportions.shoulder_width_meters * scale * 0.52 * width_factor).clamp(3.8, 7.6);
    let pelvis_half =
        (human.proportions.pelvis_width_meters * scale * 0.50 * width_factor).clamp(2.8, 5.6);
    let head_rx =
        (human.proportions.head_radius_meters * scale * 1.55 * width_factor).clamp(2.5, 4.5);
    let head_ry = (head_rx * 1.22).clamp(3.0, 5.3);
    let side_bias = facing_side * 1.7;
    let lean_px = forward_lean * height * phase + side_bias * 0.35;

    let skin = material_color(
        recipes,
        human.materials.skin_surface,
        human.materials.skin_material,
        [human.world_position.x, human.world_position.y],
        seed ^ human.materials.skin_surface.0,
        [176, 126, 94],
    );
    let cloth = material_color(
        recipes,
        human.materials.clothing_surface,
        human.materials.clothing_material,
        [human.world_position.x, human.world_position.y],
        seed ^ human.materials.clothing_surface.0,
        [46, 66, 82],
    );
    let hair = material_color(
        recipes,
        human.materials.hair_surface,
        human.materials.hair_material,
        [human.world_position.x, human.world_position.y],
        seed ^ human.materials.hair_surface.0,
        [36, 27, 21],
    );
    let shoe = material_color(
        recipes,
        human.materials.shoe_surface,
        human.materials.shoe_material,
        [human.world_position.x, human.world_position.y],
        seed ^ human.materials.shoe_surface.0,
        [30, 30, 28],
    );

    let ankle_y = foot_y as f32 - 1.0;
    let hip_y = foot_y as f32 - height * 0.45;
    let knee_y = foot_y as f32 - height * 0.22;
    let waist_y = foot_y as f32 - height * 0.53;
    let chest_y = foot_y as f32 - height * 0.66;
    let shoulder_y = foot_y as f32 - height * 0.73;
    let neck_y = foot_y as f32 - height * 0.80;
    let head_y = foot_y as f32 - height * 0.90;
    let torso_x = x as f32 + lean_px;
    let stride_px = height * 0.18 * stride;

    image.fill_ellipse(
        x,
        foot_y + 1,
        shoulder_half + stride_px * 0.70 + 4.0,
        2.4,
        [18, 17, 15],
        0.22,
    );

    if human.has_legs {
        let left_hip = (torso_x - pelvis_half, hip_y);
        let right_hip = (torso_x + pelvis_half, hip_y + 0.4);
        let left_knee = (
            torso_x - pelvis_half * 0.55 - stride_px * phase * 0.40 + side_bias,
            knee_y,
        );
        let right_knee = (
            torso_x + pelvis_half * 0.55 + stride_px * phase * 0.36 + side_bias,
            knee_y + stride * 1.0,
        );
        let left_foot = (
            x as f32 - pelvis_half - stride_px * phase + side_bias,
            ankle_y,
        );
        let right_foot = (
            x as f32 + pelvis_half + stride_px * phase + side_bias,
            ankle_y + stride * 0.6,
        );
        let trouser_shadow = darken(cloth, 0.58);
        draw_capture_limb(
            image,
            left_hip,
            left_knee,
            1.85,
            darken(cloth, 0.82),
            trouser_shadow,
            pose_seed ^ 0x101,
        );
        draw_capture_limb(
            image,
            left_knee,
            left_foot,
            1.65,
            darken(cloth, 0.78),
            trouser_shadow,
            pose_seed ^ 0x102,
        );
        draw_capture_limb(
            image,
            right_hip,
            right_knee,
            1.90,
            cloth,
            trouser_shadow,
            pose_seed ^ 0x103,
        );
        draw_capture_limb(
            image,
            right_knee,
            right_foot,
            1.70,
            darken(cloth, 0.90),
            trouser_shadow,
            pose_seed ^ 0x104,
        );
        if human.has_feet {
            let foot_rx = (human.proportions.foot_length_meters * scale * 0.72).clamp(2.5, 4.6);
            image.fill_ellipse(
                left_foot.0 as i32,
                left_foot.1 as i32 + 1,
                foot_rx,
                1.35,
                shoe,
                0.92,
            );
            image.fill_ellipse(
                right_foot.0 as i32,
                right_foot.1 as i32 + 1,
                foot_rx,
                1.35,
                brighten(shoe, 1.12),
                0.92,
            );
            image.line(
                left_foot.0 as i32 - 2,
                left_foot.1 as i32,
                left_foot.0 as i32 + 2,
                left_foot.1 as i32,
                0.55,
                Brush::new(brighten(shoe, 1.55), 0.22),
            );
        }
    }

    if human.has_pelvis {
        image.fill_ellipse(
            torso_x as i32,
            hip_y as i32,
            pelvis_half + 1.2,
            3.0,
            darken(cloth, 0.76),
            0.90,
        );
        image.line(
            (torso_x - pelvis_half) as i32,
            hip_y as i32 - 1,
            (torso_x + pelvis_half) as i32,
            hip_y as i32 - 1,
            0.65,
            Brush::new(brighten(cloth, 1.28), 0.24),
        );
    }

    if human.has_torso {
        let torso_points = [
            (
                (torso_x - shoulder_half) as i32,
                (shoulder_y + stable_unit(pose_seed, 6)) as i32,
            ),
            (
                (torso_x + shoulder_half) as i32,
                (shoulder_y + stable_unit(pose_seed, 7)) as i32,
            ),
            ((torso_x + pelvis_half * 0.78) as i32, waist_y as i32),
            ((torso_x + pelvis_half * 0.58) as i32, hip_y as i32 + 1),
            ((torso_x - pelvis_half * 0.58) as i32, hip_y as i32 + 1),
            ((torso_x - pelvis_half * 0.78) as i32, waist_y as i32),
        ];
        image.fill_polygon(&torso_points, cloth, 0.92);
        image.fill_irregular_blob(
            [torso_x as i32, ((chest_y + waist_y) * 0.5) as i32],
            [shoulder_half * 0.72, height * 0.13],
            Brush::new(brighten(cloth, 1.08), 0.34),
            pose_seed ^ 0x501,
        );
        image.line(
            (torso_x - shoulder_half * 0.55) as i32,
            (chest_y - 1.0) as i32,
            (torso_x + shoulder_half * 0.35) as i32,
            (waist_y + 1.0) as i32,
            0.55,
            Brush::new(darken(cloth, 0.55), 0.34),
        );
        image.line(
            (torso_x + shoulder_half * 0.42) as i32,
            chest_y as i32,
            (torso_x + pelvis_half * 0.24) as i32,
            hip_y as i32,
            0.55,
            Brush::new(brighten(cloth, 1.32), 0.22),
        );
        for fold in 0..3 {
            let fold_seed = pose_seed ^ (fold as u64 * 0x611);
            let fx = torso_x
                + (stable_unit(fold_seed, 1) - 0.5) * shoulder_half * 1.25
                + side_bias * 0.2;
            image.line(
                fx as i32,
                (chest_y + 1.0) as i32,
                (fx + stable_unit(fold_seed, 2) * 2.0 - 1.0) as i32,
                (waist_y + 1.0) as i32,
                0.45,
                Brush::new(darken(cloth, 0.62), 0.22),
            );
        }
    }

    if human.has_arms {
        let left_shoulder = (torso_x - shoulder_half, shoulder_y + 1.0);
        let right_shoulder = (torso_x + shoulder_half, shoulder_y + 1.0);
        let left_elbow = (
            torso_x - shoulder_half - 2.2 - stride_px * phase * arm_swing * 0.48,
            waist_y - height * 0.04,
        );
        let right_elbow = (
            torso_x + shoulder_half + 2.2 + stride_px * phase * arm_swing * 0.42,
            waist_y - height * 0.03,
        );
        let left_hand = (
            left_elbow.0 - 2.2 - stride_px * phase * arm_swing * 0.34,
            hip_y + height * 0.10,
        );
        let right_hand = (
            right_elbow.0 + 2.0 + stride_px * phase * arm_swing * 0.30,
            hip_y + height * 0.08,
        );
        draw_capture_limb(
            image,
            left_shoulder,
            left_elbow,
            1.35,
            darken(cloth, 0.80),
            darken(cloth, 0.52),
            pose_seed ^ 0x201,
        );
        draw_capture_limb(
            image,
            right_shoulder,
            right_elbow,
            1.35,
            cloth,
            darken(cloth, 0.56),
            pose_seed ^ 0x202,
        );
        draw_capture_limb(
            image,
            left_elbow,
            left_hand,
            1.05,
            brighten(cloth, 0.94),
            darken(cloth, 0.58),
            pose_seed ^ 0x203,
        );
        draw_capture_limb(
            image,
            right_elbow,
            right_hand,
            1.05,
            brighten(cloth, 1.04),
            darken(cloth, 0.62),
            pose_seed ^ 0x204,
        );
        if human.has_hands {
            image.fill_ellipse(
                left_hand.0 as i32,
                left_hand.1 as i32,
                1.45,
                1.75,
                darken(skin, 0.86),
                0.88,
            );
            image.fill_ellipse(
                right_hand.0 as i32,
                right_hand.1 as i32,
                1.45,
                1.75,
                skin,
                0.88,
            );
        }
    }

    if human.has_head {
        image.fill_rect_centered(
            torso_x as i32,
            neck_y as i32,
            2.1,
            3.4,
            darken(skin, 0.86),
            0.86,
        );
        image.fill_ellipse(
            (torso_x + side_bias * 0.18) as i32,
            head_y as i32,
            head_rx,
            head_ry,
            skin,
            0.94,
        );
        image.fill_ellipse(
            (torso_x - head_rx * 0.92 + side_bias * 0.18) as i32,
            head_y as i32,
            0.85,
            1.25,
            darken(skin, 0.82),
            0.68,
        );
        image.fill_ellipse(
            (torso_x + head_rx * 0.92 + side_bias * 0.18) as i32,
            head_y as i32,
            0.85,
            1.25,
            darken(skin, 0.88),
            0.62,
        );
        image.fill_ellipse(
            (torso_x - head_rx * 0.18) as i32,
            (head_y - head_ry * 0.12) as i32,
            head_rx * 0.34,
            head_ry * 0.20,
            brighten(skin, 1.17),
            0.24,
        );
        image.fill_rect_centered(
            (torso_x + side_bias * 0.30) as i32,
            (head_y + head_ry * 0.18) as i32,
            1.0,
            1.6,
            darken(skin, 0.74),
            0.24,
        );
        image.line(
            (torso_x - head_rx * 0.38) as i32,
            (head_y + head_ry * 0.46) as i32,
            (torso_x + head_rx * 0.36) as i32,
            (head_y + head_ry * 0.43) as i32,
            0.45,
            Brush::new([93, 48, 44], 0.34),
        );
        image.fill_ellipse(
            (torso_x - head_rx * 0.38) as i32,
            (head_y - head_ry * 0.10) as i32,
            0.55,
            0.45,
            [21, 24, 25],
            0.76,
        );
        image.fill_ellipse(
            (torso_x + head_rx * 0.34) as i32,
            (head_y - head_ry * 0.10) as i32,
            0.55,
            0.45,
            [21, 24, 25],
            0.72,
        );
    }

    if human.has_hair_or_hat_silhouette {
        image.fill_ellipse(
            (torso_x + side_bias * 0.22) as i32,
            (head_y - head_ry * 0.78) as i32,
            head_rx * 1.10,
            head_ry * 0.50,
            hair,
            0.90,
        );
        image.fill_ellipse(
            (torso_x - head_rx * 0.65 + side_bias * 0.16) as i32,
            (head_y - head_ry * 0.10) as i32,
            1.2,
            head_ry * 0.74,
            darken(hair, 0.82),
            0.72,
        );
        image.line(
            (torso_x - head_rx * 0.95) as i32,
            (head_y - head_ry * 0.42) as i32,
            (torso_x + head_rx * 0.70) as i32,
            (head_y - head_ry * 0.56) as i32,
            0.55,
            Brush::new(brighten(hair, 1.30), 0.20),
        );
    }
}

fn draw_capture_limb(
    image: &mut RgbImage,
    start: (f32, f32),
    end: (f32, f32),
    radius: f32,
    color: [u8; 3],
    shadow: [u8; 3],
    seed: u64,
) {
    let midpoint = ((start.0 + end.0) * 0.5, (start.1 + end.1) * 0.5);
    image.line(
        start.0 as i32,
        start.1 as i32,
        end.0 as i32,
        end.1 as i32,
        radius,
        Brush::new(color, 0.86),
    );
    image.fill_irregular_blob(
        [midpoint.0 as i32, midpoint.1 as i32],
        [radius * 1.45, radius * 1.08],
        Brush::new(brighten(color, 1.08), 0.20),
        seed,
    );
    image.line(
        (start.0 - 0.6) as i32,
        (start.1 - 0.3) as i32,
        (end.0 - 0.4) as i32,
        (end.1 - 0.2) as i32,
        (radius * 0.34).max(0.45),
        Brush::new(brighten(color, 1.36), 0.20),
    );
    image.line(
        (start.0 + 0.8) as i32,
        (start.1 + 0.4) as i32,
        (end.0 + 0.6) as i32,
        (end.1 + 0.4) as i32,
        (radius * 0.30).max(0.42),
        Brush::new(shadow, 0.18),
    );
}

fn draw_sky(
    image: &mut RgbImage,
    biome: WorldBiomeV20,
    environment: NaturalEnvironmentV20,
    seed: u64,
) {
    let sky_height = (image.height as f32 * GROUND_TOP_FRACTION) as u32;
    let clear = environment.clear_color_rgba();
    let clear_rgb = [
        (clear[0].sqrt() * 255.0).clamp(0.0, 255.0),
        (clear[1].sqrt() * 255.0).clamp(0.0, 255.0),
        (clear[2].sqrt() * 255.0).clamp(0.0, 255.0),
    ];
    for y in 0..sky_height {
        let t = y as f32 / sky_height.max(1) as f32;
        let biome_top = match biome {
            WorldBiomeV20::NatureReserve => [95.0, 126.0, 151.0],
            WorldBiomeV20::Landfill => [119.0, 126.0, 126.0],
            _ => [105.0, 132.0, 154.0],
        };
        let cloud_mute = environment.cloud_coverage_0_to_1.clamp(0.0, 1.0) * 0.28;
        let top = [
            lerp(biome_top[0], clear_rgb[0], 0.55 + cloud_mute),
            lerp(biome_top[1], clear_rgb[1], 0.55 + cloud_mute),
            lerp(biome_top[2], clear_rgb[2], 0.55 + cloud_mute),
        ];
        let horizon = [185.0, 184.0, 164.0];
        let color = [
            lerp(top[0], horizon[0], t),
            lerp(top[1], horizon[1], t),
            lerp(top[2], horizon[2], t),
        ];
        for x in 0..image.width {
            image.set_pixel_u32(x, y, [color[0] as u8, color[1] as u8, color[2] as u8]);
        }
    }

    image.fill_ellipse(
        ((0.70 + environment.sun_direction_world[0] * 0.08) * image.width as f32) as i32,
        ((0.13 - environment.sun_direction_world[1] * 0.03) * image.height as f32) as i32,
        14.0,
        14.0,
        [232, 205, 134],
        0.74,
    );
    image.fill_ellipse(
        ((0.22 + environment.moon_direction_world[0] * 0.05) * image.width as f32) as i32,
        ((0.11 - environment.moon_direction_world[1] * 0.03) * image.height as f32) as i32,
        8.0,
        8.0,
        [205, 214, 216],
        0.20,
    );

    let cloud_count = (4.0 + environment.cloud_coverage_0_to_1.clamp(0.0, 1.0) * 8.0) as u64;
    for cloud in 0..cloud_count {
        let x = (0.12 + stable_unit(seed, cloud * 3) * 0.76) * image.width as f32;
        let y = (0.07 + stable_unit(seed, cloud * 3 + 1) * 0.16) * image.height as f32;
        let w = 38.0 + stable_unit(seed, cloud * 3 + 2) * 70.0;
        let density = environment.cloud_density_0_to_1.clamp(0.0, 1.0);
        image.fill_ellipse(
            x as i32,
            y as i32,
            w,
            7.0 + cloud as f32 * 0.45,
            [211, 216, 211],
            0.10 + density * 0.16,
        );
        image.fill_ellipse(
            x as i32 - 18,
            y as i32 + 5,
            w * 0.58,
            5.0,
            [151, 160, 158],
            0.07 + density * 0.09,
        );
    }
}

fn draw_ground(
    image: &mut RgbImage,
    cell: &BeautyCellPackageV20,
    recipes: &[SurfaceTextureRecipeV20],
    seed: u64,
) {
    let y_start = (image.height as f32 * GROUND_TOP_FRACTION) as u32;
    let (surface_id, material_id, fallback) = match cell.biome {
        WorldBiomeV20::City => (
            BeautySurfaceIdV20(10_001),
            BeautyMaterialIdV20(0xA5FA_2020),
            [69, 66, 58],
        ),
        WorldBiomeV20::NatureReserve => (
            BeautySurfaceIdV20(20_001),
            BeautyMaterialIdV20(0x5011_2020),
            [54, 72, 42],
        ),
        WorldBiomeV20::Landfill => (
            BeautySurfaceIdV20(30_001),
            BeautyMaterialIdV20(0x1A9D_2020),
            [74, 65, 49],
        ),
        _ => (
            BeautySurfaceIdV20(20_001),
            BeautyMaterialIdV20(0x5011_2020),
            [65, 62, 55],
        ),
    };
    for y in y_start..image.height {
        let t = (y - y_start) as f32 / (image.height - y_start).max(1) as f32;
        for x in 0..image.width {
            let n = stable_unit(seed ^ x as u64, y as u64);
            let shade = 0.74 + t * 0.28 + (n - 0.5) * 0.20;
            let uv = [
                cell.bounds.min.x
                    + (x as f32 / image.width.max(1) as f32)
                        * (cell.bounds.max.x - cell.bounds.min.x),
                cell.bounds.min.y
                    + (y as f32 / image.height.max(1) as f32)
                        * (cell.bounds.max.y - cell.bounds.min.y),
            ];
            let base = material_color(recipes, surface_id, material_id, uv, seed, fallback);
            image.set_pixel_u32(
                x,
                y,
                [
                    (base[0] as f32 * shade).clamp(0.0, 255.0) as u8,
                    (base[1] as f32 * shade).clamp(0.0, 255.0) as u8,
                    (base[2] as f32 * shade).clamp(0.0, 255.0) as u8,
                ],
            );
        }
    }
}

fn draw_texture_grain(image: &mut RgbImage, seed: u64) {
    for y in 0..image.height {
        for x in 0..image.width {
            if stable_unit(seed ^ x as u64, y as u64) > 0.992 {
                image.blend_pixel(x as i32, y as i32, [230, 224, 199], 0.12);
            }
        }
    }
}

fn material_color(
    recipes: &[SurfaceTextureRecipeV20],
    surface_id: BeautySurfaceIdV20,
    material_id: BeautyMaterialIdV20,
    uv_meters: [f32; 2],
    seed: u64,
    fallback: [u8; 3],
) -> [u8; 3] {
    let Some(recipe) = recipes
        .iter()
        .find(|recipe| recipe.surface_id == surface_id || recipe.material_id == material_id)
    else {
        return fallback;
    };
    let sample = sample_material_v20(recipe, uv_meters, seed);
    let light = 0.86 + sample.height_0_to_1 * 0.18 - sample.dirt_0_to_1 * 0.10
        + sample.wetness_0_to_1 * 0.05;
    [
        linear_to_u8(sample.base_color_linear[0] * light),
        linear_to_u8(sample.base_color_linear[1] * light),
        linear_to_u8(sample.base_color_linear[2] * light),
    ]
}

fn linear_to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0).sqrt() * 255.0).clamp(0.0, 255.0) as u8
}

fn brighten(color: [u8; 3], factor: f32) -> [u8; 3] {
    [
        (color[0] as f32 * factor).clamp(0.0, 255.0) as u8,
        (color[1] as f32 * factor).clamp(0.0, 255.0) as u8,
        (color[2] as f32 * factor).clamp(0.0, 255.0) as u8,
    ]
}

fn darken(color: [u8; 3], factor: f32) -> [u8; 3] {
    brighten(color, factor)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

fn stable_unit(seed: u64, salt: u64) -> f32 {
    let mut x = seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    ((x >> 40) as f32) / ((1_u64 << 24) as f32)
}

struct RgbImage {
    width: u32,
    height: u32,
    pixels: Vec<[u8; 3]>,
}

#[derive(Clone, Copy)]
struct Brush {
    color: [u8; 3],
    alpha: f32,
}

impl Brush {
    const fn new(color: [u8; 3], alpha: f32) -> Self {
        Self { color, alpha }
    }
}

impl RgbImage {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![[0, 0, 0]; (width * height) as usize],
        }
    }

    fn set_pixel_u32(&mut self, x: u32, y: u32, color: [u8; 3]) {
        let index = (y * self.width + x) as usize;
        self.pixels[index] = color;
    }

    fn blend_pixel(&mut self, x: i32, y: i32, color: [u8; 3], alpha: f32) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let index = (y as u32 * self.width + x as u32) as usize;
        let alpha = alpha.clamp(0.0, 1.0);
        let old = self.pixels[index];
        self.pixels[index] = [
            lerp(old[0] as f32, color[0] as f32, alpha) as u8,
            lerp(old[1] as f32, color[1] as f32, alpha) as u8,
            lerp(old[2] as f32, color[2] as f32, alpha) as u8,
        ];
    }

    fn project(&self, bounds: BoundsV20, point: Vec3V20) -> (i32, i32) {
        let margin_x = self.width as f32 * 0.08;
        let ground_top = self.height as f32 * GROUND_TOP_FRACTION;
        let ground_height = self.height as f32 - ground_top - self.height as f32 * 0.06;
        let nx = ((point.x - bounds.min.x) / (bounds.max.x - bounds.min.x)).clamp(0.0, 1.0);
        let ny = ((point.y - bounds.min.y) / (bounds.max.y - bounds.min.y)).clamp(0.0, 1.0);
        let x = margin_x + nx * (self.width as f32 - margin_x * 2.0);
        let y = ground_top + (1.0 - ny) * ground_height;
        (x as i32, y as i32)
    }

    fn fill_world_rect(
        &mut self,
        bounds: BoundsV20,
        min: Vec3V20,
        max: Vec3V20,
        color: [u8; 3],
        alpha: f32,
    ) {
        let (x0, y0) = self.project(bounds, min);
        let (x1, y1) = self.project(bounds, max);
        for y in y0.min(y1)..=y0.max(y1) {
            for x in x0.min(x1)..=x0.max(x1) {
                self.blend_pixel(x, y, color, alpha);
            }
        }
    }

    fn fill_irregular_world_patch(
        &mut self,
        bounds: BoundsV20,
        min: Vec3V20,
        max: Vec3V20,
        color: [u8; 3],
        alpha: f32,
        seed: u64,
    ) {
        let (x0, y0) = self.project(bounds, min);
        let (x1, y1) = self.project(bounds, max);
        let min_x = x0.min(x1);
        let max_x = x0.max(x1);
        let min_y = y0.min(y1);
        let max_y = y0.max(y1);
        let width = (max_x - min_x).max(1) as f32;
        let height = (max_y - min_y).max(1) as f32;
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let u = (x - min_x) as f32 / width;
                let v = (y - min_y) as f32 / height;
                let edge_distance = u.min(1.0 - u).min(v).min(1.0 - v);
                let n = stable_unit(seed ^ x as u64, y as u64);
                if edge_distance < 0.030 && n < 0.48 {
                    continue;
                }
                let edge_alpha = if edge_distance < 0.12 {
                    (edge_distance / 0.12 * 0.72 + n * 0.28).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                let shade = 0.82 + (stable_unit(seed ^ y as u64, x as u64) - 0.5) * 0.24;
                self.blend_pixel(x, y, brighten(color, shade), alpha * edge_alpha);
            }
        }
    }

    fn world_line(
        &mut self,
        bounds: BoundsV20,
        a: Vec3V20,
        b: Vec3V20,
        radius: f32,
        color: [u8; 3],
        alpha: f32,
    ) {
        let (x0, y0) = self.project(bounds, a);
        let (x1, y1) = self.project(bounds, b);
        self.line(x0, y0, x1, y1, radius, Brush::new(color, alpha));
    }

    fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, radius: f32, brush: Brush) {
        let dx = (x1 - x0) as f32;
        let dy = (y1 - y0) as f32;
        let steps = dx.abs().max(dy.abs()).max(1.0) as i32;
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let x = x0 as f32 + dx * t;
            let y = y0 as f32 + dy * t;
            self.fill_ellipse(x as i32, y as i32, radius, radius, brush.color, brush.alpha);
        }
    }

    fn fill_rect_centered(
        &mut self,
        cx: i32,
        cy: i32,
        width: f32,
        height: f32,
        color: [u8; 3],
        alpha: f32,
    ) {
        let half_w = width.max(1.0) * 0.5;
        let half_h = height.max(1.0) * 0.5;
        for y in (cy as f32 - half_h) as i32..=(cy as f32 + half_h) as i32 {
            for x in (cx as f32 - half_w) as i32..=(cx as f32 + half_w) as i32 {
                self.blend_pixel(x, y, color, alpha);
            }
        }
    }

    fn fill_ellipse(&mut self, cx: i32, cy: i32, rx: f32, ry: f32, color: [u8; 3], alpha: f32) {
        let rx = rx.max(1.0);
        let ry = ry.max(1.0);
        for y in (cy as f32 - ry) as i32..=(cy as f32 + ry) as i32 {
            for x in (cx as f32 - rx) as i32..=(cx as f32 + rx) as i32 {
                let dx = (x as f32 - cx as f32) / rx;
                let dy = (y as f32 - cy as f32) / ry;
                let d = dx * dx + dy * dy;
                if d <= 1.0 {
                    self.blend_pixel(x, y, color, alpha * (1.0 - d * 0.22));
                }
            }
        }
    }

    fn fill_irregular_blob(&mut self, center: [i32; 2], radii: [f32; 2], brush: Brush, seed: u64) {
        let [cx, cy] = center;
        let rx = radii[0].max(1.0);
        let ry = radii[1].max(1.0);
        for y in (cy as f32 - ry) as i32..=(cy as f32 + ry) as i32 {
            for x in (cx as f32 - rx) as i32..=(cx as f32 + rx) as i32 {
                let dx = (x as f32 - cx as f32) / rx;
                let dy = (y as f32 - cy as f32) / ry;
                let n = stable_unit(seed ^ x as u64, y as u64);
                let ruffle = 1.0 + (n - 0.5) * 0.34;
                let d = dx * dx + dy * dy;
                if d <= ruffle {
                    let shade = 0.86 + (stable_unit(seed ^ y as u64, x as u64) - 0.5) * 0.22;
                    self.blend_pixel(
                        x,
                        y,
                        brighten(brush.color, shade),
                        brush.alpha * (1.0 - d * 0.18),
                    );
                }
            }
        }
    }

    fn fill_polygon(&mut self, points: &[(i32, i32)], color: [u8; 3], alpha: f32) {
        if points.len() < 3 {
            return;
        }
        let Some(min_x) = points.iter().map(|point| point.0).min() else {
            return;
        };
        let Some(max_x) = points.iter().map(|point| point.0).max() else {
            return;
        };
        let Some(min_y) = points.iter().map(|point| point.1).min() else {
            return;
        };
        let Some(max_y) = points.iter().map(|point| point.1).max() else {
            return;
        };

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let mut inside = false;
                let mut prev = points[points.len() - 1];
                for &current in points {
                    let (xi, yi) = (current.0 as f32, current.1 as f32);
                    let (xj, yj) = (prev.0 as f32, prev.1 as f32);
                    if (yi > py) != (yj > py)
                        && px < (xj - xi) * (py - yi) / (yj - yi).max(0.0001) + xi
                    {
                        inside = !inside;
                    }
                    prev = current;
                }
                if inside {
                    self.blend_pixel(x, y, color, alpha);
                }
            }
        }
    }

    fn fill_irregular_water(&mut self, center: [i32; 2], radii: [f32; 2], brush: Brush, seed: u64) {
        let [cx, cy] = center;
        let rx = radii[0].max(1.0);
        let ry = radii[1].max(1.0);
        for y in (cy as f32 - ry) as i32..=(cy as f32 + ry) as i32 {
            for x in (cx as f32 - rx) as i32..=(cx as f32 + rx) as i32 {
                let dx = (x as f32 - cx as f32) / rx;
                let dy = (y as f32 - cy as f32) / ry;
                let n = stable_unit(seed ^ x as u64, y as u64);
                let edge = 1.0 + (n - 0.5) * 0.26;
                let d = dx * dx + dy * dy;
                if d <= edge {
                    let ripple = 0.72 + stable_unit(seed ^ y as u64, x as u64) * 0.28;
                    self.blend_pixel(x, y, brush.color, brush.alpha * ripple * (1.0 - d * 0.15));
                }
            }
        }
    }

    fn write_bmp(&self, path: &Path) -> io::Result<()> {
        let row_stride = (self.width * 3).div_ceil(4) * 4;
        let pixel_bytes = row_stride * self.height;
        let file_bytes = 14 + 40 + pixel_bytes;
        let mut file = File::create(path)?;

        file.write_all(b"BM")?;
        file.write_all(&file_bytes.to_le_bytes())?;
        file.write_all(&[0, 0, 0, 0])?;
        file.write_all(&(54_u32).to_le_bytes())?;

        file.write_all(&(40_u32).to_le_bytes())?;
        file.write_all(&(self.width as i32).to_le_bytes())?;
        file.write_all(&(self.height as i32).to_le_bytes())?;
        file.write_all(&(1_u16).to_le_bytes())?;
        file.write_all(&(24_u16).to_le_bytes())?;
        file.write_all(&(0_u32).to_le_bytes())?;
        file.write_all(&pixel_bytes.to_le_bytes())?;
        file.write_all(&(2_835_i32).to_le_bytes())?;
        file.write_all(&(2_835_i32).to_le_bytes())?;
        file.write_all(&(0_u32).to_le_bytes())?;
        file.write_all(&(0_u32).to_le_bytes())?;

        let padding = vec![0; (row_stride - self.width * 3) as usize];
        for y in (0..self.height).rev() {
            for x in 0..self.width {
                let [r, g, b] = self.pixels[(y * self.width + x) as usize];
                file.write_all(&[b, g, r])?;
            }
            file.write_all(&padding)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn capture_human_painter_builds_anatomical_sprite_instead_of_stick_figure() {
        let mut image = RgbImage::new(96, 96);
        let bounds = BoundsV20 {
            min: Vec3V20::new(-1.0, -1.0, 0.0),
            max: Vec3V20::new(1.0, 1.0, 0.0),
        };
        let mut human = HumanProxyV20::city_pedestrian(42, Vec3V20::new(0.0, 0.0, 0.0));
        human.pose = HumanPoseStateV20::Walking;
        human.facing_yaw_radians = 0.35;

        draw_capture_human(&mut image, bounds, &[], &human, 0xC0FFEE);

        let mut colored_pixels = 0;
        let mut unique_colors = BTreeSet::new();
        let mut min_x = image.width as i32;
        let mut min_y = image.height as i32;
        let mut max_x = 0_i32;
        let mut max_y = 0_i32;

        for y in 0..image.height {
            for x in 0..image.width {
                let pixel = image.pixels[(y * image.width + x) as usize];
                if pixel == [0, 0, 0] {
                    continue;
                }
                colored_pixels += 1;
                unique_colors.insert(pixel);
                min_x = min_x.min(x as i32);
                min_y = min_y.min(y as i32);
                max_x = max_x.max(x as i32);
                max_y = max_y.max(y as i32);
            }
        }

        assert!(
            colored_pixels > 230,
            "capture human should have filled anatomical mass, got {colored_pixels} colored pixels"
        );
        assert!(
            max_x - min_x >= 18,
            "capture human should include shoulders, arms, hands, and feet"
        );
        assert!(
            max_y - min_y >= 25,
            "capture human should include full head-to-foot height"
        );
        assert!(
            unique_colors.len() >= 18,
            "capture human should layer skin, clothes, hair, shoes, folds, and face detail"
        );
    }
}
