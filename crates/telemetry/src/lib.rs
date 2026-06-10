#![forbid(unsafe_code)]

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use engine_core::InterfaceVersion;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterfaceVersionRecord {
    pub name: String,
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl From<InterfaceVersion> for InterfaceVersionRecord {
    fn from(version: InterfaceVersion) -> Self {
        Self {
            name: version.name.to_string(),
            major: version.major,
            minor: version.minor,
            patch: version.patch,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrameTelemetry {
    pub frame_index: u64,
    pub cpu_frame_ms: f32,
    pub gpu_frame_ms: Option<f32>,
    pub present_ms: Option<f32>,
    pub draw_calls: u32,
    pub dispatch_calls: u32,
    pub validation_errors: u32,
}

impl FrameTelemetry {
    pub fn smoke(frame_index: u64, cpu_frame_ms: f32) -> Self {
        Self {
            frame_index,
            cpu_frame_ms,
            gpu_frame_ms: None,
            present_ms: None,
            draw_calls: 0,
            dispatch_calls: 0,
            validation_errors: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GpuPassTiming {
    pub frame_index: u64,
    pub pass_name: String,
    pub queue_class: String,
    pub start_timestamp: u64,
    pub end_timestamp: u64,
    pub duration_ms: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryTelemetry {
    pub frame_index: u64,
    pub vram_used_mb: Option<u32>,
    pub transient_mb: Option<u32>,
    pub upload_mb: Option<u32>,
    pub readback_mb: Option<u32>,
    pub descriptor_sets: u32,
    pub pipelines: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactFile {
    pub path: String,
    pub kind: String,
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactManifest {
    pub schema_version: String,
    pub run_id: String,
    pub created_utc: String,
    pub milestone: String,
    pub component: String,
    pub seed_set: String,
    pub quality_profile: String,
    pub source_revision: String,
    pub interface_versions: Vec<InterfaceVersionRecord>,
    pub files: Vec<ArtifactFile>,
    pub status: String,
}

impl ArtifactManifest {
    pub fn sky_smoke(
        run_id: impl Into<String>,
        created_utc: impl Into<String>,
        seed_set: impl Into<String>,
        quality_profile: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: "eval_manifest.v1".to_string(),
            run_id: run_id.into(),
            created_utc: created_utc.into(),
            milestone: "sky".to_string(),
            component: "renderer_realtime.sky".to_string(),
            seed_set: seed_set.into(),
            quality_profile: quality_profile.into(),
            source_revision: "not-a-git-repo".to_string(),
            interface_versions: Vec::new(),
            files: Vec::new(),
            status: "complete".to_string(),
        }
    }

    pub fn with_interface_version(mut self, version: InterfaceVersion) -> Self {
        self.interface_versions.push(version.into());
        self
    }

    pub fn with_file(mut self, path: impl Into<String>, kind: impl Into<String>) -> Self {
        self.files.push(ArtifactFile {
            path: path.into(),
            kind: kind.into(),
            sha256: None,
        });
        self
    }
}

pub fn write_json_pretty<T: Serialize>(path: impl AsRef<Path>, value: &T) -> anyhow::Result<()> {
    let path = path.as_ref();
    ensure_parent(path)?;
    let json = serde_json::to_string_pretty(value)?;
    fs::write(path, json)?;
    Ok(())
}

pub fn write_frame_times_csv(
    path: impl AsRef<Path>,
    frames: &[FrameTelemetry],
) -> anyhow::Result<()> {
    let path = path.as_ref();
    ensure_parent(path)?;
    let mut file = fs::File::create(path)?;
    writeln!(
        file,
        "frame_index,cpu_frame_ms,gpu_frame_ms,present_ms,draw_calls,dispatch_calls,validation_errors"
    )?;
    for frame in frames {
        writeln!(
            file,
            "{},{:.3},{},{},{},{},{}",
            frame.frame_index,
            frame.cpu_frame_ms,
            optional_float(frame.gpu_frame_ms),
            optional_float(frame.present_ms),
            frame.draw_calls,
            frame.dispatch_calls,
            frame.validation_errors
        )?;
    }
    Ok(())
}

pub fn write_gpu_pass_times_csv(
    path: impl AsRef<Path>,
    passes: &[GpuPassTiming],
) -> anyhow::Result<()> {
    let path = path.as_ref();
    ensure_parent(path)?;
    let mut file = fs::File::create(path)?;
    writeln!(
        file,
        "frame_index,pass_name,queue_class,start_timestamp,end_timestamp,duration_ms"
    )?;
    for pass in passes {
        writeln!(
            file,
            "{},{},{},{},{},{:.3}",
            pass.frame_index,
            pass.pass_name,
            pass.queue_class,
            pass.start_timestamp,
            pass.end_timestamp,
            pass.duration_ms
        )?;
    }
    Ok(())
}

pub fn write_memory_csv(path: impl AsRef<Path>, frames: &[MemoryTelemetry]) -> anyhow::Result<()> {
    let path = path.as_ref();
    ensure_parent(path)?;
    let mut file = fs::File::create(path)?;
    writeln!(
        file,
        "frame_index,vram_used_mb,transient_mb,upload_mb,readback_mb,descriptor_sets,pipelines"
    )?;
    for frame in frames {
        writeln!(
            file,
            "{},{},{},{},{},{},{}",
            frame.frame_index,
            optional_u32(frame.vram_used_mb),
            optional_u32(frame.transient_mb),
            optional_u32(frame.upload_mb),
            optional_u32(frame.readback_mb),
            frame.descriptor_sets,
            frame.pipelines
        )?;
    }
    Ok(())
}

fn ensure_parent(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn optional_float(value: Option<f32>) -> String {
    value.map(|value| format!("{value:.3}")).unwrap_or_default()
}

fn optional_u32(value: Option<u32>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

pub fn artifact_path(root: impl AsRef<Path>, relative: impl AsRef<Path>) -> PathBuf {
    root.as_ref().join(relative)
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::ENGINE_CORE_VERSION;

    #[test]
    fn manifest_round_trips_as_json() {
        let dir =
            std::env::temp_dir().join(format!("ashfall_telemetry_manifest_{}", std::process::id()));
        let path = dir.join("manifest.json");
        let manifest = ArtifactManifest::sky_smoke(
            "smoke_001",
            "1970-01-01T00:00:00Z",
            "locked_smoke_001",
            "smoke",
        )
        .with_interface_version(ENGINE_CORE_VERSION)
        .with_file("telemetry/frame_times.csv", "frame_times");

        write_json_pretty(&path, &manifest).expect("manifest should write");
        let parsed: ArtifactManifest =
            serde_json::from_str(&fs::read_to_string(&path).expect("manifest should read"))
                .expect("manifest should parse");

        assert_eq!(parsed.run_id, "smoke_001");
        assert_eq!(parsed.interface_versions[0].name, "engine_core");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn frame_csv_uses_expected_header() {
        let dir = std::env::temp_dir().join(format!(
            "ashfall_telemetry_frame_csv_{}",
            std::process::id()
        ));
        let path = dir.join("telemetry").join("frame_times.csv");

        write_frame_times_csv(&path, &[FrameTelemetry::smoke(0, 0.25)])
            .expect("frame csv should write");
        let text = fs::read_to_string(&path).expect("frame csv should read");

        assert!(text.starts_with(
            "frame_index,cpu_frame_ms,gpu_frame_ms,present_ms,draw_calls,dispatch_calls,validation_errors"
        ));
        assert!(text.contains("0,0.250"));
        let _ = fs::remove_dir_all(dir);
    }
}
