#![forbid(unsafe_code)]

use swat_core::{BoundaryId, EventEnvelope, EventId, EventKind, SessionId, SwatResult};
use swat_expr::{QueryExpr, evaluate_expression, parse_expression};
use swat_resolver::{EntityRef, ResolvedEntity, TraceIndex, TraceResolver};
use swat_source::{SourceSnippet, resolve_event_source};
use swat_store::SwatStore;
use swat_value::{DecodedValue, decode_event_artifacts};

#[derive(Clone, Debug, PartialEq)]
pub struct TraceMatch {
    pub event: EventEnvelope,
    pub matched_text: String,
}

pub struct TraceInspector<'a, S: SwatStore + ?Sized> {
    store: &'a S,
}

impl<'a, S: SwatStore + ?Sized> TraceInspector<'a, S> {
    pub fn new(store: &'a S) -> Self {
        Self { store }
    }

    pub fn session_events(&self, session_id: SessionId) -> Vec<EventEnvelope> {
        self.store.events_for_session(session_id)
    }

    pub fn event_by_id(&self, event_id: EventId) -> Option<EventEnvelope> {
        self.store
            .events()
            .into_iter()
            .find(|event| event.event_id == event_id)
    }

    pub fn events_by_kind(&self, session_id: SessionId, kind: EventKind) -> Vec<EventEnvelope> {
        self.session_events(session_id)
            .into_iter()
            .filter(|event| event.kind == kind)
            .collect()
    }

    pub fn decoded_artifacts(&self, event: &EventEnvelope) -> SwatResult<Vec<DecodedValue>> {
        decode_event_artifacts(self.store, event)
    }

    pub fn search_summaries(&self, session_id: SessionId, needle: &str) -> Vec<TraceMatch> {
        self.session_events(session_id)
            .into_iter()
            .filter_map(|event| {
                let matched_text = payload_summary(&event)
                    .filter(|summary| summary.contains(needle))
                    .map(ToString::to_string);
                matched_text.map(|matched_text| TraceMatch {
                    event,
                    matched_text,
                })
            })
            .collect()
    }

    pub fn search_artifact_text(
        &self,
        session_id: SessionId,
        needle: &str,
    ) -> SwatResult<Vec<TraceMatch>> {
        let mut matches = Vec::new();
        for event in self.session_events(session_id) {
            for decoded in self.decoded_artifacts(&event)? {
                let preview = decoded.preview(512);
                if preview.contains(needle) {
                    matches.push(TraceMatch {
                        event: event.clone(),
                        matched_text: preview,
                    });
                    break;
                }
            }
        }
        Ok(matches)
    }

    pub fn query_events(&self, session_id: SessionId, expr: &QueryExpr) -> Vec<EventEnvelope> {
        self.session_events(session_id)
            .into_iter()
            .filter(|event| evaluate_expression(self.store, event, expr))
            .collect()
    }

    pub fn query_events_str(
        &self,
        session_id: SessionId,
        expr: &str,
    ) -> SwatResult<Vec<EventEnvelope>> {
        let parsed = parse_expression(expr)?;
        Ok(self.query_events(session_id, &parsed))
    }

    pub fn resolve_source(
        &self,
        event: &EventEnvelope,
        before: usize,
        after: usize,
    ) -> SwatResult<Option<SourceSnippet>> {
        resolve_event_source(self.store, event, before, after)
    }

    pub fn resolve_event_entities(&self, event: &EventEnvelope) -> SwatResult<Vec<EntityRef>> {
        TraceResolver::new(self.store).event_entities(event)
    }

    pub fn entity_index(&self, session_id: SessionId) -> SwatResult<TraceIndex> {
        TraceResolver::new(self.store).index_session(session_id)
    }

    pub fn find_entities(
        &self,
        session_id: SessionId,
        needle: &str,
    ) -> SwatResult<Vec<ResolvedEntity>> {
        TraceResolver::new(self.store).find_entities(session_id, needle)
    }

    pub fn events_for_correlation(
        &self,
        session_id: SessionId,
        correlation_id: &str,
    ) -> Vec<EventEnvelope> {
        TraceResolver::new(self.store).events_for_correlation(session_id, correlation_id)
    }

    pub fn boundary_span(
        &self,
        session_id: SessionId,
        boundary_id: BoundaryId,
    ) -> Vec<EventEnvelope> {
        TraceResolver::new(self.store).boundary_span(session_id, boundary_id)
    }
}

fn payload_summary(event: &EventEnvelope) -> Option<&str> {
    match &event.payload {
        swat_core::EventPayload::Empty => None,
        swat_core::EventPayload::Text { summary }
        | swat_core::EventPayload::Control { summary, .. }
        | swat_core::EventPayload::Boundary { summary, .. }
        | swat_core::EventPayload::Snapshot { summary, .. }
        | swat_core::EventPayload::Trigger { summary, .. }
        | swat_core::EventPayload::Value { summary, .. }
        | swat_core::EventPayload::Policy { summary, .. } => Some(summary.as_str()),
    }
}
