# Retention design

Use the existing managed planner and `KeepRecent(u32)` application contract.
Snapshots carry an optional retained token target; complete numeric turns define
candidate boundaries. Tool IDs widen a retained boundary when necessary to keep
calls/results together. Keep the newest populated turn within the primary age window, including restored history
that shares one turn, even when it exceeds the target. Report the exception rather
than silently removing active work. Earlier user requests can be summarized. The loop advances its turn counter before
planning, so an empty current turn cannot displace protection of the newest
populated recent turn. Entirely old idle history can still be summarized.

`ContextManager` derives the target from `SelectorPolicy::assembly_budget()`.
Reserve conservative system space using the larger of the existing one-fifth
assembly heuristic and known base/injection bytes, then reserve reply-sized
summary headroom. These are local estimates; generated summaries have no separate
hard output cap. Do not reorder prompt preparation or mutate injection TTLs merely
to estimate a budget. Manual paths refresh policy from shared settings.

Reuse the existing LLM message conversion and token estimator. No tokenizer or
configuration dependency is introduced. Planning and application use the same
window; the immutable snapshot is held under existing exclusive runtime ownership.
Prior summary content enters the new payload so repeated compaction does not lose
an earlier summary. Existing provider overflow recovery remains responsible for
requests whose indivisible active context or generated summary still exceeds capacity.

## Authoritative boundary alignment

Review reproduced a mismatch between canonical message counts and the latest
prepared request's context items, especially after an earlier summary. That request
can also precede tool results currently present in the authoritative draft.

Admission therefore derives the current authoritative context. The plan carries
its original message sequence and prior summary; admission checks ordered semantic
alignment before writing a compaction event or sending a provider request. Ignore
only transport/display metadata (raw provider envelopes, image source paths, and
argument summaries), not text, images, calls, IDs, results, or errors. Incompatible
mixed/merged projections fail explicitly; counts and suffix matching cannot infer
identity safely. These failures do not trigger destructive local decay.

Compaction manifests refer to current items' original semantic source events and
canonical message blobs. The retained-item reader validates those sources and
bytes, while preserving the old request/ordinal reader for existing logs. This
uses existing durable contracts and carries all post-request context into the
replacement. It avoids a separate schema or fabricated request-prepared fact.

Restored messages can have non-monotonic turn numbers. Candidate windows are
eligible only when every evicted message precedes every retained message. Legacy
unbudgeted plans record whether their selection is a prefix; durable admission
rejects nonprefix selections. Until mixed-lineage replacement can preserve its
legacy base, durable admission also requires full semantic lineage. Old retained
records still use their existing replay path.
