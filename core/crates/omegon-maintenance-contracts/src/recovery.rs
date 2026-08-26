#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DetachObservation {
    pub dispatched: bool,
    pub source_matches: bool,
    pub destination_matches: bool,
    pub conflicting_state: bool,
    pub observable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReconciliationDecision {
    Settle,
    AbortAndClearFence,
    RetainUnknownFence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordObservation {
    NotDispatched,
    IntendedCanonicalBytesPresent,
    IntendedTargetAbsentAfterDispatch,
    ConflictingRecordOrGeneration,
    Unavailable,
}

pub fn reconcile_detach(observation: DetachObservation) -> ReconciliationDecision {
    if !observation.observable || observation.conflicting_state {
        return ReconciliationDecision::RetainUnknownFence;
    }
    if !observation.source_matches && observation.destination_matches {
        return ReconciliationDecision::Settle;
    }
    if observation.source_matches && !observation.destination_matches && !observation.dispatched {
        return ReconciliationDecision::AbortAndClearFence;
    }
    ReconciliationDecision::RetainUnknownFence
}

pub const fn reconcile_record(observation: RecordObservation) -> ReconciliationDecision {
    match observation {
        RecordObservation::NotDispatched => ReconciliationDecision::AbortAndClearFence,
        RecordObservation::IntendedCanonicalBytesPresent => ReconciliationDecision::Settle,
        RecordObservation::IntendedTargetAbsentAfterDispatch
        | RecordObservation::ConflictingRecordOrGeneration
        | RecordObservation::Unavailable => ReconciliationDecision::RetainUnknownFence,
    }
}
