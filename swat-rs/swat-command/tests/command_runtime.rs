use std::fs;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use swat_adapter_agent::{AgentRuntimeAdapter, AgentRuntimeSpec};
use swat_adapter_mock::MockAdapter;
use swat_command::{
    BreakpointConditionInput, Command, CommandHost, CommandSurface, DashboardLayout,
    command_completions, command_help, command_search, load_command_history_from_path,
    parse_command, store_command_history_to_path,
};
use swat_store::InMemoryStore;

#[test]
fn mock_command_host_can_attach_pump_query_and_script() {
    let mut host = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );

    let attach = host.execute("attach").unwrap();
    assert!(attach.summary.contains("attached"));

    let session = host.execute("session").unwrap();
    assert!(session.summary.contains("session="));
    assert!(
        session
            .lines
            .iter()
            .any(|line| line.contains("counts events=1"))
    );

    let resume = host.execute("resume").unwrap();
    assert!(resume.summary.contains("resumed"));

    host.execute("pump").unwrap();
    let second = host.execute("pump").unwrap();
    assert!(
        second
            .lines
            .iter()
            .any(|line| line.contains("ModelBoundary"))
    );

    let queried = host
        .execute(r#"query kind == ModelBoundary and artifact.json $.tool == "search""#)
        .unwrap();
    assert_eq!(queried.lines.len(), 1);

    let status = host.execute("status").unwrap();
    assert!(status.lines.iter().any(|line| line.contains("snapshots=0")));

    let help = host.execute("help query").unwrap();
    assert!(help.summary.contains("help query"));
    assert!(help.lines.iter().any(|line| line.contains("event.id")));

    let dashboard = host.execute("dashboard control").unwrap();
    assert!(dashboard.summary.contains("dashboard control"));
    assert!(
        dashboard
            .lines
            .iter()
            .any(|line| line.contains("[breakpoints]"))
    );

    let history = host.execute("history 3").unwrap();
    assert!(history.summary.contains("3 command(s)"));
    assert!(
        history
            .lines
            .iter()
            .any(|line| line.contains("dashboard control"))
    );

    let scripted = host.execute("script ctx.event_count()").unwrap();
    assert_eq!(scripted.lines.len(), 1);
    assert!(scripted.lines[0].contains("result="));

    let script_packages = host.execute("script packages").unwrap();
    assert!(
        script_packages
            .lines
            .iter()
            .any(|line| line.contains("package=process loaded=false"))
    );

    let package_show = host.execute("script package show process").unwrap();
    assert!(
        package_show
            .lines
            .iter()
            .any(|line| line.contains("export=process_event_total"))
    );

    let package_load = host.execute("script package load process").unwrap();
    assert!(
        package_load
            .summary
            .contains("loaded script package process")
    );

    let script_packages = host.execute("script packages").unwrap();
    assert!(
        script_packages
            .lines
            .iter()
            .any(|line| line.contains("package=process loaded=true"))
    );

    let package_script = host
        .execute(r#"script process_has_summary("attached")"#)
        .unwrap();
    assert_eq!(package_script.lines, vec!["result=true".to_string()]);

    let autoloaded_stack = host.execute("script stack_frame_total()").unwrap();
    assert_eq!(autoloaded_stack.lines, vec!["result=1".to_string()]);
}

#[test]
fn command_registry_exposes_family_help_and_alias_parsing() {
    let help = command_help(Some("breakpoint"), CommandSurface::Shell);
    assert!(help.summary.contains("help breakpoint"));
    assert!(
        help.lines
            .iter()
            .any(|line| line.contains("breakpoint add <name> <expr>"))
    );
    assert!(
        help.lines
            .iter()
            .any(|line| line.contains("semantic breakpoints"))
    );

    let tui_help = command_help(Some("source"), CommandSurface::Tui);
    assert!(
        tui_help
            .lines
            .iter()
            .any(|line| line.contains("source file <path>"))
    );
    let stack_help = command_help(Some("stack"), CommandSurface::Shell);
    assert!(
        stack_help
            .lines
            .iter()
            .any(|line| line.contains("stack locals <index>"))
    );
    assert!(
        stack_help
            .lines
            .iter()
            .any(|line| line.contains("backtrace [frames]"))
    );
    let patient_help = command_help(Some("patient"), CommandSurface::Shell);
    assert!(
        patient_help
            .lines
            .iter()
            .any(|line| line.contains("patient show <name>"))
    );
    assert!(
        patient_help
            .lines
            .iter()
            .any(|line| line.contains("patient-default [name|off]"))
    );
    let value_help = command_help(Some("value"), CommandSurface::Shell);
    assert!(
        value_help
            .lines
            .iter()
            .any(|line| line.contains("value show <value_key>"))
    );
    let automation_help = command_help(Some("automation"), CommandSurface::Shell);
    assert!(
        automation_help
            .lines
            .iter()
            .any(|line| line.contains("script package load <name>"))
    );
    assert!(
        automation_help
            .lines
            .iter()
            .any(|line| line.contains("package=stack"))
    );
    let source_help = command_help(Some("source"), CommandSurface::Shell);
    assert!(
        source_help
            .lines
            .iter()
            .any(|line| line.contains("slist [file|line] [line]"))
    );
    let process_help = command_help(Some("process"), CommandSurface::Shell);
    assert!(process_help.summary.contains("help process"));
    assert!(
        process_help
            .lines
            .iter()
            .any(|line| line.contains("spawn [patient] [function]"))
    );
    let objwatch_help = command_help(Some("objwatch"), CommandSurface::Shell);
    assert!(objwatch_help.summary.contains("help object"));
    assert!(
        objwatch_help
            .lines
            .iter()
            .any(|line| line.contains("object_class_is"))
    );
    assert!(
        command_help(None, CommandSurface::Shell)
            .lines
            .iter()
            .any(|line| line.contains("help search <needle>"))
    );
    let dashboard_help = command_help(Some("dashboard"), CommandSurface::Shell);
    assert!(dashboard_help.summary.contains("help dashboard"));
    assert!(
        dashboard_help
            .lines
            .iter()
            .any(|line| line.contains("dashboard [execution|control|target]"))
    );

    let search = command_search("break", CommandSurface::Tui);
    assert!(search.summary.contains("help search break"));
    assert!(
        search
            .lines
            .iter()
            .any(|line| line.contains("topic=breakpoint"))
    );
    assert!(
        search
            .lines
            .iter()
            .any(|line| line.contains("breakpoint list"))
    );
    let package_search = command_search("objwatch", CommandSurface::Shell);
    assert!(
        package_search
            .lines
            .iter()
            .any(|line| line.contains("script-package=object"))
    );

    let completions = command_completions("help br", CommandSurface::Shell);
    assert!(completions.contains(&"help breakpoint".to_string()));
    let package_help_completions = command_completions("help pr", CommandSurface::Shell);
    assert!(package_help_completions.contains(&"help process".to_string()));
    let tui_completions = command_completions("sou", CommandSurface::Tui);
    assert!(tui_completions.contains(&"source show <event_id> [before] [after]".to_string()));
    let package_completions = command_completions("script package load pa", CommandSurface::Shell);
    assert!(package_completions.contains(&"script package load patient".to_string()));
    let dashboard_completions = command_completions("dashboard c", CommandSurface::Shell);
    assert!(dashboard_completions.contains(&"dashboard control".to_string()));

    assert_eq!(parse_command("stack").unwrap(), Command::Spans);
    assert_eq!(
        parse_command("help search break").unwrap(),
        Command::HelpSearch {
            needle: "break".to_string(),
        }
    );
    assert_eq!(
        parse_command("stack frame 0").unwrap(),
        Command::Frame { frame_index: 0 }
    );
    assert_eq!(
        parse_command("backtrace 5").unwrap(),
        Command::Backtrace { limit: Some(5) }
    );
    assert_eq!(parse_command("where").unwrap(), Command::Where);
    assert_eq!(
        parse_command("dashboard control").unwrap(),
        Command::Dashboard {
            layout: DashboardLayout::Control
        }
    );
    assert_eq!(
        parse_command("history 12").unwrap(),
        Command::History { limit: Some(12) }
    );
    assert_eq!(
        parse_command("func").unwrap(),
        Command::Function { name: None }
    );
    assert_eq!(
        parse_command("func run").unwrap(),
        Command::Function {
            name: Some("run".to_string()),
        }
    );
    assert_eq!(parse_command("up 2").unwrap(), Command::Up { count: 2 });
    assert_eq!(parse_command("down").unwrap(), Command::Down { count: 1 });
    assert_eq!(
        parse_command("locals 3").unwrap(),
        Command::Locals {
            frame_index: Some(3),
        }
    );
    assert_eq!(
        parse_command("stack locals 0").unwrap(),
        Command::FrameLocals { frame_index: 0 }
    );
    assert_eq!(
        parse_command("stack registers 0").unwrap(),
        Command::FrameRegisters { frame_index: 0 }
    );
    assert_eq!(
        parse_command("stack show 42").unwrap(),
        Command::Span {
            boundary_id: swat_core::BoundaryId::from_raw(42),
        }
    );
    assert_eq!(
        parse_command("source show 7 1 3").unwrap(),
        Command::Source {
            event_id: swat_core::EventId::from_raw(7),
            before: 1,
            after: 3,
        }
    );
    assert_eq!(
        parse_command("source file /tmp/agent.py").unwrap(),
        Command::SourceFile {
            file: "/tmp/agent.py".to_string(),
        }
    );
    assert_eq!(parse_command("source files").unwrap(), Command::SourceFiles);
    assert_eq!(
        parse_command("source functions").unwrap(),
        Command::SourceFunctions
    );
    assert_eq!(
        parse_command("source function run").unwrap(),
        Command::SourceFunction {
            function: "run".to_string(),
        }
    );
    assert_eq!(
        parse_command("source view /tmp/agent.py 4 1 2").unwrap(),
        Command::SourceView {
            file: "/tmp/agent.py".to_string(),
            line: 4,
            before: 1,
            after: 2,
        }
    );
    assert_eq!(
        parse_command("slist /tmp/agent.py 14").unwrap(),
        Command::SourceList {
            file: Some("/tmp/agent.py".to_string()),
            line: Some(14),
        }
    );
    assert_eq!(
        parse_command("view /tmp/agent.py 14").unwrap(),
        Command::View {
            file: Some("/tmp/agent.py".to_string()),
            line: Some(14),
        }
    );
    assert_eq!(
        parse_command("patient-default ui").unwrap(),
        Command::PatientDefault {
            patient: Some("ui".to_string()),
        }
    );
    assert_eq!(
        parse_command("spawn ui run").unwrap(),
        Command::Spawn {
            patient: Some("ui".to_string()),
            function: Some("run".to_string()),
        }
    );
    assert_eq!(
        parse_command("wakeup ui").unwrap(),
        Command::Wakeup {
            patient: Some("ui".to_string()),
        }
    );
    assert_eq!(
        parse_command("obj-name ^lui:0002").unwrap(),
        Command::ObjectName {
            object: "^lui:0002".to_string(),
        }
    );
    assert_eq!(
        parse_command("obj-class ^lui:0002").unwrap(),
        Command::ObjectClass {
            object: "^lui:0002".to_string(),
        }
    );
    assert_eq!(
        parse_command("script packages").unwrap(),
        Command::ScriptPackages
    );
    assert_eq!(
        parse_command("script package load patient").unwrap(),
        Command::ScriptPackageLoad {
            package: "patient".to_string(),
        }
    );
    assert_eq!(
        parse_command("script package show patient").unwrap(),
        Command::ScriptPackageShow {
            package: "patient".to_string(),
        }
    );
    assert_eq!(parse_command("patient").unwrap(), Command::Patients);
    assert_eq!(
        parse_command("patient show ui").unwrap(),
        Command::PatientShow {
            patient: "ui".to_string(),
        }
    );
    assert_eq!(parse_command("handles").unwrap(), Command::Handles);
    assert_eq!(
        parse_command("handle show h:1001").unwrap(),
        Command::HandleShow {
            handle: "h:1001".to_string(),
        }
    );
    assert_eq!(parse_command("resources").unwrap(), Command::Resources);
    assert_eq!(
        parse_command("resource show AppResource").unwrap(),
        Command::ResourceShow {
            resource: "AppResource".to_string(),
        }
    );
    assert_eq!(parse_command("objects").unwrap(), Command::Objects);
    assert_eq!(
        parse_command("object show ^lui:0002").unwrap(),
        Command::ObjectShow {
            object: "^lui:0002".to_string(),
        }
    );
    assert_eq!(parse_command("value").unwrap(), Command::Values);
    assert_eq!(
        parse_command("value show memory.turn").unwrap(),
        Command::ValueShow {
            value_key: "memory.turn".to_string(),
        }
    );
    assert_eq!(
        parse_command("breakpoint list").unwrap(),
        Command::Breakpoints
    );
    assert_eq!(
        parse_command("breakpoint show 9").unwrap(),
        Command::BreakpointShow {
            trigger_id: swat_core::TriggerId::from_raw(9),
        }
    );
    assert_eq!(
        parse_command("breakpoint groups").unwrap(),
        Command::BreakpointGroups
    );
    assert_eq!(
        parse_command("breakpoint group list").unwrap(),
        Command::BreakpointDefinitionGroups
    );
    assert_eq!(
        parse_command("breakpoint predicates").unwrap(),
        Command::BreakpointPredicates
    );
    assert_eq!(
        parse_command(
            r#"breakpoint predicate add search_tool kind == ModelBoundary and artifact.json $.tool == "search""#
        )
        .unwrap(),
        Command::BreakpointPredicateAdd {
            name: "search_tool".to_string(),
            expr: r#"kind == ModelBoundary and artifact.json $.tool == "search""#.to_string(),
        }
    );
    assert_eq!(
        parse_command("breakpoint add pause_search group=search @search_tool").unwrap(),
        Command::TriggerExpr {
            name: "pause_search".to_string(),
            condition: BreakpointConditionInput::PredicateRef("search_tool".to_string()),
            fire_once: false,
            group: Some("search".to_string()),
        }
    );
    assert_eq!(
        parse_command("breakpoint disable 9").unwrap(),
        Command::TriggerDisable {
            trigger_id: swat_core::TriggerId::from_raw(9),
        }
    );
    assert_eq!(
        parse_command("watchpoint list").unwrap(),
        Command::Watchpoints
    );
    assert_eq!(
        parse_command("watchpoint show 11").unwrap(),
        Command::WatchpointShow {
            trigger_id: swat_core::TriggerId::from_raw(11),
        }
    );
    assert_eq!(
        parse_command(
            "watchpoint add cache_turn agent.state path=$.status after=250 kind=Lifecycle summary=loaded group=load",
        )
        .unwrap(),
        Command::WatchpointAdd {
            spec: swat_api::WatchpointSpec::new("cache_turn", "agent.state")
                .at_path("$.status")
                .after_millis(250)
                .in_event_kind(swat_core::EventKind::Lifecycle)
                .with_summary_contains("loaded")
                .in_group("load"),
        }
    );
    assert_eq!(
        parse_command("watchpoint snapshot cache_turn agent.state after=25 -- state changed")
            .unwrap(),
        Command::WatchpointAdd {
            spec: swat_api::WatchpointSpec::new("cache_turn", "agent.state")
                .after_millis(25)
                .create_snapshot("state changed"),
        }
    );
}

#[test]
fn command_history_helpers_roundtrip_entries() {
    let path = std::env::temp_dir().join(format!(
        "swat-command-history-{}-{}.txt",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let entries = vec![
        "attach".to_string(),
        "dashboard control".to_string(),
        "history 5".to_string(),
    ];
    store_command_history_to_path(&path, &entries).unwrap();
    let loaded = load_command_history_from_path(&path, 16).unwrap();
    assert_eq!(loaded, entries);
    let _ = fs::remove_file(path);
}

#[test]
fn command_host_can_manage_and_fire_semantic_triggers() {
    let mut host = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );

    host.execute("attach").unwrap();
    host.execute("resume").unwrap();

    let added = host
        .execute(r#"breakpoint once pause_search kind == ModelBoundary and artifact.json $.tool == "search""#)
        .unwrap();
    assert!(added.summary.contains("added trigger"));

    let listed = host.execute("breakpoint list").unwrap();
    assert!(
        listed
            .lines
            .iter()
            .any(|line| line == "group=enabled kind=state count=1")
    );
    let trigger_id = listed
        .lines
        .iter()
        .find(|line| line.starts_with("bp="))
        .unwrap()
        .split_whitespace()
        .find_map(|part| part.strip_prefix("bp="))
        .unwrap()
        .to_string();
    let breakpoint_line = listed
        .lines
        .iter()
        .find(|line| line.starts_with("bp="))
        .unwrap();
    assert!(breakpoint_line.contains("pause_search"));
    assert!(breakpoint_line.contains("actions=PauseTarget"));
    assert!(breakpoint_line.contains("hits=0"));
    assert!(breakpoint_line.contains("state=enabled"));

    host.execute("pump").unwrap();
    let triggered = host.execute("pump").unwrap();
    assert!(triggered.lines.iter().any(|line| line.contains("trigger=")));
    assert!(
        triggered
            .lines
            .iter()
            .any(|line| line.contains("control accepted=true"))
    );
    let listed = host.execute("breakpoint list").unwrap();
    let breakpoint_line = listed
        .lines
        .iter()
        .find(|line| line.starts_with("bp="))
        .unwrap();
    assert!(breakpoint_line.contains("hits=1"));
    assert!(breakpoint_line.contains("last_event="));

    let shown = host
        .execute(&format!("breakpoint show {trigger_id}"))
        .unwrap();
    assert!(shown.summary.contains("breakpoint"));
    assert!(
        shown
            .lines
            .iter()
            .any(|line| line.contains("state=enabled"))
    );
    assert!(shown.lines.iter().any(|line| line.contains("activity=hit")));

    let grouped = host.execute("breakpoint groups").unwrap();
    assert!(
        grouped
            .lines
            .iter()
            .any(|line| line.contains("kind=state group=enabled"))
    );
    assert!(
        grouped
            .lines
            .iter()
            .any(|line| line.contains("kind=activity group=hit"))
    );

    let removed = host
        .execute(&format!("breakpoint remove {trigger_id}"))
        .unwrap();
    assert!(removed.summary.contains("removed trigger"));

    let listed_after = host.execute("breakpoint list").unwrap();
    assert_eq!(listed_after.lines.len(), 0);
}

#[test]
fn command_host_can_manage_named_breakpoint_predicates_and_groups() {
    let mut host = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );

    host.execute("attach").unwrap();
    host.execute("resume").unwrap();

    let predicate = host
        .execute(
            r#"breakpoint predicate add search_tool kind == ModelBoundary and artifact.json $.tool == "search""#,
        )
        .unwrap();
    assert!(predicate.summary.contains("defined breakpoint predicate"));

    let added = host
        .execute("breakpoint add pause_search group=search @search_tool")
        .unwrap();
    assert!(added.summary.contains("added trigger"));

    let predicates = host.execute("breakpoint predicates").unwrap();
    assert!(
        predicates
            .lines
            .iter()
            .any(|line| line.contains("predicate=search_tool breakpoints=1"))
    );

    let groups = host.execute("breakpoint group list").unwrap();
    assert!(
        groups
            .lines
            .iter()
            .any(|line| line.contains("definition_group=search enabled=true count=1"))
    );

    host.execute("pump").unwrap();
    host.execute("breakpoint group disable search").unwrap();
    let listed = host.execute("breakpoint list").unwrap();
    let breakpoint_line = listed
        .lines
        .iter()
        .find(|line| line.starts_with("bp="))
        .unwrap();
    assert!(breakpoint_line.contains("state=disabled"));
    assert!(breakpoint_line.contains("configured=enabled"));
    assert!(breakpoint_line.contains("group=search"));
    assert!(breakpoint_line.contains("predicate=search_tool"));

    host.execute("breakpoint group enable search").unwrap();
    let triggered = host.execute("pump").unwrap();
    assert!(
        triggered
            .lines
            .iter()
            .any(|line| line.contains("stop kind=breakpoint"))
    );
}

#[test]
fn command_host_can_manage_watchpoints_and_persist_them() {
    let mut host = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );

    host.execute("attach").unwrap();
    let added = host
        .execute(
            "watchpoint add cache_turn agent.state path=$.status after=250 kind=Lifecycle summary=loaded group=load",
        )
        .unwrap();
    assert!(added.summary.contains("added watchpoint"));
    assert!(
        added
            .lines
            .iter()
            .any(|line| line.contains("value_key=agent.state"))
    );

    let listed = host.execute("watchpoint list").unwrap();
    let watchpoint_line = listed
        .lines
        .iter()
        .find(|line| line.starts_with("wp="))
        .unwrap()
        .to_string();
    assert!(watchpoint_line.contains("value_key=agent.state"));
    assert!(watchpoint_line.contains("path=$.status"));
    assert!(watchpoint_line.contains("after=250"));
    assert!(watchpoint_line.contains("kind=Lifecycle"));
    assert!(watchpoint_line.contains("summary=loaded"));

    let trigger_id = watchpoint_line
        .split_whitespace()
        .find_map(|part| part.strip_prefix("wp="))
        .unwrap()
        .to_string();
    let shown = host
        .execute(&format!("watchpoint show {trigger_id}"))
        .unwrap();
    assert!(
        shown
            .lines
            .iter()
            .any(|line| line == "value_key=agent.state")
    );
    assert!(shown.lines.iter().any(|line| line == "path=$.status"));

    let path = std::env::temp_dir().join(format!(
        "swat-watchpoint-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    host.execute(&format!("watchpoint save {}", path.display()))
        .unwrap();

    let saved = fs::read_to_string(&path).unwrap();
    assert!(saved.contains("\"watchpoint\""));
    assert!(saved.contains("\"value_key\": \"agent.state\""));

    let mut reloaded = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );
    reloaded.execute("attach").unwrap();
    reloaded
        .execute(&format!("watchpoint load {}", path.display()))
        .unwrap();
    let reloaded_list = reloaded.execute("watchpoint list").unwrap();
    assert!(
        reloaded_list
            .lines
            .iter()
            .any(|line| line.contains("value_key=agent.state"))
    );

    fs::remove_file(path).unwrap();
}

#[test]
fn command_host_can_toggle_trigger_enabled_state() {
    let mut host = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );

    host.execute("attach").unwrap();
    host.execute("resume").unwrap();
    host.execute(
        r#"trigger-expr pause_search kind == ModelBoundary and artifact.json $.tool == "search""#,
    )
    .unwrap();

    let listed = host.execute("triggers").unwrap();
    let trigger_id = listed.lines[0]
        .split_whitespace()
        .find_map(|part| part.strip_prefix("trigger="))
        .unwrap()
        .to_string();
    assert!(listed.lines[0].contains("enabled=true"));

    let disabled = host
        .execute(&format!("trigger-disable {trigger_id}"))
        .unwrap();
    assert!(disabled.summary.contains("disabled trigger"));

    let listed = host.execute("triggers").unwrap();
    assert!(listed.lines[0].contains("enabled=false"));

    let enabled = host
        .execute(&format!("trigger-enable {trigger_id}"))
        .unwrap();
    assert!(enabled.summary.contains("enabled trigger"));

    let listed = host.execute("triggers").unwrap();
    assert!(listed.lines[0].contains("enabled=true"));
}

#[test]
fn raw_trigger_listing_remains_available_for_compatibility() {
    let mut host = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );

    host.execute("attach").unwrap();
    host.execute("resume").unwrap();
    host.execute(
        r#"trigger-expr pause_search kind == ModelBoundary and artifact.json $.tool == "search""#,
    )
    .unwrap();

    let listed = host.execute("triggers").unwrap();
    assert_eq!(listed.lines.len(), 1);
    assert!(listed.lines[0].contains("trigger="));
    assert!(listed.lines[0].contains("actions=PauseTarget"));
}

#[test]
fn command_host_can_save_and_restore_trigger_sets() {
    let path = std::env::temp_dir().join(format!(
        "swat-trigger-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let path_str = path.display().to_string();

    let mut host = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );
    host.execute("attach").unwrap();
    host.execute("resume").unwrap();
    host.execute(r#"trigger-snapshot snapshot_search kind == ModelBoundary and artifact.json $.tool == "search" capture search boundary"#)
        .unwrap();
    let listed = host.execute("triggers").unwrap();
    let trigger_id = listed.lines[0]
        .split_whitespace()
        .find_map(|part| part.strip_prefix("trigger="))
        .unwrap()
        .to_string();
    host.execute(&format!("trigger-disable {trigger_id}"))
        .unwrap();

    let saved = host.execute(&format!("trigger-save {path_str}")).unwrap();
    assert!(saved.summary.contains("saved 1 trigger"));
    let persisted = fs::read_to_string(&path).unwrap();
    assert!(persisted.contains("snapshot_search"));
    assert!(persisted.contains(r#""format_version": 4"#));
    assert!(persisted.contains(r#""kind": "create_snapshot""#));
    assert!(persisted.contains(r#""enabled": false"#));

    let mut restored = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );
    restored.execute("attach").unwrap();
    restored.execute("resume").unwrap();

    let loaded = restored
        .execute(&format!("trigger-load {path_str}"))
        .unwrap();
    assert!(loaded.summary.contains("loaded 1 trigger"));

    let listed = restored.execute("triggers").unwrap();
    assert_eq!(listed.lines.len(), 1);
    assert!(listed.lines[0].contains("snapshot_search"));
    assert!(listed.lines[0].contains("enabled=false"));
    assert!(listed.lines[0].contains("hits=0"));
    assert!(listed.lines[0].contains("CreateSnapshot(\"capture search boundary\")"));
    assert!(listed.lines[0].contains("artifact.json $.tool"));

    let trigger_id = listed.lines[0]
        .split_whitespace()
        .find_map(|part| part.strip_prefix("trigger="))
        .unwrap()
        .to_string();
    restored
        .execute(&format!("trigger-enable {trigger_id}"))
        .unwrap();
    restored.execute("pump").unwrap();
    let triggered = restored.execute("pump").unwrap();
    assert!(triggered.lines.iter().any(|line| line.contains("trigger=")));
    assert!(
        triggered
            .lines
            .iter()
            .any(|line| line.contains("mock snapshot requested: capture search boundary"))
    );
    let listed = restored.execute("triggers").unwrap();
    assert!(listed.lines[0].contains("hits=1"));

    let _ = fs::remove_file(path);
}

#[test]
fn command_host_can_run_until_expression() {
    let mut host = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );

    host.execute("attach").unwrap();

    let until = host
        .execute(r#"until kind == ModelBoundary and artifact.json $.tool == "search""#)
        .unwrap();
    assert!(until.summary.contains("until matched"));
    assert!(
        until
            .lines
            .iter()
            .any(|line| line.contains("mock target resumed"))
    );
    assert!(until.lines.iter().any(|line| line.contains("TriggerHit")));
    assert!(
        until
            .lines
            .iter()
            .any(|line| line.contains("control accepted=true"))
    );

    let listed = host.execute("triggers").unwrap();
    assert!(listed.lines.is_empty());
}

#[test]
fn command_host_can_list_show_and_replay_snapshots() {
    let mut host = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );

    host.execute("attach").unwrap();
    host.execute("resume").unwrap();
    host.execute("pump").unwrap();
    host.execute("pump").unwrap();

    let snapshot = host.execute("snapshot shell checkpoint").unwrap();
    assert!(snapshot.summary.contains("created snapshot"));
    let snapshot_id = snapshot
        .lines
        .iter()
        .find_map(|line| {
            line.split_whitespace()
                .find_map(|part| part.strip_prefix("snapshot="))
                .map(ToString::to_string)
        })
        .unwrap();

    let listed = host.execute("snapshots").unwrap();
    assert_eq!(listed.lines.len(), 1);
    assert!(listed.lines[0].contains(&format!("snapshot={snapshot_id}")));
    assert!(listed.lines[0].contains("replay=1"));

    let shown = host
        .execute(&format!("snapshot-show {snapshot_id}"))
        .unwrap();
    assert!(shown.summary.contains("snapshot"));
    assert!(
        shown
            .lines
            .iter()
            .any(|line| line.contains("reason=\"shell checkpoint\""))
    );
    assert!(
        shown
            .lines
            .iter()
            .any(|line| line.contains("replay_directives=1"))
    );

    let replay = host.execute(&format!("replay {snapshot_id}")).unwrap();
    assert!(replay.summary.contains("applied 1 replay directive"));
    assert!(replay.lines.iter().any(|line| line.contains("boundary=42")));
    assert!(
        replay
            .lines
            .iter()
            .any(|line| line.contains("prepared replay for boundary"))
    );
}

#[test]
fn command_host_can_load_legacy_v1_trigger_files() {
    let path = std::env::temp_dir().join(format!(
        "swat-trigger-v1-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(
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

    let mut host = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );
    host.execute("attach").unwrap();
    host.execute("resume").unwrap();

    let loaded = host
        .execute(&format!("trigger-load {}", path.display()))
        .unwrap();
    assert!(loaded.summary.contains("loaded 1 trigger"));

    let listed = host.execute("triggers").unwrap();
    assert!(listed.lines[0].contains("pause_search"));
    assert!(listed.lines[0].contains("enabled=true"));
    assert!(listed.lines[0].contains("actions=PauseTarget"));

    let _ = fs::remove_file(path);
}

#[test]
fn command_host_can_view_source_file_directly() {
    let path = std::env::temp_dir().join(format!(
        "swat-command-source-view-{}-{}.py",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(
        &path,
        "def alpha():\n    return 1\n\ndef beta():\n    return alpha()\n",
    )
    .unwrap();

    let mut host = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );
    let viewed = host
        .execute(&format!("source view {} 4 1 1", path.display()))
        .unwrap();
    assert!(
        viewed
            .summary
            .contains(&format!("source {}:4", path.display()))
    );
    assert!(viewed.lines.iter().any(|line| line.contains("def beta")));

    let legacy_view = host.execute(&format!("view {} 4", path.display())).unwrap();
    assert!(
        legacy_view
            .summary
            .contains(&format!("source {}:4", path.display()))
    );
    assert!(
        legacy_view
            .lines
            .iter()
            .any(|line| line.contains("def beta"))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn command_host_can_run_legacy_spawn_and_wakeup_helpers() {
    let code = r#"
import json
import sys
import time

PREFIX = "__SWATAGENT__"

def emit(record):
    sys.stdout.write(PREFIX + json.dumps(record) + "\n")
    sys.stdout.flush()

time.sleep(0.1)
emit({"kind": "tool", "phase": "start", "name": "loader", "summary": "patient started", "file": "/tmp/agent.py", "line": 8, "function": "run", "patient": {"name": "ui"}})
time.sleep(0.1)
emit({"kind": "tool", "phase": "end", "name": "loader", "summary": "patient resumed", "file": "/tmp/agent.py", "line": 10, "function": "run", "patient": {"name": "ui"}})
time.sleep(0.1)
"#;
    let adapter =
        AgentRuntimeAdapter::new(AgentRuntimeSpec::new("python3").with_args(["-u", "-c", code]));
    let mut host = CommandHost::new(Box::new(adapter), Box::new(InMemoryStore::new()));

    host.execute("attach").unwrap();

    let spawn = host.execute("spawn ui run").unwrap();
    assert!(spawn.summary.contains("spawn ui until matched"));

    host.execute("patient-default ui").unwrap();
    let wakeup = host.execute("wakeup").unwrap();
    assert!(wakeup.summary.contains("wakeup ui until matched"));
}

#[test]
fn agent_command_host_can_resolve_entities_spans_and_artifacts() {
    let code = r#"
import json
import sys
import time

PREFIX = "__SWATAGENT__"

def emit(record):
    sys.stdout.write(PREFIX + json.dumps(record) + "\n")
    sys.stdout.flush()

emit({"kind": "planner", "phase": "start", "name": "draft-answer", "summary": "planner started", "file": "/tmp/agent.py", "line": 10, "function": "run"})
emit({"kind": "model", "phase": "request", "span_id": "model-1", "correlation_id": "req-7", "name": "gpt-4.1-mini", "summary": "model requested", "file": "/tmp/agent.py", "line": 12, "function": "run"})
emit({"kind": "tool", "phase": "start", "span_id": "tool-1", "correlation_id": "req-7", "name": "web_search", "summary": "tool started", "file": "/tmp/agent.py", "line": 14, "function": "run", "locals": {"query": {"type": "str", "value": "weather"}, "limit": {"type": "int", "value": 3}}, "registers": {"pc": {"group": "trace", "type": "str", "value": "run:14"}, "phase": {"group": "trace", "type": "str", "value": "start"}}, "patient": {"name": "ui", "id": "patient-ui", "role": "application", "status": "running", "runtime": "pcgeos", "handles": [{"id": "h:1001", "kind": "resource", "state": ["in", "fixed"], "resource": "AppResource", "objects": [{"id": "^lui:0002", "class": "GenApplication"}]}], "resources": [{"name": "AppResource", "handle": "h:1001", "kind": "ui", "objects": ["^lui:0002"]}], "objects": [{"id": "^lui:0002", "class": "GenApplication", "handle": "h:1001", "resource": "AppResource", "address": "^lui:0002"}]}})
emit({"kind": "tool", "phase": "end", "span_id": "tool-1", "correlation_id": "req-7", "name": "web_search", "summary": "tool completed", "file": "/tmp/agent.py", "line": 18, "function": "run", "handle": {"id": "h:1001", "patient": "ui", "resource": "AppResource", "attached": True, "size": 8192}})
emit({"kind": "model", "phase": "response", "span_id": "model-1", "correlation_id": "req-7", "name": "gpt-4.1-mini", "summary": "model responded", "file": "/tmp/agent.py", "line": 21, "function": "run"})
emit({"kind": "state", "phase": "update", "name": "memory.turn", "summary": "memory updated"})
time.sleep(0.1)
"#;
    let adapter =
        AgentRuntimeAdapter::new(AgentRuntimeSpec::new("python3").with_args(["-u", "-c", code]));
    let mut host = CommandHost::new(Box::new(adapter), Box::new(InMemoryStore::new()));

    host.execute("attach").unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let pumped = host.execute("pump").unwrap();
        if pumped
            .lines
            .iter()
            .any(|line| line.contains("agent runtime exited"))
        {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }

    let entities = host.execute("entities web").unwrap();
    assert_eq!(entities.lines.len(), 1);
    assert!(entities.lines[0].contains("ToolName"));
    assert!(entities.lines[0].contains("web_search"));

    let correlated = host.execute("correlation req-7").unwrap();
    assert_eq!(correlated.lines.len(), 7);
    assert!(
        correlated
            .lines
            .iter()
            .any(|line| line.contains("span_ids=model-1,tool-1"))
    );
    assert!(
        correlated
            .lines
            .iter()
            .any(|line| line.contains("entity_count="))
    );

    let spans = host.execute("stack").unwrap();
    assert_eq!(spans.lines.len(), 2);
    assert!(spans.lines[0].contains("frame=0"));
    assert!(spans.lines[0].contains("label=web_search"));
    assert!(spans.lines[1].contains("frame=1"));
    assert!(spans.lines[1].contains("label=gpt-4.1-mini"));

    let backtrace = host.execute("backtrace 1").unwrap();
    assert_eq!(backtrace.lines.len(), 1);
    assert!(backtrace.lines[0].starts_with("* "));

    let func = host.execute("func").unwrap();
    assert!(func.summary.contains("func web_search"));

    let up = host.execute("up 1").unwrap();
    assert!(up.summary.contains("stack frame 1"));
    let current_func = host.execute("func").unwrap();
    assert!(current_func.summary.contains("gpt-4.1-mini"));
    let down = host.execute("down").unwrap();
    assert!(down.summary.contains("stack frame 0"));

    let frame = host.execute("stack frame 0").unwrap();
    assert!(frame.summary.contains("stack frame 0"));
    assert!(frame.lines.iter().any(|line| line.contains("depth=1")));
    assert!(
        frame
            .lines
            .iter()
            .any(|line| line.contains("label=web_search"))
    );
    assert!(frame.lines.iter().any(|line| line.contains("events:")));
    assert!(frame.lines.iter().any(|line| line.contains("locals=2")));
    assert!(frame.lines.iter().any(|line| line.contains("registers=2")));

    let locals = host.execute("stack locals 0").unwrap();
    assert!(locals.lines.iter().any(|line| line.contains("local=query")));
    assert!(locals.lines.iter().any(|line| line.contains("local=limit")));

    let legacy_locals = host.execute("locals").unwrap();
    assert!(
        legacy_locals
            .lines
            .iter()
            .any(|line| line.contains("local=query"))
    );

    let where_output = host.execute("where").unwrap();
    assert!(
        where_output
            .lines
            .iter()
            .any(|line| line.contains("source:"))
    );

    let slist = host.execute("slist").unwrap();
    assert!(
        slist.summary.contains("source unresolved")
            || slist.summary.contains("source /tmp/agent.py")
    );

    let patients = host.execute("patient").unwrap();
    assert_eq!(patients.lines.len(), 1);
    assert!(patients.lines[0].contains("patient=ui"));
    assert!(patients.lines[0].contains("resources=1"));

    let patient_default = host.execute("patient-default ui").unwrap();
    assert!(patient_default.summary.contains("patient-default ui"));

    let patient = host.execute("patient show ui").unwrap();
    assert!(
        patient
            .lines
            .iter()
            .any(|line| line.contains("handles=h:1001"))
    );
    assert!(
        patient
            .lines
            .iter()
            .any(|line| line.contains("resources=AppResource"))
    );

    let handles = host.execute("handle").unwrap();
    assert_eq!(handles.lines.len(), 1);
    assert!(handles.lines[0].contains("handle=h:1001"));
    assert!(handles.lines[0].contains("resource=AppResource"));

    let handle = host.execute("handle show h:1001").unwrap();
    assert!(
        handle
            .lines
            .iter()
            .any(|line| line.contains("attached=true"))
    );
    assert!(
        handle
            .lines
            .iter()
            .any(|line| line.contains("objects=^lui:0002"))
    );

    let resources = host.execute("resource").unwrap();
    assert_eq!(resources.lines.len(), 1);
    assert!(resources.lines[0].contains("resource=AppResource"));

    let resource = host.execute("resource show AppResource").unwrap();
    assert!(
        resource
            .lines
            .iter()
            .any(|line| line.contains("handle=h:1001"))
    );

    let objects = host.execute("object").unwrap();
    assert_eq!(objects.lines.len(), 1);
    assert!(objects.lines[0].contains("object=^lui:0002"));

    let object = host.execute("object show ^lui:0002").unwrap();
    assert!(
        object
            .lines
            .iter()
            .any(|line| line.contains("class=GenApplication"))
    );
    let object_name = host.execute("obj-name ^lui:0002").unwrap();
    assert!(
        object_name
            .lines
            .iter()
            .any(|line| line.contains("GenApplication"))
    );
    let object_class = host.execute("obj-class ^lui:0002").unwrap();
    assert_eq!(object_class.lines, vec!["class=GenApplication".to_string()]);

    let values = host.execute("value").unwrap();
    assert!(
        values
            .lines
            .iter()
            .any(|line| line.contains("value_key=agent.state"))
    );

    let value = host.execute("value show agent.state").unwrap();
    assert!(value.lines.iter().any(|line| line.contains("history:")));
    assert!(
        value
            .lines
            .iter()
            .any(|line| line.contains("summary=memory updated"))
    );

    let source_functions = host.execute("source functions").unwrap();
    assert_eq!(source_functions.lines.len(), 1);
    assert!(source_functions.lines[0].contains("function=run"));
    let source_function = host.execute("source function run").unwrap();
    assert_eq!(source_function.lines.len(), 5);
    assert!(
        source_function
            .lines
            .iter()
            .all(|line| line.contains("event="))
    );

    let registers = host.execute("stack registers 0").unwrap();
    assert!(
        registers
            .lines
            .iter()
            .any(|line| line.contains("register=pc"))
    );
    assert!(
        registers
            .lines
            .iter()
            .any(|line| line.contains("register=phase"))
    );

    let model_boundary = spans
        .lines
        .iter()
        .find_map(|line| {
            let raw = line
                .split_whitespace()
                .find_map(|part| part.strip_prefix("boundary="))?;
            if line.contains("label=gpt-4.1-mini") {
                Some(raw.to_string())
            } else {
                None
            }
        })
        .unwrap();
    let span = host
        .execute(&format!("stack show {model_boundary}"))
        .unwrap();
    assert!(span.summary.contains("stack boundary"));
    assert!(
        span.lines
            .iter()
            .any(|line| line.contains("label=gpt-4.1-mini"))
    );
    assert!(span.lines.iter().any(|line| line.contains("events:")));
    assert!(span.lines.iter().any(|line| line.contains("event=")));

    let model_events = host.execute("events ModelBoundary").unwrap();
    assert_eq!(model_events.lines.len(), 2);
    let event_id = model_events.lines[0]
        .split_whitespace()
        .find_map(|part| part.strip_prefix("event="))
        .unwrap()
        .to_string();
    let artifacts = host.execute(&format!("artifacts {event_id}")).unwrap();
    assert_eq!(artifacts.lines.len(), 1);
    assert!(artifacts.lines[0].contains("bytes="));
    assert!(artifacts.lines[0].contains("lines="));
    assert!(artifacts.lines[0].contains("gpt-4.1-mini"));

    let artifact_detail = host.execute(&format!("artifact-show {event_id}")).unwrap();
    assert!(artifact_detail.summary.contains("artifact 0"));
    assert!(
        artifact_detail
            .lines
            .iter()
            .any(|line| line.contains("detail") && line.contains("gpt-4.1-mini"))
    );

    let source_event_id = host
        .execute("events Execution")
        .unwrap()
        .lines
        .into_iter()
        .find(|line| line.contains("planner started"))
        .and_then(|line| {
            line.split_whitespace()
                .find_map(|part| part.strip_prefix("event="))
                .map(ToString::to_string)
        })
        .unwrap();
    let source = host
        .execute(&format!("source show {source_event_id}"))
        .unwrap();
    assert!(source.summary.contains("source unresolved"));
    assert!(
        source
            .lines
            .iter()
            .any(|line| line.contains("failure_kind=MissingFile"))
    );

    let source_file = host.execute("source file /tmp/agent.py").unwrap();
    assert!(source_file.summary.contains("/tmp/agent.py"));
    assert!(
        source_file
            .lines
            .iter()
            .any(|line| line.contains("planner started"))
    );

    let source_files = host.execute("source files").unwrap();
    assert_eq!(source_files.lines.len(), 1);
    assert!(source_files.lines[0].contains("file=/tmp/agent.py"));
    assert!(source_files.lines[0].contains("functions=run"));
}
