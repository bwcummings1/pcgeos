#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use swat_core::{
    BoundaryId, EventEnvelope, EventId, EventKind, EventPayload, SessionId, SwatResult,
};
use swat_store::SwatStore;
use swat_value::{QueriedValue, decode_event_artifacts};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResolvedEntityKind {
    BoundaryId,
    CorrelationId,
    SpanId,
    ModelName,
    ToolName,
    PlannerName,
    StateKey,
    PolicyName,
    SourceFile,
    FunctionName,
    ValueKey,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EntityRef {
    pub kind: ResolvedEntityKind,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedEntity {
    pub entity: EntityRef,
    pub event_ids: Vec<EventId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundarySpan {
    pub boundary_id: BoundaryId,
    pub event_ids: Vec<EventId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TraceIndex {
    pub entities: Vec<ResolvedEntity>,
    pub boundary_spans: Vec<BoundarySpan>,
}

pub struct TraceResolver<'a, S: SwatStore + ?Sized> {
    store: &'a S,
}

impl<'a, S: SwatStore + ?Sized> TraceResolver<'a, S> {
    pub fn new(store: &'a S) -> Self {
        Self { store }
    }

    pub fn index_session(&self, session_id: SessionId) -> SwatResult<TraceIndex> {
        let mut entity_map: BTreeMap<EntityRef, Vec<EventId>> = BTreeMap::new();
        let mut boundary_map: BTreeMap<BoundaryId, Vec<EventId>> = BTreeMap::new();

        for event in self.store.events_for_session(session_id) {
            for entity in self.event_entities(&event)? {
                push_event_id(entity_map.entry(entity).or_default(), event.event_id);
            }
            if let EventPayload::Boundary { boundary_id, .. } = event.payload {
                push_event_id(boundary_map.entry(boundary_id).or_default(), event.event_id);
            }
        }

        let entities = entity_map
            .into_iter()
            .map(|(entity, event_ids)| ResolvedEntity { entity, event_ids })
            .collect();
        let boundary_spans = boundary_map
            .into_iter()
            .map(|(boundary_id, event_ids)| BoundarySpan {
                boundary_id,
                event_ids,
            })
            .collect();

        Ok(TraceIndex {
            entities,
            boundary_spans,
        })
    }

    pub fn find_entities(
        &self,
        session_id: SessionId,
        needle: &str,
    ) -> SwatResult<Vec<ResolvedEntity>> {
        let needle = needle.to_ascii_lowercase();
        Ok(self
            .index_session(session_id)?
            .entities
            .into_iter()
            .filter(|entity| entity.entity.name.to_ascii_lowercase().contains(&needle))
            .collect())
    }

    pub fn event_entities(&self, event: &EventEnvelope) -> SwatResult<Vec<EntityRef>> {
        let mut entities = BTreeSet::new();

        if let Some(correlation_id) = &event.causality.correlation_id {
            entities.insert(EntityRef {
                kind: ResolvedEntityKind::CorrelationId,
                name: correlation_id.clone(),
            });
        }

        match &event.payload {
            EventPayload::Boundary { boundary_id, .. } => {
                entities.insert(EntityRef {
                    kind: ResolvedEntityKind::BoundaryId,
                    name: boundary_id.raw().to_string(),
                });
            }
            EventPayload::Value { value_key, .. } => {
                entities.insert(EntityRef {
                    kind: ResolvedEntityKind::ValueKey,
                    name: value_key.clone(),
                });
            }
            _ => {}
        }

        for decoded in decode_event_artifacts(self.store, event)? {
            if let Some(QueriedValue::String(correlation_id)) =
                decoded.query_json_path("$.correlation_id")?
            {
                entities.insert(EntityRef {
                    kind: ResolvedEntityKind::CorrelationId,
                    name: correlation_id,
                });
            }
            if let Some(QueriedValue::String(span_id)) = decoded.query_json_path("$.span_id")? {
                entities.insert(EntityRef {
                    kind: ResolvedEntityKind::SpanId,
                    name: span_id,
                });
            }
            if let Some(QueriedValue::String(file)) = decoded.query_json_path("$.file")? {
                entities.insert(EntityRef {
                    kind: ResolvedEntityKind::SourceFile,
                    name: file,
                });
            }
            if let Some(QueriedValue::String(function)) = decoded.query_json_path("$.function")? {
                entities.insert(EntityRef {
                    kind: ResolvedEntityKind::FunctionName,
                    name: function,
                });
            }

            let record_kind = match decoded.query_json_path("$.kind")? {
                Some(QueriedValue::String(value)) => Some(value),
                _ => None,
            };
            let name = match decoded.query_json_path("$.name")? {
                Some(QueriedValue::String(value)) => Some(value),
                _ => None,
            };

            if let (Some(record_kind), Some(name)) = (record_kind, name) {
                let kind = match record_kind.as_str() {
                    "model" => Some(ResolvedEntityKind::ModelName),
                    "tool" => Some(ResolvedEntityKind::ToolName),
                    "planner" => Some(ResolvedEntityKind::PlannerName),
                    "state" => Some(ResolvedEntityKind::StateKey),
                    "policy" => Some(ResolvedEntityKind::PolicyName),
                    _ => None,
                };
                if let Some(kind) = kind {
                    entities.insert(EntityRef { kind, name });
                }
            }
        }

        Ok(entities.into_iter().collect())
    }

    pub fn events_for_correlation(
        &self,
        session_id: SessionId,
        correlation_id: &str,
    ) -> Vec<EventEnvelope> {
        self.store
            .events_for_session(session_id)
            .into_iter()
            .filter(|event| event.causality.correlation_id.as_deref() == Some(correlation_id))
            .collect()
    }

    pub fn boundary_span(
        &self,
        session_id: SessionId,
        boundary_id: BoundaryId,
    ) -> Vec<EventEnvelope> {
        self.store
            .events_for_session(session_id)
            .into_iter()
            .filter(|event| {
                matches!(
                    event.payload,
                    EventPayload::Boundary {
                        boundary_id: event_boundary_id,
                        ..
                    } if event_boundary_id == boundary_id
                )
            })
            .collect()
    }

    pub fn events_for_entity(
        &self,
        session_id: SessionId,
        entity: &EntityRef,
    ) -> SwatResult<Vec<EventEnvelope>> {
        let matched_ids = self
            .index_session(session_id)?
            .entities
            .into_iter()
            .find(|resolved| resolved.entity == *entity)
            .map(|resolved| resolved.event_ids.into_iter().collect::<BTreeSet<_>>())
            .unwrap_or_default();

        Ok(self
            .store
            .events_for_session(session_id)
            .into_iter()
            .filter(|event| matched_ids.contains(&event.event_id))
            .collect())
    }

    pub fn events_by_kind(&self, session_id: SessionId, kind: EventKind) -> Vec<EventEnvelope> {
        self.store
            .events_for_session(session_id)
            .into_iter()
            .filter(|event| event.kind == kind)
            .collect()
    }
}

fn push_event_id(event_ids: &mut Vec<EventId>, event_id: EventId) {
    if event_ids.last().copied() != Some(event_id) {
        event_ids.push(event_id);
    }
}
