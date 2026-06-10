#![forbid(unsafe_code)]

use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [command, subcommand] if command == "shader" && subcommand == "check" => {
            let report = shader_lab::check_placeholder_shaders();
            println!(
                "shader check: passed={} checked={} diagnostics={}",
                report.passed,
                report.checked_shader_count,
                report.diagnostics.len()
            );
            for diagnostic in report.diagnostics {
                println!("diagnostic: {diagnostic}");
            }
            Ok(())
        }
        [command, subcommand, path] if command == "artifact" && subcommand == "validate" => {
            let report = eval_lab::validate_artifact_bundle(PathBuf::from(path))?;
            println!(
                "artifact validate: passed={} issues={}",
                report.passed,
                report.issues.len()
            );
            for issue in report.issues {
                println!("issue: {issue}");
            }
            if report.passed {
                Ok(())
            } else {
                anyhow::bail!("artifact validation failed")
            }
        }
        [command, subcommand, ..] if command == "eval" && subcommand == "sky" => {
            anyhow::bail!(
                "sky eval CLI is not implemented yet; use `cargo run -p app_headless -- --seed 1 --out target/eval_smoke` for the current smoke path"
            )
        }
        _ => {
            anyhow::bail!("usage: tools_cli shader check | artifact validate <path> | eval sky ...")
        }
    }
}
