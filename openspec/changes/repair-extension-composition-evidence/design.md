# Repair extension composition evidence design

## Evidence authority

Runtime composition output and packaged artifact metadata are authoritative.
Fixtures must match those sources and must not preserve retired in-process owner
names. The host adapter and the codescan sidecar remain distinct artifacts.

## Archive inventory

Release archive validation will allow only declared root binaries, locks, content
pack assets, and exact release-coupled extension members. Tests will start with a
valid archive and mutate one property at a time. Missing, duplicate, misplaced,
or unexpected members must fail with the offending path.

## Optional-domain proof

The proof matrix will describe codescan as a native extension. It will name
current absence and degradation tests. The gate will execute those tests or a
repository-owned command rather than only checking that function names exist.

## CI order

Fast Python policy tests run before release-only workflows. Tag workflows remain
defense in depth, not the first place that composition contradictions appear.
