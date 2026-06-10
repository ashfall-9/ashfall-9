#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShaderCheckReport {
    pub checked_shader_count: u32,
    pub passed: bool,
    pub diagnostics: Vec<String>,
}

pub fn check_placeholder_shaders() -> ShaderCheckReport {
    ShaderCheckReport {
        checked_shader_count: 0,
        passed: true,
        diagnostics: vec!["shader pipeline not implemented yet".to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_shader_check_is_explicit() {
        let report = check_placeholder_shaders();

        assert!(report.passed);
        assert_eq!(report.checked_shader_count, 0);
        assert!(!report.diagnostics.is_empty());
    }
}
