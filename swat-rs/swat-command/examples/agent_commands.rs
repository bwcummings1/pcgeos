use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use swat_adapter_agent::{AgentRuntimeAdapter, AgentRuntimeSpec};
use swat_command::CommandHost;
use swat_store::InMemoryStore;

fn python_sdk_path() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../sdk/python")
        .canonicalize()
        .unwrap()
        .display()
        .to_string()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let code = r#"
import time

from swat_agent_protocol import LineEmitter, model, planner, tool

emit = LineEmitter().emit

emit(planner("draft-answer", phase="start", summary="planner started", file="/tmp/runtime.py", line=12, function="run_agent"))
emit(model("gpt-4.1-mini", phase="request", span_id="model-1", correlation_id="req-99", summary="model requested"))
emit(tool("web_search", phase="start", span_id="tool-1", correlation_id="req-99", summary="tool started"))
emit(tool("web_search", phase="end", span_id="tool-1", correlation_id="req-99", summary="tool completed"))
emit(model("gpt-4.1-mini", phase="response", span_id="model-1", correlation_id="req-99", summary="model responded"))
time.sleep(0.1)
"#;

    let adapter = AgentRuntimeAdapter::new(
        AgentRuntimeSpec::new("python3")
            .with_args(["-u", "-c", code])
            .with_env("PYTHONPATH", python_sdk_path()),
    );
    let mut host = CommandHost::new(Box::new(adapter), Box::new(InMemoryStore::new()));
    let trigger_path = std::env::temp_dir().join(format!(
        "swat-agent-commands-{}-{}.json",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    let trigger_path = trigger_path.display().to_string();

    for command in [
        "attach".to_string(),
        "status".to_string(),
        "help breakpoint".to_string(),
        "help source".to_string(),
        "help search break".to_string(),
        r#"breakpoint once pause_web kind == ToolBoundary and artifact.json $.name == "web_search""#
            .to_string(),
        format!("breakpoint save {trigger_path}"),
        format!("breakpoint load {trigger_path}"),
        "breakpoint list".to_string(),
        "pump".to_string(),
        "pump".to_string(),
        "pump".to_string(),
        "entities web".to_string(),
        "correlation req-99".to_string(),
        r#"query kind == ToolBoundary and artifact.json $.name == "web_search""#.to_string(),
        "stack".to_string(),
        "source file /tmp/runtime.py".to_string(),
        "script ctx.stack_frame_count()".to_string(),
        "script ctx.stack_frame_label(0)".to_string(),
        "script ctx.source_file_count()".to_string(),
        "script ctx.event_count()".to_string(),
    ] {
        let output = host.execute(&command)?;
        println!("$ {command}");
        println!("{}", output.summary);
        for line in output.lines {
            println!("  {line}");
        }
        thread::sleep(Duration::from_millis(50));
    }

    let _ = fs::remove_file(trigger_path);

    Ok(())
}
