# Design

Permission prompt bodies contain context, while CommandPrompt actions own choices.
The TUI renderer measures wrapped text and reserves action rows before allocating
space to context, so a long path cannot hide the decision keys. Labels derive from
the actual persistence request; no new permission behavior is introduced.

Project browser input reuses MenuState filtering rather than adding a second search
implementation. Search owns text input; Enter inspects a matching row, never resumes.
Escape first exits search, then detail/browser navigation retains its existing order.
F2 always returns to the preserved composer. Refresh retains filter and stable identity.

Composer help fits whole hints into a display-cell budget, starting with the primary
action. Secondary hints yield to send/run instead of clipping the left edge of a
right-aligned string. Use existing extracted owners, without policy in tui/mod.rs.

Validation uses test-first regressions followed by the omegon crate gate and Clippy.
A new frozen bundle is prepared from the rebuilt source for native acceptance;
previous screenshots remain baseline evidence, never evidence of these repairs.
