use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn run_cli(args: &[&str], script: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_swat-command"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(script.as_bytes()).unwrap();
    }

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "cli failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn mock_cli_can_attach_pump_and_query() {
    let stdout = run_cli(
        &["mock"],
        "attach\nresume\npump\npump\nquery kind == ModelBoundary and artifact.json $.tool == \"search\"\nexit\n",
    );

    assert!(stdout.contains("attached session="));
    assert!(stdout.contains("mock target resumed"));
    assert!(stdout.contains("ModelBoundary"));
    assert!(stdout.contains("matched query"));
}

#[test]
fn local_cli_can_capture_process_output() {
    let stdout = run_cli(
        &[
            "local",
            "python3",
            "-u",
            "-c",
            "import time; print('hello from local cli'); time.sleep(0.1)",
        ],
        "attach\nsleep 150\npump\nevents ValueObserved\nexit\n",
    );

    assert!(stdout.contains("attached session="));
    assert!(stdout.contains("slept 150ms"));
    assert!(stdout.contains("captured stdout chunk"));
    assert!(stdout.contains("ValueObserved"));
}

#[test]
fn mock_cli_can_preload_trigger_file() {
    let path = std::env::temp_dir().join(format!(
        "swat-cli-trigger-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(
        &path,
        r#"{
  "format_version": 1,
  "triggers": [
    {
      "name": "pause_search",
      "expr": "kind == ModelBoundary and artifact.json $.tool == \"search\"",
      "fire_once": true
    }
  ]
}"#,
    )
    .unwrap();

    let stdout = run_cli(
        &["--triggers", &path.display().to_string(), "mock"],
        "attach\nresume\npump\npump\nexit\n",
    );

    assert!(stdout.contains("loaded 1 trigger"));
    assert!(stdout.contains("pause_search"));
    assert!(stdout.contains("TriggerHit"));

    let _ = std::fs::remove_file(path);
}
