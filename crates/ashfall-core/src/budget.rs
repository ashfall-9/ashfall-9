use crate::core::{ModuleId, PerformanceCounters, QualityTier};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BudgetFramePlan {
    pub directives: Vec<BudgetDirective>,
    pub total_pressure_count: usize,
    pub critical_pressure_count: usize,
}

impl BudgetFramePlan {
    pub fn from_module_evaluations(evaluations: &[ModuleBudgetEvaluation]) -> Self {
        let directives = evaluations
            .iter()
            .map(BudgetDirective::from_evaluation)
            .collect::<Vec<_>>();
        let total_pressure_count = directives
            .iter()
            .map(|directive| directive.pressure.len())
            .sum();
        let critical_pressure_count = directives
            .iter()
            .map(|directive| {
                directive
                    .pressure
                    .iter()
                    .filter(|pressure| pressure.severity() == BudgetPressureSeverity::Critical)
                    .count()
            })
            .sum();

        Self {
            directives,
            total_pressure_count,
            critical_pressure_count,
        }
    }

    pub fn directive_for(&self, module_id: ModuleId) -> Option<&BudgetDirective> {
        self.directives
            .iter()
            .find(|directive| directive.module_id == module_id)
    }

    pub fn demotions(&self) -> impl Iterator<Item = &BudgetDirective> {
        self.directives
            .iter()
            .filter(|directive| directive.action != BudgetDirectiveAction::Maintain)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModuleBudgetEvaluation {
    pub module_id: ModuleId,
    pub current_quality: QualityTier,
    pub counters: PerformanceCounters,
    pub budget: ModuleBudget,
    pub pressure: Vec<BudgetPressure>,
}

impl ModuleBudgetEvaluation {
    pub fn evaluate(
        module_id: ModuleId,
        current_quality: QualityTier,
        counters: PerformanceCounters,
    ) -> Self {
        let budget = ModuleBudget::for_quality(module_id, current_quality);
        let pressure = budget.evaluate(&counters);
        Self {
            module_id,
            current_quality,
            counters,
            budget,
            pressure,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BudgetDirective {
    pub module_id: ModuleId,
    pub current_quality: QualityTier,
    pub recommended_quality: QualityTier,
    pub action: BudgetDirectiveAction,
    pub pressure: Vec<BudgetPressure>,
    pub reason: String,
}

impl BudgetDirective {
    pub fn from_evaluation(evaluation: &ModuleBudgetEvaluation) -> Self {
        let recommended_quality =
            recommended_quality(evaluation.current_quality, &evaluation.pressure);
        let action = if evaluation.pressure.is_empty()
            || recommended_quality == evaluation.current_quality
        {
            BudgetDirectiveAction::Maintain
        } else if recommended_quality == QualityTier::Disabled {
            BudgetDirectiveAction::Disable
        } else {
            BudgetDirectiveAction::DemoteQuality
        };

        Self {
            module_id: evaluation.module_id,
            current_quality: evaluation.current_quality,
            recommended_quality,
            action,
            pressure: evaluation.pressure.clone(),
            reason: directive_reason(evaluation.current_quality, recommended_quality, action),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetDirectiveAction {
    Maintain,
    DemoteQuality,
    Disable,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModuleBudget {
    pub module_id: ModuleId,
    pub quality_tier: QualityTier,
    pub cpu_milliseconds: f32,
    pub gpu_milliseconds: f32,
    pub memory_bytes: u64,
    pub streaming_bytes: u64,
    pub latency_milliseconds: f32,
}

impl ModuleBudget {
    pub fn for_quality(module_id: ModuleId, quality_tier: QualityTier) -> Self {
        match quality_tier {
            QualityTier::Disabled => Self {
                module_id,
                quality_tier,
                cpu_milliseconds: 0.0,
                gpu_milliseconds: 0.0,
                memory_bytes: 0,
                streaming_bytes: 0,
                latency_milliseconds: 0.0,
            },
            QualityTier::BackgroundApproximation => Self {
                module_id,
                quality_tier,
                cpu_milliseconds: 0.25,
                gpu_milliseconds: 0.25,
                memory_bytes: 8 * 1024 * 1024,
                streaming_bytes: 2 * 1024 * 1024,
                latency_milliseconds: 4.0,
            },
            QualityTier::NormalRuntime => Self {
                module_id,
                quality_tier,
                cpu_milliseconds: 2.0,
                gpu_milliseconds: 4.0,
                memory_bytes: 128 * 1024 * 1024,
                streaming_bytes: 16 * 1024 * 1024,
                latency_milliseconds: 16.0,
            },
            QualityTier::HeroHighFidelityRuntime => Self {
                module_id,
                quality_tier,
                cpu_milliseconds: 4.0,
                gpu_milliseconds: 8.0,
                memory_bytes: 384 * 1024 * 1024,
                streaming_bytes: 64 * 1024 * 1024,
                latency_milliseconds: 24.0,
            },
            QualityTier::ReferenceOfflineValidation => Self {
                module_id,
                quality_tier,
                cpu_milliseconds: f32::INFINITY,
                gpu_milliseconds: f32::INFINITY,
                memory_bytes: u64::MAX,
                streaming_bytes: u64::MAX,
                latency_milliseconds: f32::INFINITY,
            },
        }
    }

    pub fn evaluate(&self, counters: &PerformanceCounters) -> Vec<BudgetPressure> {
        let mut pressure = Vec::new();
        if counters.cpu_milliseconds > self.cpu_milliseconds {
            pressure.push(BudgetPressure {
                module_id: self.module_id,
                kind: BudgetPressureKind::CpuTime,
                measured: counters.cpu_milliseconds,
                budget: self.cpu_milliseconds,
            });
        }
        if counters.gpu_milliseconds > self.gpu_milliseconds {
            pressure.push(BudgetPressure {
                module_id: self.module_id,
                kind: BudgetPressureKind::GpuTime,
                measured: counters.gpu_milliseconds,
                budget: self.gpu_milliseconds,
            });
        }
        if counters.memory_bytes > self.memory_bytes {
            pressure.push(BudgetPressure {
                module_id: self.module_id,
                kind: BudgetPressureKind::Memory,
                measured: counters.memory_bytes as f32,
                budget: self.memory_bytes as f32,
            });
        }
        pressure
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComponentBudgetProfile {
    pub module_id: ModuleId,
    pub quality_tier: QualityTier,
    pub normal_cpu_milliseconds: f32,
    pub worst_cpu_milliseconds: f32,
    pub normal_gpu_milliseconds: f32,
    pub worst_gpu_milliseconds: f32,
    pub memory_bytes: u64,
    pub streaming_bytes: u64,
    pub latency_milliseconds: f32,
    pub quality_tiers: Vec<QualityTier>,
    pub failure_behaviors: Vec<ComponentFailureBehavior>,
    pub debug_counter_count: usize,
}

impl ComponentBudgetProfile {
    pub fn from_module_budget(budget: &ModuleBudget, debug_counter_count: usize) -> Self {
        Self {
            module_id: budget.module_id,
            quality_tier: budget.quality_tier,
            normal_cpu_milliseconds: budget.cpu_milliseconds,
            worst_cpu_milliseconds: worst_milliseconds(budget.cpu_milliseconds),
            normal_gpu_milliseconds: budget.gpu_milliseconds,
            worst_gpu_milliseconds: worst_milliseconds(budget.gpu_milliseconds),
            memory_bytes: budget.memory_bytes,
            streaming_bytes: budget.streaming_bytes,
            latency_milliseconds: budget.latency_milliseconds,
            quality_tiers: vec![
                QualityTier::Disabled,
                QualityTier::BackgroundApproximation,
                QualityTier::NormalRuntime,
                QualityTier::HeroHighFidelityRuntime,
                QualityTier::ReferenceOfflineValidation,
            ],
            failure_behaviors: vec![
                ComponentFailureBehavior::KeepLastValidOutput,
                ComponentFailureBehavior::DemoteQuality,
                ComponentFailureBehavior::ExposeDebugIssue,
            ],
            debug_counter_count,
        }
    }

    pub fn is_complete(&self) -> bool {
        validate_component_budget_profile(self).is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ComponentFailureBehavior {
    KeepLastValidOutput,
    DemoteQuality,
    DisableComponent,
    UseFallbackAsset,
    DeferStreaming,
    ExposeDebugIssue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentBudgetProfileIssue {
    pub module_id: ModuleId,
    pub code: String,
    pub message: String,
}

pub fn validate_component_budget_profile(
    profile: &ComponentBudgetProfile,
) -> Vec<ComponentBudgetProfileIssue> {
    let mut issues = Vec::new();

    if profile.quality_tiers.is_empty() {
        push_component_budget_issue(
            &mut issues,
            profile.module_id,
            "missing_quality_tiers",
            "component budget profile must list supported quality tiers",
        );
    }
    if profile.failure_behaviors.is_empty() {
        push_component_budget_issue(
            &mut issues,
            profile.module_id,
            "missing_failure_behavior",
            "component budget profile must declare deterministic failure behavior",
        );
    }
    if profile.debug_counter_count == 0 {
        push_component_budget_issue(
            &mut issues,
            profile.module_id,
            "missing_debug_counters",
            "component budget profile must expose at least one debug counter",
        );
    }
    if profile.normal_cpu_milliseconds > profile.worst_cpu_milliseconds {
        push_component_budget_issue(
            &mut issues,
            profile.module_id,
            "cpu_worst_below_normal",
            "worst CPU budget must be greater than or equal to the normal CPU budget",
        );
    }
    if profile.normal_gpu_milliseconds > profile.worst_gpu_milliseconds {
        push_component_budget_issue(
            &mut issues,
            profile.module_id,
            "gpu_worst_below_normal",
            "worst GPU budget must be greater than or equal to the normal GPU budget",
        );
    }
    if profile.quality_tier != QualityTier::Disabled && profile.memory_bytes == 0 {
        push_component_budget_issue(
            &mut issues,
            profile.module_id,
            "missing_memory_budget",
            "enabled component budget profile must declare memory bytes",
        );
    }
    if profile.quality_tier != QualityTier::Disabled && profile.latency_milliseconds <= 0.0 {
        push_component_budget_issue(
            &mut issues,
            profile.module_id,
            "missing_latency_budget",
            "enabled component budget profile must declare latency milliseconds",
        );
    }

    issues
}

fn worst_milliseconds(normal: f32) -> f32 {
    if normal.is_finite() {
        normal * 2.0
    } else {
        normal
    }
}

fn push_component_budget_issue(
    issues: &mut Vec<ComponentBudgetProfileIssue>,
    module_id: ModuleId,
    code: impl Into<String>,
    message: impl Into<String>,
) {
    issues.push(ComponentBudgetProfileIssue {
        module_id,
        code: code.into(),
        message: message.into(),
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetPressureKind {
    CpuTime,
    GpuTime,
    Memory,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BudgetPressure {
    pub module_id: ModuleId,
    pub kind: BudgetPressureKind,
    pub measured: f32,
    pub budget: f32,
}

impl BudgetPressure {
    pub fn ratio(&self) -> f32 {
        if self.budget <= 0.0 {
            return f32::INFINITY;
        }

        self.measured / self.budget
    }

    pub fn severity(&self) -> BudgetPressureSeverity {
        let ratio = self.ratio();
        if ratio >= 2.0 {
            BudgetPressureSeverity::Critical
        } else if ratio >= 1.25 {
            BudgetPressureSeverity::Warning
        } else {
            BudgetPressureSeverity::Info
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetPressureSeverity {
    Info,
    Warning,
    Critical,
}

fn recommended_quality(current: QualityTier, pressure: &[BudgetPressure]) -> QualityTier {
    if pressure.is_empty() || current == QualityTier::ReferenceOfflineValidation {
        return current;
    }

    let critical = pressure
        .iter()
        .any(|pressure| pressure.severity() == BudgetPressureSeverity::Critical);
    if critical && current == QualityTier::BackgroundApproximation {
        QualityTier::Disabled
    } else {
        demote_quality(current).unwrap_or(current)
    }
}

fn demote_quality(current: QualityTier) -> Option<QualityTier> {
    match current {
        QualityTier::Disabled => None,
        QualityTier::BackgroundApproximation => Some(QualityTier::Disabled),
        QualityTier::NormalRuntime => Some(QualityTier::BackgroundApproximation),
        QualityTier::HeroHighFidelityRuntime => Some(QualityTier::NormalRuntime),
        QualityTier::ReferenceOfflineValidation => None,
    }
}

fn directive_reason(
    current_quality: QualityTier,
    recommended_quality: QualityTier,
    action: BudgetDirectiveAction,
) -> String {
    match action {
        BudgetDirectiveAction::Maintain => {
            "module is within budget or pinned to its current quality tier".to_string()
        }
        BudgetDirectiveAction::DemoteQuality => format!(
            "budget pressure recommends lowering quality from {:?} to {:?}",
            current_quality, recommended_quality
        ),
        BudgetDirectiveAction::Disable => {
            "critical pressure at background quality recommends disabling the module".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_plan_maintains_modules_within_budget() {
        let evaluation = ModuleBudgetEvaluation::evaluate(
            10,
            QualityTier::NormalRuntime,
            PerformanceCounters {
                cpu_milliseconds: 0.5,
                gpu_milliseconds: 1.0,
                memory_bytes: 16 * 1024 * 1024,
            },
        );

        let plan = BudgetFramePlan::from_module_evaluations(&[evaluation]);
        let directive = plan.directive_for(10).expect("directive should exist");

        assert_eq!(plan.total_pressure_count, 0);
        assert_eq!(directive.action, BudgetDirectiveAction::Maintain);
        assert_eq!(directive.recommended_quality, QualityTier::NormalRuntime);
    }

    #[test]
    fn budget_plan_recommends_quality_demotion_under_pressure() {
        let evaluation = ModuleBudgetEvaluation::evaluate(
            20,
            QualityTier::NormalRuntime,
            PerformanceCounters {
                cpu_milliseconds: 3.0,
                gpu_milliseconds: 2.0,
                memory_bytes: 16 * 1024 * 1024,
            },
        );

        let plan = BudgetFramePlan::from_module_evaluations(&[evaluation]);
        let directive = plan.directive_for(20).expect("directive should exist");

        assert_eq!(plan.total_pressure_count, 1);
        assert_eq!(directive.action, BudgetDirectiveAction::DemoteQuality);
        assert_eq!(
            directive.recommended_quality,
            QualityTier::BackgroundApproximation
        );
        assert_eq!(
            directive.pressure[0].severity(),
            BudgetPressureSeverity::Warning
        );
    }

    #[test]
    fn budget_plan_disables_background_modules_under_critical_pressure() {
        let evaluation = ModuleBudgetEvaluation::evaluate(
            30,
            QualityTier::BackgroundApproximation,
            PerformanceCounters {
                cpu_milliseconds: 1.0,
                gpu_milliseconds: 0.0,
                memory_bytes: 1024 * 1024,
            },
        );

        let plan = BudgetFramePlan::from_module_evaluations(&[evaluation]);
        let directive = plan.directive_for(30).expect("directive should exist");

        assert_eq!(plan.critical_pressure_count, 1);
        assert_eq!(directive.action, BudgetDirectiveAction::Disable);
        assert_eq!(directive.recommended_quality, QualityTier::Disabled);
    }

    #[test]
    fn module_budget_publishes_streaming_and_latency_limits() {
        let budget = ModuleBudget::for_quality(40, QualityTier::NormalRuntime);

        assert_eq!(budget.streaming_bytes, 16 * 1024 * 1024);
        assert_eq!(budget.latency_milliseconds, 16.0);

        let profile = ComponentBudgetProfile::from_module_budget(&budget, 3);
        assert!(profile.is_complete());
        assert_eq!(profile.module_id, 40);
        assert_eq!(profile.normal_cpu_milliseconds, 2.0);
        assert_eq!(profile.worst_cpu_milliseconds, 4.0);
        assert!(
            profile
                .quality_tiers
                .contains(&QualityTier::HeroHighFidelityRuntime)
        );
        assert!(
            profile
                .failure_behaviors
                .contains(&ComponentFailureBehavior::DemoteQuality)
        );
    }

    #[test]
    fn component_budget_profile_validation_rejects_missing_publication_data() {
        let profile = ComponentBudgetProfile {
            module_id: 50,
            quality_tier: QualityTier::NormalRuntime,
            normal_cpu_milliseconds: 2.0,
            worst_cpu_milliseconds: 1.0,
            normal_gpu_milliseconds: 2.0,
            worst_gpu_milliseconds: 1.0,
            memory_bytes: 0,
            streaming_bytes: 0,
            latency_milliseconds: 0.0,
            quality_tiers: Vec::new(),
            failure_behaviors: Vec::new(),
            debug_counter_count: 0,
        };

        let issues = validate_component_budget_profile(&profile);

        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "missing_quality_tiers")
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "missing_failure_behavior")
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "missing_debug_counters")
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "cpu_worst_below_normal")
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "gpu_worst_below_normal")
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "missing_memory_budget")
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "missing_latency_budget")
        );
    }
}
