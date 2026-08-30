# Native extension conformance campaign design

## Dependency

Implement `repair-extension-composition-evidence` first. The conformance campaign
must build on green composition evidence rather than encode current contradictions.

## Production path

The harness will use production discovery, snapshotting, trust admission,
handshake, readiness, supervisor, and host invocation paths. It must not use the
test permit that bypasses production admission. A deterministic fixture extension
will expose controlled delay, crash, child-process, and tool-shape behaviors.

## Shared protocol contract

Each first-party native extension must advertise a supported SDK version, answer
the required initialization and tool-discovery requests, accept applicable
bootstrap data, expose readiness, and stop on EOF or host shutdown. Domain RPC
methods remain extension-specific.

## Real codescan acceptance

Codescan will run the same generic lifecycle campaign plus a domain assertion:
index a temporary workspace through the host adapter, search it, and return the
expected source hit. The test must prove the host result came from the admitted
sidecar instance.

## Failure and cleanup

Cancellation after dispatch, transport loss, replacement, restart-budget
exhaustion, stale handles, and shutdown are observable outcomes. Unix tests must
prove descendant process settlement. Other targets must run the strongest
platform-specific ownership assertion available without weakening Unix coverage.
