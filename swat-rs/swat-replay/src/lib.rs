#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use swat_core::{
    AdapterEmission, BoundaryId, BoundaryReplayDirective, DeterminismClass, EventEnvelope,
    EventKind, EventPayload, SwatResult, TargetAdapter,
};

#[derive(Clone, Debug, Default)]
pub struct ReplayPlan {
    directives: BTreeMap<BoundaryId, BoundaryReplayDirective>,
}

impl ReplayPlan {
    pub fn from_events(events: &[EventEnvelope]) -> Self {
        let mut directives = BTreeMap::new();

        for event in events {
            if !matches!(
                event.kind,
                EventKind::ModelBoundary | EventKind::ToolBoundary
            ) {
                continue;
            }

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

    pub fn len(&self) -> usize {
        self.directives.len()
    }

    pub fn is_empty(&self) -> bool {
        self.directives.is_empty()
    }

    pub fn directives(&self) -> impl Iterator<Item = &BoundaryReplayDirective> {
        self.directives.values()
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
