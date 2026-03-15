use swat_ui_tui::{Mode, TuiConfig, run_headless};

#[test]
fn mock_headless_dashboard_renders_live_panes() {
    let mut config = TuiConfig::new(Mode::Mock);
    config.headless_ticks = 4;

    let rendered = run_headless(config).unwrap();
    assert!(rendered.contains("swat-ui-tui"));
    assert!(rendered.contains("dashboard=execution"));
    assert!(rendered.contains("Events"));
    assert!(rendered.contains("Stack / Entities"));
    assert!(rendered.contains("Source"));
    assert!(rendered.contains("Artifacts"));
    assert!(rendered.contains("ModelBoundary"));
}

#[test]
fn local_headless_dashboard_attaches_and_captures_output() {
    let mut config = TuiConfig::new(Mode::Local {
        program: "python3".to_string(),
        args: vec![
            "-u".to_string(),
            "-c".to_string(),
            "import time; print('hello from tui'); time.sleep(0.1)".to_string(),
        ],
    });
    config.headless_ticks = 6;

    let rendered = run_headless(config).unwrap();
    assert!(rendered.contains("local-process"));
    assert!(rendered.contains("ValueObserved"));
    assert!(rendered.contains("captured stdout chunk"));
}

#[test]
fn agent_headless_dashboard_attaches_and_renders_agent_events() {
    let code = r#"
import json
import sys
import time

PREFIX = "__SWATAGENT__"

def emit(record):
    sys.stdout.write(PREFIX + json.dumps(record) + "\n")
    sys.stdout.flush()

emit({"kind": "model", "phase": "request", "span_id": "model-1", "correlation_id": "req-5", "name": "gpt-4.1-mini", "summary": "model requested"})
emit({"kind": "tool", "phase": "start", "span_id": "tool-1", "correlation_id": "req-5", "name": "web_search", "summary": "tool started"})
emit({"kind": "tool", "phase": "end", "span_id": "tool-1", "correlation_id": "req-5", "name": "web_search", "summary": "tool completed"})
time.sleep(0.1)
"#;
    let mut config = TuiConfig::new(Mode::Agent {
        program: "python3".to_string(),
        args: vec!["-u".to_string(), "-c".to_string(), code.to_string()],
    });
    config.headless_ticks = 6;

    let rendered = run_headless(config).unwrap();
    assert!(rendered.contains("agent-runtime"));
    assert!(rendered.contains("ToolBoundary"));
    assert!(rendered.contains("tool completed"));
}

#[test]
fn pcgeos_headless_dashboard_attaches_and_renders_fixture_state() {
    let mut config = TuiConfig::new(Mode::PcGeos { fixture_path: None });
    config.headless_ticks = 2;

    let rendered = run_headless(config).unwrap();
    assert!(rendered.contains("pcgeos-fixture"));
    assert!(rendered.contains("dashboard=execution"));
    assert!(rendered.contains("GeoPointApp::OpenDocument"));
    assert!(rendered.contains("show.goc"));
}
