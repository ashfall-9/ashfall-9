#![forbid(unsafe_code)]

use app_desktop::{DesktopAppConfig, run_desktop_app};

fn main() -> anyhow::Result<()> {
    run_desktop_app(DesktopAppConfig::default())
}
