#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use swat_core::{
    AdapterEmission, ArtifactAlias, ArtifactBinding, ArtifactId, ArtifactRef, EventEnvelope,
    EventId, SessionId, SnapshotId, SnapshotRecord, SwatError, SwatResult, TargetId, Timestamp,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredArtifact {
    pub artifact_ref: ArtifactRef,
    pub created_at: Timestamp,
    pub bytes: Vec<u8>,
}

pub trait SwatStore {
    fn ingest_emission(
        &mut self,
        session_id: SessionId,
        target_id: TargetId,
        next_sequence: &mut u64,
        emission: AdapterEmission,
    ) -> SwatResult<Vec<EventEnvelope>>;

    fn events(&self) -> Vec<EventEnvelope>;

    fn events_for_session(&self, session_id: SessionId) -> Vec<EventEnvelope>;

    fn artifact(&self, artifact_id: ArtifactId) -> Option<StoredArtifact>;

    fn artifact_count(&self) -> usize;

    fn record_snapshot(&mut self, snapshot: SnapshotRecord) -> SwatResult<()>;

    fn snapshots(&self) -> Vec<SnapshotRecord>;

    fn snapshots_for_session(&self, session_id: SessionId) -> Vec<SnapshotRecord>;

    fn snapshot(&self, snapshot_id: SnapshotId) -> Option<SnapshotRecord>;
}

#[derive(Default)]
pub struct InMemoryStore {
    events: Vec<EventEnvelope>,
    artifacts: BTreeMap<ArtifactId, StoredArtifact>,
    snapshots: BTreeMap<SnapshotId, SnapshotRecord>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ingest_emission(
        &mut self,
        session_id: SessionId,
        target_id: TargetId,
        next_sequence: &mut u64,
        emission: AdapterEmission,
    ) -> SwatResult<Vec<EventEnvelope>> {
        <Self as SwatStore>::ingest_emission(self, session_id, target_id, next_sequence, emission)
    }

    pub fn events(&self) -> Vec<EventEnvelope> {
        <Self as SwatStore>::events(self)
    }

    pub fn events_for_session(&self, session_id: SessionId) -> Vec<EventEnvelope> {
        <Self as SwatStore>::events_for_session(self, session_id)
    }

    pub fn artifact(&self, artifact_id: ArtifactId) -> Option<StoredArtifact> {
        <Self as SwatStore>::artifact(self, artifact_id)
    }

    pub fn artifact_count(&self) -> usize {
        <Self as SwatStore>::artifact_count(self)
    }

    pub fn record_snapshot(&mut self, snapshot: SnapshotRecord) -> SwatResult<()> {
        <Self as SwatStore>::record_snapshot(self, snapshot)
    }

    pub fn snapshots(&self) -> Vec<SnapshotRecord> {
        <Self as SwatStore>::snapshots(self)
    }

    pub fn snapshots_for_session(&self, session_id: SessionId) -> Vec<SnapshotRecord> {
        <Self as SwatStore>::snapshots_for_session(self, session_id)
    }

    pub fn snapshot(&self, snapshot_id: SnapshotId) -> Option<SnapshotRecord> {
        <Self as SwatStore>::snapshot(self, snapshot_id)
    }
}

impl SwatStore for InMemoryStore {
    fn ingest_emission(
        &mut self,
        session_id: SessionId,
        target_id: TargetId,
        next_sequence: &mut u64,
        emission: AdapterEmission,
    ) -> SwatResult<Vec<EventEnvelope>> {
        let materialized = materialize_emission(
            session_id,
            target_id,
            next_sequence,
            emission,
            |artifact_id| self.artifacts.contains_key(&artifact_id),
        )?;

        for artifact in &materialized.artifacts {
            self.artifacts
                .insert(artifact.artifact_ref.artifact_id, artifact.clone());
        }
        self.events.extend(materialized.events.iter().cloned());

        Ok(materialized.events)
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

    fn record_snapshot(&mut self, snapshot: SnapshotRecord) -> SwatResult<()> {
        self.snapshots.insert(snapshot.snapshot_id, snapshot);
        Ok(())
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedArtifactRecord {
    pub artifact_ref: ArtifactRef,
    pub created_at: Timestamp,
    pub file_name: String,
}

pub struct FileStore {
    root: PathBuf,
    artifacts_dir: PathBuf,
    events_path: PathBuf,
    artifact_index_path: PathBuf,
    snapshots_path: PathBuf,
    events: Vec<EventEnvelope>,
    artifacts: BTreeMap<ArtifactId, PersistedArtifactRecord>,
    snapshots: BTreeMap<SnapshotId, SnapshotRecord>,
}

impl FileStore {
    pub fn open(root: impl Into<PathBuf>) -> SwatResult<Self> {
        let root = root.into();
        let artifacts_dir = root.join("artifacts");
        let events_path = root.join("events.jsonl");
        let artifact_index_path = root.join("artifacts.jsonl");
        let snapshots_path = root.join("snapshots.jsonl");

        fs::create_dir_all(&artifacts_dir).map_err(|err| {
            SwatError::new(format!(
                "failed to create file store directory {}: {err}",
                artifacts_dir.display()
            ))
        })?;
        touch_file(&events_path)?;
        touch_file(&artifact_index_path)?;
        touch_file(&snapshots_path)?;

        let events = load_jsonl::<EventEnvelope>(&events_path)?;
        let artifact_records = load_jsonl::<PersistedArtifactRecord>(&artifact_index_path)?;
        let snapshot_records = load_jsonl::<SnapshotRecord>(&snapshots_path)?;
        let artifacts = artifact_records
            .into_iter()
            .map(|record| (record.artifact_ref.artifact_id, record))
            .collect();
        let snapshots = snapshot_records
            .into_iter()
            .map(|record| (record.snapshot_id, record))
            .collect();

        Ok(Self {
            root,
            artifacts_dir,
            events_path,
            artifact_index_path,
            snapshots_path,
            events,
            artifacts,
            snapshots,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn artifact_path(&self, artifact_id: ArtifactId) -> Option<PathBuf> {
        self.artifacts
            .get(&artifact_id)
            .map(|record| self.artifacts_dir.join(&record.file_name))
    }
}

impl SwatStore for FileStore {
    fn ingest_emission(
        &mut self,
        session_id: SessionId,
        target_id: TargetId,
        next_sequence: &mut u64,
        emission: AdapterEmission,
    ) -> SwatResult<Vec<EventEnvelope>> {
        let materialized = materialize_emission(
            session_id,
            target_id,
            next_sequence,
            emission,
            |artifact_id| self.artifacts.contains_key(&artifact_id),
        )?;

        for artifact in &materialized.artifacts {
            let artifact_id = artifact.artifact_ref.artifact_id;
            let file_name = format!("{}.bin", artifact_id.raw());
            let artifact_path = self.artifacts_dir.join(&file_name);
            fs::write(&artifact_path, &artifact.bytes).map_err(|err| {
                SwatError::new(format!(
                    "failed to persist artifact {} to {}: {err}",
                    artifact_id.raw(),
                    artifact_path.display()
                ))
            })?;

            let record = PersistedArtifactRecord {
                artifact_ref: artifact.artifact_ref.clone(),
                created_at: artifact.created_at,
                file_name,
            };
            append_jsonl(&self.artifact_index_path, &record)?;
            self.artifacts.insert(artifact_id, record);
        }

        for event in &materialized.events {
            append_jsonl(&self.events_path, event)?;
        }
        self.events.extend(materialized.events.iter().cloned());

        Ok(materialized.events)
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
        let record = self.artifacts.get(&artifact_id)?;
        let bytes = fs::read(self.artifacts_dir.join(&record.file_name)).ok()?;
        Some(StoredArtifact {
            artifact_ref: record.artifact_ref.clone(),
            created_at: record.created_at,
            bytes,
        })
    }

    fn artifact_count(&self) -> usize {
        self.artifacts.len()
    }

    fn record_snapshot(&mut self, snapshot: SnapshotRecord) -> SwatResult<()> {
        append_jsonl(&self.snapshots_path, &snapshot)?;
        self.snapshots.insert(snapshot.snapshot_id, snapshot);
        Ok(())
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

struct MaterializedEmission {
    artifacts: Vec<StoredArtifact>,
    events: Vec<EventEnvelope>,
}

fn materialize_emission(
    session_id: SessionId,
    target_id: TargetId,
    next_sequence: &mut u64,
    emission: AdapterEmission,
    artifact_exists: impl Fn(ArtifactId) -> bool,
) -> SwatResult<MaterializedEmission> {
    let mut aliases: BTreeMap<ArtifactAlias, ArtifactRef> = BTreeMap::new();
    let mut materialized_artifacts = Vec::new();

    for pending in emission.pending_artifacts {
        let artifact_ref = ArtifactRef {
            artifact_id: ArtifactId::new(),
            media_type: pending.media_type,
            encoding: pending.encoding,
            size_hint: Some(pending.bytes.len() as u64),
            access: pending.access,
        };
        let stored_artifact = StoredArtifact {
            artifact_ref: artifact_ref.clone(),
            created_at: Timestamp::now(),
            bytes: pending.bytes,
        };
        aliases.insert(pending.alias, artifact_ref);
        materialized_artifacts.push(stored_artifact);
    }

    let mut stored_events = Vec::new();
    for pending in emission.pending_events {
        let mut artifact_refs = Vec::new();
        for binding in pending.artifacts {
            match binding {
                ArtifactBinding::Pending(alias) => {
                    let artifact_ref = aliases.get(&alias).cloned().ok_or_else(|| {
                        SwatError::new(format!(
                            "missing materialized artifact for alias {}",
                            alias.raw()
                        ))
                    })?;
                    artifact_refs.push(artifact_ref);
                }
                ArtifactBinding::Existing(artifact_ref) => {
                    if !artifact_exists(artifact_ref.artifact_id)
                        && !materialized_artifacts.iter().any(|artifact| {
                            artifact.artifact_ref.artifact_id == artifact_ref.artifact_id
                        })
                    {
                        return Err(SwatError::new(format!(
                            "missing referenced artifact {}",
                            artifact_ref.artifact_id.raw()
                        )));
                    }
                    artifact_refs.push(artifact_ref);
                }
            }
        }

        let event = EventEnvelope {
            event_id: EventId::new(),
            session_id,
            target_id,
            sequence_no: *next_sequence,
            observed_at: pending.observed_at,
            kind: pending.kind,
            causality: pending.causality,
            payload: pending.payload,
            artifact_refs,
        };
        *next_sequence += 1;
        stored_events.push(event);
    }

    Ok(MaterializedEmission {
        artifacts: materialized_artifacts,
        events: stored_events,
    })
}

fn touch_file(path: &Path) -> SwatResult<()> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map(|_| ())
        .map_err(|err| SwatError::new(format!("failed to open {}: {err}", path.display())))
}

fn append_jsonl<T: Serialize>(path: &Path, value: &T) -> SwatResult<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| SwatError::new(format!("failed to append {}: {err}", path.display())))?;
    let mut line = serde_json::to_vec(value)
        .map_err(|err| SwatError::new(format!("failed to serialize {}: {err}", path.display())))?;
    line.push(b'\n');
    file.write_all(&line)
        .map_err(|err| SwatError::new(format!("failed to write {}: {err}", path.display())))
}

fn load_jsonl<T>(path: &Path) -> SwatResult<Vec<T>>
where
    T: for<'de> Deserialize<'de>,
{
    let file = File::open(path)
        .map_err(|err| SwatError::new(format!("failed to read {}: {err}", path.display())))?;
    let reader = BufReader::new(file);
    let mut values = Vec::new();
    for line in reader.lines() {
        let line = line
            .map_err(|err| SwatError::new(format!("failed to read {}: {err}", path.display())))?;
        if line.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str::<T>(&line).map_err(|err| {
            SwatError::new(format!(
                "failed to parse {} as jsonl: {err}",
                path.display()
            ))
        })?;
        values.push(value);
    }
    Ok(values)
}
