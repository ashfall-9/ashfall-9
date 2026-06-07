use std::fs::{File, create_dir_all};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use ashfall_rendering::beauty_v19::{BeautyCellPackageV19, BoundsV19, Vec3V19, WorldBiomeV19};

use crate::beauty_scene_v19_bridge::{BeautySceneBuildContextV19, build_beauty_scene_v19};

const GOLDEN_CAPTURE_WIDTH: u32 = 960;
const GOLDEN_CAPTURE_HEIGHT: u32 = 540;
const GROUND_TOP_FRACTION: f32 = 0.28;

pub fn capture_beauty_v19_golden_scenes(output_dir: impl AsRef<Path>) -> io::Result<Vec<PathBuf>> {
    let output_dir = output_dir.as_ref();
    create_dir_all(output_dir)?;

    let scene = build_beauty_scene_v19(BeautySceneBuildContextV19::default());
    let captures = [
        (WorldBiomeV19::City, "ashfall_v19_city.bmp"),
        (WorldBiomeV19::NatureReserve, "ashfall_v19_nature.bmp"),
        (WorldBiomeV19::Landfill, "ashfall_v19_landfill.bmp"),
    ];

    let mut paths = Vec::new();
    for (biome, file_name) in captures {
        let Some(cell) = scene.cells.iter().find(|cell| cell.biome == biome) else {
            continue;
        };
        let mut image = RgbImage::new(GOLDEN_CAPTURE_WIDTH, GOLDEN_CAPTURE_HEIGHT);
        render_v19_golden_cell(&mut image, cell, scene.seed);
        let path = output_dir.join(file_name);
        image.write_bmp(&path)?;
        paths.push(path);
    }

    Ok(paths)
}

fn render_v19_golden_cell(image: &mut RgbImage, cell: &BeautyCellPackageV19, seed: u64) {
    draw_sky(image, cell.biome, seed ^ cell.cell_id);
    draw_ground(image, cell, seed ^ 0x9017);

    for terrain in &cell.terrain {
        let min = Vec3V19::new(
            terrain.center.x - terrain.size_meters[0] * 0.5,
            terrain.center.y - terrain.size_meters[1] * 0.5,
            0.0,
        );
        let max = Vec3V19::new(
            terrain.center.x + terrain.size_meters[0] * 0.5,
            terrain.center.y + terrain.size_meters[1] * 0.5,
            0.0,
        );
        let base = match cell.biome {
            WorldBiomeV19::NatureReserve => [62, 83, 48],
            WorldBiomeV19::Landfill => [85, 72, 55],
            _ => [91, 88, 79],
        };
        image.fill_world_rect(cell.bounds, min, max, base, 0.62);
    }

    for road in &cell.roads {
        for pair in road.control_points.windows(2) {
            image.world_line(
                cell.bounds,
                pair[0],
                pair[1],
                road.width_meters * 9.5,
                [45, 49, 46],
                0.95,
            );
            image.world_line(cell.bounds, pair[0], pair[1], 2.0, [133, 128, 105], 0.34);
        }
    }

    for curb in &cell.curbs {
        image.world_line(
            cell.bounds,
            curb.start,
            curb.end,
            curb.radius_meters * 28.0,
            [126, 118, 104],
            0.86,
        );
    }

    for facade in &cell.facades {
        let min = facade.origin;
        let max = Vec3V19::new(
            facade.origin.x + facade.width_meters,
            facade.origin.y + facade.depth_meters,
            0.0,
        );
        image.fill_world_rect(cell.bounds, min, max, [95, 92, 82], 0.92);
        for floor in 0..4 {
            let y = facade.origin.y + 0.08 + floor as f32 * 0.08;
            image.world_line(
                cell.bounds,
                Vec3V19::new(facade.origin.x + 0.3, y, 0.0),
                Vec3V19::new(facade.origin.x + facade.width_meters - 0.3, y, 0.0),
                1.4,
                [42, 53, 56],
                0.62,
            );
        }
    }

    for water in &cell.water_films {
        let (x, y) = image.project(cell.bounds, water.center);
        let radius = (water.radius_meters * 12.0).max(5.0);
        image.fill_ellipse(
            x,
            y,
            radius * (1.4 + water.edge_irregularity_0_to_1 * 0.22),
            radius * (0.55 + water.edge_irregularity_0_to_1 * 0.12),
            [64, 96, 105],
            0.42,
        );
        image.fill_ellipse(
            x - 3,
            y - 2,
            radius * 0.46,
            radius * 0.12,
            [184, 203, 203],
            0.18,
        );
    }

    for plant in &cell.plants {
        let (x, y) = image.project(cell.bounds, plant.root_position);
        let h = (plant.height_meters * 34.0).clamp(4.0, 20.0);
        image.line(
            x,
            y,
            x - (plant.bend_0_to_1 * 7.0) as i32,
            y - h as i32,
            1.6,
            Brush::new([58, 94, 45], 0.86),
        );
        image.fill_ellipse(x - 4, y - h as i32 + 2, 8.0, 3.0, [74, 119, 53], 0.66);
        image.fill_ellipse(x + 4, y - h as i32 + 4, 7.0, 3.0, [52, 90, 44], 0.58);
    }

    for stone in &cell.stones {
        let (x, y) = image.project(cell.bounds, stone.center);
        let radius = (stone.radius_meters * 30.0).max(3.0);
        image.fill_ellipse(x, y, radius * 1.24, radius * 0.84, [103, 101, 91], 0.92);
        image.fill_ellipse(
            x - 2,
            y - 1,
            radius * 0.42,
            radius * 0.22,
            [154, 149, 132],
            0.42,
        );
    }

    for prop in &cell.landfill_props {
        let (x, y) = image.project(cell.bounds, prop.center);
        let width = (prop.size_meters[0] * 28.0).max(5.0);
        let height = (prop.size_meters[1] * 28.0).max(4.0);
        let color = if prop.rust_or_stain_0_to_1 > 0.45 {
            [113, 77, 54]
        } else {
            [77, 91, 99]
        };
        image.fill_rect_centered(x, y, width, height, color, 0.88);
        image.fill_rect_centered(
            x + 1,
            y - 1,
            width * 0.42,
            height * 0.28,
            [166, 161, 126],
            0.34,
        );
    }

    for vehicle in &cell.vehicles {
        let (x, y) = image.project(cell.bounds, vehicle.world_position);
        let w = vehicle.width_meters * 18.0;
        let l = vehicle.length_meters * 13.0;
        image.fill_rect_centered(x, y, l, w, [60, 70, 72], 0.96);
        image.fill_rect_centered(x - 5, y - 1, l * 0.36, w * 0.56, [118, 143, 151], 0.76);
        image.fill_rect_centered(x + 9, y, l * 0.34, w * 0.66, [77, 82, 80], 0.7);
        image.fill_rect_centered(x - 14, y - w as i32 / 2 - 2, 7.0, 3.0, [21, 22, 21], 0.95);
        image.fill_rect_centered(x + 14, y - w as i32 / 2 - 2, 7.0, 3.0, [21, 22, 21], 0.95);
        image.fill_rect_centered(x - 14, y + w as i32 / 2 + 2, 7.0, 3.0, [21, 22, 21], 0.95);
        image.fill_rect_centered(x + 14, y + w as i32 / 2 + 2, 7.0, 3.0, [21, 22, 21], 0.95);
        image.fill_rect_centered(x + l as i32 / 2 - 2, y - 6, 2.0, 3.0, [220, 204, 144], 0.82);
        image.fill_rect_centered(x + l as i32 / 2 - 2, y + 6, 2.0, 3.0, [220, 204, 144], 0.82);
    }

    for human in &cell.humans {
        let (x, y) = image.project(cell.bounds, human.world_position);
        let height = human.height_meters * 8.0;
        image.fill_ellipse(x, y - height as i32, 4.0, 4.8, [163, 119, 88], 0.95);
        image.fill_rect_centered(x, y - height as i32 / 2, 7.0, height, [49, 67, 76], 0.94);
        image.line(
            x - 3,
            y - 6,
            x - 10,
            y + 7,
            1.5,
            Brush::new([54, 58, 60], 0.88),
        );
        image.line(
            x + 3,
            y - 6,
            x + 9,
            y + 7,
            1.5,
            Brush::new([54, 58, 60], 0.88),
        );
    }

    draw_texture_grain(image, seed ^ cell.cell_id);
}

fn draw_sky(image: &mut RgbImage, biome: WorldBiomeV19, seed: u64) {
    let sky_height = (image.height as f32 * GROUND_TOP_FRACTION) as u32;
    for y in 0..sky_height {
        let t = y as f32 / sky_height.max(1) as f32;
        let top = match biome {
            WorldBiomeV19::NatureReserve => [95.0, 126.0, 151.0],
            WorldBiomeV19::Landfill => [119.0, 126.0, 126.0],
            _ => [105.0, 132.0, 154.0],
        };
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
        (image.width as f32 * 0.78) as i32,
        (image.height as f32 * 0.11) as i32,
        22.0,
        22.0,
        [232, 205, 134],
        0.86,
    );
    image.fill_ellipse(
        (image.width as f32 * 0.22) as i32,
        (image.height as f32 * 0.10) as i32,
        12.0,
        12.0,
        [205, 214, 216],
        0.34,
    );

    for cloud in 0..7 {
        let x = (0.12 + stable_unit(seed, cloud * 3) * 0.76) * image.width as f32;
        let y = (0.07 + stable_unit(seed, cloud * 3 + 1) * 0.16) * image.height as f32;
        let w = 38.0 + stable_unit(seed, cloud * 3 + 2) * 70.0;
        image.fill_ellipse(
            x as i32,
            y as i32,
            w,
            7.0 + cloud as f32 * 0.45,
            [211, 216, 211],
            0.18,
        );
        image.fill_ellipse(
            x as i32 - 18,
            y as i32 + 5,
            w * 0.58,
            5.0,
            [151, 160, 158],
            0.12,
        );
    }
}

fn draw_ground(image: &mut RgbImage, cell: &BeautyCellPackageV19, seed: u64) {
    let y_start = (image.height as f32 * GROUND_TOP_FRACTION) as u32;
    let base = match cell.biome {
        WorldBiomeV19::City => [69, 66, 58],
        WorldBiomeV19::NatureReserve => [54, 72, 42],
        WorldBiomeV19::Landfill => [74, 65, 49],
        _ => [65, 62, 55],
    };
    for y in y_start..image.height {
        let t = (y - y_start) as f32 / (image.height - y_start).max(1) as f32;
        for x in 0..image.width {
            let n = stable_unit(seed ^ x as u64, y as u64);
            let shade = 0.74 + t * 0.28 + (n - 0.5) * 0.20;
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

    fn project(&self, bounds: BoundsV19, point: Vec3V19) -> (i32, i32) {
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
        bounds: BoundsV19,
        min: Vec3V19,
        max: Vec3V19,
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

    fn world_line(
        &mut self,
        bounds: BoundsV19,
        a: Vec3V19,
        b: Vec3V19,
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
