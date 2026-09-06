+++
title = "Conversation retention during compaction"
kind = "document"
status = "active"
tags = ["context", "compaction"]
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# Conversation retention during compaction

Compaction summarizes older conversation messages and retains a recent suffix.
Omegon now limits that suffix by estimated tokens as well as message age. Large
recent turns can therefore enter the summary even when the old turn-count window
would have retained them all. Manual, automatic, feature-requested, and overflow
compaction use the same planner and apply its selected boundary.

The target starts with the effective assembly window: the smaller of the model
capacity and requested context class. Existing reply and tool-schema reserves
are subtracted. System context reserves the larger of one fifth of that window
and known base/injection or last-observed prompt size. Another reply-sized reserve
leaves room for the new summary. Subtraction saturates at zero. No new setting is
required.

Selection preserves a chronological suffix of complete numeric agent turns and
keeps tool calls with their
results, including exchanges that cross a candidate turn boundary. The newest populated
recent agent turn remains intact even if it exceeds the target. A returned plan reports
that exception. In a long autonomous run, older user instructions can enter the
summary; the entire run since the last user message is not protected verbatim.
Loaded history sharing the current turn is likewise an indivisible group.

Previous summary text enters the next summary payload. Every successful caller
applies the selected retention window rather than a separate fixed turn count.
Failed summary generation retains the existing recovery behavior.

These are local estimates using existing message conversion and byte-to-token
heuristics. They account for thinking, tool arguments, results, and image data;
they are not an exact provider tokenizer. The summary has no new hard output cap.
An oversized current turn, tool exchange, or generated summary can still require
existing provider overflow recovery. This change does not truncate active messages
or replace the provider recovery policy.

Durable sessions also verify the planner's source messages against current
authoritative context before admitting compaction. Previous summaries and messages
added after the last provider request participate in that current view. If local
and durable projections cannot be aligned, admission fails before compaction
mutation or provider dispatch. This preserves context rather than guessing an
item boundary from message counts. Durable compaction currently requires full
semantic lineage; legacy and mixed-lineage sessions report an admission error and
preserve their context. The existing reader still supports earlier compaction
records whose retained messages reference prepared requests.
