#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};
use swat_core::{
    AdapterAttachment, AdapterControlResult, AdapterEmission, ArtifactAccess, ArtifactAlias,
    ArtifactBinding, ArtifactEncoding, ArtifactRef, BoundaryId, BoundaryReplayDirective,
    CapabilitySet, ControlAction, ControlResponse, DeterminismClass, EventKind, EventPayload,
    PendingArtifact, PendingEvent, ReplayMode, SwatError, SwatResult, TargetAdapter,
    TargetDescriptor, TargetId,
};
use swat_format_pcgeos::{GpManifest, PcGeosRepositoryModel};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PcGeosFixtureValue {
    #[serde(default, rename = "type")]
    pub type_name: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    pub value: JsonValue,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcGeosFixtureObject {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, rename = "class")]
    pub class_name: Option<String>,
    #[serde(default)]
    pub patient: Option<String>,
    #[serde(default)]
    pub handle: Option<String>,
    #[serde(default)]
    pub resource: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default, rename = "flags")]
    pub state_flags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PcGeosFixtureFrame {
    pub boundary_id: BoundaryId,
    pub summary: String,
    #[serde(default)]
    pub name: Option<String>,
    pub function: String,
    pub file: PathBuf,
    pub line: u64,
    #[serde(default)]
    pub correlation_id: Option<String>,
    #[serde(default)]
    pub span_id: Option<String>,
    #[serde(default)]
    pub patient: Option<String>,
    #[serde(default)]
    pub handle: Option<String>,
    #[serde(default)]
    pub resource: Option<String>,
    #[serde(default)]
    pub registers: BTreeMap<String, PcGeosFixtureValue>,
    #[serde(default)]
    pub locals: BTreeMap<String, PcGeosFixtureValue>,
    #[serde(default)]
    pub objects: Vec<PcGeosFixtureObject>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PcGeosFixtureStop {
    pub summary: String,
    pub frames: Vec<PcGeosFixtureFrame>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PcGeosFixtureSpec {
    #[serde(default = "default_target_name")]
    pub target_name: String,
    #[serde(default = "default_runtime_name")]
    pub runtime: String,
    #[serde(default)]
    pub gp_paths: Vec<PathBuf>,
    #[serde(default)]
    pub symbol_paths: Vec<PathBuf>,
    #[serde(default)]
    pub default_patient: Option<String>,
    #[serde(default)]
    pub stops: Vec<PcGeosFixtureStop>,
}

impl PcGeosFixtureSpec {
    pub fn parse_path(path: impl AsRef<Path>) -> SwatResult<Self> {
        let path = path.as_ref();
        let text = fs::read_to_string(path)
            .map_err(|err| SwatError::new(format!("failed to read {}: {err}", path.display())))?;
        let mut spec: Self = serde_json::from_str(&text).map_err(|err| {
            SwatError::new(format!("failed to parse fixture {}: {err}", path.display()))
        })?;
        let root = path.parent().unwrap_or_else(|| Path::new("."));
        for gp_path in &mut spec.gp_paths {
            *gp_path = resolve_fixture_path(root, gp_path);
        }
        for symbol_path in &mut spec.symbol_paths {
            *symbol_path = resolve_fixture_path(root, symbol_path);
        }
        for stop in &mut spec.stops {
            for frame in &mut stop.frames {
                frame.file = resolve_fixture_path(root, &frame.file);
            }
        }
        Ok(spec)
    }
}

pub struct PcGeosAdapter {
    spec: PcGeosFixtureSpec,
    descriptor: TargetDescriptor,
    inventory: InventoryData,
    attached: bool,
    paused: bool,
    exhausted: bool,
    next_alias_raw: u64,
    next_stop_index: usize,
    replayed_boundaries: BTreeMap<BoundaryId, ArtifactRef>,
}

impl PcGeosAdapter {
    pub fn from_fixture(spec: PcGeosFixtureSpec) -> SwatResult<Self> {
        let inventory = build_inventory(&spec)?;
        validate_fixture_stop_references(&spec, &inventory)?;

        Ok(Self {
            descriptor: TargetDescriptor {
                target_id: TargetId::new(),
                adapter_name: "swat-adapter-pcgeos".to_string(),
                target_name: spec.target_name.clone(),
                runtime: spec.runtime.clone(),
                replay_mode: ReplayMode::Recorded,
            },
            spec,
            inventory,
            attached: false,
            paused: true,
            exhausted: false,
            next_alias_raw: 1,
            next_stop_index: 0,
            replayed_boundaries: BTreeMap::new(),
        })
    }

    pub fn from_fixture_path(path: impl AsRef<Path>) -> SwatResult<Self> {
        Self::from_fixture(PcGeosFixtureSpec::parse_path(path)?)
    }

    fn ensure_attached(&self) -> SwatResult<()> {
        if self.attached {
            Ok(())
        } else {
            Err(SwatError::new("pc/geos adapter is not attached"))
        }
    }

    fn next_alias(&mut self) -> ArtifactAlias {
        let alias = ArtifactAlias::from_raw(self.next_alias_raw);
        self.next_alias_raw += 1;
        alias
    }

    fn inventory_emission(&mut self) -> SwatResult<AdapterEmission> {
        let mut emission = AdapterEmission::default();
        let alias = self.next_alias();
        let bytes = serde_json::to_vec(&self.inventory.artifact)
            .map_err(|err| SwatError::new(format!("failed to encode inventory artifact: {err}")))?;
        emission.pending_artifacts.push(PendingArtifact {
            alias,
            media_type: "application/json".to_string(),
            encoding: ArtifactEncoding::Json,
            access: ArtifactAccess::Lazy,
            bytes,
        });
        emission.pending_events.push(
            PendingEvent::new(
                EventKind::ValueObserved,
                EventPayload::Value {
                    value_key: "pcgeos.inventory".to_string(),
                    summary: "loaded PC/GEOS repository inventory".to_string(),
                },
            )
            .with_artifact(ArtifactBinding::Pending(alias)),
        );

        let source_files = self.inventory.source_files.clone();
        for source_file in source_files {
            let alias = self.next_alias();
            let artifact = serde_json::json!({
                "kind": "pcgeos-source-file",
                "file": source_file,
                "line": 1,
            });
            let bytes = serde_json::to_vec(&artifact).map_err(|err| {
                SwatError::new(format!("failed to encode source inventory artifact: {err}"))
            })?;
            emission.pending_artifacts.push(PendingArtifact {
                alias,
                media_type: "application/json".to_string(),
                encoding: ArtifactEncoding::Json,
                access: ArtifactAccess::Lazy,
                bytes,
            });
            emission.pending_events.push(
                PendingEvent::new(
                    EventKind::SourceResolution,
                    EventPayload::Text {
                        summary: format!("discovered PC/GEOS source {}", source_file),
                    },
                )
                .with_artifact(ArtifactBinding::Pending(alias)),
            );
        }

        Ok(emission)
    }

    fn stop_emission(&mut self, stop: &PcGeosFixtureStop) -> SwatResult<AdapterEmission> {
        let mut emission = AdapterEmission::default();
        for frame in &stop.frames {
            let mut event = PendingEvent::new(
                EventKind::Execution,
                EventPayload::Boundary {
                    boundary_id: frame.boundary_id,
                    determinism: DeterminismClass::ReplayOnly,
                    summary: frame.summary.clone(),
                },
            );
            if let Some(artifact_ref) = self.replayed_boundaries.get(&frame.boundary_id).cloned() {
                emission.pending_events.push(PendingEvent::new(
                    EventKind::Replay,
                    EventPayload::Text {
                        summary: format!(
                            "reused replay artifact for PC/GEOS boundary {}",
                            frame.boundary_id.raw()
                        ),
                    },
                ));
                event = event.with_artifact(ArtifactBinding::Existing(artifact_ref));
            } else {
                let alias = self.next_alias();
                let artifact = self.frame_artifact(frame);
                let bytes = serde_json::to_vec(&artifact).map_err(|err| {
                    SwatError::new(format!("failed to encode stop artifact: {err}"))
                })?;
                emission.pending_artifacts.push(PendingArtifact {
                    alias,
                    media_type: "application/json".to_string(),
                    encoding: ArtifactEncoding::Json,
                    access: ArtifactAccess::Lazy,
                    bytes,
                });
                event = event.with_artifact(ArtifactBinding::Pending(alias));
            }
            emission.pending_events.push(event);
        }

        for frame in stop.frames.iter().rev().skip(1) {
            emission.pending_events.push(PendingEvent::new(
                EventKind::Execution,
                EventPayload::Boundary {
                    boundary_id: frame.boundary_id,
                    determinism: DeterminismClass::ReplayOnly,
                    summary: format!("{} complete", frame.summary),
                },
            ));
        }

        Ok(emission)
    }

    fn frame_artifact(&self, frame: &PcGeosFixtureFrame) -> JsonValue {
        let mut root = Map::new();
        root.insert(
            "kind".to_string(),
            JsonValue::String("pcgeos-frame".to_string()),
        );
        root.insert(
            "name".to_string(),
            JsonValue::String(frame.name.clone().unwrap_or_else(|| frame.function.clone())),
        );
        root.insert(
            "function".to_string(),
            JsonValue::String(frame.function.clone()),
        );
        root.insert(
            "file".to_string(),
            JsonValue::String(frame.file.display().to_string()),
        );
        root.insert("line".to_string(), JsonValue::Number(frame.line.into()));

        if let Some(correlation_id) = &frame.correlation_id {
            root.insert(
                "correlation_id".to_string(),
                JsonValue::String(correlation_id.clone()),
            );
        }
        if let Some(span_id) = &frame.span_id {
            root.insert("span_id".to_string(), JsonValue::String(span_id.clone()));
        }
        if !frame.registers.is_empty() {
            root.insert(
                "registers".to_string(),
                JsonValue::Object(build_value_map(&frame.registers)),
            );
        }
        if !frame.locals.is_empty() {
            root.insert(
                "locals".to_string(),
                JsonValue::Object(build_value_map(&frame.locals)),
            );
        }
        if let Some(patient) = &frame.patient {
            if let Some(record) = self.inventory.patients.get(patient) {
                root.insert("patient".to_string(), record.clone());
            }
        }
        if let Some(handle) = &frame.handle {
            if let Some(record) = self.inventory.handles.get(handle) {
                root.insert("handle".to_string(), record.clone());
            }
        }
        if let Some(resource) = &frame.resource {
            if let Some(record) = self.inventory.resources.get(resource) {
                root.insert("resource".to_string(), record.clone());
            }
        }
        if !frame.objects.is_empty() {
            root.insert(
                "objects".to_string(),
                JsonValue::Array(frame.objects.iter().map(object_to_json).collect()),
            );
        }

        JsonValue::Object(root)
    }

    fn advance_stop(&mut self) -> SwatResult<Option<AdapterEmission>> {
        let Some(stop) = self.spec.stops.get(self.next_stop_index).cloned() else {
            if self.exhausted {
                return Ok(None);
            }
            self.exhausted = true;
            self.paused = true;
            let mut emission = AdapterEmission::default();
            emission.pending_events.push(PendingEvent::new(
                EventKind::Lifecycle,
                EventPayload::Text {
                    summary: "PC/GEOS fixture exhausted recorded stops".to_string(),
                },
            ));
            return Ok(Some(emission));
        };

        self.next_stop_index += 1;
        self.paused = true;
        self.stop_emission(&stop).map(Some)
    }
}

impl TargetAdapter for PcGeosAdapter {
    fn adapter_name(&self) -> &'static str {
        "swat-adapter-pcgeos"
    }

    fn attach(&mut self) -> SwatResult<AdapterAttachment> {
        if self.attached {
            return Err(SwatError::new("pc/geos adapter already attached"));
        }
        self.attached = true;
        self.paused = true;
        self.exhausted = false;
        self.next_alias_raw = 1;
        self.next_stop_index = 0;
        self.replayed_boundaries.clear();

        let mut initial_emission = AdapterEmission::default();
        initial_emission.pending_events.push(PendingEvent::new(
            EventKind::Lifecycle,
            EventPayload::Text {
                summary: "pc/geos fixture attached".to_string(),
            },
        ));
        initial_emission.extend(self.inventory_emission()?);
        if let Some(stop_emission) = self.advance_stop()? {
            initial_emission.extend(stop_emission);
        }

        Ok(AdapterAttachment {
            descriptor: self.descriptor.clone(),
            capabilities: self.capabilities(),
            initial_emission,
        })
    }

    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet {
            can_inject_replay: true,
            can_resolve_source: true,
            ..CapabilitySet::basic_observer()
        }
    }

    fn poll(&mut self) -> SwatResult<AdapterEmission> {
        self.ensure_attached()?;
        if self.paused {
            return Ok(AdapterEmission::default());
        }

        let mut emission = AdapterEmission::default();
        emission.pending_events.push(PendingEvent::new(
            EventKind::Execution,
            EventPayload::Text {
                summary: "pc/geos fixture resumed until next stop".to_string(),
            },
        ));
        if let Some(stop_emission) = self.advance_stop()? {
            emission.extend(stop_emission);
        }
        Ok(emission)
    }

    fn control(&mut self, action: ControlAction) -> SwatResult<AdapterControlResult> {
        self.ensure_attached()?;
        let mut emission = AdapterEmission::default();

        let response = match action.clone() {
            ControlAction::Pause => {
                self.paused = true;
                ControlResponse {
                    accepted: true,
                    summary: "pc/geos fixture paused".to_string(),
                }
            }
            ControlAction::Resume => {
                self.paused = false;
                ControlResponse {
                    accepted: true,
                    summary: "pc/geos fixture resumed".to_string(),
                }
            }
            ControlAction::Step => {
                if self.next_stop_index >= self.spec.stops.len() {
                    emission.pending_events.push(PendingEvent::new(
                        EventKind::Control,
                        EventPayload::Control {
                            action,
                            summary: "pc/geos fixture has no further recorded steps".to_string(),
                        },
                    ));
                    return Ok(AdapterControlResult {
                        response: ControlResponse {
                            accepted: false,
                            summary: "pc/geos fixture has no further recorded steps".to_string(),
                        },
                        emission,
                    });
                }
                let Some(stop_emission) = self.advance_stop()? else {
                    emission.pending_events.push(PendingEvent::new(
                        EventKind::Control,
                        EventPayload::Control {
                            action,
                            summary: "pc/geos fixture has no further recorded steps".to_string(),
                        },
                    ));
                    return Ok(AdapterControlResult {
                        response: ControlResponse {
                            accepted: false,
                            summary: "pc/geos fixture has no further recorded steps".to_string(),
                        },
                        emission,
                    });
                };
                self.paused = true;
                emission.extend(stop_emission);
                ControlResponse {
                    accepted: true,
                    summary: "pc/geos fixture step completed".to_string(),
                }
            }
            ControlAction::CreateSnapshot { reason } => ControlResponse {
                accepted: true,
                summary: format!("pc/geos fixture snapshot requested: {reason}"),
            },
        };

        emission.pending_events.push(PendingEvent::new(
            EventKind::Control,
            EventPayload::Control {
                action,
                summary: response.summary.clone(),
            },
        ));

        Ok(AdapterControlResult { response, emission })
    }

    fn inject_boundary_replay(
        &mut self,
        directive: BoundaryReplayDirective,
    ) -> SwatResult<AdapterEmission> {
        self.ensure_attached()?;
        self.replayed_boundaries
            .insert(directive.boundary_id, directive.artifact_ref.clone());

        Ok(AdapterEmission {
            pending_events: vec![PendingEvent::new(
                EventKind::Replay,
                EventPayload::Text {
                    summary: format!(
                        "prepared replay for PC/GEOS boundary {}",
                        directive.boundary_id.raw()
                    ),
                },
            )],
            pending_artifacts: Vec::new(),
        })
    }
}

#[derive(Clone, Debug, Default)]
struct InventoryData {
    artifact: JsonValue,
    source_files: Vec<String>,
    patients: BTreeMap<String, JsonValue>,
    handles: BTreeMap<String, JsonValue>,
    resources: BTreeMap<String, JsonValue>,
}

fn build_inventory(spec: &PcGeosFixtureSpec) -> SwatResult<InventoryData> {
    let manifests = spec
        .gp_paths
        .iter()
        .map(GpManifest::parse_path)
        .collect::<SwatResult<Vec<_>>>()?;
    let model = PcGeosRepositoryModel::from_fixture_paths(&spec.gp_paths, &spec.symbol_paths)?;

    let mut source_files = collect_manifest_source_files(&manifests)?;
    source_files.sort();
    source_files.dedup();

    let mut handles = BTreeMap::<String, JsonValue>::new();
    let mut resources = BTreeMap::<String, JsonValue>::new();
    let mut patients = BTreeMap::<String, JsonValue>::new();
    let mut objects = BTreeMap::<String, JsonValue>::new();
    let mut derived_handle_for_resource = BTreeMap::<String, String>::new();

    for manifest in &manifests {
        let Some(patient_key) = manifest.patient_key() else {
            continue;
        };
        let geode_key = manifest.name.clone().unwrap_or_else(|| patient_key.clone());
        for resource in &manifest.resources {
            let resource_key = format!("{geode_key}:{}", resource.name);
            let handle_key = format!("{geode_key}:handle:{}", resource.name);
            derived_handle_for_resource.insert(resource_key.clone(), handle_key.clone());
            handles.entry(handle_key.clone()).or_insert_with(|| {
                serde_json::json!({
                    "kind": "handle",
                    "id": handle_key,
                    "name": resource.name,
                    "patient": patient_key,
                    "resource": resource_key,
                    "type": "resource",
                    "attached": true,
                    "flags": resource.flags,
                })
            });
        }

        if let Some(app_object) = &manifest.app_object {
            let preferred_resource =
                preferred_manifest_resource(&geode_key, &model).or_else(|| {
                    manifest
                        .resources
                        .first()
                        .map(|resource| format!("{geode_key}:{}", resource.name))
                });
            let handle_key = preferred_resource
                .as_ref()
                .and_then(|resource_key| derived_handle_for_resource.get(resource_key))
                .cloned();
            objects.entry(app_object.clone()).or_insert_with(|| {
                object_to_json(&PcGeosFixtureObject {
                    id: app_object.clone(),
                    name: Some(app_object.clone()),
                    class_name: manifest.class_name.clone(),
                    patient: Some(patient_key.clone()),
                    handle: handle_key,
                    resource: preferred_resource,
                    address: None,
                    state_flags: vec!["app-object".to_string()],
                })
            });
        }
    }

    for handle in &model.handles {
        handles.insert(
            handle.key.clone(),
            serde_json::json!({
                "kind": "handle",
                "id": handle.key,
                "name": handle.resource.as_deref().map(short_resource_name),
                "patient": handle.patient,
                "resource": handle.resource,
                "type": handle.kind,
                "segment": extract_handle_segment(&handle.key),
                "attached": true,
                "flags": ["symbol-vm"],
            }),
        );
    }

    for resource in &model.resources {
        let handle_key = resource
            .handle
            .clone()
            .or_else(|| derived_handle_for_resource.get(&resource.key).cloned());
        resources.insert(
            resource.key.clone(),
            serde_json::json!({
                "kind": "resource",
                "name": resource.key,
                "identifier": short_resource_name(&resource.key),
                "patient": resource.patient,
                "handle": handle_key,
                "type": resource.kind,
                "source_file": resource.source_files.first(),
            }),
        );
    }

    for stop in &spec.stops {
        for frame in &stop.frames {
            for object in &frame.objects {
                objects
                    .entry(object.id.clone())
                    .or_insert_with(|| object_to_json(object));
            }
        }
    }

    for patient in &model.patients {
        let handle_ids = handles
            .iter()
            .filter_map(|(key, value)| {
                (json_string_field(value, "patient").as_deref() == Some(patient.key.as_str()))
                    .then_some(key.clone())
            })
            .collect::<Vec<_>>();
        let resource_ids = resources
            .keys()
            .filter(|key| patient.resources.iter().any(|candidate| candidate == *key))
            .cloned()
            .collect::<Vec<_>>();
        let object_ids = objects
            .iter()
            .filter_map(|(key, value)| {
                (json_string_field(value, "patient").as_deref() == Some(patient.key.as_str()))
                    .then_some(key.clone())
            })
            .collect::<Vec<_>>();

        patients.insert(
            patient.key.clone(),
            serde_json::json!({
                "kind": "patient",
                "name": patient.key,
                "identifier": patient.geodes.first(),
                "runtime": "pcgeos",
                "path": patient.artifact_paths.first(),
                "default": spec.default_patient.as_deref() == Some(patient.key.as_str()),
                "status": "loaded",
                "handles": handle_ids,
                "resources": resource_ids,
                "objects": object_ids,
            }),
        );
    }

    let artifact = serde_json::json!({
        "kind": "pcgeos-inventory",
        "runtime": spec.runtime,
        "patients": patients.values().cloned().collect::<Vec<_>>(),
        "handles": handles.values().cloned().collect::<Vec<_>>(),
        "resources": resources.values().cloned().collect::<Vec<_>>(),
        "objects": objects.values().cloned().collect::<Vec<_>>(),
        "source_files": source_files,
    });

    Ok(InventoryData {
        artifact,
        source_files,
        patients,
        handles,
        resources,
    })
}

fn validate_fixture_stop_references(
    spec: &PcGeosFixtureSpec,
    inventory: &InventoryData,
) -> SwatResult<()> {
    for stop in &spec.stops {
        if stop.frames.is_empty() {
            return Err(SwatError::new(format!(
                "fixture stop '{}' does not define any frames",
                stop.summary
            )));
        }
        for frame in &stop.frames {
            if !frame.file.exists() {
                return Err(SwatError::new(format!(
                    "fixture frame {} references missing source file {}",
                    frame.boundary_id.raw(),
                    frame.file.display()
                )));
            }
            if frame.line == 0 {
                return Err(SwatError::new(format!(
                    "fixture frame {} must use 1-based line numbers",
                    frame.boundary_id.raw()
                )));
            }
            if let Some(patient) = &frame.patient {
                if !inventory.patients.contains_key(patient) {
                    return Err(SwatError::new(format!(
                        "fixture frame {} references unknown patient {}",
                        frame.boundary_id.raw(),
                        patient
                    )));
                }
            }
            if let Some(handle) = &frame.handle {
                if !inventory.handles.contains_key(handle) {
                    return Err(SwatError::new(format!(
                        "fixture frame {} references unknown handle {}",
                        frame.boundary_id.raw(),
                        handle
                    )));
                }
            }
            if let Some(resource) = &frame.resource {
                if !inventory.resources.contains_key(resource) {
                    return Err(SwatError::new(format!(
                        "fixture frame {} references unknown resource {}",
                        frame.boundary_id.raw(),
                        resource
                    )));
                }
            }
        }
    }

    Ok(())
}

fn collect_manifest_source_files(manifests: &[GpManifest]) -> SwatResult<Vec<String>> {
    let mut files = BTreeSet::new();
    for manifest in manifests {
        let source_root = manifest.path.parent().ok_or_else(|| {
            SwatError::new(format!(
                "manifest {} has no parent",
                manifest.path.display()
            ))
        })?;
        for source in manifest.source_files()? {
            files.insert(source_root.join(source).display().to_string());
        }
    }
    Ok(files.into_iter().collect())
}

fn preferred_manifest_resource(geode_key: &str, model: &PcGeosRepositoryModel) -> Option<String> {
    let geode = model.geodes.iter().find(|geode| geode.key == geode_key)?;
    geode
        .resources
        .iter()
        .find(|resource| resource.ends_with(":AppResource"))
        .cloned()
        .or_else(|| geode.resources.first().cloned())
}

fn resolve_fixture_path(root: &Path, path: &Path) -> PathBuf {
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    resolved.canonicalize().unwrap_or(resolved)
}

fn default_target_name() -> String {
    "pcgeos-fixture".to_string()
}

fn default_runtime_name() -> String {
    "pcgeos-fixture".to_string()
}

fn build_value_map(values: &BTreeMap<String, PcGeosFixtureValue>) -> Map<String, JsonValue> {
    values
        .iter()
        .map(|(name, value)| {
            let mut entry = Map::new();
            if let Some(type_name) = &value.type_name {
                entry.insert("type".to_string(), JsonValue::String(type_name.clone()));
            }
            if let Some(group) = &value.group {
                entry.insert("group".to_string(), JsonValue::String(group.clone()));
            }
            entry.insert("value".to_string(), value.value.clone());
            (name.clone(), JsonValue::Object(entry))
        })
        .collect()
}

fn object_to_json(object: &PcGeosFixtureObject) -> JsonValue {
    serde_json::json!({
        "kind": "object",
        "id": object.id,
        "name": object.name,
        "class": object.class_name,
        "patient": object.patient,
        "handle": object.handle,
        "resource": object.resource,
        "address": object.address,
        "flags": object.state_flags,
    })
}

fn short_resource_name(resource_key: &str) -> String {
    resource_key
        .rsplit(':')
        .next()
        .unwrap_or(resource_key)
        .to_string()
}

fn extract_handle_segment(handle_key: &str) -> Option<String> {
    handle_key.split(':').next_back().map(ToString::to_string)
}

fn json_string_field(value: &JsonValue, name: &str) -> Option<String> {
    value
        .as_object()
        .and_then(|object| object.get(name))
        .and_then(JsonValue::as_str)
        .map(ToString::to_string)
}
