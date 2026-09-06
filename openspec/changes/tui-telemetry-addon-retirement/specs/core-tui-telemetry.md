# Core TUI telemetry retirement — Delta Spec

## ADDED Requirements

### Requirement: Core layouts allocate space to actionable information

Inline and fullscreen reserve no rows for decorative inference/tools panels or
engine glyph compositions. Full detail controls evidence density, not decorative
telemetry. Essential model/context and operation state uses compact text or the
existing on-demand views.

#### Scenario: Both default entries reclaim instrument space
Given om inline/Active and omegon fullscreen/Full without addons
When the operator opens and uses each entry
Then no inference or animated tools instrument panel is mounted
And the freed rows are available to the conversation or active workspace
And send, cancel, permission, and failure controls remain visible at supported geometry

#### Scenario: Full detail does not restore retired instruments
Given either terminal base and a legacy profile requesting instruments
When the operator selects Full detail
Then detailed conversation evidence remains available
And retired visualizations do not consume core rows
And legacy configuration receives an explicit compatibility explanation

### Requirement: Retirement removes exclusive core maintenance costs

Delete renderer-only telemetry buffers, animation state, sampling, effects, and
scheduling dependencies that have no remaining core consumer. Keep domain facts
used by runtime decisions, logs, ACP, or other clients under their existing owners.

#### Scenario: An idle core TUI does not schedule decorative instrument work
Given no active work or pending interaction
When the render scheduler evaluates the next frame
Then no retired instrument timer or effect requires a frame
And no retired visualization buffer is updated from agent events

### Requirement: Optional telemetry belongs to an explicit future addon boundary

A future telemetry addon must opt in through the existing extension lifecycle,
consume bounded semantic snapshots/events, render only within allocated regions,
and use host-mediated actions. It must not own stdout, terminal modes, raw input,
or a second conversation source. This change records that boundary without
implementing a speculative addon framework.

#### Scenario: Addon absence leaves no reserved UI or dependency
Given no telemetry addon enabled
When the core harness launches
Then no addon placeholder, telemetry region, timer, or visualization dependency is required
And the optional capability remains documented as future work

#### Scenario: A future addon cannot seize terminal ownership
Given an enabled addon that wants a telemetry surface
When the host grants rendering space
Then the host retains buffer, input, navigation, and output ownership
And addon failure or overload cannot block authoritative completion or cancellation
