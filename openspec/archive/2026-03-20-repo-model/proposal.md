# RepoModel — git state tracking in Rust core

## Intent

Shared struct initialized at agent startup. Tracks current branch, dirty files (working set), submodule map, and pending lifecycle changes. Updated by edit/write/change tools on every file mutation. Queried by cleave preflight, commit tool, and session-close handler. Replaces all ad-hoc git status calls with a coherent model.
