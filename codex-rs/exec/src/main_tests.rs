use super::*;
use pretty_assertions::assert_eq;

struct EnvRestore {
    key: &'static str,
    value: Option<std::ffi::OsString>,
}

impl EnvRestore {
    fn capture(key: &'static str) -> Self {
        Self {
            key,
            value: std::env::var_os(key),
        }
    }
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        // SAFETY: this test restores process environment variables before
        // returning, matching the entrypoint's early-startup env mutation.
        unsafe {
            if let Some(value) = &self.value {
                std::env::set_var(self.key, value);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }
}

#[test]
fn exec_entrypoint_sets_open_interpreter_home_env() {
    let _codex_home = EnvRestore::capture(CODEX_HOME_ENV_VAR);
    let _interpreter_home = EnvRestore::capture(INTERPRETER_HOME_ENV_VAR);
    let _open_interpreter_home = EnvRestore::capture(OPEN_INTERPRETER_HOME_ENV_VAR);
    let _open_interpreter_brand = EnvRestore::capture(OPEN_INTERPRETER_BRAND_ENV_VAR);
    let temp_home = tempfile::tempdir().expect("temp home");

    // SAFETY: this test mutates process env before calling the entrypoint helper
    // under test and restores it before returning.
    unsafe {
        std::env::remove_var(CODEX_HOME_ENV_VAR);
        std::env::set_var(INTERPRETER_HOME_ENV_VAR, temp_home.path());
        std::env::remove_var(OPEN_INTERPRETER_HOME_ENV_VAR);
        std::env::remove_var(OPEN_INTERPRETER_BRAND_ENV_VAR);
    }

    let resolved = ensure_interpreter_exec_home_env().expect("resolve exec home");
    let expected = temp_home
        .path()
        .canonicalize()
        .expect("canonical temp home");

    assert_eq!(resolved, expected);
    assert_eq!(
        std::env::var_os(CODEX_HOME_ENV_VAR),
        Some(expected.clone().into())
    );
    assert_eq!(
        std::env::var_os(INTERPRETER_HOME_ENV_VAR),
        Some(expected.clone().into())
    );
    assert_eq!(
        std::env::var_os(OPEN_INTERPRETER_HOME_ENV_VAR),
        Some(expected.into())
    );
    assert_eq!(
        std::env::var_os(OPEN_INTERPRETER_BRAND_ENV_VAR),
        Some("1".into())
    );
}

#[test]
fn top_cli_parses_resume_prompt_after_config_flag() {
    const PROMPT: &str = "echo resume-with-global-flags-after-subcommand";
    let cli = TopCli::parse_from([
        "interpreter-exec",
        "resume",
        "--last",
        "--json",
        "--model",
        "gpt-5.2-codex",
        "--config",
        "reasoning_level=xhigh",
        "--dangerously-bypass-approvals-and-sandbox",
        "--skip-git-repo-check",
        PROMPT,
    ]);

    let Some(codex_exec::Command::Resume(args)) = cli.inner.command else {
        panic!("expected resume command");
    };
    let effective_prompt = args.prompt.clone().or_else(|| {
        if args.last {
            args.session_id.clone()
        } else {
            None
        }
    });
    assert_eq!(effective_prompt.as_deref(), Some(PROMPT));
    assert_eq!(cli.config_overrides.raw_overrides.len(), 1);
    assert_eq!(
        cli.config_overrides.raw_overrides[0],
        "reasoning_level=xhigh"
    );
}
