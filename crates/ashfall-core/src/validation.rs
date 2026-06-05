use std::collections::BTreeSet;

use crate::core::{AssetId, ModuleId, PerformanceCounters, QualityTier, SchemaVersion};

pub type ComponentId = ModuleId;
pub type ValidationArtifactRef = AssetId;

pub const VALIDATION_REPORT_SCHEMA: SchemaVersion = SchemaVersion {
    name: "ValidationReport",
    version: 4,
};

pub const CONFORMANCE_FIXTURE_SCHEMA: SchemaVersion = SchemaVersion {
    name: "ConformanceFixture",
    version: 5,
};

#[derive(Clone, Debug, PartialEq)]
pub struct ValidationReport {
    pub asset: Option<AssetId>,
    pub component: ComponentId,
    pub schema_version: SchemaVersion,
    pub passed: bool,
    pub warnings: Vec<ValidationWarning>,
    pub errors: Vec<ValidationError>,
    pub metrics: Vec<ValidationMetric>,
    pub artifacts: Vec<ValidationArtifactRef>,
}

impl ValidationReport {
    pub fn new(component: ComponentId, schema_version: SchemaVersion) -> Self {
        Self {
            asset: None,
            component,
            schema_version,
            passed: true,
            warnings: Vec::new(),
            errors: Vec::new(),
            metrics: Vec::new(),
            artifacts: Vec::new(),
        }
    }

    pub fn for_asset(
        asset: AssetId,
        component: ComponentId,
        schema_version: SchemaVersion,
    ) -> Self {
        Self {
            asset: Some(asset),
            ..Self::new(component, schema_version)
        }
    }

    pub fn add_warning(
        &mut self,
        code: impl Into<String>,
        message: impl Into<String>,
        subject: impl Into<String>,
    ) {
        self.warnings.push(ValidationWarning {
            code: code.into(),
            message: message.into(),
            subject: subject.into(),
        });
    }

    pub fn add_error(
        &mut self,
        code: impl Into<String>,
        message: impl Into<String>,
        subject: impl Into<String>,
    ) {
        self.passed = false;
        self.errors.push(ValidationError {
            code: code.into(),
            message: message.into(),
            subject: subject.into(),
        });
    }

    pub fn add_metric(
        &mut self,
        name: impl Into<String>,
        value: f64,
        unit: impl Into<String>,
        threshold: Option<f64>,
    ) {
        self.metrics.push(ValidationMetric {
            name: name.into(),
            value,
            unit: unit.into(),
            threshold,
        });
    }

    pub fn add_artifact(&mut self, artifact: ValidationArtifactRef) {
        self.artifacts.push(artifact);
    }

    pub fn issue_count(&self) -> usize {
        self.warnings.len() + self.errors.len()
    }
}

impl Default for ValidationReport {
    fn default() -> Self {
        Self::new(0, VALIDATION_REPORT_SCHEMA)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationWarning {
    pub code: String,
    pub message: String,
    pub subject: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationError {
    pub code: String,
    pub message: String,
    pub subject: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidationMetric {
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub threshold: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PerformanceBudget {
    pub cpu_milliseconds: f32,
    pub gpu_milliseconds: f32,
    pub memory_bytes: u64,
    pub streaming_bytes: u64,
    pub latency_milliseconds: f32,
    pub max_output_count: usize,
    pub quality_tiers: Vec<QualityTier>,
    pub failure_behavior: String,
}

impl PerformanceBudget {
    pub fn debug_capture() -> Self {
        Self {
            cpu_milliseconds: 8.0,
            gpu_milliseconds: 32.0,
            memory_bytes: 768 * 1024 * 1024,
            streaming_bytes: 256 * 1024 * 1024,
            latency_milliseconds: 50.0,
            max_output_count: 1024,
            quality_tiers: vec![
                QualityTier::BackgroundApproximation,
                QualityTier::NormalRuntime,
                QualityTier::HeroHighFidelityRuntime,
                QualityTier::ReferenceOfflineValidation,
            ],
            failure_behavior: "produce validation errors and keep the last valid debug capture"
                .to_string(),
        }
    }
}

impl Default for PerformanceBudget {
    fn default() -> Self {
        Self::debug_capture()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExpectedOutputKind {
    ReplayFrame,
    ValidationReport,
    DebugView,
    AssetPackage,
    FrameGraph,
    PerformanceHeatmap,
    ComponentBudgetReport,
    FailureModeReport,
    ProductionMilestoneReport,
    ProductionRiskRegisterReport,
    ArchitectureContractReport,
    StressSceneReport,
    GoldenSceneReport,
    RuntimeDependencyPolicyReport,
    ReferenceAnchorSanityReport,
    InterfaceSchemaCoverageReport,
    ExecutiveSanityAuditReport,
    ReferenceComparison,
    RAndDGate,
    SafetyProvenanceReport,
    AssetPackageAcceptanceReport,
    ReferenceValidationAcceptanceReport,
    RendererAcceptanceReport,
    VirtualGeometryAcceptanceReport,
    VirtualGeometryPipelineAcceptanceReport,
    MaterialAcceptanceReport,
    MaterialPipelineAcceptanceReport,
    PhysicsAcceptanceReport,
    PhysicsPipelineAcceptanceReport,
    HumanAcceptanceReport,
    HumanPipelineAcceptanceReport,
    AiAcceptanceReport,
    AiPipelineAcceptanceReport,
    VoiceAcceptanceReport,
    VoicePipelineAcceptanceReport,
    CityWorldAcceptanceReport,
    CityWorldPipelineAcceptanceReport,
    ToolsAcceptanceReport,
    PerformanceAcceptanceReport,
    OpenResearchReport,
    RoadmapAcceptanceReport,
    ProductQualityBarReport,
    EngineCoreAcceptanceReport,
    GpuServicesAcceptanceReport,
    PhotorealRenderingAcceptanceReport,
    CityCellSummary,
}

impl ExpectedOutputKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::ReplayFrame => "replay_frame",
            Self::ValidationReport => "validation_report",
            Self::DebugView => "debug_view",
            Self::AssetPackage => "asset_package",
            Self::FrameGraph => "frame_graph",
            Self::PerformanceHeatmap => "performance_heatmap",
            Self::ComponentBudgetReport => "component_budget_report",
            Self::FailureModeReport => "failure_mode_report",
            Self::ProductionMilestoneReport => "production_milestone_report",
            Self::ProductionRiskRegisterReport => "production_risk_register_report",
            Self::ArchitectureContractReport => "architecture_contract_report",
            Self::StressSceneReport => "stress_scene_report",
            Self::GoldenSceneReport => "golden_scene_report",
            Self::RuntimeDependencyPolicyReport => "runtime_dependency_policy_report",
            Self::ReferenceAnchorSanityReport => "reference_anchor_sanity_report",
            Self::InterfaceSchemaCoverageReport => "interface_schema_coverage_report",
            Self::ExecutiveSanityAuditReport => "executive_sanity_audit_report",
            Self::ReferenceComparison => "reference_comparison",
            Self::RAndDGate => "r_and_d_gate",
            Self::SafetyProvenanceReport => "safety_provenance_report",
            Self::AssetPackageAcceptanceReport => "asset_package_acceptance_report",
            Self::ReferenceValidationAcceptanceReport => "reference_validation_acceptance_report",
            Self::RendererAcceptanceReport => "renderer_acceptance_report",
            Self::VirtualGeometryAcceptanceReport => "virtual_geometry_acceptance_report",
            Self::VirtualGeometryPipelineAcceptanceReport => {
                "virtual_geometry_pipeline_acceptance_report"
            }
            Self::MaterialAcceptanceReport => "material_acceptance_report",
            Self::MaterialPipelineAcceptanceReport => "material_pipeline_acceptance_report",
            Self::PhysicsAcceptanceReport => "physics_acceptance_report",
            Self::PhysicsPipelineAcceptanceReport => "physics_pipeline_acceptance_report",
            Self::HumanAcceptanceReport => "human_acceptance_report",
            Self::HumanPipelineAcceptanceReport => "human_pipeline_acceptance_report",
            Self::AiAcceptanceReport => "ai_acceptance_report",
            Self::AiPipelineAcceptanceReport => "ai_pipeline_acceptance_report",
            Self::VoiceAcceptanceReport => "voice_acceptance_report",
            Self::VoicePipelineAcceptanceReport => "voice_pipeline_acceptance_report",
            Self::CityWorldAcceptanceReport => "city_world_acceptance_report",
            Self::CityWorldPipelineAcceptanceReport => "city_world_pipeline_acceptance_report",
            Self::ToolsAcceptanceReport => "tools_acceptance_report",
            Self::PerformanceAcceptanceReport => "performance_acceptance_report",
            Self::OpenResearchReport => "open_research_report",
            Self::RoadmapAcceptanceReport => "roadmap_acceptance_report",
            Self::ProductQualityBarReport => "product_quality_bar_report",
            Self::EngineCoreAcceptanceReport => "engine_core_acceptance_report",
            Self::GpuServicesAcceptanceReport => "gpu_services_acceptance_report",
            Self::PhotorealRenderingAcceptanceReport => "photoreal_rendering_acceptance_report",
            Self::CityCellSummary => "city_cell_summary",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedOutputRule {
    pub kind: ExpectedOutputKind,
    pub min_count: usize,
    pub label: String,
}

impl ExpectedOutputRule {
    pub fn new(kind: ExpectedOutputKind, min_count: usize, label: impl Into<String>) -> Self {
        Self {
            kind,
            min_count,
            label: label.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConformanceFixture {
    pub fixture_id: AssetId,
    pub required_schema: SchemaVersion,
    pub input_packages: Vec<AssetId>,
    pub expected_outputs: Vec<ExpectedOutputRule>,
    pub performance_budget: PerformanceBudget,
}

impl ConformanceFixture {
    pub fn new(fixture_id: AssetId, expected_outputs: Vec<ExpectedOutputRule>) -> Self {
        Self {
            fixture_id,
            required_schema: CONFORMANCE_FIXTURE_SCHEMA,
            input_packages: Vec::new(),
            expected_outputs,
            performance_budget: PerformanceBudget::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConformanceObservedOutput {
    pub kind: ExpectedOutputKind,
    pub count: usize,
}

impl ConformanceObservedOutput {
    pub fn new(kind: ExpectedOutputKind, count: usize) -> Self {
        Self { kind, count }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConformanceRunInput {
    pub observed_schema: SchemaVersion,
    pub input_packages: Vec<AssetId>,
    pub outputs: Vec<ConformanceObservedOutput>,
    pub counters: PerformanceCounters,
    pub streaming_bytes: u64,
    pub latency_milliseconds: f32,
    pub quality_tier: QualityTier,
}

impl ConformanceRunInput {
    pub fn output_count(&self, kind: ExpectedOutputKind) -> usize {
        self.outputs
            .iter()
            .filter(|output| output.kind == kind)
            .map(|output| output.count)
            .sum()
    }

    pub fn total_output_count(&self) -> usize {
        self.outputs.iter().map(|output| output.count).sum()
    }
}

impl Default for ConformanceRunInput {
    fn default() -> Self {
        Self {
            observed_schema: CONFORMANCE_FIXTURE_SCHEMA,
            input_packages: Vec::new(),
            outputs: Vec::new(),
            counters: PerformanceCounters::default(),
            streaming_bytes: 0,
            latency_milliseconds: 0.0,
            quality_tier: QualityTier::NormalRuntime,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConformanceReport {
    pub fixture_id: AssetId,
    pub passed: bool,
    pub checked_output_count: usize,
    pub missing_output_count: usize,
    pub issue_count: usize,
    pub issues: Vec<ConformanceIssue>,
    pub metrics: Vec<ConformanceMetric>,
}

impl ConformanceReport {
    pub fn is_empty(&self) -> bool {
        self.fixture_id == 0 && self.metrics.is_empty() && self.issues.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConformanceIssue {
    pub severity: ConformanceIssueSeverity,
    pub code: String,
    pub subject: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConformanceIssueSeverity {
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConformanceMetric {
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub threshold: Option<f64>,
}

pub fn evaluate_conformance_fixture(
    fixture: &ConformanceFixture,
    input: &ConformanceRunInput,
) -> ConformanceReport {
    let mut issues = Vec::new();

    if fixture.required_schema != input.observed_schema {
        issues.push(conformance_issue(
            ConformanceIssueSeverity::Error,
            "conformance_schema_mismatch",
            "schema",
            format!(
                "expected {}, observed {}",
                fixture.required_schema, input.observed_schema
            ),
        ));
    }

    let observed_packages = input
        .input_packages
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    for package in &fixture.input_packages {
        if !observed_packages.contains(package) {
            issues.push(conformance_issue(
                ConformanceIssueSeverity::Error,
                "missing_conformance_input_package",
                format!("asset:{package}"),
                "fixture input package was not present in the run",
            ));
        }
    }

    let mut missing_output_count = 0usize;
    for rule in &fixture.expected_outputs {
        let count = input.output_count(rule.kind);
        if count < rule.min_count {
            missing_output_count = missing_output_count.saturating_add(rule.min_count - count);
            issues.push(conformance_issue(
                ConformanceIssueSeverity::Error,
                "missing_conformance_output",
                rule.kind.label(),
                format!(
                    "{} expected at least {}, observed {}",
                    rule.label, rule.min_count, count
                ),
            ));
        }
    }

    let budget = &fixture.performance_budget;
    if !budget.quality_tiers.contains(&input.quality_tier) {
        issues.push(conformance_issue(
            ConformanceIssueSeverity::Error,
            "unsupported_conformance_quality_tier",
            "quality_tier",
            format!("{:?} is not allowed by this fixture", input.quality_tier),
        ));
    }
    if input.counters.cpu_milliseconds > budget.cpu_milliseconds {
        issues.push(conformance_issue(
            ConformanceIssueSeverity::Error,
            "conformance_cpu_budget_exceeded",
            "performance_budget",
            format!(
                "CPU {:.2} ms exceeded {:.2} ms",
                input.counters.cpu_milliseconds, budget.cpu_milliseconds
            ),
        ));
    }
    if input.counters.gpu_milliseconds > budget.gpu_milliseconds {
        issues.push(conformance_issue(
            ConformanceIssueSeverity::Error,
            "conformance_gpu_budget_exceeded",
            "performance_budget",
            format!(
                "GPU {:.2} ms exceeded {:.2} ms",
                input.counters.gpu_milliseconds, budget.gpu_milliseconds
            ),
        ));
    }
    if input.counters.memory_bytes > budget.memory_bytes {
        issues.push(conformance_issue(
            ConformanceIssueSeverity::Error,
            "conformance_memory_budget_exceeded",
            "performance_budget",
            format!(
                "memory {} bytes exceeded {} bytes",
                input.counters.memory_bytes, budget.memory_bytes
            ),
        ));
    }
    if input.streaming_bytes > budget.streaming_bytes {
        issues.push(conformance_issue(
            ConformanceIssueSeverity::Error,
            "conformance_streaming_budget_exceeded",
            "performance_budget",
            format!(
                "streaming {} bytes exceeded {} bytes",
                input.streaming_bytes, budget.streaming_bytes
            ),
        ));
    }
    if input.latency_milliseconds > budget.latency_milliseconds {
        issues.push(conformance_issue(
            ConformanceIssueSeverity::Error,
            "conformance_latency_budget_exceeded",
            "performance_budget",
            format!(
                "latency {:.2} ms exceeded {:.2} ms",
                input.latency_milliseconds, budget.latency_milliseconds
            ),
        ));
    }
    let total_output_count = input.total_output_count();
    if total_output_count > budget.max_output_count {
        issues.push(conformance_issue(
            ConformanceIssueSeverity::Warning,
            "conformance_output_count_high",
            "expected_outputs",
            format!(
                "{} output item(s) exceeded expected maximum {}",
                total_output_count, budget.max_output_count
            ),
        ));
    }
    if budget.failure_behavior.trim().is_empty() {
        issues.push(conformance_issue(
            ConformanceIssueSeverity::Error,
            "missing_conformance_failure_behavior",
            "performance_budget",
            "fixture must declare deterministic failure behavior",
        ));
    }

    let metrics = vec![
        conformance_metric(
            "cpu_milliseconds",
            input.counters.cpu_milliseconds as f64,
            "ms",
            Some(budget.cpu_milliseconds as f64),
        ),
        conformance_metric(
            "gpu_milliseconds",
            input.counters.gpu_milliseconds as f64,
            "ms",
            Some(budget.gpu_milliseconds as f64),
        ),
        conformance_metric(
            "memory_bytes",
            input.counters.memory_bytes as f64,
            "bytes",
            Some(budget.memory_bytes as f64),
        ),
        conformance_metric(
            "streaming_bytes",
            input.streaming_bytes as f64,
            "bytes",
            Some(budget.streaming_bytes as f64),
        ),
        conformance_metric(
            "latency_milliseconds",
            input.latency_milliseconds as f64,
            "ms",
            Some(budget.latency_milliseconds as f64),
        ),
        conformance_metric(
            "total_output_count",
            total_output_count as f64,
            "count",
            Some(budget.max_output_count as f64),
        ),
    ];
    let issue_count = issues.len();
    let passed = issues
        .iter()
        .all(|issue| issue.severity != ConformanceIssueSeverity::Error);

    ConformanceReport {
        fixture_id: fixture.fixture_id,
        passed,
        checked_output_count: fixture.expected_outputs.len(),
        missing_output_count,
        issue_count,
        issues,
        metrics,
    }
}

fn conformance_issue(
    severity: ConformanceIssueSeverity,
    code: impl Into<String>,
    subject: impl Into<String>,
    message: impl Into<String>,
) -> ConformanceIssue {
    ConformanceIssue {
        severity,
        code: code.into(),
        subject: subject.into(),
        message: message.into(),
    }
}

fn conformance_metric(
    name: impl Into<String>,
    value: f64,
    unit: impl Into<String>,
    threshold: Option<f64>,
) -> ConformanceMetric {
    ConformanceMetric {
        name: name.into(),
        value,
        unit: unit.into(),
        threshold,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_report_errors_make_report_fail() {
        let mut report = ValidationReport::for_asset(42, 80, VALIDATION_REPORT_SCHEMA);
        report.add_warning("minor", "non-fatal issue", "asset:42");
        report.add_error("broken", "fatal issue", "asset:42");
        report.add_metric("score", 0.5, "ratio", Some(0.8));

        assert!(!report.passed);
        assert_eq!(report.asset, Some(42));
        assert_eq!(report.component, 80);
        assert_eq!(report.issue_count(), 2);
        assert_eq!(report.metrics.len(), 1);
    }

    #[test]
    fn conformance_fixture_accepts_expected_outputs_inside_budget() {
        let fixture = ConformanceFixture::new(
            9001,
            vec![
                ExpectedOutputRule::new(ExpectedOutputKind::ReplayFrame, 1, "debug replay"),
                ExpectedOutputRule::new(ExpectedOutputKind::FrameGraph, 1, "frame graph"),
                ExpectedOutputRule::new(
                    ExpectedOutputKind::PerformanceHeatmap,
                    1,
                    "performance heatmap",
                ),
            ],
        );
        let input = ConformanceRunInput {
            outputs: vec![
                ConformanceObservedOutput::new(ExpectedOutputKind::ReplayFrame, 2),
                ConformanceObservedOutput::new(ExpectedOutputKind::FrameGraph, 1),
                ConformanceObservedOutput::new(ExpectedOutputKind::PerformanceHeatmap, 1),
            ],
            counters: PerformanceCounters {
                cpu_milliseconds: 2.0,
                gpu_milliseconds: 12.0,
                memory_bytes: 128 * 1024 * 1024,
            },
            streaming_bytes: 12 * 1024 * 1024,
            latency_milliseconds: 20.0,
            ..ConformanceRunInput::default()
        };

        let report = evaluate_conformance_fixture(&fixture, &input);

        assert!(report.passed);
        assert_eq!(report.fixture_id, 9001);
        assert_eq!(report.checked_output_count, 3);
        assert_eq!(report.missing_output_count, 0);
        assert_eq!(
            ExpectedOutputKind::SafetyProvenanceReport.label(),
            "safety_provenance_report"
        );
        assert_eq!(
            ExpectedOutputKind::AssetPackageAcceptanceReport.label(),
            "asset_package_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::ReferenceValidationAcceptanceReport.label(),
            "reference_validation_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::RendererAcceptanceReport.label(),
            "renderer_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::VirtualGeometryAcceptanceReport.label(),
            "virtual_geometry_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::VirtualGeometryPipelineAcceptanceReport.label(),
            "virtual_geometry_pipeline_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::MaterialAcceptanceReport.label(),
            "material_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::MaterialPipelineAcceptanceReport.label(),
            "material_pipeline_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::PhysicsAcceptanceReport.label(),
            "physics_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::PhysicsPipelineAcceptanceReport.label(),
            "physics_pipeline_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::HumanAcceptanceReport.label(),
            "human_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::HumanPipelineAcceptanceReport.label(),
            "human_pipeline_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::AiAcceptanceReport.label(),
            "ai_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::AiPipelineAcceptanceReport.label(),
            "ai_pipeline_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::VoiceAcceptanceReport.label(),
            "voice_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::VoicePipelineAcceptanceReport.label(),
            "voice_pipeline_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::CityWorldAcceptanceReport.label(),
            "city_world_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::CityWorldPipelineAcceptanceReport.label(),
            "city_world_pipeline_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::ToolsAcceptanceReport.label(),
            "tools_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::PerformanceAcceptanceReport.label(),
            "performance_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::OpenResearchReport.label(),
            "open_research_report"
        );
        assert_eq!(
            ExpectedOutputKind::RoadmapAcceptanceReport.label(),
            "roadmap_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::ProductQualityBarReport.label(),
            "product_quality_bar_report"
        );
        assert_eq!(
            ExpectedOutputKind::EngineCoreAcceptanceReport.label(),
            "engine_core_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::GpuServicesAcceptanceReport.label(),
            "gpu_services_acceptance_report"
        );
        assert_eq!(
            ExpectedOutputKind::PhotorealRenderingAcceptanceReport.label(),
            "photoreal_rendering_acceptance_report"
        );
        assert!(
            report.metrics.iter().any(|metric| {
                metric.name == "gpu_milliseconds" && metric.threshold == Some(32.0)
            })
        );
    }

    #[test]
    fn conformance_fixture_rejects_missing_outputs_and_budget_overruns() {
        let mut fixture = ConformanceFixture::new(
            9002,
            vec![ExpectedOutputRule::new(
                ExpectedOutputKind::ValidationReport,
                2,
                "validation reports",
            )],
        );
        fixture.input_packages = vec![44];
        fixture.performance_budget.cpu_milliseconds = 1.0;
        fixture.performance_budget.failure_behavior.clear();
        let input = ConformanceRunInput {
            observed_schema: SchemaVersion {
                name: "WrongSchema",
                version: 1,
            },
            outputs: vec![ConformanceObservedOutput::new(
                ExpectedOutputKind::ValidationReport,
                1,
            )],
            counters: PerformanceCounters {
                cpu_milliseconds: 4.0,
                ..PerformanceCounters::default()
            },
            ..ConformanceRunInput::default()
        };

        let report = evaluate_conformance_fixture(&fixture, &input);

        assert!(!report.passed);
        assert_eq!(report.missing_output_count, 1);
        assert!(
            report
                .issues
                .iter()
                .any(|issue| { issue.code == "conformance_schema_mismatch" })
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| { issue.code == "missing_conformance_input_package" })
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| { issue.code == "missing_conformance_output" })
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| { issue.code == "conformance_cpu_budget_exceeded" })
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| { issue.code == "missing_conformance_failure_behavior" })
        );
    }
}
