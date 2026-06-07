use std::fs::{File, create_dir_all};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use ashfall_rendering::beauty_v21::{
    BeautyCellPackageV21, BeautyMaterialIdV21, BeautySurfaceIdV21, Bounds3V21, CurbSegmentV21,
    FacadeModuleV21, GroundedWaterFilmV21, HumanPoseStateV21, HumanProxyV21, LandfillPropKindV21,
    LandfillPropV21, NaturalEnvironmentV21, PlantInstanceV21, RoadPatchV21, StoneInstanceV21,
    SurfaceTextureRecipeV21, Vec3V21, VehicleKindV21, VehicleProxyV21, WorldBiomeV21,
    sample_material_v21,
};

use crate::beauty_scene_v21_bridge::{BeautySceneBuildContextV21, build_beauty_scene_v21};

const GOLDEN_CAPTURE_WIDTH: u32 = 1280;
const GOLDEN_CAPTURE_HEIGHT: u32 = 720;
const GROUND_TOP_FRACTION: f32 = 0.34;

#[derive(Clone, Debug, PartialEq)]
pub struct BeautyV21GoldenCaptureReport {
    pub image_paths: Vec<PathBuf>,
    pub timing_path: PathBuf,
    pub timings: Vec<BeautyV21CaptureTiming>,
    pub scene_build_ms: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BeautyV21CaptureTiming {
    pub biome: WorldBiomeV21,
    pub render_ms: f32,
    pub write_ms: f32,
    pub total_ms: f32,
    pub visible_instances: usize,
    pub material_recipe_count: usize,
}

pub fn capture_beauty_v21_golden_scenes(
    output_dir: impl AsRef<Path>,
) -> io::Result<BeautyV21GoldenCaptureReport> {
    let output_dir = output_dir.as_ref();
    create_dir_all(output_dir)?;

    let scene_start = Instant::now();
    let scene = build_beauty_scene_v21(BeautySceneBuildContextV21::default());
    let scene_build_ms = elapsed_ms(scene_start);

    let captures = [
        (WorldBiomeV21::City, "ashfall_v21_city.bmp"),
        (WorldBiomeV21::NatureReserve, "ashfall_v21_nature.bmp"),
        (WorldBiomeV21::Landfill, "ashfall_v21_landfill.bmp"),
    ];

    let mut image_paths = Vec::new();
    let mut timings = Vec::new();
    for (biome, file_name) in captures {
        let Some(cell) = scene.cells.iter().find(|cell| cell.biome == biome) else {
            continue;
        };

        let render_start = Instant::now();
        let mut image = RgbImage::new(GOLDEN_CAPTURE_WIDTH, GOLDEN_CAPTURE_HEIGHT);
        render_v21_golden_cell(
            &mut image,
            cell,
            scene.scene_id,
            scene.environment,
            &scene.material_recipes,
        );
        let render_ms = elapsed_ms(render_start);

        let path = output_dir.join(file_name);
        let write_start = Instant::now();
        image.write_bmp(&path)?;
        let write_ms = elapsed_ms(write_start);

        image_paths.push(path);
        timings.push(BeautyV21CaptureTiming {
            biome,
            render_ms,
            write_ms,
            total_ms: render_ms + write_ms,
            visible_instances: cell.visible_content_count(),
            material_recipe_count: scene.material_recipes.len(),
        });
    }

    let timing_path = output_dir.join("ashfall_v21_frame_timings.txt");
    write_timing_report(
        &timing_path,
        scene.scene_id,
        scene.frame_budget.target_frame_ms,
        scene_build_ms,
        &timings,
    )?;

    Ok(BeautyV21GoldenCaptureReport {
        image_paths,
        timing_path,
        timings,
        scene_build_ms,
    })
}

fn render_v21_golden_cell(
    image: &mut RgbImage,
    cell: &BeautyCellPackageV21,
    seed: u64,
    environment: NaturalEnvironmentV21,
    recipes: &[SurfaceTextureRecipeV21],
) {
    draw_sky(image, cell.biome, environment, seed ^ cell.cell_id);
    draw_ground(image, cell, recipes, seed ^ 0xA511_2021);

    for terrain in &cell.terrain {
        let base = material_color(
            recipes,
            terrain.soil_surface,
            terrain.soil_material,
            [terrain.bounds.min.x, terrain.bounds.min.y],
            seed ^ terrain.id,
            match cell.biome {
                WorldBiomeV21::NatureReserve => [55, 75, 46],
                WorldBiomeV21::Landfill => [88, 74, 54],
                WorldBiomeV21::City => [72, 69, 62],
            },
        );
        image.fill_irregular_world_patch(
            cell.bounds,
            terrain.bounds.min,
            terrain.bounds.max,
            base,
            0.32 + terrain.height_variation_meters.clamp(0.0, 0.8) * 0.35,
            seed ^ terrain.id ^ 0x7011,
        );
    }

    for road in cell.roads.iter().take(12) {
        draw_road(image, cell.bounds, recipes, road, seed ^ road.id);
    }

    for curb in cell.curbs.iter().take(24) {
        draw_curb(image, cell.bounds, recipes, curb, seed ^ curb.id);
    }

    for facade in cell.facades.iter().take(12) {
        draw_facade(image, cell.bounds, recipes, facade, seed ^ facade.id);
    }

    for curve in cell.curves.iter().take(32) {
        let color = material_color(
            recipes,
            curve.surface,
            curve.material,
            curve
                .points
                .first()
                .map_or([0.0, 0.0], |point| [point.x, point.y]),
            seed ^ curve.id,
            [58, 61, 57],
        );
        for pair in curve.points.windows(2) {
            image.world_line(
                cell.bounds,
                pair[0],
                pair[1],
                (curve.radius_meters * 42.0).max(1.0),
                darken(color, 0.72),
                0.76,
            );
            image.world_line(
                cell.bounds,
                Vec3V21::new(pair[0].x, pair[0].y, pair[0].z + curve.sag_0_to_1 * 0.16),
                Vec3V21::new(pair[1].x, pair[1].y, pair[1].z + curve.sag_0_to_1 * 0.10),
                (curve.radius_meters * 18.0).max(0.8),
                brighten(color, 1.18),
                0.30,
            );
        }
    }

    for water in &cell.water_films {
        draw_grounded_water(image, cell.bounds, water, seed ^ water.id);
    }

    for plant in cell.plants.iter().take(96) {
        draw_plant(image, cell.bounds, recipes, plant, seed ^ plant.clump_seed);
    }

    for stone in cell.stones.iter().take(64) {
        draw_stone(image, cell.bounds, recipes, stone, seed ^ stone.id);
    }

    for prop in cell.landfill_props.iter().take(72) {
        draw_landfill_prop(image, cell.bounds, recipes, prop, seed ^ prop.id);
    }

    for vehicle in cell.vehicles.iter().take(8) {
        draw_vehicle(
            image,
            cell.bounds,
            recipes,
            vehicle,
            seed ^ vehicle.entity_id,
        );
    }

    for human in cell.humans.iter().take(12) {
        draw_human(image, cell.bounds, recipes, human, seed ^ human.entity_id);
    }

    draw_texture_grain(image, seed ^ cell.cell_id ^ 0xD37A_2021);
}

fn draw_sky(
    image: &mut RgbImage,
    biome: WorldBiomeV21,
    environment: NaturalEnvironmentV21,
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
        let upper = match biome {
            WorldBiomeV21::NatureReserve => [91.0, 122.0, 146.0],
            WorldBiomeV21::Landfill => [113.0, 122.0, 121.0],
            WorldBiomeV21::City => [100.0, 126.0, 149.0],
        };
        let cloud_mute = environment.cloud_coverage_0_to_1.clamp(0.0, 1.0) * 0.24;
        let top = [
            lerp(upper[0], clear_rgb[0], 0.48 + cloud_mute),
            lerp(upper[1], clear_rgb[1], 0.48 + cloud_mute),
            lerp(upper[2], clear_rgb[2], 0.48 + cloud_mute),
        ];
        let horizon = [188.0, 187.0, 166.0];
        let haze = environment.fog_density_0_to_1.clamp(0.0, 1.0) * 0.22;
        let color = [
            lerp(lerp(top[0], horizon[0], t), 196.0, haze),
            lerp(lerp(top[1], horizon[1], t), 196.0, haze),
            lerp(lerp(top[2], horizon[2], t), 186.0, haze),
        ];
        for x in 0..image.width {
            image.set_pixel_u32(x, y, [color[0] as u8, color[1] as u8, color[2] as u8]);
        }
    }

    let sun_x = ((0.70 + environment.sun_direction_world[0] * 0.09) * image.width as f32) as i32;
    let sun_y = ((0.14 - environment.sun_direction_world[1] * 0.04) * image.height as f32) as i32;
    image.fill_ellipse(sun_x, sun_y, 12.0, 12.0, [239, 211, 139], 0.78);
    image.fill_ellipse(sun_x - 2, sun_y - 2, 5.0, 5.0, [252, 237, 179], 0.42);

    let moon_x = ((0.19 + environment.moon_direction_world[0] * 0.04) * image.width as f32) as i32;
    let moon_y = ((0.12 - environment.moon_direction_world[1] * 0.03) * image.height as f32) as i32;
    image.fill_ellipse(moon_x, moon_y, 6.0, 6.0, [205, 214, 216], 0.16);

    let cloud_count = (5.0 + environment.cloud_coverage_0_to_1.clamp(0.0, 1.0) * 10.0) as u64;
    for cloud in 0..cloud_count {
        let x = (0.10 + stable_unit(seed, cloud * 5) * 0.80) * image.width as f32;
        let y = (0.06 + stable_unit(seed, cloud * 5 + 1) * 0.20) * image.height as f32;
        let w = 42.0 + stable_unit(seed, cloud * 5 + 2) * 100.0;
        let density = environment.cloud_density_0_to_1.clamp(0.0, 1.0);
        let shadow = 0.06 + density * 0.10;
        image.fill_irregular_blob(
            [x as i32, y as i32],
            [w, 8.0 + stable_unit(seed, cloud * 5 + 3) * 5.0],
            Brush::new([214, 218, 214], 0.10 + density * 0.17),
            seed ^ cloud,
        );
        image.fill_irregular_blob(
            [x as i32 - (w * 0.18) as i32, y as i32 + 7],
            [w * 0.56, 5.5],
            Brush::new([143, 153, 151], shadow),
            seed ^ cloud ^ 0xC10D,
        );
    }
}

fn draw_ground(
    image: &mut RgbImage,
    cell: &BeautyCellPackageV21,
    recipes: &[SurfaceTextureRecipeV21],
    seed: u64,
) {
    let y_start = (image.height as f32 * GROUND_TOP_FRACTION) as u32;
    let (surface_id, material_id, fallback) = match cell.biome {
        WorldBiomeV21::City => (
            BeautySurfaceIdV21(10_001),
            BeautyMaterialIdV21(1),
            [58, 58, 53],
        ),
        WorldBiomeV21::NatureReserve => (
            BeautySurfaceIdV21(20_001),
            BeautyMaterialIdV21(3),
            [48, 68, 41],
        ),
        WorldBiomeV21::Landfill => (
            BeautySurfaceIdV21(20_001),
            BeautyMaterialIdV21(3),
            [78, 65, 47],
        ),
    };
    for y in y_start..image.height {
        let t = (y - y_start) as f32 / (image.height - y_start).max(1) as f32;
        for x in 0..image.width {
            let world = image.unproject_ground(cell.bounds, x, y);
            let n = stable_unit(seed ^ x as u64, y as u64);
            let ridge = stable_unit(seed ^ (x as u64 / 9), y as u64 / 7);
            let shade = 0.72 + t * 0.33 + (n - 0.5) * 0.16 + (ridge - 0.5) * 0.08;
            let base = material_color(
                recipes,
                surface_id,
                material_id,
                [world.x, world.y],
                seed ^ 0x9E21,
                fallback,
            );
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

fn draw_road(
    image: &mut RgbImage,
    bounds: Bounds3V21,
    recipes: &[SurfaceTextureRecipeV21],
    road: &RoadPatchV21,
    seed: u64,
) {
    for pair in road.centerline.windows(2) {
        let color = material_color(
            recipes,
            road.surface,
            road.material,
            [pair[0].x, pair[0].y],
            seed,
            [43, 47, 44],
        );
        image.world_line(
            bounds,
            pair[0],
            pair[1],
            road.width_meters * 9.8,
            color,
            0.96,
        );
        image.world_line(
            bounds,
            pair[0],
            pair[1],
            road.width_meters * 2.6,
            brighten(color, 1.16),
            0.12 + road.wet_highlight_alpha(),
        );
        for stripe in 0..7 {
            let t0 = stripe as f32 / 7.0;
            let t1 = (stripe as f32 + 0.38) / 7.0;
            let a = mix_vec3(pair[0], pair[1], t0);
            let b = mix_vec3(pair[0], pair[1], t1);
            image.world_line(bounds, a, b, 2.0, [153, 147, 119], 0.28);
        }
    }
}

fn draw_curb(
    image: &mut RgbImage,
    bounds: Bounds3V21,
    recipes: &[SurfaceTextureRecipeV21],
    curb: &CurbSegmentV21,
    seed: u64,
) {
    let color = material_color(
        recipes,
        curb.surface,
        curb.material,
        [curb.start.x, curb.start.y],
        seed,
        [121, 113, 101],
    );
    image.world_line(
        bounds,
        curb.start,
        curb.end,
        (curb.height_meters * 34.0).max(3.2),
        color,
        0.90,
    );
    image.world_line(
        bounds,
        Vec3V21::new(
            curb.start.x,
            curb.start.y,
            curb.start.z + curb.bevel_radius_meters,
        ),
        Vec3V21::new(
            curb.end.x,
            curb.end.y,
            curb.end.z + curb.bevel_radius_meters,
        ),
        (curb.bevel_radius_meters * 130.0).max(1.0),
        brighten(color, 1.24),
        0.48,
    );
    for chip in 0..5 {
        if stable_unit(seed, chip) > curb.chip_density_0_to_1 {
            continue;
        }
        let point = mix_vec3(curb.start, curb.end, stable_unit(seed ^ 0xC411, chip));
        let (x, y) = image.project(bounds, point);
        image.fill_irregular_blob(
            [x, y],
            [2.0 + stable_unit(seed, chip + 10) * 3.0, 1.2],
            Brush::new(darken(color, 0.48), 0.60),
            seed ^ chip,
        );
    }
}

fn draw_facade(
    image: &mut RgbImage,
    bounds: Bounds3V21,
    recipes: &[SurfaceTextureRecipeV21],
    facade: &FacadeModuleV21,
    seed: u64,
) {
    let base = material_color(
        recipes,
        facade.surface,
        facade.material,
        [facade.bounds.min.x, facade.bounds.min.y],
        seed,
        [92, 88, 78],
    );
    let (x0, y0) = image.project(bounds, facade.bounds.min);
    let (x1, y1_ground) = image.project(
        bounds,
        Vec3V21::new(facade.bounds.max.x, facade.bounds.max.y, 0.0),
    );
    let height_px = ((facade.bounds.max.z - facade.bounds.min.z) * 16.0).max(28.0) as i32;
    let min_x = x0.min(x1);
    let max_x = x0.max(x1);
    let ground_y = y0.min(y1_ground);
    let top_y = ground_y - height_px;

    image.fill_rect(min_x, top_y, max_x, ground_y, base, 0.96);
    image.fill_rect(min_x, top_y, max_x, top_y + 5, brighten(base, 1.18), 0.70);
    image.line(
        min_x,
        ground_y,
        max_x,
        ground_y,
        2.0,
        Brush::new(darken(base, 0.56), 0.60),
    );

    let width = (max_x - min_x).max(16);
    let window_count = facade.window_count.max(1) as i32;
    for window in 0..window_count {
        let col_x = min_x + 8 + (window * (width - 16) / window_count.max(1));
        let row_y = top_y + 12 + (window % 3) * ((height_px - 24).max(12) / 3);
        let glass = [36, 50, 54];
        image.fill_rect(col_x, row_y, col_x + 10, row_y + 8, glass, 0.74);
        image.fill_rect(col_x, row_y, col_x + 10, row_y + 1, [136, 150, 145], 0.34);
    }
    for door in 0..facade.door_count as i32 {
        let door_x = min_x + 10 + door * 18;
        image.fill_rect(
            door_x,
            ground_y - 22,
            door_x + 11,
            ground_y,
            [45, 38, 32],
            0.86,
        );
    }
    for pipe in 0..facade.pipe_count as i32 {
        let pipe_x = min_x + 4 + pipe * 9;
        image.line(
            pipe_x,
            top_y + 2,
            pipe_x + 2,
            ground_y,
            1.2,
            Brush::new([50, 52, 49], 0.62),
        );
    }
    let dirt_alpha = facade.dirt_0_to_1.clamp(0.0, 1.0) * 0.22;
    for drip in 0..12 {
        let x = min_x + (stable_unit(seed, drip) * width as f32) as i32;
        let y = top_y + 8 + (stable_unit(seed ^ 0xD117, drip) * height_px as f32 * 0.68) as i32;
        image.line(
            x,
            y,
            x + (stable_unit(seed, drip + 40) * 4.0) as i32 - 2,
            y + 8 + (stable_unit(seed, drip + 80) * 18.0) as i32,
            0.8,
            Brush::new([35, 32, 25], dirt_alpha),
        );
    }
}

fn draw_grounded_water(
    image: &mut RgbImage,
    bounds: Bounds3V21,
    water: &GroundedWaterFilmV21,
    seed: u64,
) {
    let points = water
        .polygon
        .iter()
        .map(|point| image.project(bounds, *point))
        .collect::<Vec<_>>();
    image.fill_polygon(
        &points,
        [55, 88, 96],
        0.34 + water.depth_meters.clamp(0.0, 0.08),
    );
    for edge in points.windows(2) {
        image.line(
            edge[0].0,
            edge[0].1,
            edge[1].0,
            edge[1].1,
            1.0 + water.edge_softness_0_to_1 * 1.6,
            Brush::new([145, 168, 166], 0.18),
        );
    }
    if let (Some(first), Some(last)) = (points.first(), points.last()) {
        image.line(
            first.0,
            first.1,
            last.0,
            last.1,
            1.0 + water.edge_softness_0_to_1 * 1.6,
            Brush::new([145, 168, 166], 0.18),
        );
    }
    for ripple in 0..5 {
        let t = stable_unit(seed, ripple);
        let Some(a) = water.polygon.first() else {
            return;
        };
        let Some(b) = water.polygon.get(2).or_else(|| water.polygon.last()) else {
            return;
        };
        let p = mix_vec3(*a, *b, t);
        let (x, y) = image.project(bounds, p);
        image.line(
            x - 12,
            y + ripple as i32,
            x + 16,
            y + ripple as i32 - 1,
            0.8,
            Brush::new([201, 212, 205], 0.12 * (1.0 - water.roughness_0_to_1)),
        );
    }
}

fn draw_plant(
    image: &mut RgbImage,
    bounds: Bounds3V21,
    recipes: &[SurfaceTextureRecipeV21],
    plant: &PlantInstanceV21,
    seed: u64,
) {
    let (x, y) = image.project(bounds, plant.position);
    let leaf = material_color(
        recipes,
        plant.leaf_surface,
        plant.leaf_material,
        [plant.position.x, plant.position.y],
        seed,
        [45, 96, 39],
    );
    let h = (plant.height_meters * 62.0).clamp(5.0, 34.0);
    let radius = (plant.radius_meters * 140.0).clamp(3.0, 16.0);
    image.line(
        x,
        y,
        x + (stable_unit(seed, 1) * 5.0) as i32 - 2,
        y - h as i32,
        1.0,
        Brush::new([41, 56, 27], 0.68),
    );
    let leaf_count = 4 + (plant.height_meters * 12.0) as u64;
    for leaf_index in 0..leaf_count {
        let angle = stable_unit(seed, leaf_index) * std::f32::consts::TAU;
        let spread = radius * (0.45 + stable_unit(seed ^ 0x1EAF, leaf_index) * 0.85);
        let lx = x + (angle.cos() * spread) as i32;
        let ly = y - (h * (0.28 + stable_unit(seed, leaf_index + 80) * 0.70)) as i32
            + (angle.sin() * spread * 0.42) as i32;
        image.fill_irregular_blob(
            [lx, ly],
            [radius * 0.72, radius * 0.28],
            Brush::new(
                brighten(leaf, 0.78 + stable_unit(seed, leaf_index + 130) * 0.54),
                0.76,
            ),
            seed ^ leaf_index,
        );
    }
}

fn draw_stone(
    image: &mut RgbImage,
    bounds: Bounds3V21,
    recipes: &[SurfaceTextureRecipeV21],
    stone: &StoneInstanceV21,
    seed: u64,
) {
    let (x, y) = image.project(bounds, stone.position);
    let color = material_color(
        recipes,
        stone.surface,
        stone.material,
        [stone.position.x, stone.position.y],
        seed,
        [92, 90, 83],
    );
    let radius = (stone.radius_meters * 95.0).clamp(3.0, 14.0);
    image.fill_irregular_blob(
        [x, y],
        [
            radius * (1.2 + stone.irregularity_0_to_1 * 0.3),
            radius * 0.72,
        ],
        Brush::new(color, 0.86),
        seed,
    );
    image.fill_irregular_blob(
        [x - (radius * 0.25) as i32, y - (radius * 0.25) as i32],
        [radius * 0.42, radius * 0.22],
        Brush::new(brighten(color, 1.25), 0.26),
        seed ^ 0x5710,
    );
}

fn draw_landfill_prop(
    image: &mut RgbImage,
    bounds: Bounds3V21,
    recipes: &[SurfaceTextureRecipeV21],
    prop: &LandfillPropV21,
    seed: u64,
) {
    let color = material_color(
        recipes,
        prop.surface,
        prop.material,
        [prop.position.x, prop.position.y],
        seed,
        [104, 96, 74],
    );
    let (x, y) = image.project(bounds, prop.position);
    let width = ((prop.bounds.max.x - prop.bounds.min.x) * 19.0).clamp(9.0, 42.0);
    let height = ((prop.bounds.max.y - prop.bounds.min.y) * 18.0).clamp(5.0, 28.0);
    match prop.kind {
        LandfillPropKindV21::PlasticSheet => {
            image.fill_irregular_blob(
                [x, y],
                [width * 0.82, height * 0.66],
                Brush::new(brighten(color, 1.22), 0.74),
                seed,
            );
            image.line(
                x - width as i32 / 2,
                y,
                x + width as i32 / 2,
                y - 2,
                1.0,
                Brush::new([199, 191, 166], 0.22),
            );
        }
        LandfillPropKindV21::Cardboard => {
            image.fill_polygon(
                &[
                    (x - width as i32 / 2, y - height as i32 / 2),
                    (x + width as i32 / 2, y - height as i32 / 2 + 2),
                    (x + width as i32 / 2 - 2, y + height as i32 / 2),
                    (x - width as i32 / 2 + 2, y + height as i32 / 2),
                ],
                [123, 92, 55],
                0.82,
            );
            image.line(
                x,
                y - height as i32 / 2,
                x - 2,
                y + height as i32 / 2,
                1.0,
                Brush::new([72, 53, 34], 0.36),
            );
        }
        LandfillPropKindV21::RustedMetalPanel => {
            image.fill_polygon(
                &[
                    (x - width as i32 / 2, y - height as i32 / 2),
                    (x + width as i32 / 2, y - height as i32 / 2 - 2),
                    (x + width as i32 / 2 - 4, y + height as i32 / 2),
                    (x - width as i32 / 2 + 1, y + height as i32 / 2 + 2),
                ],
                [96, 62, 36],
                0.86,
            );
            for seam in 0..3 {
                image.line(
                    x - width as i32 / 2,
                    y - height as i32 / 2 + seam * height as i32 / 3,
                    x + width as i32 / 2,
                    y - height as i32 / 2 + seam * height as i32 / 3 + 1,
                    0.8,
                    Brush::new([46, 40, 35], 0.30),
                );
            }
        }
        LandfillPropKindV21::Tire => {
            image.fill_ellipse(x, y, width * 0.42, height.max(8.0), [34, 34, 31], 0.88);
            image.fill_ellipse(
                x,
                y,
                width * 0.22,
                height.max(8.0) * 0.48,
                [76, 70, 57],
                0.82,
            );
        }
        LandfillPropKindV21::FabricBundle
        | LandfillPropKindV21::BrokenGlass
        | LandfillPropKindV21::Crate => {
            image.fill_irregular_blob(
                [x, y],
                [width * 0.52, height * 0.70],
                Brush::new(darken(color, 0.86), 0.82),
                seed,
            );
        }
    }
    let dirt = prop.dirt_0_to_1.clamp(0.0, 1.0) * 0.24;
    image.fill_irregular_blob(
        [x, y + height as i32 / 3],
        [width * 0.44, height * 0.22],
        Brush::new([46, 39, 28], dirt),
        seed ^ 0xD121,
    );
}

fn draw_vehicle(
    image: &mut RgbImage,
    bounds: Bounds3V21,
    recipes: &[SurfaceTextureRecipeV21],
    vehicle: &VehicleProxyV21,
    seed: u64,
) {
    let (x, y) = image.project(bounds, vehicle.world_position);
    let body = material_color(
        recipes,
        vehicle.materials.body_surface,
        vehicle.materials.body_material,
        [vehicle.world_position.x, vehicle.world_position.y],
        seed,
        match vehicle.kind {
            VehicleKindV21::LandfillLoader => [142, 118, 58],
            _ => [47, 58, 64],
        },
    );
    let length = (vehicle.proportions.length_meters * 15.0).clamp(38.0, 92.0);
    let width = (vehicle.proportions.width_meters * 18.0).clamp(20.0, 52.0);
    image.fill_irregular_blob(
        [x, y],
        [length * 0.52, width * 0.46],
        Brush::new(body, 0.96),
        seed,
    );
    image.fill_irregular_blob(
        [x - (length * 0.07) as i32, y - (width * 0.08) as i32],
        [length * 0.22, width * 0.30],
        Brush::new([56, 78, 84], 0.76),
        seed ^ 0x61A5,
    );
    image.line(
        x - length as i32 / 2,
        y,
        x + length as i32 / 2,
        y,
        0.8,
        Brush::new(darken(body, 0.45), 0.42),
    );
    image.line(
        x - length as i32 / 4,
        y - width as i32 / 2,
        x - length as i32 / 4,
        y + width as i32 / 2,
        0.8,
        Brush::new(darken(body, 0.50), 0.38),
    );
    let wheel_r = (vehicle.proportions.wheel_radius_meters * 12.0).clamp(3.0, 8.0);
    for wx in [-0.36_f32, 0.34] {
        for wy in [-0.43_f32, 0.43] {
            image.fill_ellipse(
                x + (wx * length) as i32,
                y + (wy * width) as i32,
                wheel_r,
                wheel_r * 0.72,
                [22, 22, 20],
                0.88,
            );
        }
    }
    image.fill_ellipse(
        x + length as i32 / 2 - 5,
        y - width as i32 / 5,
        3.2,
        1.8,
        [221, 204, 139],
        0.46,
    );
    image.fill_ellipse(
        x + length as i32 / 2 - 5,
        y + width as i32 / 5,
        3.2,
        1.8,
        [221, 204, 139],
        0.46,
    );
    if vehicle.kind == VehicleKindV21::LandfillLoader {
        image.fill_polygon(
            &[
                (x + length as i32 / 2 - 2, y - width as i32 / 2),
                (x + length as i32 / 2 + 26, y - width as i32 / 3),
                (x + length as i32 / 2 + 22, y + width as i32 / 3),
                (x + length as i32 / 2 - 2, y + width as i32 / 2),
            ],
            [99, 82, 43],
            0.82,
        );
    }
}

fn draw_human(
    image: &mut RgbImage,
    bounds: Bounds3V21,
    recipes: &[SurfaceTextureRecipeV21],
    human: &HumanProxyV21,
    seed: u64,
) {
    let (x, foot_y) = image.project(bounds, human.world_position);
    let height = (human.proportions.height_meters * 32.0).clamp(38.0, 70.0);
    let head_h = (human.proportions.head_height_meters / human.proportions.height_meters * height)
        .clamp(5.0, 10.0);
    let shoulder_w = (human.proportions.shoulder_width_meters / human.proportions.height_meters
        * height)
        .clamp(10.0, 20.0);
    let pelvis_w = (human.proportions.pelvis_width_meters / human.proportions.height_meters
        * height)
        .clamp(7.0, 16.0);
    let skin = material_color(
        recipes,
        human.materials.skin_surface,
        human.materials.skin_material,
        [human.world_position.x, human.world_position.y],
        seed,
        [169, 119, 91],
    );
    let cloth = material_color(
        recipes,
        human.materials.clothing_surface,
        human.materials.clothing_material,
        [human.world_position.x, human.world_position.y],
        seed ^ 0xC107,
        [40, 45, 54],
    );
    let hair = darken(skin, 0.27);
    let top_y = foot_y - height as i32;
    let head_y = top_y + head_h as i32 / 2;
    let neck_y = top_y + head_h as i32 + 4;
    let shoulder_y = neck_y + 7;
    let waist_y = top_y + (height * 0.55) as i32;
    let knee_y = top_y + (height * 0.78) as i32;

    let stride = match human.pose {
        HumanPoseStateV21::Walking => 5,
        HumanPoseStateV21::Running => 8,
        HumanPoseStateV21::Crouched | HumanPoseStateV21::Sitting => 2,
        HumanPoseStateV21::Idle => 3,
    };

    image.line(
        x - (shoulder_w * 0.45) as i32,
        shoulder_y + 2,
        x - (shoulder_w * 0.64) as i32 - stride,
        waist_y - 5,
        3.0,
        Brush::new(cloth, 0.88),
    );
    image.line(
        x + (shoulder_w * 0.45) as i32,
        shoulder_y + 2,
        x + (shoulder_w * 0.64) as i32 + stride,
        waist_y - 7,
        3.0,
        Brush::new(cloth, 0.88),
    );
    image.fill_ellipse(
        x - (shoulder_w * 0.68) as i32 - stride,
        waist_y - 4,
        2.2,
        2.8,
        skin,
        0.88,
    );
    image.fill_ellipse(
        x + (shoulder_w * 0.68) as i32 + stride,
        waist_y - 6,
        2.2,
        2.8,
        skin,
        0.88,
    );

    image.fill_polygon(
        &[
            (x - shoulder_w as i32 / 2, shoulder_y),
            (x + shoulder_w as i32 / 2, shoulder_y + 1),
            (x + pelvis_w as i32 / 2, waist_y),
            (x - pelvis_w as i32 / 2, waist_y),
        ],
        cloth,
        0.96,
    );
    image.fill_irregular_blob(
        [x, waist_y + 4],
        [pelvis_w * 0.55, 5.0],
        Brush::new(darken(cloth, 0.78), 0.90),
        seed ^ 0x9E15,
    );
    image.line(
        x - (pelvis_w * 0.22) as i32,
        waist_y + 7,
        x - stride,
        knee_y,
        3.2,
        Brush::new(darken(cloth, 0.72), 0.92),
    );
    image.line(
        x - stride,
        knee_y,
        x - (pelvis_w * 0.45) as i32 - stride,
        foot_y - 4,
        2.8,
        Brush::new(darken(cloth, 0.62), 0.92),
    );
    image.line(
        x + (pelvis_w * 0.22) as i32,
        waist_y + 7,
        x + stride,
        knee_y - 1,
        3.2,
        Brush::new(darken(cloth, 0.74), 0.92),
    );
    image.line(
        x + stride,
        knee_y - 1,
        x + (pelvis_w * 0.45) as i32 + stride,
        foot_y - 4,
        2.8,
        Brush::new(darken(cloth, 0.64), 0.92),
    );
    image.fill_ellipse(
        x - (pelvis_w * 0.48) as i32 - stride,
        foot_y - 2,
        4.6,
        1.8,
        [25, 23, 21],
        0.92,
    );
    image.fill_ellipse(
        x + (pelvis_w * 0.48) as i32 + stride,
        foot_y - 2,
        4.6,
        1.8,
        [25, 23, 21],
        0.92,
    );
    image.fill_ellipse(x, neck_y, 2.3, 3.5, skin, 0.92);
    image.fill_ellipse(x, head_y, head_h * 0.42, head_h * 0.55, skin, 0.96);
    image.fill_irregular_blob(
        [x, head_y - (head_h * 0.35) as i32],
        [head_h * 0.42, head_h * 0.20],
        Brush::new(hair, 0.88),
        seed ^ 0xA17,
    );
    image.fill_ellipse(x - 2, head_y, 0.9, 0.9, [35, 31, 29], 0.68);
    image.fill_ellipse(x + 2, head_y, 0.9, 0.9, [35, 31, 29], 0.68);
    image.line(
        x - 2,
        head_y + 3,
        x + 2,
        head_y + 3,
        0.6,
        Brush::new([90, 55, 49], 0.48),
    );
    image.line(
        x - (shoulder_w * 0.36) as i32,
        shoulder_y + 4,
        x + (shoulder_w * 0.32) as i32,
        waist_y - 3,
        0.8,
        Brush::new(brighten(cloth, 1.32), 0.18),
    );
}

fn draw_texture_grain(image: &mut RgbImage, seed: u64) {
    for y in 0..image.height {
        for x in 0..image.width {
            let n = stable_unit(seed ^ x as u64, y as u64);
            if n > 0.994 {
                image.blend_pixel(x as i32, y as i32, [235, 226, 198], 0.10);
            } else if n < 0.006 {
                image.blend_pixel(x as i32, y as i32, [24, 23, 20], 0.06);
            }
        }
    }
}

fn material_color(
    recipes: &[SurfaceTextureRecipeV21],
    surface_id: BeautySurfaceIdV21,
    material_id: BeautyMaterialIdV21,
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
    let sample = sample_material_v21(recipe, uv_meters, seed);
    let light = 0.83 + sample.height_0_to_1 * 0.20 - sample.dirt_0_to_1 * 0.12
        + sample.wetness_0_to_1 * 0.06;
    [
        linear_to_u8(sample.base_color_linear[0] * light),
        linear_to_u8(sample.base_color_linear[1] * light),
        linear_to_u8(sample.base_color_linear[2] * light),
    ]
}

fn write_timing_report(
    path: &Path,
    scene_id: u64,
    target_frame_ms: f32,
    scene_build_ms: f32,
    timings: &[BeautyV21CaptureTiming],
) -> io::Result<()> {
    let mut file = File::create(path)?;
    writeln!(file, "Ashfall V21 golden scene frame timings")?;
    writeln!(file, "scene_id={scene_id}")?;
    writeln!(file, "target_frame_ms={target_frame_ms:.3}")?;
    writeln!(file, "scene_build_ms={scene_build_ms:.3}")?;
    for timing in timings {
        writeln!(
            file,
            "{} render_ms={:.3} write_ms={:.3} total_ms={:.3} visible_instances={} material_recipes={}",
            biome_label(timing.biome),
            timing.render_ms,
            timing.write_ms,
            timing.total_ms,
            timing.visible_instances,
            timing.material_recipe_count
        )?;
    }
    Ok(())
}

fn elapsed_ms(start: Instant) -> f32 {
    start.elapsed().as_secs_f64() as f32 * 1000.0
}

fn biome_label(biome: WorldBiomeV21) -> &'static str {
    match biome {
        WorldBiomeV21::City => "city",
        WorldBiomeV21::NatureReserve => "nature",
        WorldBiomeV21::Landfill => "landfill",
    }
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

fn mix_vec3(a: Vec3V21, b: Vec3V21, t: f32) -> Vec3V21 {
    Vec3V21::new(lerp(a.x, b.x, t), lerp(a.y, b.y, t), lerp(a.z, b.z, t))
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

trait RoadWetHighlight {
    fn wet_highlight_alpha(&self) -> f32;
}

impl RoadWetHighlight for RoadPatchV21 {
    fn wet_highlight_alpha(&self) -> f32 {
        (0.04 + self.unevenness_0_to_1 * 0.08 + self.camber_0_to_1 * 0.04).clamp(0.0, 0.18)
    }
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

    fn project(&self, bounds: Bounds3V21, point: Vec3V21) -> (i32, i32) {
        let margin_x = self.width as f32 * 0.07;
        let ground_top = self.height as f32 * GROUND_TOP_FRACTION;
        let ground_height = self.height as f32 - ground_top - self.height as f32 * 0.05;
        let nx = ((point.x - bounds.min.x) / (bounds.max.x - bounds.min.x)).clamp(0.0, 1.0);
        let ny = ((point.y - bounds.min.y) / (bounds.max.y - bounds.min.y)).clamp(0.0, 1.0);
        let x = margin_x + nx * (self.width as f32 - margin_x * 2.0);
        let y = ground_top + (1.0 - ny) * ground_height - point.z * 10.0;
        (x as i32, y as i32)
    }

    fn unproject_ground(&self, bounds: Bounds3V21, x: u32, y: u32) -> Vec3V21 {
        let margin_x = self.width as f32 * 0.07;
        let ground_top = self.height as f32 * GROUND_TOP_FRACTION;
        let ground_height = self.height as f32 - ground_top - self.height as f32 * 0.05;
        let nx = ((x as f32 - margin_x) / (self.width as f32 - margin_x * 2.0)).clamp(0.0, 1.0);
        let ny = (1.0 - ((y as f32 - ground_top) / ground_height).clamp(0.0, 1.0)).clamp(0.0, 1.0);
        Vec3V21::new(
            lerp(bounds.min.x, bounds.max.x, nx),
            lerp(bounds.min.y, bounds.max.y, ny),
            0.0,
        )
    }

    fn fill_rect(
        &mut self,
        min_x: i32,
        min_y: i32,
        max_x: i32,
        max_y: i32,
        color: [u8; 3],
        alpha: f32,
    ) {
        for y in min_y.min(max_y)..=min_y.max(max_y) {
            for x in min_x.min(max_x)..=min_x.max(max_x) {
                self.blend_pixel(x, y, color, alpha);
            }
        }
    }

    fn fill_irregular_world_patch(
        &mut self,
        bounds: Bounds3V21,
        min: Vec3V21,
        max: Vec3V21,
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
                if edge_distance < 0.035 && n < 0.44 {
                    continue;
                }
                let edge_alpha = if edge_distance < 0.15 {
                    (edge_distance / 0.15 * 0.76 + n * 0.24).clamp(0.0, 1.0)
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
        bounds: Bounds3V21,
        a: Vec3V21,
        b: Vec3V21,
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
                let ruffle = 1.0 + (n - 0.5) * 0.36;
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

    fn write_bmp(&self, path: &Path) -> io::Result<()> {
        let row_stride = (self.width * 3).div_ceil(4) * 4;
        let pixel_bytes = row_stride * self.height;
        let file_bytes = 14 + 40 + pixel_bytes;
        let mut file = BufWriter::new(File::create(path)?);

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
        let mut row = Vec::with_capacity(row_stride as usize);
        for y in (0..self.height).rev() {
            row.clear();
            for x in 0..self.width {
                let [r, g, b] = self.pixels[(y * self.width + x) as usize];
                row.extend_from_slice(&[b, g, r]);
            }
            row.extend_from_slice(&padding);
            file.write_all(&row)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn capture_human_painter_uses_body_masses_and_material_layers() {
        let mut image = RgbImage::new(112, 112);
        let bounds = Bounds3V21::new(Vec3V21::new(-1.0, -1.0, 0.0), Vec3V21::new(1.0, 1.0, 2.0));
        let mut human = HumanProxyV21::city_pedestrian(42, Vec3V21::new(0.0, 0.0, 0.0));
        human.pose = HumanPoseStateV21::Walking;

        draw_human(&mut image, bounds, &[], &human, 0xC0FFEE);

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

        assert!(colored_pixels > 260);
        assert!(max_x - min_x >= 20);
        assert!(max_y - min_y >= 38);
        assert!(unique_colors.len() >= 7);
    }
}
