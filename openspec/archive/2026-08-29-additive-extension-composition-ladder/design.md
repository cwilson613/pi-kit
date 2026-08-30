# Additive extension composition ladder design

## Dependency

Implement `native-extension-conformance-campaign` first. Composition rows will
reuse its host-backed acceptance driver rather than invent another RPC harness.

## Ladder model

The first executable rows are `kernel-only`, `kernel+codescan`, and `full-product`.
`kernel-only` is defined by a positive dependency and resident-capability policy,
not only a forbidden package list. Each later domain extraction adds one row or
extends an accumulated row using the same assertions.

## Functional evidence

Every row must build a distinct artifact or install composition, start it in an
isolated state root, inspect compiled and admitted identity, and execute one
representative operation. Runtime labels over one unchanged binary do not count
as separate artifact rows.

## Absence and restoration

Kernel-only tests assert that optional schemas either remain as typed unavailable
host contracts or are absent by explicit policy. Adding one extension must change
only its declared inventory and behavior. Unrelated kernel behavior must remain
stable.

Each extracted domain declares its canonical service and extension identities.
It maps kernel absence, additive restoration, and accumulated-product retention
to three distinct artifact rows and their machine-readable evidence inventories.
The checker rejects missing or aliased rows and evidence that does not retain the
same identities through the ladder.

## Budgets

Measure host dependency graph, host binary size, each sidecar size, aggregate
installed size, startup tasks, processes, schema size, and callable capabilities.
Target-specific baselines and bounded deltas remain explicit policy data.
