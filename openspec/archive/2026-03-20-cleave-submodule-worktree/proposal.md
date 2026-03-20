# Cleave worktree submodule failures — root cause and fix

## Intent

Security assessment runs showed 2/4 child failures in both cleave runs. All failures were on children whose scope targeted files inside the `core` git submodule. Root cause analysis below.
