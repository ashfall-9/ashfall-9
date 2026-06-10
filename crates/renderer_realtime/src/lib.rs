#![forbid(unsafe_code)]

use std::{path::Path, time::Instant};

use engine_core::{CameraState, InterfaceVersion, RenderFrameId};
use image::{ImageBuffer, Rgba};
use rayon::prelude::*;
use scene_schema::SkySceneRequest;
use serde::{Deserialize, Serialize};

pub const RENDERER_REALTIME_VERSION: InterfaceVersion =
    InterfaceVersion::new("renderer_realtime", 0, 1, 0);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkyRenderResult {
    pub frame: RenderFrameId,
    pub capture_requested: bool,
    pub feature_stats: Vec<String>,
}

pub fn plan_sky_smoke_frame(request: &SkySceneRequest, frame_index: u64) -> SkyRenderResult {
    let mut feature_stats = vec![
        "sun_disc:planned".to_string(),
        "tone_map:planned".to_string(),
    ];
    if request.clouds.is_empty() {
        feature_stats.push("clouds:skipped_empty".to_string());
    } else {
        feature_stats.push("clouds:planned".to_string());
    }

    SkyRenderResult {
        frame: RenderFrameId(frame_index),
        capture_requested: false,
        feature_stats,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkyRenderSettings {
    pub width: u32,
    pub height: u32,
    pub noise_octaves: u8,
    pub raymarch_steps: u8,
    pub cloud_coverage: f32,
    pub detail_strength: f32,
    pub sun_intensity: f32,
    pub exposure: f32,
}

impl SkyRenderSettings {
    pub const fn balanced() -> Self {
        Self {
            width: 640,
            height: 360,
            noise_octaves: 4,
            raymarch_steps: 3,
            cloud_coverage: 0.48,
            detail_strength: 0.82,
            sun_intensity: 1.0,
            exposure: 1.0,
        }
    }

    pub const fn fast() -> Self {
        Self {
            width: 384,
            height: 216,
            noise_octaves: 4,
            raymarch_steps: 4,
            cloud_coverage: 0.46,
            detail_strength: 0.68,
            sun_intensity: 1.0,
            exposure: 1.0,
        }
    }

    pub const fn quality() -> Self {
        Self {
            width: 960,
            height: 540,
            noise_octaves: 6,
            raymarch_steps: 8,
            cloud_coverage: 0.50,
            detail_strength: 0.86,
            sun_intensity: 1.0,
            exposure: 1.0,
        }
    }

    pub fn candidate_ladder() -> Vec<Self> {
        vec![
            Self::quality(),
            Self {
                width: 768,
                height: 432,
                noise_octaves: 6,
                raymarch_steps: 7,
                cloud_coverage: 0.50,
                detail_strength: 0.84,
                sun_intensity: 1.0,
                exposure: 1.0,
            },
            Self::balanced(),
            Self {
                width: 512,
                height: 288,
                noise_octaves: 5,
                raymarch_steps: 5,
                cloud_coverage: 0.47,
                detail_strength: 0.76,
                sun_intensity: 1.0,
                exposure: 1.0,
            },
            Self::fast(),
            Self {
                width: 320,
                height: 180,
                noise_octaves: 4,
                raymarch_steps: 4,
                cloud_coverage: 0.45,
                detail_strength: 0.64,
                sun_intensity: 1.0,
                exposure: 1.0,
            },
            Self {
                width: 272,
                height: 153,
                noise_octaves: 3,
                raymarch_steps: 3,
                cloud_coverage: 0.47,
                detail_strength: 0.78,
                sun_intensity: 1.04,
                exposure: 1.02,
            },
            Self {
                width: 256,
                height: 144,
                noise_octaves: 3,
                raymarch_steps: 2,
                cloud_coverage: 0.49,
                detail_strength: 0.82,
                sun_intensity: 1.06,
                exposure: 1.03,
            },
            Self {
                width: 224,
                height: 126,
                noise_octaves: 3,
                raymarch_steps: 2,
                cloud_coverage: 0.51,
                detail_strength: 0.84,
                sun_intensity: 1.08,
                exposure: 1.04,
            },
        ]
    }

    fn uses_fast_cloud_path(self) -> bool {
        self.width <= 288 || self.raymarch_steps <= 3
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkyImageMetrics {
    pub width: u32,
    pub height: u32,
    pub render_ms: f32,
    pub fps: f32,
    pub mean_luminance: f32,
    pub contrast: f32,
    pub cloud_coverage: f32,
    pub clear_sky_fraction: f32,
    pub cloud_alpha_variance: f32,
    pub cloud_core_fraction: f32,
    pub cloud_bbox_margin: f32,
    pub cloud_boundary_contact: f32,
    pub edge_density: f32,
    pub detail_energy: f32,
    pub sun_visibility: f32,
    pub sun_cloud_contrast: f32,
    pub horizon_gradient: f32,
    pub color_variance: f32,
}

impl SkyImageMetrics {
    pub fn neural_features(&self) -> [f32; 14] {
        [
            triangular_score(self.cloud_coverage, 0.10, 0.20, 0.34),
            saturate(self.edge_density * 5.0),
            saturate(self.detail_energy * 220.0),
            saturate(self.sun_visibility),
            triangular_score(self.contrast, 0.08, 0.22, 0.48),
            saturate(self.color_variance * 7.0),
            triangular_score(self.horizon_gradient, 0.06, 0.20, 0.44),
            triangular_score(self.mean_luminance, 0.22, 0.52, 0.82),
            triangular_score(self.clear_sky_fraction, 0.62, 0.78, 0.93),
            saturate(self.cloud_alpha_variance * 18.0),
            triangular_score(self.cloud_core_fraction, 0.08, 0.48, 0.88),
            triangular_score(self.sun_cloud_contrast, 0.02, 0.18, 0.48),
            triangular_score(self.cloud_bbox_margin, 0.04, 0.14, 0.32),
            1.0 - saturate(self.cloud_boundary_contact * 16.0),
        ]
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkyCapture {
    pub settings: SkyRenderSettings,
    pub metrics: SkyImageMetrics,
    pub rgba: Vec<u8>,
}

impl SkyCapture {
    pub fn write_png(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let image = ImageBuffer::<Rgba<u8>, _>::from_raw(
            self.metrics.width,
            self.metrics.height,
            self.rgba.clone(),
        )
        .ok_or_else(|| anyhow::anyhow!("sky capture buffer does not match dimensions"))?;
        image.save(path)?;
        Ok(())
    }
}

pub fn render_sky_capture(request: &SkySceneRequest, settings: SkyRenderSettings) -> SkyCapture {
    let start = Instant::now();
    let width = settings.width.max(1);
    let height = settings.height.max(1);
    let pixel_count = (width * height) as usize;
    let mut rgba = vec![0_u8; pixel_count * 4];
    let mut alpha = vec![0.0_f32; pixel_count];
    let mut luminance = vec![0.0_f32; pixel_count];
    let mut red = vec![0.0_f32; pixel_count];
    let mut green = vec![0.0_f32; pixel_count];
    let mut blue = vec![0.0_f32; pixel_count];

    let aspect = width as f32 / height as f32;
    let camera = camera_basis(&request.camera, aspect);
    let sun = normalized_or_default(request.sun.direction, [0.24, -0.18, 0.95]);
    let sun_screen = project_direction_to_screen(sun, camera);
    let seed = request.seed ^ 0xA5A5_79D3_C1E5_1234;
    let wind_time = request.time_seconds.max(0.0);
    let cloud_domain_offset = [
        request.camera.position_m[0] * 0.00012
            + request.camera.position_m[2] * 0.000035
            + wind_time * 0.018,
        request.camera.position_m[1] * 0.00008 + wind_time * 0.006,
    ];
    let coverage = request
        .clouds
        .first()
        .map(|cloud| cloud.coverage)
        .unwrap_or(settings.cloud_coverage)
        .max(settings.cloud_coverage)
        .clamp(0.18, 0.74);
    let density_gain = request
        .clouds
        .first()
        .map(|cloud| cloud.density)
        .unwrap_or(0.48)
        .clamp(0.18, 1.0);
    let fast_cloud_path = settings.uses_fast_cloud_path();
    let footprint =
        projected_cloud_footprint(request, coverage, aspect, seed, fast_cloud_path, camera);

    let width_usize = width as usize;
    rgba.par_chunks_mut(4)
        .zip(alpha.par_iter_mut())
        .zip(luminance.par_iter_mut())
        .zip(red.par_iter_mut())
        .zip(green.par_iter_mut())
        .zip(blue.par_iter_mut())
        .enumerate()
        .for_each(
            |(
                idx,
                (
                    ((((rgba_pixel, alpha_pixel), luminance_pixel), red_pixel), green_pixel),
                    blue_pixel,
                ),
            )| {
                let x = idx % width_usize;
                let y = idx / width_usize;
                let u = (x as f32 + 0.5) / width as f32;
                let v = (y as f32 + 0.5) / height as f32;
                let ray = camera_ray(camera, u, v);
                let horizon = smoothstep(-0.16, 0.34, ray[1]);
                let zenith = smoothstep(0.08, 0.86, ray[1]);
                let mut color = mix3([0.76, 0.86, 1.0], [0.055, 0.16, 0.42], zenith.powf(0.74));
                color = mix3(color, [0.98, 0.72, 0.42], (1.0 - horizon) * 0.20);

                let dx = (u - sun_screen[0]) * aspect;
                let dy = v - sun_screen[1];
                let sun_distance = (dx * dx + dy * dy).sqrt();
                let sun_radius = (request.sun.angular_radius_degrees.to_radians()
                    / camera.fov_y_radians)
                    .clamp(0.010, 0.026);
                let sun_disc = 1.0 - smoothstep(sun_radius, sun_radius * 3.3, sun_distance);
                let sun_glow = (-sun_distance * 12.0).exp();
                let warm_glow = (-sun_distance * 4.0).exp() * 0.16;

                color = add3(
                    color,
                    mul3([1.0, 0.76, 0.42], warm_glow * settings.sun_intensity),
                );

                let footprint_envelope =
                    finite_cloud_footprint(u, v, aspect, footprint, seed, fast_cloud_path);
                let lobe_mask = cumulus_lobe_mask(u, v, aspect, footprint, seed);
                let cloud_footprint = (footprint_envelope * (0.34 + lobe_mask * 0.88))
                    .clamp(0.0, 1.0)
                    * if footprint.visible { 1.0 } else { 0.0 };
                let cloud_scale = 2.15;
                let wind = seed_unit(seed.wrapping_add(13)) * 0.42;
                let (warp_x, warp_y) = if fast_cloud_path {
                    (
                        value_noise(
                            (u + cloud_domain_offset[0]) * 2.1 + 11.7,
                            (v + cloud_domain_offset[1]) * 1.5 + 2.3,
                            seed ^ 0xCA11,
                        ) - 0.5,
                        value_noise(
                            (u + cloud_domain_offset[0]) * 1.4 + 3.4,
                            (v + cloud_domain_offset[1]) * 1.9 + 9.1,
                            seed ^ 0xF00D,
                        ) - 0.5,
                    )
                } else {
                    (
                        fbm(
                            (u + cloud_domain_offset[0]) * 1.8 + 11.7,
                            (v + cloud_domain_offset[1]) * 1.35 + 2.3,
                            3,
                            seed ^ 0xCA11,
                        ) - 0.5,
                        fbm(
                            (u + cloud_domain_offset[0]) * 1.2 + 3.4,
                            (v + cloud_domain_offset[1]) * 1.65 + 9.1,
                            3,
                            seed ^ 0xF00D,
                        ) - 0.5,
                    )
                };
                let warped_u = u + cloud_domain_offset[0] + warp_x * 0.10 + wind;
                let warped_v = v + cloud_domain_offset[1] + warp_y * 0.07;
                let cloud_noise = if fast_cloud_path {
                    fast_cloud_density(warped_u, warped_v, settings, seed, cloud_scale)
                } else {
                    cloud_density(warped_u, warped_v, settings, seed, cloud_scale)
                };
                let detail_octaves = if fast_cloud_path {
                    2
                } else {
                    settings.noise_octaves.saturating_sub(1).max(2)
                };
                let detail = fbm(
                    (u + cloud_domain_offset[0]) * 18.0 + warp_x * 1.4,
                    (v + cloud_domain_offset[1]) * 13.0 + warp_y * 1.1,
                    detail_octaves,
                    seed ^ 0x00D3_7A11,
                );
                let billow = fbm(
                    (u + cloud_domain_offset[0]) * 34.0 + warp_x * 2.2 + wind * 0.35,
                    (v + cloud_domain_offset[1]) * 25.0 + warp_y * 1.8,
                    2,
                    seed ^ 0xB111_0A55,
                );
                let body_bias = cloud_footprint * (0.13 + coverage * 0.08) + lobe_mask * 0.20;
                let shaped_cloud_noise =
                    (cloud_noise * 0.65 + detail * 0.20 + body_bias).clamp(0.0, 1.0);
                let threshold = if fast_cloud_path {
                    (0.61 - coverage * 0.34).clamp(0.38, 0.60)
                } else {
                    (0.53 - coverage * 0.32).clamp(0.28, 0.56)
                };
                let threshold_width = if fast_cloud_path { 0.14 } else { 0.16 };
                let mut cloud_alpha = smoothstep(
                    threshold - lobe_mask * 0.05,
                    threshold + threshold_width,
                    shaped_cloud_noise,
                ) * cloud_footprint;
                cloud_alpha = (cloud_alpha * (0.84 + density_gain * 0.32)).clamp(0.0, 0.96);
                cloud_alpha *= (0.60 + detail * settings.detail_strength * 0.72).clamp(0.52, 1.14);
                let edge_erosion =
                    (0.48 - billow).max(0.0) * (1.0 - lobe_mask * 0.72) * cloud_footprint * 0.28;
                cloud_alpha = (cloud_alpha - edge_erosion).clamp(0.0, 0.98);
                if fast_cloud_path {
                    cloud_alpha = (cloud_alpha - (0.55 - billow).max(0.0) * cloud_alpha * 0.22)
                        .clamp(0.0, 0.98);
                }
                cloud_alpha *= (0.82 + billow * 0.24 + lobe_mask * 0.12).clamp(0.72, 1.16);
                cloud_alpha = cloud_alpha.clamp(0.0, 0.98);

                let sun_shadow_sample = if fast_cloud_path {
                    cloud_noise * 0.72 + detail * 0.28
                } else {
                    cloud_density(
                        u + 0.055 * sun[0] + warp_x * 0.06,
                        v - 0.035 * sun[1] + warp_y * 0.05,
                        settings,
                        seed ^ 0x514D,
                        cloud_scale,
                    )
                };
                let self_shadow = (1.0 - sun_shadow_sample * 0.58).clamp(0.38, 1.0);
                let sun_facing = smoothstep(0.42, 0.0, sun_distance);
                let cloud_light = (0.56 + sun[2].max(0.0) * 0.34) * self_shadow;
                let cloud_base = mix3(
                    [0.50, 0.54, 0.61],
                    [0.96, 0.97, 1.0],
                    (detail * 0.58 + billow * 0.34 + lobe_mask * 0.12).clamp(0.0, 1.0),
                );
                let relative_cloud_x =
                    ((u - footprint.center[0]) * aspect / footprint.radius[0]).clamp(-1.0, 1.0);
                let relative_cloud_y =
                    ((v - footprint.center[1]) / footprint.radius[1]).clamp(-1.0, 1.0);
                let relative_radius = (relative_cloud_x * relative_cloud_x
                    + relative_cloud_y * relative_cloud_y)
                    .sqrt();
                let sunward =
                    (relative_cloud_x * sun[0] - relative_cloud_y * sun[1]).clamp(-1.0, 1.0);
                let rim_light = smoothstep(0.48, 0.95, relative_radius)
                    * smoothstep(-0.18, 0.72, sunward)
                    * cloud_alpha;
                let texture_shadow = ((1.0 - billow) * 0.15 + (1.0 - detail) * 0.10) * cloud_alpha;
                let bottom_shade = smoothstep(-0.10, 0.88, relative_cloud_y) * cloud_alpha * 0.40
                    + smoothstep(0.42, 0.95, relative_cloud_y) * cloud_alpha * 0.17
                    + texture_shadow;
                let shaded_light = (cloud_light - bottom_shade).max(0.06);
                let silver_lining =
                    (detail * 0.18 + billow * 0.18 + sun_glow * 1.55 + sun_facing * 0.30)
                        * (1.0 - cloud_alpha).max(0.06)
                        + rim_light * 0.34;
                let cloud_color = add3(
                    mul3(cloud_base, shaded_light),
                    mul3([1.0, 0.82, 0.56], silver_lining * cloud_alpha),
                );

                color = mix3(color, cloud_color, cloud_alpha);
                let sun_occlusion = 1.0 - cloud_alpha * smoothstep(0.22, 0.0, sun_distance) * 0.72;
                color = add3(
                    color,
                    mul3(
                        [1.0, 0.91, 0.72],
                        sun_disc * sun_occlusion * settings.sun_intensity * 2.8,
                    ),
                );
                color = add3(
                    color,
                    mul3([1.0, 0.80, 0.48], sun_glow * sun_occlusion * 0.18),
                );

                color = tone_map(color, settings.exposure);

                *alpha_pixel = cloud_alpha;
                *luminance_pixel = dot3(color, [0.2126, 0.7152, 0.0722]);
                *red_pixel = color[0];
                *green_pixel = color[1];
                *blue_pixel = color[2];

                rgba_pixel[0] = to_u8(color[0]);
                rgba_pixel[1] = to_u8(color[1]);
                rgba_pixel[2] = to_u8(color[2]);
                rgba_pixel[3] = 255;
            },
        );

    let render_ms = start.elapsed().as_secs_f32() * 1000.0;
    let metrics = compute_metrics(
        width,
        height,
        render_ms,
        MetricInputs {
            alpha: &alpha,
            luminance: &luminance,
            red: &red,
            green: &green,
            blue: &blue,
            sun_screen,
        },
    );

    SkyCapture {
        settings,
        metrics,
        rgba,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CloudFootprint {
    center: [f32; 2],
    radius: [f32; 2],
    visible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CameraBasis {
    forward: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    fov_y_radians: f32,
    tan_x: f32,
    tan_y: f32,
}

fn projected_cloud_footprint(
    request: &SkySceneRequest,
    coverage: f32,
    aspect: f32,
    seed: u64,
    fast_cloud_path: bool,
    camera: CameraBasis,
) -> CloudFootprint {
    let fov_y = request.camera.fov_y_degrees.clamp(35.0, 90.0);
    let coverage_scale = lerp(0.96, 1.20, ((coverage - 0.18) / 0.56).clamp(0.0, 1.0));
    let cloud = request.clouds.first();
    let thickness_m = cloud.map(|cloud| cloud.thickness_m).unwrap_or(1_250.0);
    let base_altitude_m = cloud.map(|cloud| cloud.base_altitude_m).unwrap_or(1_500.0);
    let quality_scale = if fast_cloud_path { 0.94 } else { 1.0 };
    let distance_m = 5_100.0 + (seed_unit(seed ^ 0xD157_AACE) - 0.5) * 500.0;
    let cloud_center = [
        (seed_unit(seed ^ 0xC10D_500D) - 0.5) * 700.0 + request.time_seconds * 24.0,
        base_altitude_m + thickness_m * 0.45,
        distance_m,
    ];
    let to_cloud = sub3(cloud_center, request.camera.position_m);
    let view_z = dot3(to_cloud, camera.forward);
    let visible = view_z > 350.0;
    if !visible {
        return CloudFootprint {
            center: [0.5, 0.5],
            radius: [0.2, 0.2],
            visible: false,
        };
    }

    let view_x = dot3(to_cloud, camera.right);
    let view_y = dot3(to_cloud, camera.up);
    let projected_x = 0.5 + view_x / (view_z * camera.tan_x * 2.0).max(0.01);
    let projected_y = 0.5 - view_y / (view_z * camera.tan_y * 2.0).max(0.01);
    let half_width_m = 3_900.0 * coverage_scale * quality_scale;
    let half_height_m = (thickness_m * 1.56).clamp(1_250.0, 2_250.0) * quality_scale;
    let radius_x = (half_width_m / (view_z * camera.tan_x * 2.0).max(1.0) * aspect)
        .clamp(aspect * 0.13, aspect * 0.42);
    let radius_y = (half_height_m / (view_z * camera.tan_y * 2.0).max(1.0)).clamp(0.15, 0.44);
    let margin_x = (radius_x / aspect + 0.04).min(0.46);
    let margin_y = (radius_y + 0.04).min(0.46);
    let distance_scale = (60.0 / fov_y).clamp(0.72, 1.18);
    let center_x = lerp(
        projected_x.clamp(-0.35, 1.35),
        projected_x.clamp(margin_x, 1.0 - margin_x),
        distance_scale.min(1.0),
    );
    let center_y = lerp(
        projected_y.clamp(-0.35, 1.35),
        projected_y.clamp(margin_y, 1.0 - margin_y),
        distance_scale.min(1.0),
    );

    CloudFootprint {
        center: [center_x, center_y],
        radius: [radius_x, radius_y],
        visible,
    }
}

fn finite_cloud_footprint(
    u: f32,
    v: f32,
    aspect: f32,
    footprint: CloudFootprint,
    seed: u64,
    fast_cloud_path: bool,
) -> f32 {
    let dx = (u - footprint.center[0]) * aspect / footprint.radius[0].max(0.01);
    let dy = (v - footprint.center[1]) / footprint.radius[1].max(0.01);
    let radius = (dx * dx + dy * dy).sqrt();
    let edge_noise = if fast_cloud_path {
        value_noise(u * 3.1 + 17.0, v * 2.7 + 5.0, seed ^ 0xB0D1)
    } else {
        fbm(u * 3.1 + 17.0, v * 2.7 + 5.0, 3, seed ^ 0xB0D1)
    };
    let lobe_scale = 0.84 + edge_noise * 0.22;

    1.0 - smoothstep(lobe_scale, lobe_scale + 0.22, radius)
}

fn cumulus_lobe_mask(u: f32, v: f32, aspect: f32, footprint: CloudFootprint, seed: u64) -> f32 {
    let x = (u - footprint.center[0]) * aspect / footprint.radius[0].max(0.01);
    let y = (v - footprint.center[1]) / footprint.radius[1].max(0.01);
    let lobes = [
        (-0.48, 0.10, 0.42),
        (-0.25, -0.12, 0.48),
        (0.02, -0.19, 0.54),
        (0.29, -0.06, 0.47),
        (0.48, 0.11, 0.38),
        (-0.05, 0.20, 0.44),
    ];
    let mut mask = 0.0_f32;
    for (index, (center_x, center_y, radius)) in lobes.into_iter().enumerate() {
        let jitter_x = (seed_unit(seed.wrapping_add(index as u64 * 37) ^ 0x0071_0BE5) - 0.5) * 0.10;
        let jitter_y = (seed_unit(seed.wrapping_add(index as u64 * 53) ^ 0x7A11_5EED) - 0.5) * 0.08;
        let dx = x - center_x - jitter_x;
        let dy = y - center_y - jitter_y;
        let distance = (dx * dx + dy * dy).sqrt();
        mask = mask.max(1.0 - smoothstep(radius * 0.58, radius, distance));
    }

    let ragged = fbm(u * 8.0 + 2.0, v * 5.7 + 4.0, 2, seed ^ 0x51F7);
    (mask * (0.84 + ragged * 0.22)).clamp(0.0, 1.0)
}

fn cloud_density(u: f32, v: f32, settings: SkyRenderSettings, seed: u64, scale: f32) -> f32 {
    let mut density = 0.0;
    let steps = settings.raymarch_steps.max(1);
    for step in 0..steps {
        let t = if steps == 1 {
            0.0
        } else {
            step as f32 / (steps - 1) as f32
        };
        let parallax = (t - 0.5) * 0.075;
        let layer = fbm(
            (u + parallax) * scale + t * 0.73,
            (v - parallax * 0.65) * scale * 1.24,
            settings.noise_octaves,
            seed.wrapping_add(step as u64 * 97),
        );
        density += layer * (1.0 - t * 0.18);
    }
    density / steps as f32
}

fn fast_cloud_density(u: f32, v: f32, settings: SkyRenderSettings, seed: u64, scale: f32) -> f32 {
    let octaves = settings.noise_octaves.max(2);
    let body = fbm(u * scale * 0.92, v * scale * 1.06, octaves, seed ^ 0x8A7E);
    let lobes = fbm(
        u * scale * 2.25 + 5.1,
        v * scale * 1.78 + 1.7,
        octaves.saturating_sub(1).max(2),
        seed ^ 0xC10D,
    );
    let erosion = fbm(
        u * scale * 7.3 + 13.0,
        v * scale * 5.7 + 8.0,
        2,
        seed ^ 0xE904,
    );

    (body * 0.64 + lobes * 0.33 + erosion * 0.12 - 0.04).clamp(0.0, 1.0)
}

struct MetricInputs<'a> {
    alpha: &'a [f32],
    luminance: &'a [f32],
    red: &'a [f32],
    green: &'a [f32],
    blue: &'a [f32],
    sun_screen: [f32; 2],
}

fn compute_metrics(
    width: u32,
    height: u32,
    render_ms: f32,
    inputs: MetricInputs<'_>,
) -> SkyImageMetrics {
    let alpha = inputs.alpha;
    let luminance = inputs.luminance;
    let red = inputs.red;
    let green = inputs.green;
    let blue = inputs.blue;
    let sun_screen = inputs.sun_screen;
    let count = luminance.len().max(1) as f32;
    let mean_luminance = luminance.iter().sum::<f32>() / count;
    let variance = luminance
        .iter()
        .map(|value| {
            let delta = value - mean_luminance;
            delta * delta
        })
        .sum::<f32>()
        / count;
    let contrast = variance.sqrt();
    let cloud_coverage = alpha.iter().filter(|value| **value > 0.12).count() as f32 / count;
    let clear_sky_fraction = alpha.iter().filter(|value| **value < 0.08).count() as f32 / count;
    let mean_alpha = alpha.iter().sum::<f32>() / count;
    let cloud_alpha_variance = alpha
        .iter()
        .map(|value| {
            let delta = value - mean_alpha;
            delta * delta
        })
        .sum::<f32>()
        / count;
    let cloud_pixel_count = alpha.iter().filter(|value| **value > 0.12).count().max(1) as f32;
    let cloud_core_fraction =
        alpha.iter().filter(|value| **value > 0.55).count() as f32 / cloud_pixel_count;
    let (cloud_bbox_margin, cloud_boundary_contact) = cloud_framing_metrics(alpha, width, height);
    let mean_cloud_luminance = alpha
        .iter()
        .zip(luminance.iter())
        .filter_map(|(alpha, luminance)| {
            if *alpha > 0.12 {
                Some(*luminance)
            } else {
                None
            }
        })
        .sum::<f32>()
        / cloud_pixel_count;

    let mut edge_sum = 0.0;
    let mut detail_sum = 0.0;
    let mut gradient_samples = 0.0;
    for y in 1..height.saturating_sub(1) {
        for x in 1..width.saturating_sub(1) {
            let idx = (y * width + x) as usize;
            let dx = alpha[(y * width + x + 1) as usize] - alpha[(y * width + x - 1) as usize];
            let dy = alpha[((y + 1) * width + x) as usize] - alpha[((y - 1) * width + x) as usize];
            let grad = (dx * dx + dy * dy).sqrt();
            edge_sum += grad;

            let lap = luminance[(y * width + x + 1) as usize]
                + luminance[(y * width + x - 1) as usize]
                + luminance[((y + 1) * width + x) as usize]
                + luminance[((y - 1) * width + x) as usize]
                - luminance[idx] * 4.0;
            detail_sum += lap.abs();
            gradient_samples += 1.0;
        }
    }
    let resolution_metric_scale = (height.max(1) as f32 / 153.0).clamp(1.0, 4.0);
    let edge_density = if gradient_samples > 0.0 {
        edge_sum / gradient_samples
    } else {
        0.0
    } * resolution_metric_scale;
    let detail_energy = if gradient_samples > 0.0 {
        detail_sum / gradient_samples
    } else {
        0.0
    } * resolution_metric_scale;

    let sun_x = (sun_screen[0] * width as f32).round() as i32;
    let sun_y = (sun_screen[1] * height as f32).round() as i32;
    let mut sun_visibility = 0.0;
    let mut sun_samples = 0.0;
    let radius = (width.min(height) as f32 * 0.035).max(3.0) as i32;
    for y in (sun_y - radius)..=(sun_y + radius) {
        for x in (sun_x - radius)..=(sun_x + radius) {
            if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                continue;
            }
            let dx = x - sun_x;
            let dy = y - sun_y;
            if dx * dx + dy * dy <= radius * radius {
                sun_visibility += luminance[(y as u32 * width + x as u32) as usize];
                sun_samples += 1.0;
            }
        }
    }
    if sun_samples > 0.0 {
        sun_visibility = (sun_visibility / sun_samples).clamp(0.0, 1.0);
    }
    let sun_cloud_contrast = (sun_visibility - mean_cloud_luminance).abs();

    let top_luma = row_luminance(luminance, width, 0);
    let horizon_row = ((height as f32 * 0.76).round() as u32).min(height.saturating_sub(1));
    let horizon_luma = row_luminance(luminance, width, horizon_row);
    let horizon_gradient = (horizon_luma - top_luma).abs();
    let mean_r = red.iter().sum::<f32>() / count;
    let mean_g = green.iter().sum::<f32>() / count;
    let mean_b = blue.iter().sum::<f32>() / count;
    let color_variance = ((red
        .iter()
        .zip(green.iter())
        .zip(blue.iter())
        .map(|((r, g), b)| {
            let dr = r - mean_r;
            let dg = g - mean_g;
            let db = b - mean_b;
            dr * dr + dg * dg + db * db
        })
        .sum::<f32>()
        / count)
        / 3.0)
        .sqrt();

    SkyImageMetrics {
        width,
        height,
        render_ms,
        fps: if render_ms > f32::EPSILON {
            1000.0 / render_ms
        } else {
            0.0
        },
        mean_luminance,
        contrast,
        cloud_coverage,
        clear_sky_fraction,
        cloud_alpha_variance,
        cloud_core_fraction,
        cloud_bbox_margin,
        cloud_boundary_contact,
        edge_density,
        detail_energy,
        sun_visibility,
        sun_cloud_contrast,
        horizon_gradient,
        color_variance,
    }
}

fn cloud_framing_metrics(alpha: &[f32], width: u32, height: u32) -> (f32, f32) {
    let mut cloud_pixels = 0_u32;
    let mut boundary_pixels = 0_u32;
    let mut min_x = width;
    let mut max_x = 0_u32;
    let mut min_y = height;
    let mut max_y = 0_u32;
    let border_x = ((width as f32 * 0.035).ceil() as u32).max(1);
    let border_y = ((height as f32 * 0.035).ceil() as u32).max(1);
    let right_border = width.saturating_sub(border_x);
    let bottom_border = height.saturating_sub(border_y);

    for (index, alpha) in alpha.iter().enumerate() {
        if *alpha <= 0.12 {
            continue;
        }
        let x = index as u32 % width.max(1);
        let y = index as u32 / width.max(1);
        cloud_pixels += 1;
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
        if x < border_x || x >= right_border || y < border_y || y >= bottom_border {
            boundary_pixels += 1;
        }
    }

    if cloud_pixels == 0 {
        return (0.0, 0.0);
    }

    let width_f = width.max(1) as f32;
    let height_f = height.max(1) as f32;
    let left_margin = min_x as f32 / width_f;
    let right_margin = width.saturating_sub(1).saturating_sub(max_x) as f32 / width_f;
    let top_margin = min_y as f32 / height_f;
    let bottom_margin = height.saturating_sub(1).saturating_sub(max_y) as f32 / height_f;
    let bbox_margin = left_margin
        .min(right_margin)
        .min(top_margin)
        .min(bottom_margin)
        .clamp(0.0, 1.0);
    let boundary_contact = (boundary_pixels as f32 / cloud_pixels as f32).clamp(0.0, 1.0);

    (bbox_margin, boundary_contact)
}

fn row_luminance(luminance: &[f32], width: u32, row: u32) -> f32 {
    let start = (row * width) as usize;
    let end = start + width as usize;
    luminance[start..end].iter().sum::<f32>() / width.max(1) as f32
}

fn fbm(x: f32, y: f32, octaves: u8, seed: u64) -> f32 {
    let mut value = 0.0;
    let mut amplitude = 0.5;
    let mut frequency = 1.0;
    let mut norm = 0.0;
    for octave in 0..octaves.max(1) {
        value += value_noise(
            x * frequency,
            y * frequency,
            seed.wrapping_add(octave as u64 * 131),
        ) * amplitude;
        norm += amplitude;
        amplitude *= 0.52;
        frequency *= 2.03;
    }
    if norm > 0.0 { value / norm } else { 0.0 }
}

fn value_noise(x: f32, y: f32, seed: u64) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let xf = x - xi as f32;
    let yf = y - yi as f32;
    let sx = xf * xf * (3.0 - 2.0 * xf);
    let sy = yf * yf * (3.0 - 2.0 * yf);
    let n00 = lattice_noise(xi, yi, seed);
    let n10 = lattice_noise(xi + 1, yi, seed);
    let n01 = lattice_noise(xi, yi + 1, seed);
    let n11 = lattice_noise(xi + 1, yi + 1, seed);
    let ix0 = lerp(n00, n10, sx);
    let ix1 = lerp(n01, n11, sx);
    lerp(ix0, ix1, sy)
}

fn lattice_noise(x: i32, y: i32, seed: u64) -> f32 {
    let mut value = seed
        ^ (x as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as i64 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    ((value >> 40) as f32) / ((1_u64 << 24) as f32)
}

fn seed_unit(seed: u64) -> f32 {
    lattice_noise(seed as i32, (seed >> 32) as i32, seed)
}

fn tone_map(color: [f32; 3], exposure: f32) -> [f32; 3] {
    let exposed = mul3(color, exposure.max(0.01));
    [
        aces(exposed[0]).powf(1.0 / 2.2),
        aces(exposed[1]).powf(1.0 / 2.2),
        aces(exposed[2]).powf(1.0 / 2.2),
    ]
}

fn aces(value: f32) -> f32 {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    ((value * (a * value + b)) / (value * (c * value + d) + e)).clamp(0.0, 1.0)
}

fn normalized_or_default(value: [f32; 3], default: [f32; 3]) -> [f32; 3] {
    let length = dot3(value, value).sqrt();
    if length > f32::EPSILON {
        [value[0] / length, value[1] / length, value[2] / length]
    } else {
        default
    }
}

fn camera_basis(camera: &CameraState, aspect: f32) -> CameraBasis {
    let forward = normalized_or_default(camera.forward, [0.0, 0.0, 1.0]);
    let requested_up = normalized_or_default(camera.up, [0.0, 1.0, 0.0]);
    let mut right = cross3(forward, requested_up);
    if dot3(right, right) <= 0.0001 {
        right = [1.0, 0.0, 0.0];
    } else {
        right = normalized_or_default(right, [1.0, 0.0, 0.0]);
    }
    let up = normalized_or_default(cross3(right, forward), [0.0, 1.0, 0.0]);
    let fov_y_radians = camera.fov_y_degrees.clamp(35.0, 95.0).to_radians();
    let tan_y = (fov_y_radians * 0.5).tan();

    CameraBasis {
        forward,
        right,
        up,
        fov_y_radians,
        tan_x: tan_y * aspect.max(0.1),
        tan_y,
    }
}

fn project_direction_to_screen(direction: [f32; 3], camera: CameraBasis) -> [f32; 2] {
    let view_z = dot3(direction, camera.forward);
    if view_z <= 0.01 {
        return [10.0, 10.0];
    }

    [
        0.5 + dot3(direction, camera.right) / (view_z * camera.tan_x * 2.0),
        0.5 - dot3(direction, camera.up) / (view_z * camera.tan_y * 2.0),
    ]
}

fn camera_ray(camera: CameraBasis, u: f32, v: f32) -> [f32; 3] {
    let ndc_x = (u * 2.0 - 1.0) * camera.tan_x;
    let ndc_y = (1.0 - v * 2.0) * camera.tan_y;
    normalized_or_default(
        add3(
            add3(camera.forward, mul3(camera.right, ndc_x)),
            mul3(camera.up, ndc_y),
        ),
        camera.forward,
    )
}

fn add3(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}

fn sub3(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn mul3(value: [f32; 3], scale: f32) -> [f32; 3] {
    [value[0] * scale, value[1] * scale, value[2] * scale]
}

fn mix3(left: [f32; 3], right: [f32; 3], t: f32) -> [f32; 3] {
    [
        lerp(left[0], right[0], t),
        lerp(left[1], right[1], t),
        lerp(left[2], right[2], t),
    ]
}

fn dot3(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn cross3(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn lerp(left: f32, right: f32, t: f32) -> f32 {
    left + (right - left) * t.clamp(0.0, 1.0)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if (edge1 - edge0).abs() <= f32::EPSILON {
        return if x >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn triangular_score(value: f32, low: f32, target: f32, high: f32) -> f32 {
    if value <= low || value >= high {
        0.0
    } else if value <= target {
        ((value - low) / (target - low)).clamp(0.0, 1.0)
    } else {
        ((high - value) / (high - target)).clamp(0.0, 1.0)
    }
}

fn saturate(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

fn to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_sky_plan_skips_clouds() {
        let result = plan_sky_smoke_frame(&SkySceneRequest::clear_sky_smoke(1), 3);

        assert_eq!(result.frame, RenderFrameId(3));
        assert!(
            result
                .feature_stats
                .iter()
                .any(|item| item == "clouds:skipped_empty")
        );
    }

    #[test]
    fn sky_capture_has_pixels_metrics_and_clouds() {
        let request = SkySceneRequest::cloudy_smoke(7);
        let capture = render_sky_capture(
            &request,
            SkyRenderSettings {
                width: 96,
                height: 54,
                ..SkyRenderSettings::fast()
            },
        );

        assert_eq!(capture.rgba.len(), 96 * 54 * 4);
        assert!(capture.metrics.fps > 0.0);
        assert!(capture.metrics.cloud_coverage > 0.05);
        assert!(capture.metrics.cloud_bbox_margin > 0.03);
        assert!(capture.metrics.cloud_boundary_contact <= 0.04);
        assert!(capture.metrics.sun_visibility > 0.2);
    }

    #[test]
    fn neural_features_are_normalized() {
        let request = SkySceneRequest::cloudy_smoke(8);
        let capture = render_sky_capture(
            &request,
            SkyRenderSettings {
                width: 64,
                height: 36,
                ..SkyRenderSettings::fast()
            },
        );

        for feature in capture.metrics.neural_features() {
            assert!((0.0..=1.0).contains(&feature));
        }
    }
}
