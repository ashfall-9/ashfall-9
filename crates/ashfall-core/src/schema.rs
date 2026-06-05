use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use crate::core::{ModuleId, SchemaVersion};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisteredSchema {
    pub version: SchemaVersion,
    pub owner: ModuleId,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SchemaRegistry {
    schemas: BTreeMap<String, RegisteredSchema>,
    migrations: SchemaMigrationPlan,
}

impl SchemaRegistry {
    pub fn register(&mut self, owner: ModuleId, version: SchemaVersion) {
        self.schemas.insert(
            version.name.to_string(),
            RegisteredSchema { version, owner },
        );
    }

    pub fn get(&self, name: &str) -> Option<&RegisteredSchema> {
        self.schemas.get(name)
    }

    pub fn register_migration(&mut self, step: SchemaMigrationStep) {
        self.migrations.steps.push(step);
        self.migrations.steps.sort_by(|left, right| {
            left.schema_name
                .cmp(&right.schema_name)
                .then_with(|| left.from_version.cmp(&right.from_version))
                .then_with(|| left.to_version.cmp(&right.to_version))
        });
    }

    pub fn migration_plan(&self) -> &SchemaMigrationPlan {
        &self.migrations
    }

    pub fn migration_path(
        &self,
        schema_name: &str,
        from_version: u32,
        to_version: u32,
    ) -> Option<Vec<SchemaMigrationStep>> {
        self.migrations.path(schema_name, from_version, to_version)
    }

    pub fn require_at_least(
        &self,
        name: &str,
        minimum_version: u32,
    ) -> Result<&RegisteredSchema, SchemaCompatibilityError> {
        let registered =
            self.schemas
                .get(name)
                .ok_or_else(|| SchemaCompatibilityError::Missing {
                    name: name.to_string(),
                    minimum_version,
                })?;

        if registered.version.version < minimum_version {
            return Err(SchemaCompatibilityError::TooOld {
                name: name.to_string(),
                minimum_version,
                actual_version: registered.version.version,
            });
        }

        Ok(registered)
    }

    pub fn check_requirements(
        &self,
        requirements: &[SchemaRequirement],
    ) -> SchemaCompatibilityReport {
        let mut issues = Vec::new();
        for requirement in requirements {
            match self.require_at_least(&requirement.schema_name, requirement.minimum_version) {
                Ok(registered) => {
                    if requirement.optional {
                        issues.push(SchemaCompatibilityIssue {
                            severity: SchemaCompatibilitySeverity::Info,
                            code: "optional_schema_available".to_string(),
                            requester: requirement.requester,
                            schema_name: requirement.schema_name.clone(),
                            minimum_version: requirement.minimum_version,
                            actual_version: Some(registered.version.version),
                            owner: Some(registered.owner),
                            reason: requirement.reason.clone(),
                            message: format!(
                                "optional schema {}@v{} is available",
                                requirement.schema_name, registered.version.version
                            ),
                            migration_path: Vec::new(),
                        });
                    }
                }
                Err(SchemaCompatibilityError::Missing { .. }) => {
                    issues.push(schema_issue(
                        requirement,
                        if requirement.optional {
                            SchemaCompatibilitySeverity::Warning
                        } else {
                            SchemaCompatibilitySeverity::Error
                        },
                        "schema_missing",
                        None,
                        None,
                        "required schema is not registered",
                    ));
                }
                Err(SchemaCompatibilityError::TooOld { actual_version, .. }) => {
                    let owner = self
                        .schemas
                        .get(&requirement.schema_name)
                        .map(|registered| registered.owner);
                    if let Some(path) = self.migration_path(
                        &requirement.schema_name,
                        actual_version,
                        requirement.minimum_version,
                    ) {
                        let lossless = path.iter().all(|step| step.lossless);
                        if lossless {
                            issues.push(schema_migration_issue(
                                requirement,
                                if requirement.optional {
                                    SchemaCompatibilitySeverity::Info
                                } else {
                                    SchemaCompatibilitySeverity::Warning
                                },
                                "schema_migration_available",
                                actual_version,
                                owner,
                                path,
                                "registered schema is older, but a lossless migration path is available",
                            ));
                        } else {
                            issues.push(schema_migration_issue(
                                requirement,
                                if requirement.optional {
                                    SchemaCompatibilitySeverity::Warning
                                } else {
                                    SchemaCompatibilitySeverity::Error
                                },
                                "schema_migration_lossy",
                                actual_version,
                                owner,
                                path,
                                "registered schema is older and its migration path contains lossy steps",
                            ));
                        }
                    } else {
                        issues.push(schema_issue(
                            requirement,
                            if requirement.optional {
                                SchemaCompatibilitySeverity::Warning
                            } else {
                                SchemaCompatibilitySeverity::Error
                            },
                            "schema_too_old",
                            Some(actual_version),
                            owner,
                            "registered schema version is too old for this requirement",
                        ));
                    }
                }
            }
        }

        SchemaCompatibilityReport {
            passed: !issues
                .iter()
                .any(|issue| issue.severity == SchemaCompatibilitySeverity::Error),
            checked_requirements: requirements.len(),
            issues,
        }
    }

    pub fn all(&self) -> impl Iterator<Item = &RegisteredSchema> {
        self.schemas.values()
    }

    pub fn len(&self) -> usize {
        self.schemas.len()
    }

    pub fn is_empty(&self) -> bool {
        self.schemas.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaRequirement {
    pub requester: ModuleId,
    pub schema_name: String,
    pub minimum_version: u32,
    pub optional: bool,
    pub reason: String,
}

impl SchemaRequirement {
    pub fn required(
        requester: ModuleId,
        schema_name: impl Into<String>,
        minimum_version: u32,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            requester,
            schema_name: schema_name.into(),
            minimum_version,
            optional: false,
            reason: reason.into(),
        }
    }

    pub fn optional(
        requester: ModuleId,
        schema_name: impl Into<String>,
        minimum_version: u32,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            requester,
            schema_name: schema_name.into(),
            minimum_version,
            optional: true,
            reason: reason.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaCompatibilityReport {
    pub passed: bool,
    pub checked_requirements: usize,
    pub issues: Vec<SchemaCompatibilityIssue>,
}

impl Default for SchemaCompatibilityReport {
    fn default() -> Self {
        Self {
            passed: true,
            checked_requirements: 0,
            issues: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaCompatibilityIssue {
    pub severity: SchemaCompatibilitySeverity,
    pub code: String,
    pub requester: ModuleId,
    pub schema_name: String,
    pub minimum_version: u32,
    pub actual_version: Option<u32>,
    pub owner: Option<ModuleId>,
    pub reason: String,
    pub message: String,
    pub migration_path: Vec<SchemaMigrationStep>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaCompatibilitySeverity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaMigrationStep {
    pub schema_name: String,
    pub from_version: u32,
    pub to_version: u32,
    pub description: String,
    pub lossless: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SchemaMigrationPlan {
    pub steps: Vec<SchemaMigrationStep>,
}

impl SchemaMigrationPlan {
    pub fn add_step(&mut self, step: SchemaMigrationStep) {
        self.steps.push(step);
    }

    pub fn can_migrate(&self, schema_name: &str, from_version: u32, to_version: u32) -> bool {
        self.path(schema_name, from_version, to_version).is_some()
    }

    pub fn path(
        &self,
        schema_name: &str,
        from_version: u32,
        to_version: u32,
    ) -> Option<Vec<SchemaMigrationStep>> {
        if from_version >= to_version {
            return Some(Vec::new());
        }

        let mut current = from_version;
        let mut path = Vec::new();

        while current < to_version {
            let step = self.steps.iter().find(|step| {
                step.schema_name == schema_name
                    && step.from_version == current
                    && step.to_version > current
                    && step.to_version <= to_version
            })?;
            current = step.to_version;
            path.push(step.clone());
        }

        Some(path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaCompatibilityError {
    Missing {
        name: String,
        minimum_version: u32,
    },
    TooOld {
        name: String,
        minimum_version: u32,
        actual_version: u32,
    },
}

impl fmt::Display for SchemaCompatibilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing {
                name,
                minimum_version,
            } => write!(f, "schema {name}@v{minimum_version}+ is not registered"),
            Self::TooOld {
                name,
                minimum_version,
                actual_version,
            } => write!(
                f,
                "schema {name} is v{actual_version}, but v{minimum_version}+ is required"
            ),
        }
    }
}

impl Error for SchemaCompatibilityError {}

impl fmt::Display for SchemaCompatibilityReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.passed {
            write!(
                f,
                "schema compatibility passed for {} requirements",
                self.checked_requirements
            )
        } else {
            let first_error = self
                .issues
                .iter()
                .find(|issue| issue.severity == SchemaCompatibilitySeverity::Error);
            if let Some(issue) = first_error {
                write!(
                    f,
                    "schema compatibility failed: module {} requires {}@v{} ({})",
                    issue.requester, issue.schema_name, issue.minimum_version, issue.message
                )
            } else {
                write!(
                    f,
                    "schema compatibility has {} non-fatal diagnostics",
                    self.issues.len()
                )
            }
        }
    }
}

fn schema_issue(
    requirement: &SchemaRequirement,
    severity: SchemaCompatibilitySeverity,
    code: &'static str,
    actual_version: Option<u32>,
    owner: Option<ModuleId>,
    message: &'static str,
) -> SchemaCompatibilityIssue {
    SchemaCompatibilityIssue {
        severity,
        code: code.to_string(),
        requester: requirement.requester,
        schema_name: requirement.schema_name.clone(),
        minimum_version: requirement.minimum_version,
        actual_version,
        owner,
        reason: requirement.reason.clone(),
        message: message.to_string(),
        migration_path: Vec::new(),
    }
}

fn schema_migration_issue(
    requirement: &SchemaRequirement,
    severity: SchemaCompatibilitySeverity,
    code: &'static str,
    actual_version: u32,
    owner: Option<ModuleId>,
    migration_path: Vec<SchemaMigrationStep>,
    message: &'static str,
) -> SchemaCompatibilityIssue {
    SchemaCompatibilityIssue {
        severity,
        code: code.to_string(),
        requester: requirement.requester,
        schema_name: requirement.schema_name.clone(),
        minimum_version: requirement.minimum_version,
        actual_version: Some(actual_version),
        owner,
        reason: requirement.reason.clone(),
        message: message.to_string(),
        migration_path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_report_flags_missing_required_schema() {
        let registry = SchemaRegistry::default();
        let report = registry.check_requirements(&[SchemaRequirement::required(
            10,
            "PhysicsOutput",
            1,
            "renderer consumes fracture and fluid outputs",
        )]);

        assert!(!report.passed);
        assert_eq!(report.checked_requirements, 1);
        assert!(report.issues.iter().any(|issue| {
            issue.severity == SchemaCompatibilitySeverity::Error && issue.code == "schema_missing"
        }));
    }

    #[test]
    fn compatibility_report_accepts_newer_schema_and_warns_optional_missing() {
        let mut registry = SchemaRegistry::default();
        registry.register(
            20,
            SchemaVersion {
                name: "PhysicsOutput",
                version: 2,
            },
        );

        let report = registry.check_requirements(&[
            SchemaRequirement::required(10, "PhysicsOutput", 1, "rendering depends on physics"),
            SchemaRequirement::optional(10, "ReferenceRenderPacket", 1, "debug validation"),
        ]);

        assert!(report.passed);
        assert_eq!(report.checked_requirements, 2);
        assert!(report.issues.iter().any(|issue| {
            issue.severity == SchemaCompatibilitySeverity::Warning && issue.optional_missing()
        }));
    }

    #[test]
    fn compatibility_report_accepts_lossless_required_migration() {
        let mut registry = SchemaRegistry::default();
        registry.register(
            20,
            SchemaVersion {
                name: "RenderPacket",
                version: 1,
            },
        );
        registry.register_migration(SchemaMigrationStep {
            schema_name: "RenderPacket".to_string(),
            from_version: 1,
            to_version: 2,
            description: "add visibility records".to_string(),
            lossless: true,
        });

        let report = registry.check_requirements(&[SchemaRequirement::required(
            30,
            "RenderPacket",
            2,
            "renderer debug view consumes v2 packets",
        )]);

        assert!(report.passed);
        assert_eq!(report.issues.len(), 1);
        assert_eq!(
            report.issues[0].severity,
            SchemaCompatibilitySeverity::Warning
        );
        assert_eq!(report.issues[0].code, "schema_migration_available");
        assert_eq!(report.issues[0].actual_version, Some(1));
        assert_eq!(report.issues[0].migration_path.len(), 1);
    }

    #[test]
    fn compatibility_report_rejects_lossy_required_migration() {
        let mut registry = SchemaRegistry::default();
        registry.register(
            20,
            SchemaVersion {
                name: "NarrativeDirectorState",
                version: 1,
            },
        );
        registry.register_migration(SchemaMigrationStep {
            schema_name: "NarrativeDirectorState".to_string(),
            from_version: 1,
            to_version: 2,
            description: "drop deprecated rumor weights".to_string(),
            lossless: false,
        });

        let report = registry.check_requirements(&[SchemaRequirement::required(
            50,
            "NarrativeDirectorState",
            2,
            "story director requires current state shape",
        )]);

        assert!(!report.passed);
        assert_eq!(report.issues.len(), 1);
        assert_eq!(
            report.issues[0].severity,
            SchemaCompatibilitySeverity::Error
        );
        assert_eq!(report.issues[0].code, "schema_migration_lossy");
        assert_eq!(report.issues[0].migration_path.len(), 1);
    }

    #[test]
    fn migration_plan_checks_contiguous_upgrade_steps() {
        let plan = SchemaMigrationPlan {
            steps: vec![
                SchemaMigrationStep {
                    schema_name: "RenderPacket".to_string(),
                    from_version: 1,
                    to_version: 2,
                    description: "add debug views".to_string(),
                    lossless: true,
                },
                SchemaMigrationStep {
                    schema_name: "RenderPacket".to_string(),
                    from_version: 2,
                    to_version: 3,
                    description: "add visibility records".to_string(),
                    lossless: true,
                },
            ],
        };

        assert!(plan.can_migrate("RenderPacket", 1, 3));
        assert!(!plan.can_migrate("RenderPacket", 1, 4));
        assert_eq!(
            plan.path("RenderPacket", 1, 3)
                .expect("path should exist")
                .len(),
            2
        );
    }

    impl SchemaCompatibilityIssue {
        fn optional_missing(&self) -> bool {
            self.code == "schema_missing" && self.schema_name == "ReferenceRenderPacket"
        }
    }
}
