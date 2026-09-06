# Provider connections verification

## Implemented behavior

Startup prints one resolved-route summary and at most one route diagnostic. It no
longer prints the provider inventory. Explicit harness diagnostics retain the
inventory. `/connect` opens existing connections and an Add provider action; the
catalog appears only after that action and uses the existing searchable menu.

Direct setup reuses supported OAuth handlers or hidden API-key input. Local and
externally managed providers receive configuration guidance. API consoles require
an explicit action. `/login` and `/auth login` remain compatible. Remote and ACP
callers receive secure-terminal guidance before authentication can dispatch.
API-key persistence does not promise immediate route replacement; `/model` selects
the route through existing admission.

## Test-first evidence

Behavioral failures were captured before the corresponding fixes:

- Initial connection tests: four failures for menu grouping, command authorization,
  bare `/connect`, and NullBridge guidance (`omegon-connect-red.log`).
- Compact startup projection: two failures (`omegon-connect-stage2.log`).
- Unsupported local/external setup: one failure (`omegon-connect-stage3.log`).
- Whole-store credential parsing regression: failed before separating malformed
  stores from present malformed entries (`omegon-credential-broken-red.log`).
- Abandoned decisions: two failures before authoritative cleanup
  (`omegon-decision-cleanup-red2.log`).
- Wait completion: three failures before tool-call correlation
  (`omegon-wait-end-red.log`).

Focused green runs cover connection state/search, route summaries, hidden input,
cancellation and isolated credential submission, command safety, authoritative
cleanup, and active/queued/stale wait completion. The final gates below supersede
intermediate test counts. Earlier crate runs exposed inherited `NO_COLOR=1`
snapshot mismatches and stale expectations; final Rust gates unset `NO_COLOR` and
`OMEGON_ASCII_GLYPHS` and use the repository's nerd-font setting.

## Captured runtime evidence

Built with `cargo build -p omegon --locked` from clean source revision
`bc372ced5e5ee4fc89cc08d1cc33b68a5bd5007f`. A frozen copy of the binary has SHA-256
`c04b87bb648e4501274a26aea680dec620162963cdc40e9a9545493808e89d4e`.
The evidence directory is the checkout sibling `omegon-connect-evidence-01`.
Temporary runtime homes, credentials, projects and HTTP fixture providers were
isolated. No desktop terminal windows were opened.

| Presentation | Detail | Captures | Local inference requests | Result |
| --- | --- | ---: | ---: | --- |
| Fullscreen | Active | 21 | 4 | Passed |
| Fullscreen | Full | 21 | 4 | Passed |
| Inline | Active | 22 | 4 | Passed |
| Inline | Full | 22 | 4 | Passed |

Each trial used `python3 scripts/tui_acceptance.py --binary <frozen-binary>
--output <new-evidence-directory> --tui <presentation> --ui <detail>`.
Manifests record clean source revision, binary hash, process group and launch
identity, timestamps, geometry, actions and capture hashes. Capture hashes were
verified after completion.

Inspected captures show the compact startup line, configured OpenAI fixture
connection, filtered OpenRouter catalog, masked key entry, cancellation and the
restored presentation. Browsing/search/cancellation made zero inference requests
and did not write an auth file. Each trial subsequently completed two prompts,
opened Project, denied a write, preserved the prior surface, completed the next
response, and returned to the shell with alternate-screen and mouse modes off.
All four owned process groups exited without forced cleanup or cleanup errors.
Draft preservation across connection cancellation is additionally covered in the
shared input tests across both presentations and detail levels.

## Scenario coverage

| Requirement area | Evidence |
| --- | --- |
| Bounded startup, missing/expired/failed/fallback routes | `bootstrap_projection::tests::connect_*`; route-only projection input; four PTY startup captures |
| NullBridge and detailed diagnostic preservation | NullBridge regression; existing bootstrap/status tests and affected snapshot |
| Existing/expired/unreadable versus available providers | `auth_menu_projection` grouping tests; credential-source tests; PTY existing/search captures |
| Shared menu, cancellation and draft/route preservation | `connect_discovery_and_secret_cancel_preserve_drafts_and_routes_in_both_layouts`; four PTY trials |
| Setup aliases, supported methods and external guidance | slash-command connection tests; existing login dispatch tests |
| Hidden input and explicit console | isolated secret-submission test; masking/cancel tests and PTY captures; console remains a separate menu action |
| Remote and ACP safety | registry/control classification, remote bypass denial and ACP secure-interaction tests |
| Authoritative cleanup and stale wait completion | two authoritative-cleanup tests, three correlated ToolEnd tests, seven broader operator-wait tests |
| WezTerm resize ownership | twelve headless native-runner mock tests, including removal ordering and retry ownership |

## Adversarial review

An independent read-only reviewer assessed `/connect` routing, credential
projection and secret handling, remote/ACP authorization, and the inherited
integration corrections. Review findings were fixed: malformed whole stores no
longer populate every connection, raw credential parse errors no longer propagate
secret canaries, and wait completion is correlated by optional tool-call ID.
Final rereview found no remaining code blockers. Web payloads remain unchanged;
the Rust event enum has a source-level field addition, requiring workspace
validation. Producers that know the visible tool-call ID populate `Some(id)`;
legacy producers must explicitly use `None`, and consumers must not infer ownership
from prompt text. In-tree constructors and consumers were updated together.

A separate integration reviewer inspected selected terminal, decision, startup,
permission, publication and cleanup boundaries and implemented the decision and
native cleanup corrections. This was a bounded review, not a fresh line-by-line
audit of all inherited parity changes.

## Final gates

Passed on the final source:

- `env -u NO_COLOR -u OMEGON_ASCII_GLYPHS just test-rust`: complete serialized
  workspace gate, including 5,146 passing main-harness unit tests, ten ignored
  main-harness tests, integration tests, extracted crates and doctests.
- `env -u NO_COLOR -u OMEGON_ASCII_GLYPHS just lint`: formatting, workspace check
  and Clippy across all targets with warnings denied.
- `just test-dev-scripts` and the affected standalone acceptance/operator tests;
  `python3 -m unittest scripts.tests.test_tui_native_acceptance`: twelve passed.
- Site `npm test`: eighteen passed, including the site build. Generated site
  release/statistics outputs were restored after the build.
- Structural OpenSpec validation and `git diff --check`.

The first workspace run had one failure in
`extensions::conformance_tests::crash_budget_quarantines_only_the_failing_extension`:
its healthy fixture did not advertise the expected SDK handshake. The test passed
in isolation without a code change, then passed in the complete workspace rerun.
The cause was not established; no test was disabled or assertion relaxed. Both
runs and the isolated check are retained in the evidence directory. Compiler
output also contains existing linker unwind-size and dependency future-compatibility
notices; the final commands exited successfully.

## Limits and PR scope

These are headless tmux runtime captures and mocked native cleanup tests. They do
not establish fresh compatibility across macOS GUI terminal clients. No real
provider OAuth exchange, browser launch, paid inference or system keychain write
was performed. Credential submission was verified in an isolated subprocess by
intercepting the queued secrets-service action.

The PR to main includes the inherited parity and shared inline/fullscreen project
TUI branch, not only provider connections. It includes instruction discovery, MCP
phase deadlines, reconnect/duplicate-action handling, token-budgeted retention,
terminal ownership and Project navigation. The broader scope was disclosed and
authorized. The future `/login` renewal semantics and optional decorative telemetry
addon remain deferred. No merge is performed by this task.
