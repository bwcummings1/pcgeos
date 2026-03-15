#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use rhai::{Dynamic, Engine, EvalAltResult, Scope};
use swat_api::{
    BreakpointGroupKind, BreakpointState, LiveSessionApi, TraceInspector, WatchpointSpec,
};
use swat_control::{Trigger, TriggerAction, TriggerEngine, TriggerPredicate};
use swat_core::{
    ArtifactId, ControlAction, EventEnvelope, EventKind, SessionId, SnapshotId, SnapshotRecord,
    SwatError, SwatResult, TargetAdapter, TriggerId,
};
use swat_expr::parse_expression;
use swat_session::SessionManager;
use swat_store::{StoredArtifact, SwatStore};

#[derive(Clone, Default)]
struct TraceSnapshotStore {
    events: Vec<EventEnvelope>,
    artifacts: BTreeMap<ArtifactId, StoredArtifact>,
    snapshots: BTreeMap<SnapshotId, SnapshotRecord>,
}

impl TraceSnapshotStore {
    fn from_store<S: SwatStore + ?Sized>(store: &S, session_id: SessionId) -> Self {
        let events = store.events_for_session(session_id);
        let mut artifacts = BTreeMap::new();
        for event in &events {
            for artifact_ref in &event.artifact_refs {
                if let Some(artifact) = store.artifact(artifact_ref.artifact_id) {
                    artifacts.insert(artifact_ref.artifact_id, artifact);
                }
            }
        }
        let snapshots = store
            .snapshots_for_session(session_id)
            .into_iter()
            .map(|snapshot| (snapshot.snapshot_id, snapshot))
            .collect();
        Self {
            events,
            artifacts,
            snapshots,
        }
    }
}

impl SwatStore for TraceSnapshotStore {
    fn ingest_emission(
        &mut self,
        _session_id: SessionId,
        _target_id: swat_core::TargetId,
        _next_sequence: &mut u64,
        _emission: swat_core::AdapterEmission,
    ) -> SwatResult<Vec<EventEnvelope>> {
        Err(SwatError::new(
            "trace snapshot store is read-only and cannot ingest emissions",
        ))
    }

    fn events(&self) -> Vec<EventEnvelope> {
        self.events.clone()
    }

    fn events_for_session(&self, session_id: SessionId) -> Vec<EventEnvelope> {
        self.events
            .iter()
            .filter(|event| event.session_id == session_id)
            .cloned()
            .collect()
    }

    fn artifact(&self, artifact_id: ArtifactId) -> Option<StoredArtifact> {
        self.artifacts.get(&artifact_id).cloned()
    }

    fn artifact_count(&self) -> usize {
        self.artifacts.len()
    }

    fn record_snapshot(&mut self, _snapshot: SnapshotRecord) -> SwatResult<()> {
        Err(SwatError::new(
            "trace snapshot store is read-only and cannot persist snapshots",
        ))
    }

    fn snapshots(&self) -> Vec<SnapshotRecord> {
        self.snapshots.values().cloned().collect()
    }

    fn snapshots_for_session(&self, session_id: SessionId) -> Vec<SnapshotRecord> {
        self.snapshots
            .values()
            .filter(|snapshot| snapshot.session_id == session_id)
            .cloned()
            .collect()
    }

    fn snapshot(&self, snapshot_id: SnapshotId) -> Option<SnapshotRecord> {
        self.snapshots.get(&snapshot_id).cloned()
    }
}

#[derive(Clone)]
pub struct ScriptContext {
    session_id: SessionId,
    store: TraceSnapshotStore,
}

impl ScriptContext {
    pub fn from_store<S: SwatStore + ?Sized>(store: &S, session_id: SessionId) -> Self {
        Self {
            session_id,
            store: TraceSnapshotStore::from_store(store, session_id),
        }
    }

    fn inspector(&self) -> TraceInspector<'_, TraceSnapshotStore> {
        TraceInspector::new(&self.store)
    }

    pub fn event_count(&mut self) -> i64 {
        self.inspector().session_events(self.session_id).len() as i64
    }

    pub fn summary_search_count(&mut self, needle: &str) -> i64 {
        self.inspector()
            .search_summaries(self.session_id, needle)
            .len() as i64
    }

    pub fn artifact_search_count(&mut self, needle: &str) -> i64 {
        self.inspector()
            .search_artifact_text(self.session_id, needle)
            .map(|matches| matches.len() as i64)
            .unwrap_or(0)
    }

    pub fn query_count(&mut self, expr: &str) -> i64 {
        self.inspector()
            .query_events_str(self.session_id, expr)
            .map(|events| events.len() as i64)
            .unwrap_or(0)
    }

    pub fn first_summary(&mut self, expr: &str) -> String {
        self.inspector()
            .query_events_str(self.session_id, expr)
            .ok()
            .and_then(|events| events.into_iter().next())
            .and_then(|event| match event.payload {
                swat_core::EventPayload::Text { summary }
                | swat_core::EventPayload::Control { summary, .. }
                | swat_core::EventPayload::Boundary { summary, .. }
                | swat_core::EventPayload::Snapshot { summary, .. }
                | swat_core::EventPayload::Trigger { summary, .. }
                | swat_core::EventPayload::Value { summary, .. }
                | swat_core::EventPayload::Policy { summary, .. } => Some(summary),
                swat_core::EventPayload::Empty => None,
            })
            .unwrap_or_default()
    }

    pub fn source_contains(&mut self, expr: &str, needle: &str, before: i64, after: i64) -> bool {
        self.inspector()
            .query_events_str(self.session_id, expr)
            .ok()
            .and_then(|events| events.into_iter().next())
            .and_then(|event| {
                self.inspector()
                    .resolve_source(&event, before.max(0) as usize, after.max(0) as usize)
                    .ok()
                    .flatten()
            })
            .map(|snippet| snippet.lines.iter().any(|line| line.text.contains(needle)))
            .unwrap_or(false)
    }

    pub fn stack_frame_count(&mut self) -> i64 {
        self.inspector()
            .stack_frames(self.session_id)
            .map(|frames| frames.len() as i64)
            .unwrap_or(0)
    }

    pub fn stack_frame_label(&mut self, frame_index: i64) -> String {
        if frame_index < 0 {
            return String::new();
        }
        self.inspector()
            .stack_frame(self.session_id, frame_index as usize)
            .ok()
            .flatten()
            .map(|frame| frame.label)
            .unwrap_or_default()
    }

    pub fn stack_frame_local_count(&mut self, frame_index: i64) -> i64 {
        if frame_index < 0 {
            return 0;
        }
        self.inspector()
            .stack_frame_locals(self.session_id, frame_index as usize)
            .map(|locals| locals.len() as i64)
            .unwrap_or(0)
    }

    pub fn stack_frame_local_name(&mut self, frame_index: i64, local_index: i64) -> String {
        if frame_index < 0 || local_index < 0 {
            return String::new();
        }
        self.inspector()
            .stack_frame_locals(self.session_id, frame_index as usize)
            .ok()
            .and_then(|locals| {
                locals
                    .get(local_index as usize)
                    .map(|local| local.name.clone())
            })
            .unwrap_or_default()
    }

    pub fn stack_frame_register_count(&mut self, frame_index: i64) -> i64 {
        if frame_index < 0 {
            return 0;
        }
        self.inspector()
            .stack_frame_registers(self.session_id, frame_index as usize)
            .map(|registers| registers.len() as i64)
            .unwrap_or(0)
    }

    pub fn stack_frame_register_name(&mut self, frame_index: i64, register_index: i64) -> String {
        if frame_index < 0 || register_index < 0 {
            return String::new();
        }
        self.inspector()
            .stack_frame_registers(self.session_id, frame_index as usize)
            .ok()
            .and_then(|registers| {
                registers
                    .get(register_index as usize)
                    .map(|register| register.name.clone())
            })
            .unwrap_or_default()
    }

    pub fn patient_count(&mut self) -> i64 {
        self.inspector()
            .patients(self.session_id)
            .map(|patients| patients.len() as i64)
            .unwrap_or(0)
    }

    pub fn patient_name(&mut self, patient_index: i64) -> String {
        if patient_index < 0 {
            return String::new();
        }
        self.inspector()
            .patients(self.session_id)
            .ok()
            .and_then(|patients| {
                patients
                    .get(patient_index as usize)
                    .map(|patient| patient.name.clone())
            })
            .unwrap_or_default()
    }

    pub fn patient_handle_count(&mut self, patient: &str) -> i64 {
        self.inspector()
            .patient_detail(self.session_id, patient)
            .ok()
            .flatten()
            .map(|detail| detail.handles.len() as i64)
            .unwrap_or(0)
    }

    pub fn handle_count(&mut self) -> i64 {
        self.inspector()
            .handles(self.session_id)
            .map(|handles| handles.len() as i64)
            .unwrap_or(0)
    }

    pub fn handle_name(&mut self, handle_index: i64) -> String {
        if handle_index < 0 {
            return String::new();
        }
        self.inspector()
            .handles(self.session_id)
            .ok()
            .and_then(|handles| {
                handles
                    .get(handle_index as usize)
                    .map(|handle| handle.key.clone())
            })
            .unwrap_or_default()
    }

    pub fn handle_object_count(&mut self, handle: &str) -> i64 {
        self.inspector()
            .handle_detail(self.session_id, handle)
            .ok()
            .flatten()
            .map(|detail| detail.objects.len() as i64)
            .unwrap_or(0)
    }

    pub fn resource_count(&mut self) -> i64 {
        self.inspector()
            .resources(self.session_id)
            .map(|resources| resources.len() as i64)
            .unwrap_or(0)
    }

    pub fn resource_name(&mut self, resource_index: i64) -> String {
        if resource_index < 0 {
            return String::new();
        }
        self.inspector()
            .resources(self.session_id)
            .ok()
            .and_then(|resources| {
                resources
                    .get(resource_index as usize)
                    .map(|resource| resource.name.clone())
            })
            .unwrap_or_default()
    }

    pub fn resource_object_count(&mut self, resource: &str) -> i64 {
        self.inspector()
            .resource_detail(self.session_id, resource)
            .ok()
            .flatten()
            .map(|detail| detail.objects.len() as i64)
            .unwrap_or(0)
    }

    pub fn object_count(&mut self) -> i64 {
        self.inspector()
            .objects(self.session_id)
            .map(|objects| objects.len() as i64)
            .unwrap_or(0)
    }

    pub fn object_identity(&mut self, object_index: i64) -> String {
        if object_index < 0 {
            return String::new();
        }
        self.inspector()
            .objects(self.session_id)
            .ok()
            .and_then(|objects| {
                objects
                    .get(object_index as usize)
                    .map(|object| object.key.clone())
            })
            .unwrap_or_default()
    }

    pub fn object_class(&mut self, object: &str) -> String {
        self.inspector()
            .object_detail(self.session_id, object)
            .ok()
            .flatten()
            .and_then(|detail| detail.object.class_name)
            .unwrap_or_default()
    }

    pub fn value_count(&mut self) -> i64 {
        self.inspector()
            .observed_values(self.session_id)
            .map(|values| values.len() as i64)
            .unwrap_or(0)
    }

    pub fn value_key(&mut self, value_index: i64) -> String {
        if value_index < 0 {
            return String::new();
        }
        self.inspector()
            .observed_values(self.session_id)
            .ok()
            .and_then(|values| {
                values
                    .get(value_index as usize)
                    .map(|value| value.value_key.clone())
            })
            .unwrap_or_default()
    }

    pub fn value_event_count(&mut self, value_key: &str) -> i64 {
        self.inspector()
            .observed_value_detail(self.session_id, value_key)
            .ok()
            .flatten()
            .map(|detail| detail.value.event_count as i64)
            .unwrap_or(0)
    }

    pub fn source_function_count(&mut self) -> i64 {
        self.inspector()
            .source_functions(self.session_id)
            .map(|functions| functions.len() as i64)
            .unwrap_or(0)
    }

    pub fn source_function_name(&mut self, function_index: i64) -> String {
        if function_index < 0 {
            return String::new();
        }
        self.inspector()
            .source_functions(self.session_id)
            .ok()
            .and_then(|functions| {
                functions
                    .get(function_index as usize)
                    .map(|function| function.function.clone())
            })
            .unwrap_or_default()
    }

    pub fn source_function_event_count(&mut self, function: &str) -> i64 {
        self.inspector()
            .events_for_source_function(self.session_id, function)
            .map(|events| events.len() as i64)
            .unwrap_or(0)
    }

    pub fn source_file_count(&mut self) -> i64 {
        self.inspector()
            .source_files(self.session_id)
            .map(|files| files.len() as i64)
            .unwrap_or(0)
    }

    pub fn source_file_event_count(&mut self, file: &str) -> i64 {
        self.inspector()
            .events_for_source_file(self.session_id, file)
            .map(|events| events.len() as i64)
            .unwrap_or(0)
    }

    pub fn source_view_contains(
        &mut self,
        file: &str,
        line: i64,
        before: i64,
        after: i64,
        needle: &str,
    ) -> bool {
        if line <= 0 {
            return false;
        }
        self.inspector()
            .source_file_view(
                file,
                line as usize,
                before.max(0) as usize,
                after.max(0) as usize,
            )
            .map(|snippet| snippet.lines.iter().any(|line| line.text.contains(needle)))
            .unwrap_or(false)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScriptPackageExportMetadata {
    pub name: &'static str,
    pub summary: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScriptPackageMetadata {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub summary: &'static str,
    pub notes: &'static [&'static str],
    pub legacy_references: &'static [&'static str],
    pub exports: &'static [ScriptPackageExportMetadata],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScriptPackageSpec {
    metadata: ScriptPackageMetadata,
    source: &'static str,
}

const PROCESS_EXPORTS: &[ScriptPackageExportMetadata] = &[
    ScriptPackageExportMetadata {
        name: "process_event_total",
        summary: "return the current session event count",
    },
    ScriptPackageExportMetadata {
        name: "process_has_summary",
        summary: "check whether any event summary contains a substring",
    },
    ScriptPackageExportMetadata {
        name: "process_query_total",
        summary: "run a shared query expression and return its match count",
    },
];

const STACK_EXPORTS: &[ScriptPackageExportMetadata] = &[
    ScriptPackageExportMetadata {
        name: "stack_frame_total",
        summary: "return the number of projected stack frames",
    },
    ScriptPackageExportMetadata {
        name: "stack_top_label",
        summary: "return the label for frame 0",
    },
    ScriptPackageExportMetadata {
        name: "stack_has_local",
        summary: "check whether one frame exposes a typed local binding",
    },
    ScriptPackageExportMetadata {
        name: "stack_has_register",
        summary: "check whether one frame exposes a typed register binding",
    },
];

const PATIENT_EXPORTS: &[ScriptPackageExportMetadata] = &[
    ScriptPackageExportMetadata {
        name: "patient_total",
        summary: "return the typed patient count",
    },
    ScriptPackageExportMetadata {
        name: "patient_has",
        summary: "check whether a typed patient exists by name",
    },
    ScriptPackageExportMetadata {
        name: "patient_handle_total",
        summary: "return the number of handles related to one patient",
    },
];

const OBJECT_EXPORTS: &[ScriptPackageExportMetadata] = &[
    ScriptPackageExportMetadata {
        name: "handle_total",
        summary: "return the typed handle count",
    },
    ScriptPackageExportMetadata {
        name: "handle_has",
        summary: "check whether a typed handle exists by id",
    },
    ScriptPackageExportMetadata {
        name: "handle_object_total",
        summary: "return the number of objects related to one handle",
    },
    ScriptPackageExportMetadata {
        name: "resource_total",
        summary: "return the typed resource count",
    },
    ScriptPackageExportMetadata {
        name: "resource_has",
        summary: "check whether a typed resource exists by name",
    },
    ScriptPackageExportMetadata {
        name: "resource_object_total",
        summary: "return the number of objects related to one resource",
    },
    ScriptPackageExportMetadata {
        name: "object_total",
        summary: "return the typed object count",
    },
    ScriptPackageExportMetadata {
        name: "object_has",
        summary: "check whether a typed object identity exists",
    },
    ScriptPackageExportMetadata {
        name: "object_class_is",
        summary: "compare an object's projected class name",
    },
];

const SOURCE_EXPORTS: &[ScriptPackageExportMetadata] = &[
    ScriptPackageExportMetadata {
        name: "source_function_total",
        summary: "return the discovered source-function count",
    },
    ScriptPackageExportMetadata {
        name: "source_has_function",
        summary: "check whether a discovered source function exists",
    },
    ScriptPackageExportMetadata {
        name: "source_function_event_total",
        summary: "return the number of events for one source function",
    },
    ScriptPackageExportMetadata {
        name: "source_file_total",
        summary: "return the discovered source-file count",
    },
    ScriptPackageExportMetadata {
        name: "source_has_file",
        summary: "check whether a discovered source file exists",
    },
    ScriptPackageExportMetadata {
        name: "source_file_event_total",
        summary: "return the number of events for one source file",
    },
    ScriptPackageExportMetadata {
        name: "source_view_has",
        summary: "check whether a source snippet contains a substring",
    },
];

const SCRIPT_PACKAGES: &[ScriptPackageSpec] = &[
    ScriptPackageSpec {
        metadata: ScriptPackageMetadata {
            name: "process",
            aliases: &[],
            summary: "session and query helpers derived from legacy process/toplevel workflows",
            notes: &[
                "The process package stays read-only in the frozen script host and routes through the shared query and summary APIs.",
            ],
            legacy_references: &[
                "/home/ubuntu/pcgeos/Tools/swat/lib.new/process.tcl",
                "/home/ubuntu/pcgeos/Tools/swat/lib.new/toplevel.tcl",
            ],
            exports: PROCESS_EXPORTS,
        },
        source: include_str!("packages/process.rhai"),
    },
    ScriptPackageSpec {
        metadata: ScriptPackageMetadata {
            name: "stack",
            aliases: &[],
            summary: "frame-oriented helpers derived from the legacy stack Tcl family",
            notes: &[
                "Stack helpers sit on top of shared frame/local/register inspection instead of reconstructing stack state in script-space.",
            ],
            legacy_references: &["/home/ubuntu/pcgeos/Tools/swat/lib.new/stack.tcl"],
            exports: STACK_EXPORTS,
        },
        source: include_str!("packages/stack.rhai"),
    },
    ScriptPackageSpec {
        metadata: ScriptPackageMetadata {
            name: "patient",
            aliases: &[],
            summary: "typed patient helpers derived from the legacy patient Tcl family",
            notes: &[
                "Patient helpers use the typed patient/handle/resource/object projections added to the shared inspection API.",
            ],
            legacy_references: &["/home/ubuntu/pcgeos/Tools/swat/lib.new/patient.tcl"],
            exports: PATIENT_EXPORTS,
        },
        source: include_str!("packages/patient.rhai"),
    },
    ScriptPackageSpec {
        metadata: ScriptPackageMetadata {
            name: "object",
            aliases: &["objwatch"],
            summary: "typed handle/resource/object helpers derived from the legacy object Tcl families",
            notes: &[
                "The object package preserves debugger-native object graph workflows without leaking adapter-specific storage rules into `swat-core`.",
            ],
            legacy_references: &[
                "/home/ubuntu/pcgeos/Tools/swat/lib.new/object.tcl",
                "/home/ubuntu/pcgeos/Tools/swat/lib.new/objwatch.tcl",
            ],
            exports: OBJECT_EXPORTS,
        },
        source: include_str!("packages/object.rhai"),
    },
    ScriptPackageSpec {
        metadata: ScriptPackageMetadata {
            name: "source",
            aliases: &["srclist", "slist"],
            summary: "source navigation helpers derived from the legacy srclist Tcl family",
            notes: &[
                "Source helpers stay on the shared source and resolver layers, so shell, TUI, and scripts see the same file and function projections.",
            ],
            legacy_references: &["/home/ubuntu/pcgeos/Tools/swat/lib.new/srclist.tcl"],
            exports: SOURCE_EXPORTS,
        },
        source: include_str!("packages/source.rhai"),
    },
];

pub fn builtin_script_packages() -> Vec<ScriptPackageMetadata> {
    SCRIPT_PACKAGES
        .iter()
        .map(|package| package.metadata)
        .collect()
}

pub fn builtin_script_package(name: &str) -> Option<ScriptPackageMetadata> {
    find_script_package(name).map(|package| package.metadata)
}

fn find_script_package(name: &str) -> Option<&'static ScriptPackageSpec> {
    SCRIPT_PACKAGES.iter().find(|package| {
        package.metadata.name.eq_ignore_ascii_case(name)
            || package
                .metadata
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(name))
    })
}

fn script_requires_export(script: &str, export: &str) -> bool {
    script.match_indices(export).any(|(index, _)| {
        let prefix_ok = script[..index]
            .chars()
            .next_back()
            .map(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .unwrap_or(true);
        if !prefix_ok {
            return false;
        }
        let suffix = &script[index + export.len()..];
        let suffix = suffix.trim_start();
        suffix.starts_with('(')
    })
}

fn rewrite_export_calls(script: &str, export: &str) -> String {
    let mut rewritten = String::with_capacity(script.len() + 32);
    let mut cursor = 0;

    while let Some(relative_index) = script[cursor..].find(export) {
        let index = cursor + relative_index;
        let prefix = &script[..index];
        let prefix_ok = prefix
            .chars()
            .next_back()
            .map(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .unwrap_or(true);
        let previous_token = prefix
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .next_back()
            .unwrap_or("");
        let mut open_paren = index + export.len();
        while let Some(ch) = script[open_paren..].chars().next() {
            if !ch.is_whitespace() {
                break;
            }
            open_paren += ch.len_utf8();
        }
        let call_ok = script[open_paren..].starts_with('(');

        if !prefix_ok || previous_token == "fn" || !call_ok {
            rewritten.push_str(&script[cursor..index + export.len()]);
            cursor = index + export.len();
            continue;
        }

        rewritten.push_str(&script[cursor..open_paren + 1]);
        let has_args = !script[open_paren + 1..].trim_start().starts_with(')');
        if has_args {
            rewritten.push_str("ctx, ");
        } else {
            rewritten.push_str("ctx");
        }
        cursor = open_paren + 1;
    }

    rewritten.push_str(&script[cursor..]);
    rewritten
}

pub struct ScriptHost {
    engine: Engine,
    scope: Scope<'static>,
    loaded_packages: BTreeSet<&'static str>,
}

impl ScriptHost {
    pub fn new<S: SwatStore + ?Sized>(store: &S, session_id: SessionId) -> Self {
        let mut engine = Engine::new();
        engine.register_type::<ScriptContext>();
        engine.register_fn("event_count", ScriptContext::event_count);
        engine.register_fn("summary_search_count", ScriptContext::summary_search_count);
        engine.register_fn(
            "artifact_search_count",
            ScriptContext::artifact_search_count,
        );
        engine.register_fn("query_count", ScriptContext::query_count);
        engine.register_fn("first_summary", ScriptContext::first_summary);
        engine.register_fn("source_contains", ScriptContext::source_contains);
        engine.register_fn("stack_frame_count", ScriptContext::stack_frame_count);
        engine.register_fn("stack_frame_label", ScriptContext::stack_frame_label);
        engine.register_fn(
            "stack_frame_local_count",
            ScriptContext::stack_frame_local_count,
        );
        engine.register_fn(
            "stack_frame_local_name",
            ScriptContext::stack_frame_local_name,
        );
        engine.register_fn(
            "stack_frame_register_count",
            ScriptContext::stack_frame_register_count,
        );
        engine.register_fn(
            "stack_frame_register_name",
            ScriptContext::stack_frame_register_name,
        );
        engine.register_fn("patient_count", ScriptContext::patient_count);
        engine.register_fn("patient_name", ScriptContext::patient_name);
        engine.register_fn("patient_handle_count", ScriptContext::patient_handle_count);
        engine.register_fn("handle_count", ScriptContext::handle_count);
        engine.register_fn("handle_name", ScriptContext::handle_name);
        engine.register_fn("handle_object_count", ScriptContext::handle_object_count);
        engine.register_fn("resource_count", ScriptContext::resource_count);
        engine.register_fn("resource_name", ScriptContext::resource_name);
        engine.register_fn(
            "resource_object_count",
            ScriptContext::resource_object_count,
        );
        engine.register_fn("object_count", ScriptContext::object_count);
        engine.register_fn("object_identity", ScriptContext::object_identity);
        engine.register_fn("object_class", ScriptContext::object_class);
        engine.register_fn("value_count", ScriptContext::value_count);
        engine.register_fn("value_key", ScriptContext::value_key);
        engine.register_fn("value_event_count", ScriptContext::value_event_count);
        engine.register_fn(
            "source_function_count",
            ScriptContext::source_function_count,
        );
        engine.register_fn("source_function_name", ScriptContext::source_function_name);
        engine.register_fn(
            "source_function_event_count",
            ScriptContext::source_function_event_count,
        );
        engine.register_fn("source_file_count", ScriptContext::source_file_count);
        engine.register_fn(
            "source_file_event_count",
            ScriptContext::source_file_event_count,
        );
        engine.register_fn("source_view_contains", ScriptContext::source_view_contains);

        let mut scope = Scope::new();
        scope.push("ctx", ScriptContext::from_store(store, session_id));

        Self {
            engine,
            scope,
            loaded_packages: BTreeSet::new(),
        }
    }

    pub fn available_packages(&self) -> Vec<ScriptPackageMetadata> {
        builtin_script_packages()
    }

    pub fn loaded_packages(&self) -> Vec<ScriptPackageMetadata> {
        self.loaded_packages
            .iter()
            .filter_map(|name| builtin_script_package(name))
            .collect()
    }

    pub fn load_package(&mut self, name: &str) -> SwatResult<()> {
        let package = find_script_package(name).ok_or_else(|| {
            let available = builtin_script_packages()
                .into_iter()
                .map(|package| package.name)
                .collect::<Vec<_>>()
                .join(", ");
            SwatError::new(format!(
                "unknown script package {name}; available: {available}"
            ))
        })?;
        self.loaded_packages.insert(package.metadata.name);
        Ok(())
    }

    fn prepare_script(&mut self, script: &str) -> String {
        for package in SCRIPT_PACKAGES {
            if package
                .metadata
                .exports
                .iter()
                .any(|export| script_requires_export(script, export.name))
            {
                self.loaded_packages.insert(package.metadata.name);
            }
        }

        let mut script = script.to_string();
        for package in SCRIPT_PACKAGES {
            if !self.loaded_packages.contains(package.metadata.name) {
                continue;
            }
            for export in package.metadata.exports {
                script = rewrite_export_calls(&script, export.name);
            }
        }

        let mut assembled = String::new();
        for package in SCRIPT_PACKAGES {
            if self.loaded_packages.contains(package.metadata.name) {
                assembled.push_str(package.source);
                assembled.push_str("\n\n");
            }
        }
        assembled.push_str(&script);
        assembled
    }

    pub fn eval_dynamic(&mut self, script: &str) -> SwatResult<Dynamic> {
        let script = self.prepare_script(script);
        self.engine
            .eval_with_scope::<Dynamic>(&mut self.scope, &script)
            .map_err(script_error)
    }

    pub fn eval_i64(&mut self, script: &str) -> SwatResult<i64> {
        let script = self.prepare_script(script);
        self.engine
            .eval_with_scope::<i64>(&mut self.scope, &script)
            .map_err(script_error)
    }

    pub fn eval_bool(&mut self, script: &str) -> SwatResult<bool> {
        let script = self.prepare_script(script);
        self.engine
            .eval_with_scope::<bool>(&mut self.scope, &script)
            .map_err(script_error)
    }

    pub fn eval_string(&mut self, script: &str) -> SwatResult<String> {
        let script = self.prepare_script(script);
        self.engine
            .eval_with_scope::<String>(&mut self.scope, &script)
            .map_err(script_error)
    }
}

fn script_error(err: Box<EvalAltResult>) -> SwatError {
    SwatError::new(format!("script evaluation failed: {err}"))
}

pub struct LiveScriptSession<'a, A: TargetAdapter + ?Sized, S: SwatStore + ?Sized> {
    session_id: SessionId,
    manager: &'a mut SessionManager,
    adapter: &'a mut A,
    store: &'a mut S,
    trigger_engine: &'a mut TriggerEngine,
}

impl<'a, A: TargetAdapter + ?Sized, S: SwatStore + ?Sized> LiveScriptSession<'a, A, S> {
    pub fn new(
        session_id: SessionId,
        manager: &'a mut SessionManager,
        adapter: &'a mut A,
        store: &'a mut S,
        trigger_engine: &'a mut TriggerEngine,
    ) -> Self {
        Self {
            session_id,
            manager,
            adapter,
            store,
            trigger_engine,
        }
    }

    pub fn event_count(&mut self) -> i64 {
        let session_id = self.session_id;
        self.api().inspector().session_events(session_id).len() as i64
    }

    pub fn query_count(&mut self, expr: &str) -> SwatResult<i64> {
        let session_id = self.session_id;
        Ok(self
            .api()
            .inspector()
            .query_events_str(session_id, expr)?
            .len() as i64)
    }

    pub fn trigger_count(&mut self) -> i64 {
        self.api().triggers().len() as i64
    }

    pub fn breakpoint_count(&mut self) -> i64 {
        self.api().breakpoint_summaries().len() as i64
    }

    pub fn breakpoint_enabled_count(&mut self) -> i64 {
        self.api()
            .breakpoint_summaries()
            .into_iter()
            .filter(|breakpoint| breakpoint.state == BreakpointState::Enabled)
            .count() as i64
    }

    pub fn breakpoint_group_count(&mut self, kind: &str, label: &str) -> SwatResult<i64> {
        let kind = parse_breakpoint_group_kind(kind)?;
        Ok(self
            .api()
            .breakpoint_groups()
            .into_iter()
            .find(|group| group.kind == kind && group.label == label)
            .map(|group| group.breakpoints.len() as i64)
            .unwrap_or(0))
    }

    pub fn breakpoint_hit_count(&mut self, trigger_id: TriggerId) -> SwatResult<i64> {
        self.api()
            .breakpoint_detail(trigger_id)
            .map(|detail| detail.breakpoint.hit_count as i64)
            .ok_or_else(|| SwatError::new(format!("unknown breakpoint {}", trigger_id.raw())))
    }

    pub fn breakpoint_definition_group_count(&mut self) -> i64 {
        self.api().breakpoint_definition_groups().len() as i64
    }

    pub fn breakpoint_definition_group_size(&mut self, name: &str) -> i64 {
        self.api()
            .breakpoint_definition_groups()
            .into_iter()
            .find(|group| group.name == name)
            .map(|group| group.breakpoints.len() as i64)
            .unwrap_or(0)
    }

    pub fn breakpoint_definition_group_enabled(&mut self, name: &str) -> SwatResult<bool> {
        self.api()
            .breakpoint_definition_groups()
            .into_iter()
            .find(|group| group.name == name)
            .map(|group| group.enabled)
            .ok_or_else(|| SwatError::new(format!("unknown breakpoint group {name}")))
    }

    pub fn breakpoint_predicate_count(&mut self) -> i64 {
        self.api().breakpoint_predicates().len() as i64
    }

    pub fn breakpoint_predicate_breakpoint_count(&mut self, name: &str) -> SwatResult<i64> {
        self.api()
            .breakpoint_predicates()
            .into_iter()
            .find(|predicate| predicate.name == name)
            .map(|predicate| predicate.breakpoint_count as i64)
            .ok_or_else(|| SwatError::new(format!("unknown breakpoint predicate {name}")))
    }

    pub fn define_breakpoint_predicate(&mut self, name: &str, expr: &str) -> SwatResult<bool> {
        let session_id = self.session_id;
        let predicate = TriggerPredicate::Expr(parse_expression(expr)?);
        self.api()
            .define_breakpoint_predicate(session_id, name, predicate)
            .map(|report| report.value)
    }

    pub fn remove_breakpoint_predicate(&mut self, name: &str) -> SwatResult<String> {
        let session_id = self.session_id;
        self.api()
            .remove_breakpoint_predicate(session_id, name)
            .map(|report| report.value.name)
    }

    pub fn set_breakpoint_group_enabled(&mut self, group: &str, enabled: bool) -> SwatResult<bool> {
        let session_id = self.session_id;
        self.api()
            .set_breakpoint_group_enabled(session_id, group, enabled)
            .map(|report| report.value)
    }

    pub fn watchpoint_count(&mut self) -> i64 {
        self.api().watchpoint_summaries().len() as i64
    }

    pub fn watchpoint_hit_count(&mut self, trigger_id: TriggerId) -> SwatResult<i64> {
        self.api()
            .watchpoint_detail(trigger_id)
            .map(|detail| detail.watchpoint.breakpoint.hit_count as i64)
            .ok_or_else(|| SwatError::new(format!("unknown watchpoint {}", trigger_id.raw())))
    }

    pub fn add_watchpoint(
        &mut self,
        name: &str,
        value_key: &str,
        path: &str,
        fire_once: bool,
    ) -> SwatResult<TriggerId> {
        self.add_watchpoint_with_scope(name, value_key, path, -1, "", "", fire_once, "")
    }

    pub fn add_watchpoint_with_scope(
        &mut self,
        name: &str,
        value_key: &str,
        path: &str,
        after_millis: i64,
        event_kind: &str,
        summary_contains: &str,
        fire_once: bool,
        group: &str,
    ) -> SwatResult<TriggerId> {
        let session_id = self.session_id;
        let mut spec = WatchpointSpec::new(name, value_key);
        if !path.is_empty() {
            spec = spec.at_path(path);
        }
        if after_millis >= 0 {
            spec = spec.after_millis(after_millis as u64);
        }
        if !event_kind.is_empty() {
            spec = spec.in_event_kind(parse_event_kind(event_kind)?);
        }
        if !summary_contains.is_empty() {
            spec = spec.with_summary_contains(summary_contains);
        }
        if fire_once {
            spec = spec.fire_once();
        }
        if !group.is_empty() {
            spec = spec.in_group(group);
        }
        self.api()
            .add_watchpoint(session_id, spec)
            .map(|report| report.value)
    }

    pub fn pump_once(&mut self) -> SwatResult<usize> {
        Ok(self
            .manager
            .pump(self.session_id, self.adapter, self.store)?
            .stored_events
            .len())
    }

    pub fn stack_frame_count(&mut self) -> SwatResult<i64> {
        let session_id = self.session_id;
        Ok(self.api().inspector().stack_frames(session_id)?.len() as i64)
    }

    pub fn stack_frame_label(&mut self, frame_index: i64) -> SwatResult<String> {
        if frame_index < 0 {
            return Ok(String::new());
        }
        let session_id = self.session_id;
        Ok(self
            .api()
            .inspector()
            .stack_frame(session_id, frame_index as usize)?
            .map(|frame| frame.label)
            .unwrap_or_default())
    }

    pub fn stack_frame_local_count(&mut self, frame_index: i64) -> SwatResult<i64> {
        if frame_index < 0 {
            return Ok(0);
        }
        let session_id = self.session_id;
        Ok(self
            .api()
            .inspector()
            .stack_frame_locals(session_id, frame_index as usize)?
            .len() as i64)
    }

    pub fn stack_frame_local_name(
        &mut self,
        frame_index: i64,
        local_index: i64,
    ) -> SwatResult<String> {
        if frame_index < 0 || local_index < 0 {
            return Ok(String::new());
        }
        let session_id = self.session_id;
        Ok(self
            .api()
            .inspector()
            .stack_frame_locals(session_id, frame_index as usize)?
            .get(local_index as usize)
            .map(|local| local.name.clone())
            .unwrap_or_default())
    }

    pub fn stack_frame_register_count(&mut self, frame_index: i64) -> SwatResult<i64> {
        if frame_index < 0 {
            return Ok(0);
        }
        let session_id = self.session_id;
        Ok(self
            .api()
            .inspector()
            .stack_frame_registers(session_id, frame_index as usize)?
            .len() as i64)
    }

    pub fn stack_frame_register_name(
        &mut self,
        frame_index: i64,
        register_index: i64,
    ) -> SwatResult<String> {
        if frame_index < 0 || register_index < 0 {
            return Ok(String::new());
        }
        let session_id = self.session_id;
        Ok(self
            .api()
            .inspector()
            .stack_frame_registers(session_id, frame_index as usize)?
            .get(register_index as usize)
            .map(|register| register.name.clone())
            .unwrap_or_default())
    }

    pub fn patient_count(&mut self) -> SwatResult<i64> {
        let session_id = self.session_id;
        Ok(self.api().inspector().patients(session_id)?.len() as i64)
    }

    pub fn patient_name(&mut self, patient_index: i64) -> SwatResult<String> {
        if patient_index < 0 {
            return Ok(String::new());
        }
        let session_id = self.session_id;
        Ok(self
            .api()
            .inspector()
            .patients(session_id)?
            .get(patient_index as usize)
            .map(|patient| patient.name.clone())
            .unwrap_or_default())
    }

    pub fn patient_handle_count(&mut self, patient: &str) -> SwatResult<i64> {
        let session_id = self.session_id;
        Ok(self
            .api()
            .inspector()
            .patient_detail(session_id, patient)?
            .map(|detail| detail.handles.len() as i64)
            .unwrap_or(0))
    }

    pub fn handle_count(&mut self) -> SwatResult<i64> {
        let session_id = self.session_id;
        Ok(self.api().inspector().handles(session_id)?.len() as i64)
    }

    pub fn handle_name(&mut self, handle_index: i64) -> SwatResult<String> {
        if handle_index < 0 {
            return Ok(String::new());
        }
        let session_id = self.session_id;
        Ok(self
            .api()
            .inspector()
            .handles(session_id)?
            .get(handle_index as usize)
            .map(|handle| handle.key.clone())
            .unwrap_or_default())
    }

    pub fn handle_object_count(&mut self, handle: &str) -> SwatResult<i64> {
        let session_id = self.session_id;
        Ok(self
            .api()
            .inspector()
            .handle_detail(session_id, handle)?
            .map(|detail| detail.objects.len() as i64)
            .unwrap_or(0))
    }

    pub fn resource_count(&mut self) -> SwatResult<i64> {
        let session_id = self.session_id;
        Ok(self.api().inspector().resources(session_id)?.len() as i64)
    }

    pub fn resource_name(&mut self, resource_index: i64) -> SwatResult<String> {
        if resource_index < 0 {
            return Ok(String::new());
        }
        let session_id = self.session_id;
        Ok(self
            .api()
            .inspector()
            .resources(session_id)?
            .get(resource_index as usize)
            .map(|resource| resource.name.clone())
            .unwrap_or_default())
    }

    pub fn resource_object_count(&mut self, resource: &str) -> SwatResult<i64> {
        let session_id = self.session_id;
        Ok(self
            .api()
            .inspector()
            .resource_detail(session_id, resource)?
            .map(|detail| detail.objects.len() as i64)
            .unwrap_or(0))
    }

    pub fn object_count(&mut self) -> SwatResult<i64> {
        let session_id = self.session_id;
        Ok(self.api().inspector().objects(session_id)?.len() as i64)
    }

    pub fn object_identity(&mut self, object_index: i64) -> SwatResult<String> {
        if object_index < 0 {
            return Ok(String::new());
        }
        let session_id = self.session_id;
        Ok(self
            .api()
            .inspector()
            .objects(session_id)?
            .get(object_index as usize)
            .map(|object| object.key.clone())
            .unwrap_or_default())
    }

    pub fn object_class(&mut self, object: &str) -> SwatResult<String> {
        let session_id = self.session_id;
        Ok(self
            .api()
            .inspector()
            .object_detail(session_id, object)?
            .and_then(|detail| detail.object.class_name)
            .unwrap_or_default())
    }

    pub fn value_count(&mut self) -> SwatResult<i64> {
        let session_id = self.session_id;
        Ok(self.api().inspector().observed_values(session_id)?.len() as i64)
    }

    pub fn value_key(&mut self, value_index: i64) -> SwatResult<String> {
        if value_index < 0 {
            return Ok(String::new());
        }
        let session_id = self.session_id;
        Ok(self
            .api()
            .inspector()
            .observed_values(session_id)?
            .get(value_index as usize)
            .map(|value| value.value_key.clone())
            .unwrap_or_default())
    }

    pub fn value_event_count(&mut self, value_key: &str) -> SwatResult<i64> {
        let session_id = self.session_id;
        Ok(self
            .api()
            .inspector()
            .observed_value_detail(session_id, value_key)?
            .map(|detail| detail.value.event_count as i64)
            .unwrap_or(0))
    }

    pub fn source_function_count(&mut self) -> SwatResult<i64> {
        let session_id = self.session_id;
        Ok(self.api().inspector().source_functions(session_id)?.len() as i64)
    }

    pub fn source_function_name(&mut self, function_index: i64) -> SwatResult<String> {
        if function_index < 0 {
            return Ok(String::new());
        }
        let session_id = self.session_id;
        Ok(self
            .api()
            .inspector()
            .source_functions(session_id)?
            .get(function_index as usize)
            .map(|function| function.function.clone())
            .unwrap_or_default())
    }

    pub fn source_function_event_count(&mut self, function: &str) -> SwatResult<i64> {
        let session_id = self.session_id;
        Ok(self
            .api()
            .inspector()
            .events_for_source_function(session_id, function)?
            .len() as i64)
    }

    pub fn source_file_count(&mut self) -> SwatResult<i64> {
        let session_id = self.session_id;
        Ok(self.api().inspector().source_files(session_id)?.len() as i64)
    }

    pub fn source_file_event_count(&mut self, file: &str) -> SwatResult<i64> {
        let session_id = self.session_id;
        Ok(self
            .api()
            .inspector()
            .events_for_source_file(session_id, file)?
            .len() as i64)
    }

    pub fn source_view_contains(
        &mut self,
        file: &str,
        line: i64,
        before: i64,
        after: i64,
        needle: &str,
    ) -> SwatResult<bool> {
        if line <= 0 {
            return Ok(false);
        }
        let snippet = self.api().inspector().source_file_view(
            file,
            line as usize,
            before.max(0) as usize,
            after.max(0) as usize,
        )?;
        Ok(snippet.lines.iter().any(|line| line.text.contains(needle)))
    }

    pub fn resume(&mut self) -> SwatResult<String> {
        let session_id = self.session_id;
        Ok(self
            .api()
            .control(session_id, ControlAction::Resume)?
            .value
            .response
            .summary)
    }

    pub fn pause(&mut self) -> SwatResult<String> {
        let session_id = self.session_id;
        Ok(self
            .api()
            .control(session_id, ControlAction::Pause)?
            .value
            .response
            .summary)
    }

    pub fn snapshot(&mut self, reason: &str) -> SwatResult<SnapshotId> {
        let session_id = self.session_id;
        self.api()
            .control(
                session_id,
                ControlAction::CreateSnapshot {
                    reason: reason.to_string(),
                },
            )?
            .value
            .snapshot
            .map(|snapshot| snapshot.snapshot_id)
            .ok_or_else(|| SwatError::new("snapshot request did not create a snapshot"))
    }

    pub fn add_trigger_expr(
        &mut self,
        name: &str,
        expr: &str,
        fire_once: bool,
    ) -> SwatResult<TriggerId> {
        let session_id = self.session_id;
        let parsed = parse_expression(expr)?;
        let mut trigger = Trigger::new(
            name,
            TriggerPredicate::Expr(parsed),
            vec![TriggerAction::PauseTarget],
        );
        if fire_once {
            trigger = trigger.fire_once();
        }
        Ok(self.api().add_trigger(session_id, trigger)?.value)
    }

    pub fn enable_trigger(&mut self, trigger_id: TriggerId) -> SwatResult<bool> {
        let session_id = self.session_id;
        self.api()
            .set_trigger_enabled(session_id, trigger_id, true)
            .map(|report| report.value)
    }

    pub fn disable_trigger(&mut self, trigger_id: TriggerId) -> SwatResult<bool> {
        let session_id = self.session_id;
        self.api()
            .set_trigger_enabled(session_id, trigger_id, false)
            .map(|report| report.value)
    }

    pub fn remove_trigger(&mut self, trigger_id: TriggerId) -> SwatResult<String> {
        let session_id = self.session_id;
        self.api()
            .remove_trigger(session_id, trigger_id)
            .map(|report| report.value.name)
    }

    fn api(&mut self) -> LiveSessionApi<'_, A, S> {
        LiveSessionApi::new(self.manager, self.adapter, self.store, self.trigger_engine)
    }
}

fn parse_breakpoint_group_kind(kind: &str) -> SwatResult<BreakpointGroupKind> {
    match kind {
        "state" => Ok(BreakpointGroupKind::State),
        "lifetime" => Ok(BreakpointGroupKind::Lifetime),
        "disposition" => Ok(BreakpointGroupKind::Disposition),
        "activity" => Ok(BreakpointGroupKind::Activity),
        other => Err(SwatError::new(format!(
            "unknown breakpoint group kind '{other}'"
        ))),
    }
}

fn parse_event_kind(kind: &str) -> SwatResult<EventKind> {
    match kind {
        "Lifecycle" => Ok(EventKind::Lifecycle),
        "Control" => Ok(EventKind::Control),
        "Execution" => Ok(EventKind::Execution),
        "StateMutation" => Ok(EventKind::StateMutation),
        "ModelBoundary" => Ok(EventKind::ModelBoundary),
        "ToolBoundary" => Ok(EventKind::ToolBoundary),
        "SourceResolution" => Ok(EventKind::SourceResolution),
        "SchemaResolution" => Ok(EventKind::SchemaResolution),
        "Snapshot" => Ok(EventKind::Snapshot),
        "Replay" => Ok(EventKind::Replay),
        "TriggerHit" => Ok(EventKind::TriggerHit),
        "ValueObserved" => Ok(EventKind::ValueObserved),
        "PolicyDecision" => Ok(EventKind::PolicyDecision),
        other => Err(SwatError::new(format!("unknown event kind '{other}'"))),
    }
}
