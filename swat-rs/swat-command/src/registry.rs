use crate::CommandOutput;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandSurface {
    Shell,
    Tui,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CommandDescriptor {
    key: &'static str,
    synopsis: &'static str,
    summary: &'static str,
    aliases: &'static [&'static str],
    tui_supported: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CommandFamily {
    topic: &'static str,
    aliases: &'static [&'static str],
    summary: &'static str,
    commands: &'static [&'static str],
    notes: &'static [&'static str],
    examples: &'static [&'static str],
}

const COMMANDS: &[CommandDescriptor] = &[
    CommandDescriptor {
        key: "attach",
        synopsis: "attach",
        summary: "attach to the configured target",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "session",
        synopsis: "session | status",
        summary: "show the active session summary and capabilities",
        aliases: &["status"],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "pump",
        synopsis: "pump",
        summary: "capture newly emitted events from the target",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "pause",
        synopsis: "pause",
        summary: "request a target pause",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "resume",
        synopsis: "resume",
        summary: "resume target execution",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "step",
        synopsis: "step",
        summary: "step the target once where the adapter supports it",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "snapshot",
        synopsis: "snapshot <reason>",
        summary: "capture a live snapshot through the shared control API",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "snapshots",
        synopsis: "snapshots",
        summary: "list durable snapshots for the active session",
        aliases: &[],
        tui_supported: false,
    },
    CommandDescriptor {
        key: "snapshot-show",
        synopsis: "snapshot-show <snapshot_id>",
        summary: "inspect one snapshot and its replay metadata",
        aliases: &[],
        tui_supported: false,
    },
    CommandDescriptor {
        key: "replay",
        synopsis: "replay <snapshot_id|boundary_id>",
        summary: "preview or apply replay directives",
        aliases: &[],
        tui_supported: false,
    },
    CommandDescriptor {
        key: "events",
        synopsis: "events [EventKind]",
        summary: "list session events, optionally filtered by kind",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "event",
        synopsis: "event <event_id>",
        summary: "select or inspect one event",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "artifacts",
        synopsis: "artifacts <event_id>",
        summary: "list decoded artifact previews for an event",
        aliases: &[],
        tui_supported: false,
    },
    CommandDescriptor {
        key: "artifact-show",
        synopsis: "artifact-show <event_id> [index]",
        summary: "render one decoded artifact in detail",
        aliases: &[],
        tui_supported: false,
    },
    CommandDescriptor {
        key: "query",
        synopsis: "query <expr>",
        summary: "filter events through the shared query engine",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "entities",
        synopsis: "entities <needle>",
        summary: "search resolved entities by name",
        aliases: &[],
        tui_supported: false,
    },
    CommandDescriptor {
        key: "correlation",
        synopsis: "correlation <id>",
        summary: "show one correlation group's events and relation summary",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "stack",
        synopsis: "stack | stack frame <index> | stack show <boundary_id>",
        summary: "inspect frame-oriented stack projections over boundary spans",
        aliases: &["spans", "span"],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "source-show",
        synopsis: "source show <event_id> [before] [after]",
        summary: "show source context for one event",
        aliases: &["source"],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "source-file",
        synopsis: "source file <path>",
        summary: "list events that resolved to a specific source file",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "source-files",
        synopsis: "source files",
        summary: "list discovered source files across the active session",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "source-view",
        synopsis: "source view <path> [line] [before] [after]",
        summary: "show a file-backed source snippet without starting from one event",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "breakpoint-list",
        synopsis: "breakpoint list",
        summary: "list semantic breakpoints in grouped debugger-oriented form",
        aliases: &["breakpoint", "breakpoints"],
        tui_supported: false,
    },
    CommandDescriptor {
        key: "breakpoint-show",
        synopsis: "breakpoint show <id>",
        summary: "inspect one breakpoint's metadata, predicate, and last-hit event",
        aliases: &[],
        tui_supported: false,
    },
    CommandDescriptor {
        key: "breakpoint-groups",
        synopsis: "breakpoint groups",
        summary: "summarize breakpoint groups by state, lifetime, disposition, and activity",
        aliases: &[],
        tui_supported: false,
    },
    CommandDescriptor {
        key: "breakpoint-add",
        synopsis: "breakpoint add <name> <expr>",
        summary: "add a persistent pause breakpoint",
        aliases: &["trigger-expr"],
        tui_supported: false,
    },
    CommandDescriptor {
        key: "breakpoint-once",
        synopsis: "breakpoint once <name> <expr>",
        summary: "add a fire-once pause breakpoint",
        aliases: &["trigger-expr-once"],
        tui_supported: false,
    },
    CommandDescriptor {
        key: "breakpoint-snapshot",
        synopsis: "breakpoint snapshot <name> <expr> <reason>",
        summary: "add a snapshot-taking breakpoint",
        aliases: &["trigger-snapshot"],
        tui_supported: false,
    },
    CommandDescriptor {
        key: "breakpoint-enable",
        synopsis: "breakpoint enable <id>",
        summary: "enable one breakpoint without deleting it",
        aliases: &["trigger-enable"],
        tui_supported: false,
    },
    CommandDescriptor {
        key: "breakpoint-disable",
        synopsis: "breakpoint disable <id>",
        summary: "disable one breakpoint without deleting it",
        aliases: &["trigger-disable"],
        tui_supported: false,
    },
    CommandDescriptor {
        key: "breakpoint-remove",
        synopsis: "breakpoint remove <id>",
        summary: "remove one breakpoint definition",
        aliases: &["trigger-remove"],
        tui_supported: false,
    },
    CommandDescriptor {
        key: "breakpoint-save",
        synopsis: "breakpoint save <path>",
        summary: "persist breakpoint definitions to disk",
        aliases: &["trigger-save"],
        tui_supported: false,
    },
    CommandDescriptor {
        key: "breakpoint-load",
        synopsis: "breakpoint load <path>",
        summary: "load persisted breakpoint definitions",
        aliases: &["trigger-load"],
        tui_supported: false,
    },
    CommandDescriptor {
        key: "until",
        synopsis: "until <expr>",
        summary: "resume until an expression-backed breakpoint matches",
        aliases: &["breakpoint until"],
        tui_supported: false,
    },
    CommandDescriptor {
        key: "script",
        synopsis: "script <rhai>",
        summary: "run a script against a frozen session snapshot",
        aliases: &[],
        tui_supported: false,
    },
];

const FAMILIES: &[CommandFamily] = &[
    CommandFamily {
        topic: "session",
        aliases: &["status", "control"],
        summary: "attach, inspect, and drive a live target session",
        commands: &["attach", "session", "pump", "pause", "resume", "step"],
        notes: &[],
        examples: &["attach", "session", "resume"],
    },
    CommandFamily {
        topic: "snapshot",
        aliases: &["snapshots", "replay"],
        summary: "capture, inspect, and replay durable snapshots",
        commands: &["snapshot", "snapshots", "snapshot-show", "replay"],
        notes: &[],
        examples: &["snapshot shell checkpoint", "snapshots", "replay 1"],
    },
    CommandFamily {
        topic: "query",
        aliases: &["queries", "inspection"],
        summary: "search events, entities, and correlations through shared APIs",
        commands: &["events", "event", "query", "entities", "correlation"],
        notes: &[
            r#"fields: kind, event.id, sequence, correlation, boundary, span, value.key, source.file, source.function, summary, artifact.text, artifact.json"#,
            r#"operators: ==, contains, exists, and, or, not"#,
        ],
        examples: &[
            r#"query kind == ModelBoundary and correlation == "req-7""#,
            r#"query not source.file exists and summary contains "attached""#,
        ],
    },
    CommandFamily {
        topic: "stack",
        aliases: &["spans", "span"],
        summary: "inspect frame-oriented stack projections over the shared boundary model",
        commands: &["stack"],
        notes: &[
            "Stack frames are projected from shared boundary spans through `swat-api`, not rebuilt inside the shell or TUI.",
            "Use `stack frame <index>` for debugger-style frame details and `stack show <boundary_id>` when you need the raw span identity.",
        ],
        examples: &["stack", "stack frame 0", "stack show 42"],
    },
    CommandFamily {
        topic: "source",
        aliases: &["srclist", "slist"],
        summary: "navigate from events into source context and file-backed views",
        commands: &["source-show", "source-file", "source-files", "source-view"],
        notes: &[
            "Legacy-style source workflows now start from shared event and resolver APIs instead of shell-local helpers.",
            "Use `source files` to discover the file set for a session and `source view` to open a file directly on the shared source layer.",
            "The existing shorthand `source <event_id> [before] [after]` remains available.",
        ],
        examples: &[
            "source files",
            "source show 7",
            "source file /tmp/agent.py",
            "source view /tmp/agent.py 42 2 4",
        ],
    },
    CommandFamily {
        topic: "artifact",
        aliases: &["artifacts"],
        summary: "inspect artifact-backed values attached to events",
        commands: &["artifacts", "artifact-show"],
        notes: &[],
        examples: &["artifacts 7", "artifact-show 7 1"],
    },
    CommandFamily {
        topic: "breakpoint",
        aliases: &["breakpoints", "trigger", "triggers"],
        summary: "manage semantic breakpoints on top of the shared trigger engine",
        commands: &[
            "breakpoint-list",
            "breakpoint-show",
            "breakpoint-groups",
            "breakpoint-add",
            "breakpoint-once",
            "breakpoint-snapshot",
            "breakpoint-enable",
            "breakpoint-disable",
            "breakpoint-remove",
            "breakpoint-save",
            "breakpoint-load",
            "until",
        ],
        notes: &[
            "This first slice maps debugger breakpoint vocabulary onto the existing trigger engine instead of adding a parallel control model.",
            "Use `breakpoint list`, `breakpoint show`, and `breakpoint groups` for debugger-oriented inspection; the raw `triggers` command remains for compatibility.",
        ],
        examples: &[
            r#"breakpoint add stop_search kind == ModelBoundary and artifact.json $.tool == "search""#,
            "breakpoint list",
            "breakpoint show 7",
            "breakpoint groups",
            "until kind == TriggerHit",
        ],
    },
    CommandFamily {
        topic: "automation",
        aliases: &["script"],
        summary: "run script-backed inspection workflows on the public API",
        commands: &["script"],
        notes: &[],
        examples: &["script ctx.event_count()"],
    },
];

pub fn command_help(topic: Option<&str>, surface: CommandSurface) -> CommandOutput {
    let normalized = topic.unwrap_or("").trim();
    if normalized.is_empty() {
        return CommandOutput::new("available commands", overview_lines(surface));
    }

    let Some(family) = find_family(normalized) else {
        let topics = FAMILIES
            .iter()
            .map(|family| family.topic)
            .collect::<Vec<_>>()
            .join(", ");
        return CommandOutput::new(
            format!("unknown help topic {normalized}"),
            vec![format!("topics: {topics}")],
        );
    };

    let mut lines = vec![family.summary.to_string()];
    lines.extend(
        family
            .commands
            .iter()
            .filter_map(|key| find_command(key))
            .map(|command| format_command_line(command, surface)),
    );
    if !family.notes.is_empty() {
        lines.extend(family.notes.iter().map(|note| note.to_string()));
    }
    if !family.examples.is_empty() {
        lines.push(format!("examples: {}", family.examples.join(" | ")));
    }
    CommandOutput::new(format!("help {}", family.topic), lines)
}

fn overview_lines(surface: CommandSurface) -> Vec<String> {
    let mut lines = FAMILIES
        .iter()
        .map(|family| {
            let commands = family
                .commands
                .iter()
                .filter_map(|key| find_command(key))
                .map(|command| format_synopsis(command, surface))
                .collect::<Vec<_>>()
                .join(" | ");
            format!("{}: {commands}", family.topic)
        })
        .collect::<Vec<_>>();
    lines.push("examples: help query | help stack | help source | help breakpoint".to_string());
    if matches!(surface, CommandSurface::Tui) {
        lines.push("Commands marked [shell] are discoverable in the TUI help surface but execute from the shell today.".to_string());
    }
    lines
}

fn find_family(topic: &str) -> Option<&'static CommandFamily> {
    FAMILIES.iter().find(|family| {
        family.topic.eq_ignore_ascii_case(topic)
            || family
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(topic))
    })
}

fn find_command(key: &str) -> Option<&'static CommandDescriptor> {
    COMMANDS.iter().find(|command| {
        command.key == key
            || command
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(key))
    })
}

fn format_command_line(command: &CommandDescriptor, surface: CommandSurface) -> String {
    format!("{}  {}", format_synopsis(command, surface), command.summary)
}

fn format_synopsis(command: &CommandDescriptor, surface: CommandSurface) -> String {
    if matches!(surface, CommandSurface::Tui) && !command.tui_supported {
        format!("{} [shell]", command.synopsis)
    } else {
        command.synopsis.to_string()
    }
}
