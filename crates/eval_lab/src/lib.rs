#![forbid(unsafe_code)]

use std::{fs, path::Path};

use burn::{
    backend::{NdArray, ndarray::NdArrayDevice},
    tensor::{Tensor, activation},
};
use renderer_realtime::SkyImageMetrics;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use telemetry::ArtifactManifest;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactValidationReport {
    pub passed: bool,
    pub issues: Vec<String>,
}

pub fn validate_artifact_bundle(
    root: impl AsRef<Path>,
) -> anyhow::Result<ArtifactValidationReport> {
    let root = root.as_ref();
    let mut issues = Vec::new();

    for required in [
        "manifest.json",
        "build_info.json",
        "settings.json",
        "gpu_info.json",
        "captures/final_sdr.png",
        "telemetry/frame_times.csv",
        "telemetry/cloud_iteration.json",
        "ai_reports/visual_judge.json",
        "decision/pass_fail.json",
    ] {
        if !root.join(required).exists() {
            issues.push(format!("missing required artifact: {required}"));
        }
    }

    let settings_path = root.join("settings.json");
    let settings = if settings_path.exists() {
        read_json::<Value>(&settings_path, &mut issues, "settings.json")?
    } else {
        None
    };
    let settings_case_set = settings
        .as_ref()
        .and_then(|settings| settings.get("case_set"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let settings_case_count = settings
        .as_ref()
        .and_then(|settings| settings.get("cases"))
        .and_then(Value::as_array)
        .map(Vec::len);

    let manifest_path = root.join("manifest.json");
    let manifest = if manifest_path.exists() {
        read_json::<ArtifactManifest>(&manifest_path, &mut issues, "manifest.json")?
    } else {
        None
    };
    if let Some(manifest) = &manifest {
        if manifest.status != "complete" {
            issues.push(format!(
                "manifest status is not complete: {}",
                manifest.status
            ));
        }
        if let Some(settings_case_set) = &settings_case_set
            && manifest.seed_set != *settings_case_set
        {
            issues.push(format!(
                "manifest seed_set {} does not match settings case_set {}",
                manifest.seed_set, settings_case_set
            ));
        }
        for file in &manifest.files {
            if !root.join(&file.path).exists() {
                issues.push(format!("manifest references missing file: {}", file.path));
            }
        }
        if let Some(case_count) = settings_case_count
            && case_count > 1
        {
            let case_capture_count = manifest
                .files
                .iter()
                .filter(|file| file.kind == "case_sdr_preview")
                .count();
            if case_capture_count < case_count {
                issues.push(format!(
                    "manifest has {case_capture_count} case_sdr_preview file(s), expected at least {case_count}"
                ));
            }
            let case_judge_count = manifest
                .files
                .iter()
                .filter(|file| file.kind == "case_visual_judge")
                .count();
            if case_judge_count < case_count {
                issues.push(format!(
                    "manifest has {case_judge_count} case_visual_judge file(s), expected at least {case_count}"
                ));
            }
        }
    }

    let frame_csv = root.join("telemetry/frame_times.csv");
    if frame_csv.exists() {
        let header = fs::read_to_string(&frame_csv)?
            .lines()
            .next()
            .unwrap_or_default()
            .to_string();
        if header
            != "frame_index,cpu_frame_ms,gpu_frame_ms,present_ms,draw_calls,dispatch_calls,validation_errors"
        {
            issues.push("frame_times.csv header does not match schema".to_string());
        }
    }

    let png_path = root.join("captures/final_sdr.png");
    if png_path.exists() {
        let bytes = fs::read(&png_path)?;
        if !bytes.starts_with(b"\x89PNG\r\n\x1A\n") {
            issues.push("captures/final_sdr.png is not a PNG file".to_string());
        }
    }

    let visual_judge_path = root.join("ai_reports/visual_judge.json");
    if visual_judge_path.exists()
        && let Some(report) = read_json::<CloudPhotorealismReport>(
            &visual_judge_path,
            &mut issues,
            "ai_reports/visual_judge.json",
        )?
    {
        validate_visual_judge_report(&report, "ai_reports/visual_judge.json", &mut issues);
    }

    if let Some(manifest) = &manifest {
        for file in manifest
            .files
            .iter()
            .filter(|file| file.kind == "case_visual_judge")
        {
            let path = root.join(&file.path);
            if path.exists()
                && let Some(report) =
                    read_json::<CloudPhotorealismReport>(&path, &mut issues, &file.path)?
            {
                validate_visual_judge_report(&report, &file.path, &mut issues);
            }
        }
    }

    let iteration_path = root.join("telemetry/cloud_iteration.json");
    if iteration_path.exists()
        && let Some(summary) = read_json::<Value>(
            &iteration_path,
            &mut issues,
            "telemetry/cloud_iteration.json",
        )?
    {
        if summary.get("schema_version").and_then(Value::as_str)
            != Some("cloud_iteration_summary.v1")
        {
            issues.push("cloud_iteration.json schema version is unexpected".to_string());
        }
        if summary
            .get("iterations")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
        {
            issues.push("cloud_iteration.json has no iteration records".to_string());
        }
        if let Some(settings_case_set) = &settings_case_set
            && summary.get("case_set").and_then(Value::as_str) != Some(settings_case_set.as_str())
        {
            issues.push("cloud_iteration.json case_set does not match settings.json".to_string());
        }
        if let Some(case_count) = settings_case_count
            && summary.get("case_count").and_then(Value::as_u64) != Some(case_count as u64)
        {
            issues.push("cloud_iteration.json case_count does not match settings.json".to_string());
        }
    }

    let decision_path = root.join("decision/pass_fail.json");
    if decision_path.exists()
        && let Some(decision) =
            read_json::<Value>(&decision_path, &mut issues, "decision/pass_fail.json")?
    {
        if let Some(decision_value) = decision.get("decision").and_then(Value::as_str) {
            if !matches!(decision_value, "pass" | "fail" | "inconclusive") {
                issues.push(format!(
                    "pass_fail.json has invalid decision: {decision_value}"
                ));
            }
        } else {
            issues.push("pass_fail.json is missing decision".to_string());
        }
        if let Some(case_count) = settings_case_count
            && decision.get("case_count").and_then(Value::as_u64) != Some(case_count as u64)
        {
            issues.push("pass_fail.json case_count does not match settings.json".to_string());
        }
    }

    Ok(ArtifactValidationReport {
        passed: issues.is_empty(),
        issues,
    })
}

fn validate_visual_judge_report(
    report: &CloudPhotorealismReport,
    label: &str,
    issues: &mut Vec<String>,
) {
    if report.schema_version != "cloud_photorealism_report.v1" {
        issues.push(format!(
            "{label} schema version is unexpected: {}",
            report.schema_version
        ));
    }
    if !(0.0..=1.0).contains(&report.score) {
        issues.push(format!("{label} score is outside 0..=1"));
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(
    path: &Path,
    issues: &mut Vec<String>,
    label: &str,
) -> anyhow::Result<Option<T>> {
    let json = fs::read_to_string(path)?;
    match serde_json::from_str(&json) {
        Ok(value) => Ok(Some(value)),
        Err(error) => {
            issues.push(format!("{label} does not parse as expected JSON: {error}"));
            Ok(None)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CloudPhotorealismReport {
    pub schema_version: String,
    pub evaluator: String,
    pub model: String,
    pub threshold: f32,
    pub score: f32,
    pub pass: bool,
    pub features: CloudPhotorealismFeatures,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CloudPhotorealismFeatures {
    pub cloud_coverage_score: f32,
    pub edge_density_score: f32,
    pub detail_energy_score: f32,
    pub sun_visibility_score: f32,
    pub contrast_score: f32,
    pub color_variance_score: f32,
    pub horizon_gradient_score: f32,
    pub mean_luminance_score: f32,
    pub clear_sky_score: f32,
    pub cloud_alpha_variance_score: f32,
    pub cloud_core_fraction_score: f32,
    pub sun_cloud_contrast_score: f32,
    pub whole_cloud_margin_score: f32,
    pub frame_boundary_clearance_score: f32,
}

impl From<[f32; 14]> for CloudPhotorealismFeatures {
    fn from(features: [f32; 14]) -> Self {
        Self {
            cloud_coverage_score: features[0],
            edge_density_score: features[1],
            detail_energy_score: features[2],
            sun_visibility_score: features[3],
            contrast_score: features[4],
            color_variance_score: features[5],
            horizon_gradient_score: features[6],
            mean_luminance_score: features[7],
            clear_sky_score: features[8],
            cloud_alpha_variance_score: features[9],
            cloud_core_fraction_score: features[10],
            sun_cloud_contrast_score: features[11],
            whole_cloud_margin_score: features[12],
            frame_boundary_clearance_score: features[13],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BurnCloudPhotorealismJudge {
    threshold: f32,
}

impl BurnCloudPhotorealismJudge {
    pub fn new(threshold: f32) -> Self {
        Self {
            threshold: threshold.clamp(0.0, 1.0),
        }
    }

    pub fn judge(&self, metrics: &SkyImageMetrics) -> CloudPhotorealismReport {
        let features = metrics.neural_features();
        let score = burn_photorealism_score(features);
        let mut notes = Vec::new();
        let mut hard_gate_pass = true;
        if score < self.threshold {
            notes.push("Burn metric network score is below threshold".to_string());
        }
        if metrics.cloud_coverage < 0.10 {
            hard_gate_pass = false;
            notes.push("Cloud coverage is too sparse for the cloud milestone".to_string());
        }
        if metrics.cloud_coverage > 0.78 {
            hard_gate_pass = false;
            notes.push("Cloud coverage is too dense; visible sky structure is lost".to_string());
        }
        if metrics.clear_sky_fraction < 0.08 {
            hard_gate_pass = false;
            notes.push("Clear-sky fraction is too low for the sun/cloud milestone".to_string());
        }
        if metrics.cloud_bbox_margin < 0.035 {
            hard_gate_pass = false;
            notes.push(
                "Cloud footprint margin is too small; the whole cloud is not visible".to_string(),
            );
        }
        if metrics.cloud_boundary_contact > 0.04 {
            hard_gate_pass = false;
            notes.push(
                "Cloud footprint touches the frame edge; camera is too close or cloud is clipped"
                    .to_string(),
            );
        }
        if metrics.edge_density < 0.014 {
            hard_gate_pass = false;
            notes.push("Cloud edge density is too low; cloud shape reads as flat".to_string());
        }
        if metrics.detail_energy < 0.00020 {
            hard_gate_pass = false;
            notes.push("Cloud detail energy is too low; fine structure is missing".to_string());
        }
        if metrics.cloud_alpha_variance < 0.010 {
            hard_gate_pass = false;
            notes.push(
                "Cloud alpha variance is too low; layer looks like a uniform sheet".to_string(),
            );
        }
        if metrics.cloud_core_fraction > 0.88 && metrics.edge_density < 0.022 {
            hard_gate_pass = false;
            notes.push(
                "Cloud core fraction is too high; cloud reads as a smooth solid blob".to_string(),
            );
        }
        if metrics.sun_visibility < 0.25 {
            hard_gate_pass = false;
            notes.push("Sun is too obscured or dim in the rendered image".to_string());
        }
        if metrics.sun_cloud_contrast < 0.04 {
            hard_gate_pass = false;
            notes
                .push("Sun and cloud luminance are too similar for plausible lighting".to_string());
        }
        let pass = score >= self.threshold && hard_gate_pass;

        CloudPhotorealismReport {
            schema_version: "cloud_photorealism_report.v1".to_string(),
            evaluator: "burn_ndarray_fixed_weight_metric_network".to_string(),
            model: "ashfall_distant_cloud_metric_mlp_v2_fixed_weights".to_string(),
            threshold: self.threshold,
            score,
            pass,
            features: features.into(),
            notes,
        }
    }
}

impl Default for BurnCloudPhotorealismJudge {
    fn default() -> Self {
        Self::new(0.72)
    }
}

type JudgeBackend = NdArray<f32>;

fn burn_photorealism_score(features: [f32; 14]) -> f32 {
    let device = NdArrayDevice::Cpu;
    let input = Tensor::<JudgeBackend, 2>::from_data([features], &device);
    let w1 = Tensor::<JudgeBackend, 2>::from_data(
        [
            [1.10, 0.20, 0.35, 0.10, 0.15, 0.05, 0.40, 0.18],
            [0.25, 0.95, 0.40, 0.15, 0.08, 0.12, 0.24, 0.20],
            [0.30, 0.35, 1.08, 0.25, 0.20, 0.10, 0.22, 0.32],
            [0.45, 0.10, 0.18, 1.05, 0.25, 0.30, 0.28, 0.22],
            [0.18, 0.25, 0.25, 0.20, 0.92, 0.18, 0.18, 0.20],
            [0.20, 0.18, 0.35, 0.12, 0.70, 0.85, 0.22, 0.24],
            [0.28, 0.08, 0.14, 0.42, 0.30, 0.78, 0.16, 0.18],
            [0.24, 0.16, 0.12, 0.30, 0.58, 0.30, 0.18, 0.22],
            [0.76, 0.16, 0.10, 0.24, 0.18, 0.16, 0.78, 0.28],
            [0.22, 0.50, 0.58, 0.08, 0.34, 0.22, 0.26, 0.84],
            [0.48, 0.18, 0.34, 0.10, 0.30, 0.16, 0.40, 0.62],
            [0.18, 0.26, 0.28, 0.48, 0.52, 0.24, 0.30, 0.68],
            [0.42, 0.16, 0.20, 0.20, 0.12, 0.18, 0.50, 0.30],
            [0.50, 0.30, 0.22, 0.22, 0.10, 0.18, 0.52, 0.30],
        ],
        &device,
    );
    let b1 = Tensor::<JudgeBackend, 2>::from_data(
        [[-0.62, -0.48, -0.52, -0.60, -0.46, -0.50, -0.62, -0.58]],
        &device,
    );
    let w2 = Tensor::<JudgeBackend, 2>::from_data(
        [
            [0.68],
            [0.76],
            [0.78],
            [0.80],
            [0.66],
            [0.70],
            [0.74],
            [0.82],
        ],
        &device,
    );
    let b2 = Tensor::<JudgeBackend, 2>::from_data([[-1.35]], &device);

    let hidden = activation::relu(input.matmul(w1).add(b1));
    let score = activation::sigmoid(hidden.matmul(w2).add(b2));
    score
        .to_data()
        .to_vec::<f32>()
        .unwrap_or_default()
        .first()
        .copied()
        .unwrap_or_default()
        .clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn burn_judge_passes_plausible_cloud_metrics() {
        let metrics = SkyImageMetrics {
            width: 640,
            height: 360,
            render_ms: 8.0,
            fps: 125.0,
            mean_luminance: 0.52,
            contrast: 0.18,
            cloud_coverage: 0.18,
            clear_sky_fraction: 0.80,
            cloud_alpha_variance: 0.07,
            cloud_core_fraction: 0.58,
            cloud_bbox_margin: 0.18,
            cloud_boundary_contact: 0.0,
            edge_density: 0.09,
            detail_energy: 0.12,
            sun_visibility: 0.82,
            sun_cloud_contrast: 0.16,
            horizon_gradient: 0.20,
            color_variance: 0.13,
        };

        let report = BurnCloudPhotorealismJudge::default().judge(&metrics);

        assert!(report.score > 0.72, "{report:?}");
        assert!(report.pass);
    }

    #[test]
    fn burn_judge_rejects_flat_empty_sky_metrics() {
        let metrics = SkyImageMetrics {
            width: 640,
            height: 360,
            render_ms: 8.0,
            fps: 125.0,
            mean_luminance: 0.52,
            contrast: 0.02,
            cloud_coverage: 0.02,
            clear_sky_fraction: 0.98,
            cloud_alpha_variance: 0.0,
            cloud_core_fraction: 0.0,
            cloud_bbox_margin: 0.0,
            cloud_boundary_contact: 0.0,
            edge_density: 0.0,
            detail_energy: 0.0,
            sun_visibility: 0.05,
            sun_cloud_contrast: 0.01,
            horizon_gradient: 0.02,
            color_variance: 0.01,
        };

        let report = BurnCloudPhotorealismJudge::default().judge(&metrics);

        assert!(report.score < 0.72, "{report:?}");
        assert!(!report.pass);
    }

    #[test]
    fn burn_judge_rejects_uniform_cloud_sheet() {
        let metrics = SkyImageMetrics {
            width: 288,
            height: 162,
            render_ms: 6.0,
            fps: 166.0,
            mean_luminance: 0.78,
            contrast: 0.09,
            cloud_coverage: 0.92,
            clear_sky_fraction: 0.03,
            cloud_alpha_variance: 0.002,
            cloud_core_fraction: 0.85,
            cloud_bbox_margin: 0.0,
            cloud_boundary_contact: 0.31,
            edge_density: 0.003,
            detail_energy: 0.00004,
            sun_visibility: 0.86,
            sun_cloud_contrast: 0.01,
            horizon_gradient: 0.30,
            color_variance: 0.10,
        };

        let report = BurnCloudPhotorealismJudge::default().judge(&metrics);

        assert!(!report.pass, "{report:?}");
        assert!(
            report
                .notes
                .iter()
                .any(|note| note.contains("uniform sheet")),
            "{report:?}"
        );
    }

    #[test]
    fn burn_judge_rejects_cropped_cloud_framing() {
        let metrics = SkyImageMetrics {
            width: 640,
            height: 360,
            render_ms: 8.0,
            fps: 125.0,
            mean_luminance: 0.52,
            contrast: 0.22,
            cloud_coverage: 0.48,
            clear_sky_fraction: 0.42,
            cloud_alpha_variance: 0.06,
            cloud_core_fraction: 0.18,
            cloud_bbox_margin: 0.01,
            cloud_boundary_contact: 0.12,
            edge_density: 0.16,
            detail_energy: 0.15,
            sun_visibility: 0.72,
            sun_cloud_contrast: 0.18,
            horizon_gradient: 0.20,
            color_variance: 0.13,
        };

        let report = BurnCloudPhotorealismJudge::default().judge(&metrics);

        assert!(!report.pass, "{report:?}");
        assert!(
            report
                .notes
                .iter()
                .any(|note| note.contains("whole cloud") || note.contains("frame edge")),
            "{report:?}"
        );
    }

    #[test]
    fn artifact_validator_accepts_multi_case_bundle_with_case_judges() {
        let dir = temp_artifact_dir("multi_case_valid");
        write_multi_case_bundle(&dir, "locked_smoke_003", true);

        let report = validate_artifact_bundle(&dir).expect("artifact bundle should validate");

        assert!(report.passed, "{:?}", report.issues);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn artifact_validator_rejects_missing_case_judge_reports() {
        let dir = temp_artifact_dir("multi_case_missing_judges");
        write_multi_case_bundle(&dir, "locked_smoke_003", false);

        let report = validate_artifact_bundle(&dir).expect("artifact bundle should validate");

        assert!(!report.passed);
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.contains("case_visual_judge")),
            "{report:?}"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn artifact_validator_rejects_manifest_case_set_mismatch() {
        let dir = temp_artifact_dir("multi_case_mismatch");
        write_multi_case_bundle(&dir, "locked_smoke_001", true);

        let report = validate_artifact_bundle(&dir).expect("artifact bundle should validate");

        assert!(!report.passed);
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.contains("manifest seed_set")),
            "{report:?}"
        );
        let _ = fs::remove_dir_all(dir);
    }

    fn temp_artifact_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ashfall_eval_lab_{name}_{}", process_id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn process_id() -> u32 {
        std::process::id()
    }

    fn write_multi_case_bundle(root: &Path, manifest_case_set: &str, include_case_judges: bool) {
        let cases = ["broken_cumulus", "sun_edge", "wide_mixed"];
        write_test_png(root, "captures/final_sdr.png");
        for case in cases {
            write_test_png(root, &format!("captures/{case}_sdr.png"));
        }
        write_test_text(
            root,
            "telemetry/frame_times.csv",
            "frame_index,cpu_frame_ms,gpu_frame_ms,present_ms,draw_calls,dispatch_calls,validation_errors\n0,8.000,,,1,0,0\n",
        );
        write_test_json(
            root,
            "build_info.json",
            &json!({ "schema_version": "build_info.v1" }),
        );
        write_test_json(root, "gpu_info.json", &json!({ "device_name": "test" }));
        write_test_json(
            root,
            "settings.json",
            &json!({
                "schema_version": "sky_settings.v1",
                "case_set": "locked_smoke_003",
                "cases": cases.iter().map(|case| json!({ "id": case })).collect::<Vec<_>>(),
            }),
        );
        write_test_json(
            root,
            "telemetry/cloud_iteration.json",
            &json!({
                "schema_version": "cloud_iteration_summary.v1",
                "case_set": "locked_smoke_003",
                "case_count": cases.len(),
                "iterations": [{ "iteration": 0 }],
            }),
        );
        write_test_json(
            root,
            "decision/pass_fail.json",
            &json!({ "decision": "pass", "case_count": cases.len() }),
        );
        let visual_report = sample_visual_report();
        write_test_json(root, "ai_reports/visual_judge.json", &visual_report);

        let mut manifest = ArtifactManifest::sky_smoke(
            "test_run",
            "1970-01-01T00:00:00Z",
            manifest_case_set,
            "high",
        )
        .with_file("settings.json", "settings")
        .with_file("build_info.json", "build_info")
        .with_file("gpu_info.json", "gpu_info")
        .with_file("captures/final_sdr.png", "sdr_preview")
        .with_file("telemetry/frame_times.csv", "frame_times")
        .with_file("telemetry/cloud_iteration.json", "cloud_iteration")
        .with_file("ai_reports/visual_judge.json", "visual_judge")
        .with_file("decision/pass_fail.json", "pass_fail");
        for case in cases {
            manifest = manifest.with_file(format!("captures/{case}_sdr.png"), "case_sdr_preview");
        }
        if include_case_judges {
            for case in cases {
                let path = format!("ai_reports/{case}_visual_judge.json");
                write_test_json(root, &path, &visual_report);
                manifest = manifest.with_file(path, "case_visual_judge");
            }
        }
        write_test_json(root, "manifest.json", &manifest);
    }

    fn sample_visual_report() -> CloudPhotorealismReport {
        CloudPhotorealismReport {
            schema_version: "cloud_photorealism_report.v1".to_string(),
            evaluator: "test".to_string(),
            model: "test".to_string(),
            threshold: 0.72,
            score: 0.91,
            pass: true,
            features: CloudPhotorealismFeatures {
                cloud_coverage_score: 0.8,
                edge_density_score: 0.8,
                detail_energy_score: 0.8,
                sun_visibility_score: 0.8,
                contrast_score: 0.8,
                color_variance_score: 0.8,
                horizon_gradient_score: 0.8,
                mean_luminance_score: 0.8,
                clear_sky_score: 0.8,
                cloud_alpha_variance_score: 0.8,
                cloud_core_fraction_score: 0.8,
                sun_cloud_contrast_score: 0.8,
                whole_cloud_margin_score: 0.8,
                frame_boundary_clearance_score: 0.8,
            },
            notes: Vec::new(),
        }
    }

    fn write_test_json<T: Serialize>(root: &Path, relative: &str, value: &T) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().expect("test path should have parent"))
            .expect("test artifact parent should write");
        fs::write(
            path,
            serde_json::to_string_pretty(value).expect("test JSON should serialize"),
        )
        .expect("test JSON should write");
    }

    fn write_test_text(root: &Path, relative: &str, text: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().expect("test path should have parent"))
            .expect("test artifact parent should write");
        fs::write(path, text).expect("test text should write");
    }

    fn write_test_png(root: &Path, relative: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().expect("test path should have parent"))
            .expect("test artifact parent should write");
        fs::write(path, b"\x89PNG\r\n\x1A\n").expect("test PNG should write");
    }
}
