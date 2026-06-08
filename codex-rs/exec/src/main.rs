//! Entry-point for the `interpreter-exec` binary.
//!
//! When this CLI is invoked normally, it parses the standard `interpreter-exec` CLI
//! options and launches the non-interactive Codex agent. However, if it is
//! invoked with arg0 as `codex-linux-sandbox`, we instead treat the invocation
//! as a request to run the logic for the standalone `codex-linux-sandbox`
//! executable (i.e., parse any -s args and then run a *sandboxed* command under
//! Landlock + seccomp.
//!
//! This allows us to ship a completely separate set of functionality as part
//! of the `interpreter-exec` binary.
use clap::Parser;
use codex_arg0::Arg0DispatchPaths;
use codex_arg0::arg0_dispatch_or_else;
use codex_exec::Cli;
use codex_exec::run_main;
use codex_utils_cli::CliConfigOverrides;
use std::path::PathBuf;

const CODEX_HOME_ENV_VAR: &str = "CODEX_HOME";
const INTERPRETER_HOME_ENV_VAR: &str = "INTERPRETER_HOME";
const OPEN_INTERPRETER_HOME_ENV_VAR: &str = "OPEN_INTERPRETER_HOME";
const OPEN_INTERPRETER_BRAND_ENV_VAR: &str = "OPEN_INTERPRETER_BRAND";

#[derive(Parser, Debug)]
struct TopCli {
    #[clap(flatten)]
    config_overrides: CliConfigOverrides,

    #[clap(flatten)]
    inner: Cli,
}

fn main() -> anyhow::Result<()> {
    ensure_interpreter_exec_home_env()?;
    arg0_dispatch_or_else(|arg0_paths: Arg0DispatchPaths| async move {
        let top_cli = TopCli::parse();
        // Merge root-level overrides into inner CLI struct so downstream logic remains unchanged.
        let mut inner = top_cli.inner;
        inner
            .config_overrides
            .raw_overrides
            .splice(0..0, top_cli.config_overrides.raw_overrides);

        run_main(inner, arg0_paths).await?;
        Ok(())
    })
}

fn ensure_interpreter_exec_home_env() -> anyhow::Result<PathBuf> {
    let home = std::env::var_os(INTERPRETER_HOME_ENV_VAR)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var_os(OPEN_INTERPRETER_HOME_ENV_VAR).filter(|value| !value.is_empty())
        })
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home_dir| home_dir.join(".openinterpreter")))
        .ok_or_else(|| anyhow::anyhow!("failed to resolve Open Interpreter home directory"))?;
    std::fs::create_dir_all(&home)?;
    let canonical = home.canonicalize().unwrap_or(home);

    // SAFETY: main() calls this before the tokio runtime starts any background
    // threads, so mutating the process environment here is safe.
    unsafe {
        std::env::set_var(CODEX_HOME_ENV_VAR, &canonical);
        std::env::set_var(INTERPRETER_HOME_ENV_VAR, &canonical);
        std::env::set_var(OPEN_INTERPRETER_HOME_ENV_VAR, &canonical);
        std::env::set_var(OPEN_INTERPRETER_BRAND_ENV_VAR, "1");
    }

    Ok(canonical)
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
