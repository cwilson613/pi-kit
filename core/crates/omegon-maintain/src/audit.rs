use omegon_maintenance_contracts::{
    AuditCheckpointV1, AuditRecordV1, AuthorityKey, ContractError, InstallationStateV1, LockMode,
    MaintenanceResultV1, ProtocolLock, ResultStatus, Severity, canonical_digest,
    open_secure_dir_at, parse_record, read_bytes_at, read_record_at,
};
use std::{thread, time::Duration};

use super::{AuditCommand, Context, diagnostic, fail};

const AUDIT_SEGMENT_RECORDS: u64 = 100_000;
const MAX_AUDIT_RECORDS: usize = AUDIT_SEGMENT_RECORDS as usize;
const AUDIT_PAGE_RECORDS: usize = 1_000;
const MAX_AUDIT_SEGMENT_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
struct SegmentAnchor {
    first_sequence: u64,
    tail_sequence: u64,
    tail_digest: AuthorityKey,
}

pub(super) fn execute(command: &AuditCommand, context: &Context, result: &mut MaintenanceResultV1) {
    if let Err(message) = execute_inner(command, context, result) {
        fail(result, "audit_invalid", "audit", true, &message);
    }
}

fn execute_inner(
    command: &AuditCommand,
    context: &Context,
    result: &mut MaintenanceResultV1,
) -> Result<(), String> {
    let Some(maintain) =
        open_secure_dir_at(&context.home.file, b"maintain").map_err(|error| error.to_string())?
    else {
        return empty_audit(result);
    };
    let Some(root) = open_secure_dir_at(&maintain, b"v1").map_err(|error| error.to_string())?
    else {
        return Err("maintenance state is partially initialized without v1 root".into());
    };
    let locks = open_secure_dir_at(&root, b"locks")
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "maintenance state lacks lock directory".to_string())?;
    let _audit_lock = acquire_audit_lock(&locks, context)?;
    let Some(audit) = open_secure_dir_at(&root, b"audit").map_err(|error| error.to_string())?
    else {
        return Err("maintenance state lacks audit directory".into());
    };
    let Some(segments) =
        open_secure_dir_at(&audit, b"segments").map_err(|error| error.to_string())?
    else {
        return Err("maintenance state lacks audit segments directory".into());
    };
    let installation: InstallationStateV1 = read_record_at(&root, b"state.json")
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "maintenance state lacks installation state".to_string())?;
    let checkpoint: AuditCheckpointV1 = read_record_at(&audit, b"checkpoint.json")
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "maintenance state lacks audit checkpoint".to_string())?;
    let checkpoint_frontier = checkpoint
        .last_sequence
        .checked_add(1)
        .ok_or_else(|| "audit checkpoint sequence overflow".to_string())?;
    if checkpoint.installation_uuid != installation.installation_uuid
        || checkpoint_frontier != installation.next_audit_sequence
    {
        return Err("installation state and audit checkpoint disagree".into());
    }

    let (cursor, inspect) = match command {
        AuditCommand::Verify { cursor } => (cursor.as_deref(), false),
        AuditCommand::Inspect { cursor } => (cursor.as_deref(), true),
    };
    let (anchor, upper) = resolve_cursor(cursor, inspect, &installation, &checkpoint)?;
    let records = read_linked_segment(
        &segments,
        anchor,
        checkpoint_anchor(&checkpoint),
        &installation,
        context,
    )?;
    if inspect {
        inspect_page(upper, anchor, &records, &installation, &checkpoint, result)?;
    } else {
        verify_segment(anchor, &records, &installation, &checkpoint, result)?;
    }
    Ok(())
}

fn checkpoint_anchor(checkpoint: &AuditCheckpointV1) -> SegmentAnchor {
    SegmentAnchor {
        first_sequence: segment_first(checkpoint.last_sequence),
        tail_sequence: checkpoint.last_sequence,
        tail_digest: checkpoint.last_digest,
    }
}

fn segment_first(sequence: u64) -> u64 {
    if sequence == 0 {
        1
    } else {
        ((sequence - 1) / AUDIT_SEGMENT_RECORDS) * AUDIT_SEGMENT_RECORDS + 1
    }
}

fn read_segment(
    segments: &std::fs::File,
    anchor: SegmentAnchor,
    installation: &InstallationStateV1,
    context: &Context,
) -> Result<Vec<(AuditRecordV1, AuthorityKey)>, String> {
    if anchor.tail_sequence == 0 {
        if anchor.first_sequence != 1 || anchor.tail_digest != AuthorityKey::from_bytes([0; 32]) {
            return Err("empty audit segment anchor is invalid".into());
        }
        return Ok(Vec::new());
    }
    if segment_first(anchor.tail_sequence) != anchor.first_sequence {
        return Err("audit cursor segment boundary is invalid".into());
    }
    let name = format!("{}.jsonl", anchor.first_sequence);
    let bytes = read_bytes_at(segments, name.as_bytes(), MAX_AUDIT_SEGMENT_BYTES)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("audit segment {name} is absent"))?;
    if context.expired() {
        return Err("deadline expired during audit segment read".into());
    }
    parse_segment(&bytes, anchor, installation, context)
}

fn read_linked_segment(
    segments: &std::fs::File,
    requested: SegmentAnchor,
    mut authoritative: SegmentAnchor,
    installation: &InstallationStateV1,
    context: &Context,
) -> Result<Vec<(AuditRecordV1, AuthorityKey)>, String> {
    loop {
        let records = read_segment(segments, authoritative, installation, context)?;
        if authoritative.first_sequence == requested.first_sequence {
            if authoritative != requested {
                return Err("audit cursor anchor is not linked to the current checkpoint".into());
            }
            return Ok(records);
        }
        if authoritative.first_sequence < requested.first_sequence {
            return Err("audit cursor segment is not linked to the current checkpoint".into());
        }
        authoritative = previous_anchor(authoritative, &records)?.ok_or_else(|| {
            "audit cursor segment is not linked to the current checkpoint".to_string()
        })?;
    }
}

fn parse_segment(
    bytes: &[u8],
    anchor: SegmentAnchor,
    installation: &InstallationStateV1,
    context: &Context,
) -> Result<Vec<(AuditRecordV1, AuthorityKey)>, String> {
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        return Err("audit segment has an incomplete trailing record".into());
    }
    let mut records = Vec::new();
    let mut previous = None;
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        if context.expired() {
            return Err("deadline expired during audit verification".into());
        }
        if line.is_empty() {
            continue;
        }
        if records.len() == MAX_AUDIT_RECORDS {
            return Err("audit segment exceeds the 100000-record verification limit".into());
        }
        let record: AuditRecordV1 = parse_record(line).map_err(|error| error.to_string())?;
        let expected_sequence = anchor
            .first_sequence
            .checked_add(records.len() as u64)
            .ok_or_else(|| "audit record sequence overflow".to_string())?;
        let expected_previous = if records.is_empty() {
            record.previous_digest
        } else {
            previous
        };
        if record.installation_uuid != installation.installation_uuid
            || record.sequence != expected_sequence
            || record.previous_digest != expected_previous
            || (record.sequence == 1 && record.previous_digest.is_some())
            || (record.sequence > 1 && record.previous_digest.is_none())
        {
            return Err(
                "audit record sequence, installation, or previous digest is invalid".into(),
            );
        }
        let digest = canonical_digest(&record).map_err(|error| error.to_string())?;
        previous = Some(digest);
        records.push((record, digest));
    }
    if records.last().map(|(record, _)| record.sequence) != Some(anchor.tail_sequence)
        || previous != Some(anchor.tail_digest)
    {
        return Err("audit segment tail does not match its checkpoint or cursor anchor".into());
    }
    Ok(records)
}

fn verify_segment(
    anchor: SegmentAnchor,
    records: &[(AuditRecordV1, AuthorityKey)],
    installation: &InstallationStateV1,
    checkpoint: &AuditCheckpointV1,
    result: &mut MaintenanceResultV1,
) -> Result<(), String> {
    let continuation_cursor = previous_anchor(anchor, records)?
        .map(|previous| verify_cursor(installation, checkpoint, previous));
    diagnostic(
        result,
        "audit_chain_valid",
        Severity::Info,
        "audit",
        "maintenance audit chain segment is structurally continuous",
        Some(serde_json::json!({
            "installation_uuid": installation.installation_uuid,
            "first_sequence": records.first().map(|(record, _)| record.sequence),
            "last_sequence": anchor.tail_sequence,
            "records_verified": records.len(),
            "continuation_cursor": continuation_cursor,
        })),
    );
    Ok(())
}

fn previous_anchor(
    anchor: SegmentAnchor,
    records: &[(AuditRecordV1, AuthorityKey)],
) -> Result<Option<SegmentAnchor>, String> {
    if anchor.first_sequence == 1 {
        return Ok(None);
    }
    let tail_sequence = anchor.first_sequence - 1;
    let tail_digest = records
        .first()
        .and_then(|(record, _)| record.previous_digest)
        .ok_or_else(|| "audit segment boundary lacks its previous digest".to_string())?;
    Ok(Some(SegmentAnchor {
        first_sequence: segment_first(tail_sequence),
        tail_sequence,
        tail_digest,
    }))
}

fn inspect_page(
    upper: u64,
    anchor: SegmentAnchor,
    records: &[(AuditRecordV1, AuthorityKey)],
    installation: &InstallationStateV1,
    checkpoint: &AuditCheckpointV1,
    result: &mut MaintenanceResultV1,
) -> Result<(), String> {
    if anchor.tail_sequence == 0 {
        return Ok(());
    }
    if upper <= anchor.first_sequence || upper > anchor.tail_sequence.saturating_add(1) {
        return Err("audit cursor page boundary is outside its segment".into());
    }
    let selected: Vec<_> = records
        .iter()
        .rev()
        .filter(|(record, _)| record.sequence < upper)
        .take(AUDIT_PAGE_RECORDS)
        .collect();
    for (record, digest) in &selected {
        diagnostic(
            result,
            "audit_record",
            Severity::Info,
            "audit",
            &format!("audit sequence {}: {}", record.sequence, record.command),
            Some(serde_json::json!({
                "sequence": record.sequence,
                "request_id": record.request_id,
                "command": record.command,
                "outcome": record.outcome,
                "digest": digest,
            })),
        );
    }
    let Some((oldest, _)) = selected.last() else {
        return Ok(());
    };
    let next = if oldest.sequence > anchor.first_sequence {
        Some((oldest.sequence, anchor))
    } else {
        previous_anchor(anchor, records)?.map(|previous| (anchor.first_sequence, previous))
    };
    if let Some((next_upper, next_anchor)) = next {
        result.status = ResultStatus::Degraded;
        result.truncated = true;
        result.next_cursor = Some(inspect_cursor(
            installation,
            checkpoint,
            next_upper,
            next_anchor,
        ));
    }
    Ok(())
}

fn resolve_cursor(
    cursor: Option<&str>,
    inspect: bool,
    installation: &InstallationStateV1,
    checkpoint: &AuditCheckpointV1,
) -> Result<(SegmentAnchor, u64), String> {
    let default = checkpoint_anchor(checkpoint);
    let Some(cursor) = cursor else {
        return Ok((default, checkpoint.last_sequence.saturating_add(1)));
    };
    let fields: Vec<_> = cursor.split(':').collect();
    if inspect && fields.len() == 4 && fields[0] == "audit" {
        let upper = parse_cursor_header(&fields, installation, checkpoint, 1, 2, 3)?;
        if segment_first(upper.saturating_sub(1)) != default.first_sequence {
            return Err("legacy audit cursor cannot cross a segment boundary".into());
        }
        return Ok((default, upper));
    }
    let expected_kind = if inspect {
        "audit-inspect"
    } else {
        "audit-verify"
    };
    if fields.len() != if inspect { 7 } else { 6 } || fields[0] != expected_kind {
        return Err("audit cursor is malformed".into());
    }
    if fields[1] != installation.installation_uuid
        || fields.last().copied() != Some(checkpoint.last_digest.to_hex().as_str())
    {
        return Err("audit cursor belongs to a different installation or checkpoint".into());
    }
    let offset = usize::from(inspect);
    let upper = if inspect {
        fields[2]
            .parse::<u64>()
            .map_err(|_| "audit cursor page sequence is malformed".to_string())?
    } else {
        0
    };
    let first_sequence = fields[2 + offset]
        .parse::<u64>()
        .map_err(|_| "audit cursor segment sequence is malformed".to_string())?;
    let tail_sequence = fields[3 + offset]
        .parse::<u64>()
        .map_err(|_| "audit cursor tail sequence is malformed".to_string())?;
    let tail_digest = fields[4 + offset]
        .parse::<AuthorityKey>()
        .map_err(|_| "audit cursor tail digest is malformed".to_string())?;
    let anchor = SegmentAnchor {
        first_sequence,
        tail_sequence,
        tail_digest,
    };
    Ok((anchor, upper))
}

fn parse_cursor_header(
    fields: &[&str],
    installation: &InstallationStateV1,
    checkpoint: &AuditCheckpointV1,
    uuid: usize,
    upper: usize,
    digest: usize,
) -> Result<u64, String> {
    if fields[uuid] != installation.installation_uuid
        || fields[digest] != checkpoint.last_digest.to_hex()
    {
        return Err("audit cursor belongs to a different installation or checkpoint".into());
    }
    fields[upper]
        .parse::<u64>()
        .map_err(|_| "audit cursor sequence is malformed".to_string())
}

fn verify_cursor(
    installation: &InstallationStateV1,
    checkpoint: &AuditCheckpointV1,
    anchor: SegmentAnchor,
) -> String {
    format!(
        "audit-verify:{}:{}:{}:{}:{}",
        installation.installation_uuid,
        anchor.first_sequence,
        anchor.tail_sequence,
        anchor.tail_digest,
        checkpoint.last_digest,
    )
}

fn inspect_cursor(
    installation: &InstallationStateV1,
    checkpoint: &AuditCheckpointV1,
    upper: u64,
    anchor: SegmentAnchor,
) -> String {
    format!(
        "audit-inspect:{}:{upper}:{}:{}:{}:{}",
        installation.installation_uuid,
        anchor.first_sequence,
        anchor.tail_sequence,
        anchor.tail_digest,
        checkpoint.last_digest,
    )
}

fn acquire_audit_lock(locks: &std::fs::File, context: &Context) -> Result<ProtocolLock, String> {
    loop {
        if context.expired() {
            return Err("deadline expired while acquiring audit lock".into());
        }
        match ProtocolLock::acquire_at(locks, b"audit.lock", LockMode::Shared, false, true) {
            Ok(lock) => return Ok(lock),
            Err(ContractError::Lock(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

fn empty_audit(result: &mut MaintenanceResultV1) -> Result<(), String> {
    diagnostic(
        result,
        "audit_empty",
        Severity::Info,
        "audit",
        "maintenance audit state is absent",
        None,
    );
    Ok(())
}
