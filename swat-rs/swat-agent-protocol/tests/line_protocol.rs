use serde_json::json;
use swat_agent_protocol::{
    AgentEventRecord, CURRENT_AGENT_PROTOCOL_VERSION, LineEmitter, encode_prefixed_line,
    parse_prefixed_line, parse_validated_prefixed_line,
};

#[test]
fn roundtrips_prefixed_agent_event_lines() {
    let record = AgentEventRecord::model("gpt-4.1-mini")
        .with_phase("request")
        .with_span_id("model-1")
        .with_correlation_id("req-42")
        .with_summary("model requested")
        .with_source("/tmp/agent.py", 42, "run_agent")
        .with_attribute_value("messages", json!(2));

    let line = encode_prefixed_line(&record).unwrap();
    let parsed = parse_prefixed_line(&line).unwrap().unwrap();

    assert_eq!(parsed.kind_name(), "model");
    assert_eq!(parsed.protocol_version(), CURRENT_AGENT_PROTOCOL_VERSION);
    assert_eq!(parsed.phase(), Some("request"));
    assert_eq!(parsed.span_id(), Some("model-1"));
    assert_eq!(parsed.correlation_id(), Some("req-42"));
    assert_eq!(parsed.attributes.get("messages"), Some(&json!(2)));
}

#[test]
fn line_emitter_writes_prefixed_json_lines() {
    let mut emitter = LineEmitter::new(Vec::new());
    let record = AgentEventRecord::tool("web_search")
        .with_phase("start")
        .with_summary("tool started")
        .with_status("running");

    emitter.emit(&record).unwrap();
    let bytes = emitter.into_inner();
    let line = String::from_utf8(bytes).unwrap();

    assert!(line.starts_with("__SWATAGENT__"));
    assert!(line.ends_with('\n'));
    assert!(line.contains(&format!(
        "\"protocol_version\":\"{CURRENT_AGENT_PROTOCOL_VERSION}\""
    )));
    assert!(line.contains("\"kind\":\"tool\""));
}

#[test]
fn rejects_unsupported_protocol_versions() {
    let line = "__SWATAGENT__{\"protocol_version\":\"9.9.9-test\",\"kind\":\"model\"}";
    let err = parse_validated_prefixed_line(line).unwrap().unwrap_err();

    assert!(
        err.to_string()
            .contains("unsupported agent protocol version")
    );
}
