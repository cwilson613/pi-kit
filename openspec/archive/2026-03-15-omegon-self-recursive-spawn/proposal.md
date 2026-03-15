# Recursive subprocesses must invoke Omegon-owned entrypoint, not bare `pi`

## Intent

Ensure all internal recursive subprocess launches re-enter the Omegon-owned executable boundary explicitly, rather than depending on PATH resolution of the legacy `pi` compatibility alias. Audit cleave and adjacent subprocess sites, then route them through a shared Omegon executable resolver so side-by-side installs cannot escape the self-contained runtime boundary.
