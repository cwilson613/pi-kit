# memory-branch-aware-facts-transport-impl — Tasks

## 1. startup import still seeds live memory from tracked transport

- [ ] 1.1 startup import seeds an empty or stale DB
- [ ] 1.2 Write tests for startup import still seeds live memory from tracked transport

## 2. tracked facts transport is not rewritten on ordinary session shutdown

- [ ] 2.1 branch-local session work does not dirty tracked transport by default
- [ ] 2.2 Write tests for tracked facts transport is not rewritten on ordinary session shutdown

## 3. memory transport can be exported explicitly

- [ ] 3.1 explicit export writes deterministic tracked transport
- [ ] 3.2 Write tests for memory transport can be exported explicitly

## 4. memory transport drift is reported separately from lifecycle artifact blockers

- [ ] 4.1 incidental memory drift does not masquerade as a lifecycle-doc failure
- [ ] 4.2 Write tests for memory transport drift is reported separately from lifecycle artifact blockers
