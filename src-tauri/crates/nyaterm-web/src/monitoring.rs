use crate::{
    commands::{argument, text},
    error::{Result, WebError},
    remote_exec,
    state::State,
};
use nyaterm_core::core::monitoring::{ascend_npu, gpu, process, stats};
use serde_json::{Value, json};
pub fn supports(command: &str) -> bool {
    matches!(
        command,
        "get_remote_stats"
            | "get_remote_gpu_overview"
            | "get_remote_ascend_npu_overview"
            | "get_remote_processes"
            | "signal_remote_process"
    )
}
pub async fn command(state: &State, owner: &str, command: &str, args: &Value) -> Result<Value> {
    let id = text(args, "sessionId")?;
    let session = state.session(owner, id).await?;
    Ok(match command {
        "get_remote_stats" => {
            let lease = session.stats.lock_session(id).await;
            let plan = session.stats.probe_plan(id).await;
            let output = remote_exec::exec(&session, &stats::build_stats_script(plan)).await?;
            let parsed = stats::parse_stats_output(&output.stdout)?;
            json!(
                session
                    .stats
                    .complete_snapshot_with_lease(id, &lease, parsed)
                    .await
            )
        }
        "get_remote_gpu_overview" => json!(gpu::parse_gpu_overview_output(
            &remote_exec::exec(&session, gpu::GPU_OVERVIEW_SCRIPT)
                .await?
                .stdout
        )),
        "get_remote_ascend_npu_overview" => json!(ascend_npu::parse_npu_overview_output(
            &remote_exec::exec(&session, ascend_npu::ASCEND_NPU_OVERVIEW_SCRIPT)
                .await?
                .stdout
        )),
        "get_remote_processes" => {
            let output = remote_exec::exec(&session, process::PROCESS_LIST_SCRIPT).await?;
            if process::is_process_list_unsupported(&output.stdout) {
                return Err(WebError::bad(process::PROCESS_LIST_UNSUPPORTED_ERROR));
            }
            json!(process::parse_process_output(&output.stdout))
        }
        _ => {
            let pid: u32 = argument(args, "pid")?;
            if pid == 0 {
                return Err(WebError::bad("Invalid PID"));
            }
            let signal = match text(args, "signal")?.trim().to_ascii_uppercase().as_str() {
                "TERM" | "SIGTERM" | "15" => "TERM",
                "KILL" | "SIGKILL" | "9" => "KILL",
                "HUP" | "SIGHUP" | "1" => "HUP",
                "STOP" | "SIGSTOP" | "19" => "STOP",
                "CONT" | "SIGCONT" | "18" => "CONT",
                _ => return Err(WebError::bad("Unsupported signal")),
            };
            json!(remote_exec::exec(&session, &format!("kill -{signal} -- {pid}")).await?)
        }
    })
}
