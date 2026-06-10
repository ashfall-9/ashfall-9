#![forbid(unsafe_code)]

use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use engine_core::{ENGINE_CORE_VERSION, EngineRuntime, GpuCapabilities, RenderQualityProfile};
use eval_lab::{BurnCloudPhotorealismJudge, CloudPhotorealismReport};
use gfx_vk::{VulkanBootstrapConfig, try_collect_gpu_capabilities};
use renderer_realtime::{
    RENDERER_REALTIME_VERSION, SkyCapture, SkyImageMetrics, SkyRenderSettings,
    plan_sky_smoke_frame, render_sky_capture,
};
use scene_schema::{SCENE_SCHEMA_VERSION, SkySceneRequest};
use serde::Serialize;
use telemetry::{
    ArtifactManifest, FrameTelemetry, artifact_path, write_frame_times_csv, write_json_pretty,
};

#[derive(Clone, Debug, PartialEq)]
struct HeadlessArgs {
    seed: u64,
    case_set: String,
    quality: RenderQualityProfile,
    out_dir: PathBuf,
    target_fps: f32,
    photorealism_threshold: f32,
    max_iterations: usize,
    warmup_frames: usize,
    measurement_frames: usize,
}

impl Default for HeadlessArgs {
    fn default() -> Self {
        Self {
            seed: 1,
            case_set: "locked_smoke_003".to_string(),
            quality: RenderQualityProfile::High,
            out_dir: PathBuf::from("target/eval_smoke"),
            target_fps: 80.0,
            photorealism_threshold: 0.72,
            max_iterations: SkyRenderSettings::candidate_ladder().len(),
            warmup_frames: 1,
            measurement_frames: 3,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct HeadlessSettings {
    schema_version: String,
    seed: u64,
    case_set: String,
    quality_profile: String,
    target_fps: f32,
    photorealism_threshold: f32,
    warmup_frames: usize,
    measurement_frames: usize,
    selected_settings: SkyRenderSettings,
    cases: Vec<SkyEvalCase>,
}

#[derive(Clone, Debug, Serialize)]
struct BuildInfo {
    schema_version: String,
    cargo_profile: String,
    features: Vec<String>,
    debug_assertions: bool,
}

fn main() -> anyhow::Result<()> {
    let args = parse_args(std::env::args().skip(1))?;
    let report = run_headless_once(&args)?;

    println!(
        "Ashfall headless cloud iteration wrote {} with {} frame(s). GPU: {}. FPS {:.1}, photorealism {:.3}, pass {}",
        args.out_dir.display(),
        report.frame_count,
        report.gpu_name,
        report.fps(),
        report.photorealism_score(),
        report.passed
    );
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HeadlessRunReport {
    frame_count: usize,
    gpu_name: String,
    passed: bool,
    fps_label: String,
    score_label: String,
}

impl HeadlessRunReport {
    fn fps(&self) -> f32 {
        self.fps_label.parse().unwrap_or_default()
    }

    fn photorealism_score(&self) -> f32 {
        self.score_label.parse().unwrap_or_default()
    }
}

#[derive(Clone, Debug, Serialize)]
struct IterationReport {
    iteration: usize,
    settings: SkyRenderSettings,
    min_fps: f32,
    min_photorealism_score: f32,
    cases: Vec<CaseIterationReport>,
    fps_pass: bool,
    photorealism_pass: bool,
    pass: bool,
}

#[derive(Clone, Debug, Serialize)]
struct CaseIterationReport {
    case_id: String,
    metrics: SkyImageMetrics,
    fps_measurement: FpsMeasurement,
    photorealism: CloudPhotorealismReport,
    fps_pass: bool,
    photorealism_pass: bool,
    pass: bool,
}

#[derive(Clone, Debug, Serialize)]
struct CloudIterationSummary {
    schema_version: String,
    case_set: String,
    case_count: usize,
    target_fps: f32,
    photorealism_threshold: f32,
    warmup_frames: usize,
    measurement_frames: usize,
    selected_iteration: usize,
    passed: bool,
    iterations: Vec<IterationReport>,
}

#[derive(Clone, Debug, Serialize)]
struct FpsMeasurement {
    frame_count: usize,
    mean_frame_ms: f32,
    p50_frame_ms: f32,
    p95_frame_ms: f32,
    mean_fps: f32,
    p95_fps: f32,
    frame_times_ms: Vec<f32>,
}

#[derive(Clone, Debug, Serialize)]
struct PassFailDecision {
    schema_version: String,
    milestone: String,
    decision: String,
    fps: f32,
    target_fps: f32,
    photorealism_score: f32,
    photorealism_threshold: f32,
    case_count: usize,
    reasons: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct SkyEvalCase {
    id: String,
    sky: SkySceneRequest,
}

#[derive(Clone, Debug)]
struct EvaluatedCase {
    case_id: String,
    capture: SkyCapture,
    photorealism: CloudPhotorealismReport,
    fps_measurement: FpsMeasurement,
}

fn run_headless_once(args: &HeadlessArgs) -> anyhow::Result<HeadlessRunReport> {
    let run_id = run_id(args.seed);
    let created_utc = created_utc_string();
    let mut runtime = EngineRuntime::new(args.quality);
    let frame = runtime.advance_frame(1.0 / 60.0);
    let cases = eval_cases(args)?;
    for case in &cases {
        let _planned = plan_sky_smoke_frame(&case.sky, frame.frame_index);
    }
    let gpu = collect_gpu_or_fallback();
    let judge = BurnCloudPhotorealismJudge::new(args.photorealism_threshold);
    let (selected_iteration, selected_settings, selected_cases, iterations) =
        iterate_cloud_settings(&cases, args, &judge)?;
    let final_png = "captures/final_sdr.png";
    let representative = selected_cases
        .first()
        .ok_or_else(|| anyhow::anyhow!("no selected sky cases were evaluated"))?;
    representative
        .capture
        .write_png(artifact_path(&args.out_dir, final_png))?;

    let mut case_capture_files = Vec::new();
    for selected_case in &selected_cases {
        let path = format!("captures/{}_sdr.png", selected_case.case_id);
        selected_case
            .capture
            .write_png(artifact_path(&args.out_dir, &path))?;
        case_capture_files.push(path);
    }

    write_json_pretty(
        artifact_path(&args.out_dir, "settings.json"),
        &HeadlessSettings {
            schema_version: "sky_settings.v1".to_string(),
            seed: args.seed,
            case_set: args.case_set.clone(),
            quality_profile: quality_label(args.quality).to_string(),
            target_fps: args.target_fps,
            photorealism_threshold: args.photorealism_threshold,
            warmup_frames: args.warmup_frames,
            measurement_frames: args.measurement_frames,
            selected_settings,
            cases: cases.clone(),
        },
    )?;
    write_json_pretty(
        artifact_path(&args.out_dir, "build_info.json"),
        &BuildInfo {
            schema_version: "build_info.v1".to_string(),
            cargo_profile: std::env::var("PROFILE").unwrap_or_else(|_| "dev".to_string()),
            features: Vec::new(),
            debug_assertions: cfg!(debug_assertions),
        },
    )?;
    write_json_pretty(artifact_path(&args.out_dir, "gpu_info.json"), &gpu)?;
    let worst_visual_judge = worst_visual_judge(&selected_cases)?;
    write_json_pretty(
        artifact_path(&args.out_dir, "ai_reports/visual_judge.json"),
        worst_visual_judge,
    )?;
    let mut case_judge_files = Vec::new();
    for selected_case in &selected_cases {
        let path = format!("ai_reports/{}_visual_judge.json", selected_case.case_id);
        write_json_pretty(
            artifact_path(&args.out_dir, &path),
            &selected_case.photorealism,
        )?;
        case_judge_files.push(path);
    }
    write_json_pretty(
        artifact_path(&args.out_dir, "telemetry/cloud_iteration.json"),
        &CloudIterationSummary {
            schema_version: "cloud_iteration_summary.v1".to_string(),
            case_set: args.case_set.clone(),
            case_count: cases.len(),
            target_fps: args.target_fps,
            photorealism_threshold: args.photorealism_threshold,
            warmup_frames: args.warmup_frames,
            measurement_frames: args.measurement_frames,
            selected_iteration,
            passed: selected_cases
                .iter()
                .all(|case| case.capture.metrics.fps >= args.target_fps && case.photorealism.pass),
            iterations,
        },
    )?;
    let decision = decision_for(args, &selected_cases);
    write_json_pretty(
        artifact_path(&args.out_dir, "decision/pass_fail.json"),
        &decision,
    )?;
    write_frame_times_csv(
        artifact_path(&args.out_dir, "telemetry/frame_times.csv"),
        &selected_frame_telemetry(frame.frame_index, &selected_cases),
    )?;

    let manifest = ArtifactManifest::sky_smoke(
        run_id,
        created_utc,
        args.case_set.clone(),
        quality_label(args.quality),
    )
    .with_interface_version(ENGINE_CORE_VERSION)
    .with_interface_version(SCENE_SCHEMA_VERSION)
    .with_interface_version(RENDERER_REALTIME_VERSION)
    .with_file("settings.json", "settings")
    .with_file("build_info.json", "build_info")
    .with_file("gpu_info.json", "gpu_info")
    .with_file(final_png, "sdr_preview")
    .with_file("telemetry/frame_times.csv", "frame_times")
    .with_file("telemetry/cloud_iteration.json", "cloud_iteration")
    .with_file("ai_reports/visual_judge.json", "visual_judge")
    .with_file("decision/pass_fail.json", "pass_fail");
    let mut manifest = manifest;
    for path in case_capture_files {
        manifest = manifest.with_file(path, "case_sdr_preview");
    }
    for path in case_judge_files {
        manifest = manifest.with_file(path, "case_visual_judge");
    }
    write_json_pretty(artifact_path(&args.out_dir, "manifest.json"), &manifest)?;

    Ok(HeadlessRunReport {
        frame_count: selected_cases.len() * args.measurement_frames.max(1),
        gpu_name: gpu.device_name,
        passed: decision.decision == "pass",
        fps_label: format!("{:.1}", min_selected_fps(&selected_cases)),
        score_label: format!("{:.3}", min_selected_score(&selected_cases)),
    })
}

fn iterate_cloud_settings(
    cases: &[SkyEvalCase],
    args: &HeadlessArgs,
    judge: &BurnCloudPhotorealismJudge,
) -> anyhow::Result<(
    usize,
    SkyRenderSettings,
    Vec<EvaluatedCase>,
    Vec<IterationReport>,
)> {
    let mut iterations = Vec::new();
    let mut best: Option<(usize, SkyRenderSettings, Vec<EvaluatedCase>, f32)> = None;

    for (index, settings) in SkyRenderSettings::candidate_ladder()
        .into_iter()
        .take(args.max_iterations.max(1))
        .enumerate()
    {
        let evaluated_cases = evaluate_candidate(cases, settings, args, judge)?;
        let report = candidate_report(index, settings, &evaluated_cases, args);
        let pass = report.pass;
        let rank = candidate_rank(&evaluated_cases, args);
        iterations.push(report);

        if pass {
            return Ok((index, settings, evaluated_cases, iterations));
        }

        if best
            .as_ref()
            .map(|(_, _, _, best_rank)| rank > *best_rank)
            .unwrap_or(true)
        {
            best = Some((index, settings, evaluated_cases, rank));
        }
    }

    best.map(|(index, settings, evaluated_cases, _)| (index, settings, evaluated_cases, iterations))
        .ok_or_else(|| anyhow::anyhow!("no cloud render settings were attempted"))
}

fn evaluate_candidate(
    cases: &[SkyEvalCase],
    settings: SkyRenderSettings,
    args: &HeadlessArgs,
    judge: &BurnCloudPhotorealismJudge,
) -> anyhow::Result<Vec<EvaluatedCase>> {
    let mut evaluated = Vec::with_capacity(cases.len());
    for case in cases {
        let (capture, fps_measurement) = measure_candidate(&case.sky, settings, args)?;
        let photorealism = judge.judge(&capture.metrics);
        evaluated.push(EvaluatedCase {
            case_id: case.id.clone(),
            capture,
            photorealism,
            fps_measurement,
        });
    }
    Ok(evaluated)
}

fn candidate_report(
    iteration: usize,
    settings: SkyRenderSettings,
    evaluated_cases: &[EvaluatedCase],
    args: &HeadlessArgs,
) -> IterationReport {
    let cases = evaluated_cases
        .iter()
        .map(|evaluated| {
            let fps_pass = evaluated.capture.metrics.fps >= args.target_fps;
            let photorealism_pass = evaluated.photorealism.pass;
            CaseIterationReport {
                case_id: evaluated.case_id.clone(),
                metrics: evaluated.capture.metrics.clone(),
                fps_measurement: evaluated.fps_measurement.clone(),
                photorealism: evaluated.photorealism.clone(),
                fps_pass,
                photorealism_pass,
                pass: fps_pass && photorealism_pass,
            }
        })
        .collect::<Vec<_>>();
    let fps_pass = cases.iter().all(|case| case.fps_pass);
    let photorealism_pass = cases.iter().all(|case| case.photorealism_pass);

    IterationReport {
        iteration,
        settings,
        min_fps: evaluated_cases
            .iter()
            .map(|case| case.capture.metrics.fps)
            .fold(f32::INFINITY, f32::min),
        min_photorealism_score: evaluated_cases
            .iter()
            .map(|case| case.photorealism.score)
            .fold(f32::INFINITY, f32::min),
        cases,
        fps_pass,
        photorealism_pass,
        pass: fps_pass && photorealism_pass,
    }
}

fn measure_candidate(
    sky: &SkySceneRequest,
    settings: SkyRenderSettings,
    args: &HeadlessArgs,
) -> anyhow::Result<(SkyCapture, FpsMeasurement)> {
    for _ in 0..args.warmup_frames {
        let _ = render_sky_capture(sky, settings);
    }

    let frame_count = args.measurement_frames.max(1);
    let mut frame_times = Vec::with_capacity(frame_count);
    let mut last_capture = None;
    for _ in 0..frame_count {
        let capture = render_sky_capture(sky, settings);
        frame_times.push(capture.metrics.render_ms);
        last_capture = Some(capture);
    }

    let measurement = FpsMeasurement::from_frame_times(frame_times);
    let mut capture =
        last_capture.ok_or_else(|| anyhow::anyhow!("no measured render frames were produced"))?;
    capture.metrics.render_ms = measurement.p95_frame_ms;
    capture.metrics.fps = measurement.p95_fps;
    Ok((capture, measurement))
}

impl FpsMeasurement {
    fn from_frame_times(mut frame_times_ms: Vec<f32>) -> Self {
        frame_times_ms.retain(|value| value.is_finite() && *value >= 0.0);
        if frame_times_ms.is_empty() {
            frame_times_ms.push(0.0);
        }
        let frame_count = frame_times_ms.len();
        let mean_frame_ms = frame_times_ms.iter().sum::<f32>() / frame_count as f32;
        let mut sorted = frame_times_ms.clone();
        sorted.sort_by(|left, right| left.total_cmp(right));
        let p50_frame_ms = percentile(&sorted, 0.50);
        let p95_frame_ms = percentile(&sorted, 0.95);
        Self {
            frame_count,
            mean_frame_ms,
            p50_frame_ms,
            p95_frame_ms,
            mean_fps: fps_from_ms(mean_frame_ms),
            p95_fps: fps_from_ms(p95_frame_ms),
            frame_times_ms,
        }
    }
}

fn percentile(sorted_values: &[f32], percentile: f32) -> f32 {
    if sorted_values.is_empty() {
        return 0.0;
    }
    let last_index = sorted_values.len() - 1;
    let rank = (last_index as f32 * percentile.clamp(0.0, 1.0)).ceil() as usize;
    sorted_values[rank.min(last_index)]
}

fn fps_from_ms(frame_ms: f32) -> f32 {
    if frame_ms > f32::EPSILON {
        1000.0 / frame_ms
    } else {
        0.0
    }
}

fn selected_frame_telemetry(
    first_frame_index: u64,
    cases: &[EvaluatedCase],
) -> Vec<FrameTelemetry> {
    cases
        .iter()
        .flat_map(|case| case.fps_measurement.frame_times_ms.iter())
        .enumerate()
        .map(|(offset, frame_ms)| FrameTelemetry {
            frame_index: first_frame_index + offset as u64,
            cpu_frame_ms: *frame_ms,
            gpu_frame_ms: None,
            present_ms: None,
            draw_calls: 1,
            dispatch_calls: 0,
            validation_errors: 0,
        })
        .collect()
}

fn candidate_rank(cases: &[EvaluatedCase], args: &HeadlessArgs) -> f32 {
    let min_fps_score = cases
        .iter()
        .map(|case| (case.capture.metrics.fps / args.target_fps).clamp(0.0, 1.25))
        .fold(f32::INFINITY, f32::min);
    let min_visual_score = cases
        .iter()
        .map(|case| {
            if case.photorealism.pass {
                case.photorealism.score
            } else {
                case.photorealism.score * 0.65
            }
        })
        .fold(f32::INFINITY, f32::min);
    min_fps_score * 0.45 + min_visual_score * 0.55
}

fn decision_for(args: &HeadlessArgs, cases: &[EvaluatedCase]) -> PassFailDecision {
    let mut reasons = Vec::new();
    for case in cases {
        if case.capture.metrics.fps < args.target_fps {
            reasons.push(format!(
                "{} FPS {:.1} is below target {:.1}",
                case.case_id, case.capture.metrics.fps, args.target_fps
            ));
        }
        if !case.photorealism.pass {
            if case.photorealism.score < args.photorealism_threshold {
                reasons.push(format!(
                    "{} Burn photorealism score {:.3} is below threshold {:.3}",
                    case.case_id, case.photorealism.score, args.photorealism_threshold
                ));
            }
            for note in &case.photorealism.notes {
                reasons.push(format!("{} Burn photorealism gate: {note}", case.case_id));
            }
        }
    }
    if reasons.is_empty() {
        reasons.push("All sky cases passed FPS and Burn photorealism gates".to_string());
    }

    let fps = min_selected_fps(cases);
    let photorealism_score = min_selected_score(cases);
    let passed = cases
        .iter()
        .all(|case| case.capture.metrics.fps >= args.target_fps && case.photorealism.pass);

    PassFailDecision {
        schema_version: "milestone_decision.v1".to_string(),
        milestone: "sky_cloud_smoke".to_string(),
        decision: if passed {
            "pass".to_string()
        } else {
            "fail".to_string()
        },
        fps,
        target_fps: args.target_fps,
        photorealism_score,
        photorealism_threshold: args.photorealism_threshold,
        case_count: cases.len(),
        reasons,
    }
}

fn min_selected_fps(cases: &[EvaluatedCase]) -> f32 {
    cases
        .iter()
        .map(|case| case.capture.metrics.fps)
        .fold(f32::INFINITY, f32::min)
}

fn min_selected_score(cases: &[EvaluatedCase]) -> f32 {
    cases
        .iter()
        .map(|case| case.photorealism.score)
        .fold(f32::INFINITY, f32::min)
}

fn worst_visual_judge(cases: &[EvaluatedCase]) -> anyhow::Result<&CloudPhotorealismReport> {
    cases
        .iter()
        .map(|case| &case.photorealism)
        .min_by(|left, right| left.score.total_cmp(&right.score))
        .ok_or_else(|| anyhow::anyhow!("no visual judge reports were produced"))
}

fn eval_cases(args: &HeadlessArgs) -> anyhow::Result<Vec<SkyEvalCase>> {
    match args.case_set.as_str() {
        "locked_smoke_001" | "single" => Ok(vec![SkyEvalCase {
            id: format!("seed_{}", args.seed),
            sky: SkySceneRequest::cloudy_smoke(args.seed),
        }]),
        "locked_smoke_003" => Ok(vec![
            SkyEvalCase {
                id: "broken_cumulus".to_string(),
                sky: cloudy_case(args.seed, 0.35, 0.45, [0.14, 0.32, 0.94]),
            },
            SkyEvalCase {
                id: "sun_edge".to_string(),
                sky: cloudy_case(args.seed.wrapping_add(17), 0.42, 0.58, [0.26, 0.20, 0.94]),
            },
            SkyEvalCase {
                id: "wide_mixed".to_string(),
                sky: cloudy_case(args.seed.wrapping_add(37), 0.30, 0.48, [-0.18, 0.34, 0.92]),
            },
        ]),
        other => anyhow::bail!("unknown case set: {other}"),
    }
}

fn cloudy_case(seed: u64, coverage: f32, density: f32, sun_direction: [f32; 3]) -> SkySceneRequest {
    let mut sky = SkySceneRequest::cloudy_smoke(seed);
    sky.camera.fov_y_degrees = 72.0;
    sky.sun.direction = sun_direction;
    if let Some(cloud) = sky.clouds.first_mut() {
        cloud.coverage = coverage;
        cloud.density = density;
        cloud.seed = seed;
    }
    sky
}

fn collect_gpu_or_fallback() -> GpuCapabilities {
    try_collect_gpu_capabilities(VulkanBootstrapConfig {
        print_device_name: false,
        ..VulkanBootstrapConfig::default()
    })
    .unwrap_or_else(|error| GpuCapabilities::unavailable(error.to_string()))
}

fn parse_args(args: impl IntoIterator<Item = String>) -> anyhow::Result<HeadlessArgs> {
    let mut parsed = HeadlessArgs::default();
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seed" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--seed requires a value"))?;
                parsed.seed = value.parse()?;
            }
            "--case-set" => {
                parsed.case_set = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--case-set requires a value"))?;
            }
            "--profile" | "--quality" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("{arg} requires a value"))?;
                parsed.quality = parse_quality(&value)?;
            }
            "--out" | "--output-dir" => {
                parsed.out_dir = PathBuf::from(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a value"))?,
                );
            }
            "--target-fps" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--target-fps requires a value"))?;
                parsed.target_fps = value.parse()?;
            }
            "--photorealism-threshold" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--photorealism-threshold requires a value"))?;
                parsed.photorealism_threshold = value.parse::<f32>()?.clamp(0.0, 1.0);
            }
            "--max-iterations" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--max-iterations requires a value"))?;
                parsed.max_iterations = value.parse::<usize>()?.max(1);
            }
            "--warmup-frames" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--warmup-frames requires a value"))?;
                parsed.warmup_frames = value.parse::<usize>()?;
            }
            "--measurement-frames" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--measurement-frames requires a value"))?;
                parsed.measurement_frames = value.parse::<usize>()?.max(1);
            }
            "--help" | "-h" => {
                anyhow::bail!(
                    "usage: app_headless [--seed N] [--case-set locked_smoke_001|locked_smoke_003|single] [--profile smoke|low|medium|high|certification] [--out PATH] [--target-fps N] [--photorealism-threshold N] [--max-iterations N] [--warmup-frames N] [--measurement-frames N]"
                );
            }
            other => anyhow::bail!("unknown argument: {other}"),
        }
    }

    Ok(parsed)
}

fn parse_quality(value: &str) -> anyhow::Result<RenderQualityProfile> {
    match value {
        "smoke" => Ok(RenderQualityProfile::Smoke),
        "low" => Ok(RenderQualityProfile::Low),
        "medium" => Ok(RenderQualityProfile::Medium),
        "high" => Ok(RenderQualityProfile::High),
        "certification" | "cert" => Ok(RenderQualityProfile::Certification),
        _ => anyhow::bail!("unknown quality profile: {value}"),
    }
}

fn quality_label(quality: RenderQualityProfile) -> &'static str {
    match quality {
        RenderQualityProfile::Smoke => "smoke",
        RenderQualityProfile::Low => "low",
        RenderQualityProfile::Medium => "medium",
        RenderQualityProfile::High => "high",
        RenderQualityProfile::Certification => "certification",
    }
}

fn run_id(seed: u64) -> String {
    format!(
        "{}_sky_smoke_seed_{seed}",
        compact_utc_timestamp(epoch_seconds())
    )
}

fn created_utc_string() -> String {
    iso_utc_timestamp(epoch_seconds())
}

fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn iso_utc_timestamp(seconds: u64) -> String {
    let (year, month, day, hour, minute, second) = utc_parts(seconds);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn compact_utc_timestamp(seconds: u64) -> String {
    let (year, month, day, hour, minute, second) = utc_parts(seconds);
    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

fn utc_parts(seconds: u64) -> (i32, u32, u32, u32, u32, u32) {
    let days = (seconds / 86_400) as i64;
    let seconds_of_day = seconds % 86_400;
    let hour = (seconds_of_day / 3_600) as u32;
    let minute = ((seconds_of_day % 3_600) / 60) as u32;
    let second = (seconds_of_day % 60) as u32;
    let (year, month, day) = civil_from_days(days);
    (year, month, day, hour, minute, second)
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_param = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_param + 2) / 5 + 1;
    let month = month_param + if month_param < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year as i32, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_headless_args() {
        let args = parse_args([
            "--seed".to_string(),
            "42".to_string(),
            "--profile".to_string(),
            "high".to_string(),
            "--out".to_string(),
            "target/custom".to_string(),
        ])
        .expect("args should parse");

        assert_eq!(args.seed, 42);
        assert_eq!(args.case_set, "locked_smoke_003");
        assert_eq!(args.quality, RenderQualityProfile::High);
        assert_eq!(args.out_dir, PathBuf::from("target/custom"));
        assert_eq!(args.target_fps, 80.0);
        assert_eq!(args.warmup_frames, 1);
        assert_eq!(args.measurement_frames, 3);
    }

    #[test]
    fn timestamps_use_utc_iso_format() {
        assert_eq!(iso_utc_timestamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(compact_utc_timestamp(0), "19700101T000000Z");
    }
}
