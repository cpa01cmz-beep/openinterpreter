use codex_app_server::run_main_from_cli_args;
use codex_arg0::Arg0DispatchPaths;
use codex_arg0::arg0_dispatch_or_else_current_thread;
use codex_server_cli::home::ensure_interpreter_home_env;
use codex_server_cli::startup_trace::record_startup_trace_event;
use std::ffi::OsString;

fn main() -> anyhow::Result<()> {
    codex_arg0::run_on_large_stack(main_inner)
}

fn main_inner() -> anyhow::Result<()> {
    record_startup_trace_event("interpreter_app_server.main.enter");
    ensure_interpreter_home_env()?;
    record_startup_trace_event("interpreter_app_server.main.home.ready");
    arg0_dispatch_or_else_current_thread(|arg0_paths: Arg0DispatchPaths| async move {
        record_startup_trace_event("interpreter_app_server.dispatch.enter");
        let cli_args = std::iter::once(OsString::from("interpreter-app-server"))
            .chain(std::env::args_os().skip(1));
        run_main_from_cli_args(arg0_paths, cli_args).await
    })
}
