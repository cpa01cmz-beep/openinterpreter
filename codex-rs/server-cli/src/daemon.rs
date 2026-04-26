use std::path::PathBuf;

use anyhow::Context;

pub use codex_app_server_launcher::LocalAppServerStatus;
pub use codex_app_server_launcher::StopLocalAppServerOutcome;

pub async fn ensure_local_app_server_url(
    app_server_bin: Option<PathBuf>,
    cli_overrides: Vec<String>,
) -> anyhow::Result<String> {
    let codex_home = crate::home::current_interpreter_home()
        .map_err(anyhow::Error::from)
        .context("failed to resolve Open Interpreter home")?;
    codex_app_server_launcher::ensure_local_app_server_url(
        &codex_home,
        app_server_bin,
        cli_overrides,
    )
    .await
}

pub async fn local_app_server_status() -> anyhow::Result<Option<LocalAppServerStatus>> {
    let codex_home = crate::home::current_interpreter_home()
        .map_err(anyhow::Error::from)
        .context("failed to resolve Open Interpreter home")?;
    codex_app_server_launcher::local_app_server_status(&codex_home).await
}

pub async fn stop_local_app_server() -> anyhow::Result<StopLocalAppServerOutcome> {
    let codex_home = crate::home::current_interpreter_home()
        .map_err(anyhow::Error::from)
        .context("failed to resolve Open Interpreter home")?;
    codex_app_server_launcher::stop_local_app_server(&codex_home).await
}
