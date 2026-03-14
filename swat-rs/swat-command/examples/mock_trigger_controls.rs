use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use swat_adapter_mock::MockAdapter;
use swat_command::{CommandHost, CommandOutput};
use swat_store::InMemoryStore;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut host = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );
    let trigger_path = std::env::temp_dir().join(format!(
        "swat-mock-trigger-controls-{}-{}.json",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    let trigger_path = trigger_path.display().to_string();

    let attach = host.execute("attach")?;
    print_output("attach", &attach);

    let added = host.execute(
        r#"trigger-snapshot snapshot_search kind == ModelBoundary and artifact.json $.tool == "search" capture search boundary"#,
    )?;
    print_output(
        r#"trigger-snapshot snapshot_search kind == ModelBoundary and artifact.json $.tool == "search" capture search boundary"#,
        &added,
    );
    let trigger_id = extract_trigger_id(&added);

    for command in [
        "triggers".to_string(),
        format!("trigger-disable {trigger_id}"),
        "triggers".to_string(),
        format!("trigger-enable {trigger_id}"),
        format!("trigger-save {trigger_path}"),
        format!("trigger-load {trigger_path}"),
        r#"until kind == ModelBoundary and artifact.json $.tool == "search""#.to_string(),
    ] {
        let output = host.execute(&command)?;
        print_output(&command, &output);
    }

    let _ = fs::remove_file(trigger_path);
    Ok(())
}

fn extract_trigger_id(output: &CommandOutput) -> String {
    output
        .lines
        .iter()
        .find_map(|line| {
            line.split_whitespace()
                .find_map(|part| part.strip_prefix("trigger="))
                .map(ToString::to_string)
        })
        .expect("trigger output should include a trigger id")
}

fn print_output(command: &str, output: &CommandOutput) {
    println!("$ {command}");
    println!("{}", output.summary);
    for line in &output.lines {
        println!("  {line}");
    }
}
