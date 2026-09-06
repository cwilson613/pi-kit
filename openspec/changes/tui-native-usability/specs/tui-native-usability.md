# Native TUI usability — Delta Spec

## ADDED Requirements

### Requirement: Readable permission choices
Permission choices appear once, with context separate from action labels. Wrapped
context must not displace the choices at supported compact terminal sizes.

#### Scenario: Narrow permission prompt
Given a long permission target in a 50-column by 18-row terminal
When the permission prompt renders
Then y, a, Shift+A and n choices each appear once and remain fully readable
And the request's actual persistence scope determines the persistent-choice label

### Requirement: Functional project filtering
Project browser search filters the current tab's existing rows without modifying
the conversation draft or implicitly resuming a session.

#### Scenario: Filter and inspect
Given an unsent draft and multiple Project rows
When a search query matches one row
Then only matching rows are visible and Enter inspects that row
And Escape returns through search and browser state without losing the draft

#### Scenario: No matching rows
Given a Project filter with no matches
When Enter is pressed
Then the browser remains visible with an explicit empty result and no action runs

#### Scenario: Covered search state
Given an active Project filter and selection
When a permission decision is resolved above the browser
Then the filter and selected row remain intact

### Requirement: Primary composer action survives narrow layouts
Composer hints fit the available terminal display cells by preserving complete
primary actions before secondary navigation and editing help.

#### Scenario: Narrow composer
Given an idle composer at 40, 56 or 90 columns
When normal, slash-command or shell input is displayed
Then the corresponding send or run hint is completely visible
And the displayed hint text fits the available width
