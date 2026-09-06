# Token-budgeted retained context

## Intent

Complete ROI rank 4 from the OpenCode2 comparison: recent large messages can remain
inside every age-based retention window and prevent useful compaction.

## Scope

Bound retained conversation estimates using the existing assembly policy. Preserve
complete turns and connected tool exchanges, carry prior summaries forward, and
make loop and manual application honor the selected boundary. Retain existing
summary generation, session authority, and provider overflow recovery owners.
Exact tokenization, active-turn truncation, durable instruction refresh, and new
configuration surfaces are outside this pass.

## Success criteria

- A test reproduces oversized recent history receiving no useful age-based plan.
- A budgeted plan evicts older complete turns until its retained estimate fits,
  except when the protected newest turn or connected tool exchange itself exceeds it.
- Every caller applies the selected window and repeated summaries retain prior context.
- Focused regressions and appropriate Rust landing gates pass.
