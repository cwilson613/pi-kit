# runtime-contributions/lifecycle - Delta Spec

## ADDED Requirements

### Requirement: New extension generations activate at quiescent boundaries

Newly installed extension bytes may be discovered, admitted, staged, and
published without restarting the host, but publication must occur only at a
quiescent runtime boundary. Active work retains its captured generation, stale
handles cannot gain authority in the new generation, and failed staging leaves
the active generation unchanged.

#### Scenario: Candidate arrives during an active turn
Given a session has captured the active contribution generation
And a newly installed extension candidate passes discovery and admission
When activation is requested during the active turn
Then publication remains pending until the runtime is quiescent
And the active turn cannot observe the candidate generation

#### Scenario: A newer candidate supersedes a pending candidate
Given generation A is active and generation B is staged pending quiescence
And admitted generation C arrives for the same contribution
When the lifecycle owner accepts C
Then B is removed from candidate state only after all B-owned resources settle
And C becomes the sole pending candidate without changing A

#### Scenario: Quiescent activation succeeds
Given an admitted candidate generation is fully staged and the runtime is idle
When the supervisor publication coordinator explicitly commits the candidate
Then new work captures the new generation without host restart
And superseded processes and handles settle under bounded lifecycle ownership

#### Scenario: Turn completion does not imply publication
Given a changed generation is pending during an active turn
When that turn closes and another turn is requested before an explicit coordinator commit
Then the pending generation remains hidden and the next turn retains the active generation
And only a later quiescent commit can publish the candidate

#### Scenario: Stale extension authority survives in a caller cache
Given generation B has replaced generation A
And a caller retains an A-bound invocation lease or polling handle
When the caller attempts native RPC through the retained authority
Then the shared generation fence denies dispatch before owner entry
And fresh admission resolves B while aliases and caches cannot revive A

#### Scenario: Candidate staging fails
Given the active generation is healthy and a candidate fails probe or staging
When activation settles
Then the prior generation remains published and callable
And no partial schemas, actions, routes, or processes from the candidate become visible

#### Scenario: Remote cleanup cannot be observed
Given a remote contribution has host-owned transport resources and unobservable remote state
When cancellation, replacement, or shutdown cleanup reaches its deadline
Then every host-owned resource settles within its declared boundary
And remote cleanup is reported as best-effort or unverified rather than strict success
