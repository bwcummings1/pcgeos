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
    Patient,
    Handle,
    Resource,
    Object,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityRelation {
    pub left: EntityRef,
    pub right: EntityRef,
    pub event_ids: Vec<EventId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorrelationGroup {
    pub correlation_id: String,
    pub event_ids: Vec<EventId>,
    pub boundary_ids: Vec<BoundaryId>,
    pub span_ids: Vec<String>,
    pub entities: Vec<EntityRef>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TraceIndex {
    pub entities: Vec<ResolvedEntity>,
    pub boundary_spans: Vec<BoundarySpan>,
    pub relations: Vec<EntityRelation>,
    pub correlation_groups: Vec<CorrelationGroup>,
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
        let mut relation_map: BTreeMap<(EntityRef, EntityRef), Vec<EventId>> = BTreeMap::new();
        let mut correlation_groups: BTreeMap<String, CorrelationGroupBuilder> = BTreeMap::new();

        for event in self.store.events_for_session(session_id) {
            let entities = self.event_entities(&event)?;
            for entity in &entities {
                push_event_id(
                    entity_map.entry(entity.clone()).or_default(),
                    event.event_id,
                );
            }
            if let EventPayload::Boundary { boundary_id, .. } = &event.payload {
                push_event_id(
                    boundary_map.entry(*boundary_id).or_default(),
                    event.event_id,
                );
            }

            for (index, left) in entities.iter().enumerate() {
                for right in entities.iter().skip(index + 1) {
                    let key = ordered_pair(left.clone(), right.clone());
                    push_event_id(relation_map.entry(key).or_default(), event.event_id);
                }
            }

            for correlation in entities
                .iter()
                .filter(|entity| entity.kind == ResolvedEntityKind::CorrelationId)
            {
                let group = correlation_groups
                    .entry(correlation.name.clone())
                    .or_insert_with(CorrelationGroupBuilder::default);
                push_event_id(&mut group.event_ids, event.event_id);
                if let EventPayload::Boundary { boundary_id, .. } = &event.payload {
                    group.boundary_ids.insert(*boundary_id);
                }
                for entity in &entities {
                    group.entities.insert(entity.clone());
                    if entity.kind == ResolvedEntityKind::SpanId {
                        group.span_ids.insert(entity.name.clone());
                    }
                }
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
        let relations = relation_map
            .into_iter()
            .map(|((left, right), event_ids)| EntityRelation {
                left,
                right,
                event_ids,
            })
            .collect();
        let correlation_groups = correlation_groups
            .into_iter()
            .map(|(correlation_id, group)| CorrelationGroup {
                correlation_id,
                event_ids: group.event_ids,
                boundary_ids: group.boundary_ids.into_iter().collect(),
                span_ids: group.span_ids.into_iter().collect(),
                entities: group.entities.into_iter().collect(),
            })
            .collect();

        Ok(TraceIndex {
            entities,
            boundary_spans,
            relations,
            correlation_groups,
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

            let typed_entities = decoded.typed_entities();
            for patient in typed_entities.patients {
                entities.insert(EntityRef {
                    kind: ResolvedEntityKind::Patient,
                    name: patient.key,
                });
                entities.extend(patient.handle_ids.into_iter().map(|handle| EntityRef {
                    kind: ResolvedEntityKind::Handle,
                    name: handle,
                }));
                entities.extend(
                    patient
                        .resource_names
                        .into_iter()
                        .map(|resource| EntityRef {
                            kind: ResolvedEntityKind::Resource,
                            name: resource,
                        }),
                );
                entities.extend(patient.object_ids.into_iter().map(|object| EntityRef {
                    kind: ResolvedEntityKind::Object,
                    name: object,
                }));
            }
            for handle in typed_entities.handles {
                entities.insert(EntityRef {
                    kind: ResolvedEntityKind::Handle,
                    name: handle.key,
                });
                if let Some(patient) = handle.patient {
                    entities.insert(EntityRef {
                        kind: ResolvedEntityKind::Patient,
                        name: patient,
                    });
                }
                if let Some(resource) = handle.resource {
                    entities.insert(EntityRef {
                        kind: ResolvedEntityKind::Resource,
                        name: resource,
                    });
                }
                entities.extend(handle.object_ids.into_iter().map(|object| EntityRef {
                    kind: ResolvedEntityKind::Object,
                    name: object,
                }));
            }
            for resource in typed_entities.resources {
                entities.insert(EntityRef {
                    kind: ResolvedEntityKind::Resource,
                    name: resource.key,
                });
                if let Some(patient) = resource.patient {
                    entities.insert(EntityRef {
                        kind: ResolvedEntityKind::Patient,
                        name: patient,
                    });
                }
                if let Some(handle) = resource.handle {
                    entities.insert(EntityRef {
                        kind: ResolvedEntityKind::Handle,
                        name: handle,
                    });
                }
                entities.extend(resource.object_ids.into_iter().map(|object| EntityRef {
                    kind: ResolvedEntityKind::Object,
                    name: object,
                }));
            }
            for object in typed_entities.objects {
                entities.insert(EntityRef {
                    kind: ResolvedEntityKind::Object,
                    name: object.key,
                });
                if let Some(patient) = object.patient {
                    entities.insert(EntityRef {
                        kind: ResolvedEntityKind::Patient,
                        name: patient,
                    });
                }
                if let Some(handle) = object.handle {
                    entities.insert(EntityRef {
                        kind: ResolvedEntityKind::Handle,
                        name: handle,
                    });
                }
                if let Some(resource) = object.resource {
                    entities.insert(EntityRef {
                        kind: ResolvedEntityKind::Resource,
                        name: resource,
                    });
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

    pub fn entity_relations(&self, session_id: SessionId) -> SwatResult<Vec<EntityRelation>> {
        Ok(self.index_session(session_id)?.relations)
    }

    pub fn related_entities(
        &self,
        session_id: SessionId,
        entity: &EntityRef,
    ) -> SwatResult<Vec<EntityRelation>> {
        Ok(self
            .entity_relations(session_id)?
            .into_iter()
            .filter(|relation| relation.left == *entity || relation.right == *entity)
            .collect())
    }

    pub fn correlation_groups(&self, session_id: SessionId) -> SwatResult<Vec<CorrelationGroup>> {
        Ok(self.index_session(session_id)?.correlation_groups)
    }

    pub fn events_for_span(
        &self,
        session_id: SessionId,
        span_id: &str,
    ) -> SwatResult<Vec<EventEnvelope>> {
        self.events_for_entity(
            session_id,
            &EntityRef {
                kind: ResolvedEntityKind::SpanId,
                name: span_id.to_string(),
            },
        )
    }

    pub fn events_for_value_key(
        &self,
        session_id: SessionId,
        value_key: &str,
    ) -> SwatResult<Vec<EventEnvelope>> {
        self.events_for_entity(
            session_id,
            &EntityRef {
                kind: ResolvedEntityKind::ValueKey,
                name: value_key.to_string(),
            },
        )
    }

    pub fn events_for_source_file(
        &self,
        session_id: SessionId,
        file: &str,
    ) -> SwatResult<Vec<EventEnvelope>> {
        self.events_for_entity(
            session_id,
            &EntityRef {
                kind: ResolvedEntityKind::SourceFile,
                name: file.to_string(),
            },
        )
    }

    pub fn events_for_function(
        &self,
        session_id: SessionId,
        function: &str,
    ) -> SwatResult<Vec<EventEnvelope>> {
        self.events_for_entity(
            session_id,
            &EntityRef {
                kind: ResolvedEntityKind::FunctionName,
                name: function.to_string(),
            },
        )
    }

    pub fn events_for_patient(
        &self,
        session_id: SessionId,
        patient: &str,
    ) -> SwatResult<Vec<EventEnvelope>> {
        self.events_for_entity(
            session_id,
            &EntityRef {
                kind: ResolvedEntityKind::Patient,
                name: patient.to_string(),
            },
        )
    }

    pub fn events_for_handle(
        &self,
        session_id: SessionId,
        handle: &str,
    ) -> SwatResult<Vec<EventEnvelope>> {
        self.events_for_entity(
            session_id,
            &EntityRef {
                kind: ResolvedEntityKind::Handle,
                name: handle.to_string(),
            },
        )
    }

    pub fn events_for_resource(
        &self,
        session_id: SessionId,
        resource: &str,
    ) -> SwatResult<Vec<EventEnvelope>> {
        self.events_for_entity(
            session_id,
            &EntityRef {
                kind: ResolvedEntityKind::Resource,
                name: resource.to_string(),
            },
        )
    }

    pub fn events_for_object(
        &self,
        session_id: SessionId,
        object: &str,
    ) -> SwatResult<Vec<EventEnvelope>> {
        self.events_for_entity(
            session_id,
            &EntityRef {
                kind: ResolvedEntityKind::Object,
                name: object.to_string(),
            },
        )
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

#[derive(Default)]
struct CorrelationGroupBuilder {
    event_ids: Vec<EventId>,
    boundary_ids: BTreeSet<BoundaryId>,
    span_ids: BTreeSet<String>,
    entities: BTreeSet<EntityRef>,
}

fn ordered_pair(left: EntityRef, right: EntityRef) -> (EntityRef, EntityRef) {
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}
