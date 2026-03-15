use std::collections::BTreeSet;

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
        key: "value",
        synopsis: "value | value show <value_key>",
        summary: "inspect observed value histories through shared APIs",
        aliases: &["values"],
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
        key: "patient",
        synopsis: "patient | patient show <name>",
        summary: "inspect typed patient identities across the active session",
        aliases: &["patients"],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "handle",
        synopsis: "handle | handle show <id>",
        summary: "inspect typed handle/resource ownership and state",
        aliases: &["handles"],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "resource",
        synopsis: "resource | resource show <name>",
        summary: "inspect typed resource ownership, source, and object counts",
        aliases: &["resources"],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "object",
        synopsis: "object | object show <id>",
        summary: "inspect typed object identity, class, and storage relations",
        aliases: &["objects"],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "stack",
        synopsis: "stack | stack frame <index> | stack locals <index> | stack registers <index> | stack show <boundary_id>",
        summary: "inspect frame-oriented stack projections plus typed locals/registers",
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
        key: "source-functions",
        synopsis: "source functions",
        summary: "list discovered source functions across the active session",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "source-function",
        synopsis: "source function <name>",
        summary: "list events that resolved to a specific source function",
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
        tui_supported: true,
    },
    CommandDescriptor {
        key: "breakpoint-show",
        synopsis: "breakpoint show <id>",
        summary: "inspect one breakpoint's metadata, predicate, and last-hit event",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "breakpoint-groups",
        synopsis: "breakpoint groups",
        summary: "summarize breakpoint groups by state, lifetime, disposition, and activity",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "breakpoint-group-list",
        synopsis: "breakpoint group list",
        summary: "list user-defined breakpoint groups and their enable policies",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "breakpoint-group-enable",
        synopsis: "breakpoint group enable <group>",
        summary: "enable a whole breakpoint group without touching per-breakpoint state",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "breakpoint-group-disable",
        synopsis: "breakpoint group disable <group>",
        summary: "disable a whole breakpoint group without deleting its definitions",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "breakpoint-predicates",
        synopsis: "breakpoint predicates",
        summary: "list reusable named breakpoint predicates",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "breakpoint-predicate-add",
        synopsis: "breakpoint predicate add <name> <expr>",
        summary: "define or update a reusable named breakpoint predicate",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "breakpoint-predicate-remove",
        synopsis: "breakpoint predicate remove <name>",
        summary: "remove an unused named breakpoint predicate",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "breakpoint-add",
        synopsis: "breakpoint add <name> <expr>",
        summary: "add a persistent pause breakpoint",
        aliases: &["trigger-expr"],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "breakpoint-once",
        synopsis: "breakpoint once <name> <expr>",
        summary: "add a fire-once pause breakpoint",
        aliases: &["trigger-expr-once"],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "breakpoint-snapshot",
        synopsis: "breakpoint snapshot <name> <expr> <reason>",
        summary: "add a snapshot-taking breakpoint",
        aliases: &["trigger-snapshot"],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "breakpoint-enable",
        synopsis: "breakpoint enable <id>",
        summary: "enable one breakpoint without deleting it",
        aliases: &["trigger-enable"],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "breakpoint-disable",
        synopsis: "breakpoint disable <id>",
        summary: "disable one breakpoint without deleting it",
        aliases: &["trigger-disable"],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "breakpoint-remove",
        synopsis: "breakpoint remove <id>",
        summary: "remove one breakpoint definition",
        aliases: &["trigger-remove"],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "watchpoint-list",
        synopsis: "watchpoint list",
        summary: "list value-change watchpoints and their gating conditions",
        aliases: &["watchpoint", "watchpoints"],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "watchpoint-show",
        synopsis: "watchpoint show <id>",
        summary: "inspect one watchpoint's watched value and gating metadata",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "watchpoint-add",
        synopsis: "watchpoint add <name> <value_key> [path=...] [after=...] [kind=...] [summary=...] [group=...]",
        summary: "add a persistent value-change watchpoint",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "watchpoint-once",
        synopsis: "watchpoint once <name> <value_key> [path=...] [after=...] [kind=...] [summary=...] [group=...]",
        summary: "add a fire-once value-change watchpoint",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "watchpoint-snapshot",
        synopsis: "watchpoint snapshot <name> <value_key> [path=...] [after=...] [kind=...] [summary=...] [group=...] -- <reason>",
        summary: "add a snapshot-taking watchpoint",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "watchpoint-enable",
        synopsis: "watchpoint enable <id>",
        summary: "enable one watchpoint without deleting it",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "watchpoint-disable",
        synopsis: "watchpoint disable <id>",
        summary: "disable one watchpoint without deleting it",
        aliases: &[],
        tui_supported: true,
    },
    CommandDescriptor {
        key: "watchpoint-remove",
        synopsis: "watchpoint remove <id>",
        summary: "remove one watchpoint definition",
        aliases: &[],
        tui_supported: true,
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
            r#"fields: kind, event.id, sequence, correlation, boundary, span, value.key, patient, handle, resource, object, source.file, source.line, source.function, summary, artifact.text, artifact.json"#,
            r#"operators: ==, contains, exists, and, or, not"#,
        ],
        examples: &[
            r#"query kind == ModelBoundary and correlation == "req-7""#,
            r#"query patient == "ui" and object contains "^lui""#,
            r#"query not source.file exists and summary contains "attached""#,
        ],
    },
    CommandFamily {
        topic: "patient",
        aliases: &["patients"],
        summary: "inspect debugger-style patient identities and their related handles/resources/objects",
        commands: &["patient"],
        notes: &[
            "Patients are projected from structured target artifacts through `swat-api`; the shell and TUI do not invent a separate target model.",
            "Use `patient` for the session summary and `patient show <name>` for one patient's related handles, resources, objects, and source files.",
        ],
        examples: &["patient", "patient show ui"],
    },
    CommandFamily {
        topic: "handle",
        aliases: &["handles"],
        summary: "inspect debugger-style handles without leaking raw kernel details into the core",
        commands: &["handle"],
        notes: &[
            "Handle summaries expose ownership, resource bindings, state flags, and related objects through the shared inspection API.",
        ],
        examples: &["handle", "handle show h:1001"],
    },
    CommandFamily {
        topic: "resource",
        aliases: &["resources"],
        summary: "inspect typed resources and their patient/handle/object relationships",
        commands: &["resource"],
        notes: &[],
        examples: &["resource", "resource show AppResource"],
    },
    CommandFamily {
        topic: "object",
        aliases: &["objects"],
        summary: "inspect typed object identities, classes, and storage relations",
        commands: &["object"],
        notes: &[],
        examples: &["object", "object show ^lui:0002"],
    },
    CommandFamily {
        topic: "stack",
        aliases: &["spans", "span"],
        summary: "inspect frame-oriented stack projections over the shared boundary model",
        commands: &["stack"],
        notes: &[
            "Stack frames are projected from shared boundary spans through `swat-api`, not rebuilt inside the shell or TUI.",
            "Use `stack frame <index>` for debugger-style frame details, `stack locals <index>` and `stack registers <index>` for typed bindings, and `stack show <boundary_id>` when you need the raw span identity.",
        ],
        examples: &[
            "stack",
            "stack frame 0",
            "stack locals 0",
            "stack registers 0",
            "stack show 42",
        ],
    },
    CommandFamily {
        topic: "source",
        aliases: &["srclist", "slist"],
        summary: "navigate from events into source context and file-backed views",
        commands: &[
            "source-show",
            "source-file",
            "source-files",
            "source-functions",
            "source-function",
            "source-view",
        ],
        notes: &[
            "Legacy-style source workflows now start from shared event and resolver APIs instead of shell-local helpers.",
            "Use `source files` to discover the file set for a session, `source functions` to traverse by function, and `source view` to open a file directly on the shared source layer.",
            "The existing shorthand `source <event_id> [before] [after]` remains available.",
        ],
        examples: &[
            "source files",
            "source functions",
            "source show 7",
            "source function helper",
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
        topic: "value",
        aliases: &["values"],
        summary: "inspect observed value histories instead of only querying value events",
        commands: &["value"],
        notes: &[
            "Value history is projected from shared `EventPayload::Value` records plus artifact previews, so shell and TUI formatting stay aligned.",
        ],
        examples: &["value", "value show memory.turn"],
    },
    CommandFamily {
        topic: "breakpoint",
        aliases: &["breakpoints", "trigger", "triggers"],
        summary: "manage semantic breakpoints on top of the shared trigger engine",
        commands: &[
            "breakpoint-list",
            "breakpoint-show",
            "breakpoint-groups",
            "breakpoint-group-list",
            "breakpoint-group-enable",
            "breakpoint-group-disable",
            "breakpoint-predicates",
            "breakpoint-predicate-add",
            "breakpoint-predicate-remove",
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
            "Advanced breakpoint state still runs on the shared trigger engine, but named predicates, group enable policies, and stop-reason reporting now project a debugger-grade model on top of it.",
            "Use `breakpoint groups` for automatic state/lifetime/disposition/activity projections, and `breakpoint group list` for user-defined breakpoint groups.",
            "Predicate references use `@name`, for example `breakpoint add stop_search @search_tool` or `breakpoint add stop_search group=search @search_tool`.",
            "The raw `triggers` command remains for compatibility.",
        ],
        examples: &[
            r#"breakpoint add stop_search kind == ModelBoundary and artifact.json $.tool == "search""#,
            r#"breakpoint predicate add search_tool kind == ModelBoundary and artifact.json $.tool == "search""#,
            r#"breakpoint add stop_search group=search @search_tool"#,
            "breakpoint list",
            "breakpoint show 7",
            "breakpoint groups",
            "until kind == TriggerHit",
        ],
    },
    CommandFamily {
        topic: "watchpoint",
        aliases: &["watchpoints"],
        summary: "manage value-change watchpoints with shared lifecycle and time gates",
        commands: &[
            "watchpoint-list",
            "watchpoint-show",
            "watchpoint-add",
            "watchpoint-once",
            "watchpoint-snapshot",
            "watchpoint-enable",
            "watchpoint-disable",
            "watchpoint-remove",
        ],
        notes: &[
            "Watchpoints stay on the shared trigger engine, but `swat-api` now projects watched value keys, JSON paths, elapsed-time gates, and lifecycle filters as first-class inspection fields.",
            "Use `after=<millis>` for elapsed-time gates, `kind=<EventKind>` to restrict the triggering event kind, and `summary=<needle>` for coarse lifecycle/load filters.",
            "The same persisted trigger files used by `breakpoint save` and `breakpoint load` now preserve watchpoint definitions too.",
        ],
        examples: &[
            "watchpoint add memory_turn agent.state after=250 kind=Lifecycle summary=updated",
            "watchpoint once cache_tool tool.cache path=$.status",
            "watchpoint snapshot load_guard agent.state kind=Lifecycle summary=loaded -- state changed after load",
            "watchpoint list",
            "watchpoint show 7",
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

pub fn command_search(needle: &str, surface: CommandSurface) -> CommandOutput {
    let normalized = needle.trim();
    if normalized.is_empty() {
        return CommandOutput::new(
            "help search requires a pattern",
            vec!["usage: help search <needle>".to_string()],
        );
    }

    let needle = normalized.to_ascii_lowercase();
    let mut lines = Vec::new();

    for family in FAMILIES {
        let aliases = family.aliases.join(",");
        if family.topic.to_ascii_lowercase().contains(&needle)
            || family
                .aliases
                .iter()
                .any(|alias| alias.to_ascii_lowercase().contains(&needle))
            || family.summary.to_ascii_lowercase().contains(&needle)
            || family
                .examples
                .iter()
                .any(|example| example.to_ascii_lowercase().contains(&needle))
        {
            lines.push(format!(
                "topic={} aliases={} summary={}",
                family.topic, aliases, family.summary
            ));
        }
    }

    for command in COMMANDS {
        let aliases = command.aliases.join(",");
        if command.synopsis.to_ascii_lowercase().contains(&needle)
            || command.summary.to_ascii_lowercase().contains(&needle)
            || command
                .aliases
                .iter()
                .any(|alias| alias.to_ascii_lowercase().contains(&needle))
        {
            lines.push(format!(
                "command={} aliases={} summary={}",
                format_synopsis(command, surface),
                aliases,
                command.summary
            ));
        }
    }

    if lines.is_empty() {
        lines.push("no command topics matched".to_string());
    }

    CommandOutput::new(format!("help search {normalized}"), lines)
}

pub fn command_completions(prefix: &str, surface: CommandSurface) -> Vec<String> {
    let normalized = prefix.trim();
    let normalized_lower = normalized.to_ascii_lowercase();
    let mut completions = BTreeSet::new();

    if normalized.is_empty() || "help".starts_with(&normalized_lower) {
        completions.insert("help".to_string());
    }

    for family in FAMILIES {
        let help_topic = format!("help {}", family.topic);
        if normalized.is_empty()
            || help_topic
                .to_ascii_lowercase()
                .starts_with(&normalized_lower)
        {
            completions.insert(help_topic);
        }
        for alias in family.aliases {
            let help_alias = format!("help {alias}");
            if normalized.is_empty()
                || help_alias
                    .to_ascii_lowercase()
                    .starts_with(&normalized_lower)
            {
                completions.insert(help_alias);
            }
        }
    }

    for command in COMMANDS {
        if !command_available_for_completion(command, surface) {
            continue;
        }

        if normalized.is_empty()
            || command
                .synopsis
                .to_ascii_lowercase()
                .starts_with(&normalized_lower)
        {
            completions.insert(command.synopsis.to_string());
        }
        for alias in command.aliases {
            if normalized.is_empty() || alias.to_ascii_lowercase().starts_with(&normalized_lower) {
                completions.insert((*alias).to_string());
            }
        }
    }

    completions.into_iter().collect()
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
    lines.push("search: help search <needle>".to_string());
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

fn command_available_for_completion(command: &CommandDescriptor, surface: CommandSurface) -> bool {
    matches!(surface, CommandSurface::Shell) || command.tui_supported
}
