#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use swat_core::{
    AdapterEmission, BoundaryId, BoundaryReplayDirective, DeterminismClass, EventEnvelope,
    EventPayload, SwatResult, TargetAdapter,
};

#[derive(Clone, Debug, Default)]
pub struct ReplayPlan {
    directives: BTreeMap<BoundaryId, BoundaryReplayDirective>,
}

impl ReplayPlan {
    pub fn from_events(events: &[EventEnvelope]) -> Self {
        let mut directives = BTreeMap::new();

        for event in events {
            let EventPayload::Boundary {
                boundary_id,
                determinism,
                ..
            } = &event.payload
            else {
                continue;
            };

            if matches!(determinism, DeterminismClass::Deterministic) {
                continue;
            }

            if let Some(artifact_ref) = event.artifact_refs.first() {
                directives
                    .entry(*boundary_id)
                    .or_insert(BoundaryReplayDirective {
                        boundary_id: *boundary_id,
                        artifact_ref: artifact_ref.clone(),
                    });
            }
        }

        Self { directives }
    }

    pub fn from_events_up_to(events: &[EventEnvelope], max_sequence_no: u64) -> Self {
        let filtered = events
            .iter()
            .filter(|event| event.sequence_no <= max_sequence_no)
            .cloned()
            .collect::<Vec<_>>();
        Self::from_events(&filtered)
    }

    pub fn for_boundary(events: &[EventEnvelope], boundary_id: BoundaryId) -> Self {
        let filtered = events
            .iter()
            .filter(|event| {
                matches!(
                    event.payload,
                    EventPayload::Boundary {
                        boundary_id: event_boundary_id,
                        ..
                    } if event_boundary_id == boundary_id
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        Self::from_events(&filtered)
    }

    pub fn len(&self) -> usize {
        self.directives.len()
    }

    pub fn is_empty(&self) -> bool {
        self.directives.is_empty()
    }

    pub fn directives(&self) -> impl Iterator<Item = &BoundaryReplayDirective> {
        self.directives.values()
    }

    pub fn has_boundary(&self, boundary_id: BoundaryId) -> bool {
        self.directives.contains_key(&boundary_id)
    }
}

#[derive(Default)]
pub struct ReplayController;

impl ReplayController {
    pub fn apply<A: TargetAdapter + ?Sized>(
        &self,
        adapter: &mut A,
        plan: &ReplayPlan,
    ) -> SwatResult<AdapterEmission> {
        let mut merged = AdapterEmission::default();
        for directive in plan.directives.values() {
            let emission = adapter.inject_boundary_replay(directive.clone())?;
            merged.extend(emission);
        }
        Ok(merged)
    }
}
