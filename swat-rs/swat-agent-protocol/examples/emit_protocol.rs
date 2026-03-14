use serde_json::json;
use swat_agent_protocol::{AgentEventRecord, LineEmitter};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdout = std::io::stdout();
    let mut emitter = LineEmitter::new(stdout.lock());

    emitter.emit(
        &AgentEventRecord::planner("draft-answer")
            .with_phase("start")
            .with_summary("planner started"),
    )?;
    emitter.emit(
        &AgentEventRecord::model("gpt-4.1-mini")
            .with_phase("request")
            .with_span_id("model-1")
            .with_correlation_id("req-42")
            .with_summary("model requested")
            .with_attribute_value("messages", json!(2)),
    )?;
    emitter.emit(
        &AgentEventRecord::tool("web_search")
            .with_phase("start")
            .with_span_id("tool-1")
            .with_correlation_id("req-42")
            .with_summary("tool started")
            .with_status("running"),
    )?;

    Ok(())
}
