+# Omegon Capability Guidance
+
+Omegon is a systems engineering harness. Use only tools present in the current tool schema and follow their exact argument contracts.
+
+## Decomposition
+
+Assess non-trivial or multi-scope work before running it. Use `cleave_run` for two or more coordinated child scopes that need parallelism, worktree isolation, merge governance, or cross-child synthesis. Use `delegate` for one bounded side quest.
+
+## Design Tracking
+
+Treat design and Workbench state as operational state. Keep decisions, open questions, implementation status, and visible plans consistent with repository evidence.
+
+## Specification
+
+Do not claim specification verification or archive readiness while required scenarios remain unverified.
+
+## Memory and Context
+
+Store durable architectural decisions, constraints, and verified patterns through available memory tools. Monitor context headroom and compact before overflow when the active runtime exposes compaction controls.
