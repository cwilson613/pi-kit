# Omegon release procedure

This is the canonical operator and agent procedure for Omegon releases.

## The short version

Omegon has two public channels:

- **Stable** is the highest published stable tag, such as `v0.29.5`.
- **Nightly** is a dated snapshot of `main`.

`main` carries a long-running development-line version. Cargo requires a three-part SemVer version, so the 0.29 development line is encoded in `Cargo.toml` as:

```toml
version = "0.29.0-dev"
```

For humans and product surfaces, read that as **`0.29-dev`**, not as “the upcoming 0.29.0 release.” The `.0` is only Cargo's required patch slot.

A stable release target is chosen when the release is cut:

```text
main development line: 0.29.0-dev  (meaning 0.29-dev)
stable release target: 0.29.5      (chosen at release time)
main after release:     0.29.0-dev  (still meaning 0.29-dev)
```

Patch releases do not advance the development-line version. The line changes only by an explicit project decision, for example from `0.29.0-dev` to `0.30.0-dev`.

## Why the version looks unusual

Cargo package versions must contain `major.minor.patch`. Cargo therefore rejects the literal version we would otherwise prefer:

```toml
version = "0.29-dev" # invalid Cargo SemVer
```

No valid Cargo version can mean “patchless 0.29 development” and also sort newer than every possible `0.29.x` stable release:

```text
0.29.0-dev < 0.29.0 < 0.29.5
```

We do not work around this with fake patch numbers such as `0.29.999-dev`, nor do we claim `0.30.0-dev` before deciding that development has moved to 0.30. Those alternatives merely move or disguise the ambiguity.

Instead, channel and version have separate jobs:

- Stable freshness is ordered by stable SemVer tags.
- Nightly freshness is ordered by build date and commit SHA.
- `X.Y.0-dev` identifies the development line; it is not compared with `X.Y.Z` to decide which channel is newer.

This is one stored version with one deterministic interpretation. There is no second metadata version to keep synchronized.

## Version and channel rules

| Surface | Form | Meaning |
|---|---|---|
| `Cargo.toml` on `main` | `X.Y.0-dev` | Cargo-compatible encoding of the `X.Y-dev` development line |
| Human-facing dev identity | `X.Y-dev` plus SHA/date | Unreleased work on the X.Y line |
| Nightly tag | `vX.Y.0-nightly.YYYYMMDD` | Dated snapshot of `main` |
| Stable tag | `vX.Y.Z` | Immutable stable release selected at release time |
| Feature preview | Branch/PR name plus SHA | Non-channel artifact; never interpreted as stable or nightly |

Rules:

1. `main` owns the active development line and nightlies.
2. Stable tags are immutable and authoritative. If a branch or document disagrees with a tag about a shipped release, the tag wins.
3. A stable tag must be reachable from `origin/main`.
4. Nightly and stable versions are not globally ordered against each other.
5. Patch releases do not change `main` from `X.Y.0-dev`.
6. Moving from `X.Y-dev` to `X.(Y+1)-dev` is an explicit planning decision, not an automatic post-release bump.
7. Feature branches inherit the development-line version from `main`; they do not own version progression.
8. `release/X.Y` branches are reactive stabilization tools, not the routine release path or an operator-facing channel.

## Routine development

### Main

Normal work lands on `main`. While the project is on the 0.29 development line, `Cargo.toml` remains:

```toml
version = "0.29.0-dev"
```

Do not increment it after each `0.29.x` release. Do not infer the next stable target from it.

### Long-running feature branches

A long-running branch keeps the version it inherited. Before merging, synchronize with `main` and resolve version files in favor of `main`.

Do not bump versions on feature branches merely because stable releases occurred while the branch was open. Commit SHA and branch identity distinguish feature builds.

If preview artifacts are needed, identify them with branch/PR and SHA. They must not use stable or nightly tags and must not enter either automatic update channel.

### Moving to a new development line

Change `X.Y.0-dev` only when the project deliberately opens a new minor or major line. This is a planning decision with its own commit and changelog explanation where operator behavior changes.

Examples:

```text
0.29.0-dev -> 0.30.0-dev  # deliberately open 0.30 development
0.30.0-dev -> 1.0.0-dev   # deliberately open 1.0 development
```

This transition is independent of the most recent patch tag.

## Nightly procedure

Nightly is a snapshot of `main`; it does not select or promise a stable patch.

The nightly workflow:

1. Reads `X.Y.0-dev` from `Cargo.toml`.
2. Derives the `X.Y` development line.
3. Creates `X.Y.0-nightly.YYYYMMDD` for the dated artifact.
4. Stamps that version only in the nightly release commit/tag context.
5. Publishes the nightly tag.
6. Leaves `main` at `X.Y.0-dev`.

A nightly should be displayed to operators as something equivalent to:

```text
omegon nightly 0.29-dev (abcdef1 2026-07-26)
```

The date and SHA order and identify nightlies. Do not claim that `0.29.0-dev` is semantically newer or older than stable `0.29.5`; they belong to different channels.

If today's nightly tag already exists, the workflow skips rather than mutating the tag.

## Stable release procedure

### 1. Choose the release target

Choose `X.Y.Z` from the actual release scope. The trunk development-line version does not constrain the patch number.

Examples while `main` is `0.29.0-dev`:

```text
0.29.1  patch release
0.29.5  later patch release
0.30.0  minor release, if scope warrants it
1.0.0   major release, if scope warrants it
```

If releasing a new minor or major line, decide separately whether `main` should remain on the old development line or move to the new one after publication.

### 2. Prepare release memory

Before mutation:

- Ensure `CHANGELOG.md` has a complete section for the exact `X.Y.Z` target.
- Do not rewrite an already-published release section to match a branch. Compare it with the immutable tag and preserve tag truth.
- Ensure OpenSpec/design/task state relevant to the release reflects reality.
- Confirm the working tree is clean apart from intentional release files.

### 3. Run preflight and quality gates

The release procedure must pass:

```bash
python3 scripts/release_preflight.py --release-version X.Y.Z
cargo test --workspace --locked
cargo clippy -p omegon --all-targets -- -D warnings
```

Use the project recipes where they provide these gates. A focused test run is not a substitute for the full release gate.

### 4. Stamp the release commit

Temporarily change the Cargo workspace version from `X.Y.0-dev` to `X.Y.Z`, refresh `Cargo.lock`, and update release milestone state. The release commit, tag, and built binary must all report the same stable version.

Create the immutable tag:

```text
vX.Y.Z
```

Never move a pushed stable tag. If an unpublished tag is defective, supersede it with the next patch and document why.

### 5. Publish

Push the release commit first. Then verify that the stable tag's commit is reachable from `origin/main`. Push the tag separately so CI receives an unambiguous tag event.

CI builds and signs distributable artifacts. Local signing is workstation validation, not the source of published binaries.

### 6. Restore the development line

After publishing an `X.Y.Z` patch release, restore `main` to the existing line encoding:

```text
X.Y.Z -> X.Y.0-dev
```

Do **not** calculate `X.Y.(Z+1)-dev`.

If the project explicitly decided to open another line, restore to that chosen line instead:

```text
0.29.5 -> 0.30.0-dev
```

The restoration commit is part of release completion and must be pushed. It is not an optional follow-up.

### 7. Verify publication

Confirm:

- The GitHub release exists for `vX.Y.Z`.
- Expected platform artifacts and the release manifest are present.
- Homebrew/site/downstream workflows succeeded where applicable.
- `origin/main` contains the tag commit and the development-line restoration commit.
- The public changelog matches the immutable tag's release history.

## Reactive stabilization branches

Use a temporary stabilization or `release/X.Y` branch only when `main` cannot safely produce the required release directly.

Procedure:

1. Branch from the intended release base.
2. Apply only stabilization changes.
3. Validate the branch.
4. Merge it forward into `main`.
5. Tag only after the release commit is reachable from `origin/main`.
6. Retire the branch when its patch window closes.

A release branch does not become a third public channel. Stable still resolves to tags; nightly still resolves to `main`.

## Failure and disagreement handling

### Tag disagrees with a branch or changelog

The immutable tag is authoritative for what shipped. Correct mutable branch documentation to match the tag. Never rewrite the tag to match later branch state.

### A tag was pushed but cannot publish

Do not force-move it. Fix the release machinery, increment the patch, and document that the new release supersedes the unpublished tag.

### Nightly looks older than stable by SemVer

Expected. Compare nightly by timestamp/SHA and stable by SemVer. Channel selection is explicit.

### Feature branch has an old development version

Synchronize with `main` before merge and take `main`'s version files. Do not independently advance the branch version.

### Release target differs from `X.Y.0-dev`

Expected. The stable target is selected from scope at release time. Validate compatibility and changelog intent; do not infer the target mechanically from the development-line encoding.

## Agent checklist

Before changing release state, an agent must:

- Read this document, `Cargo.toml`, `CHANGELOG.md`, the relevant tag, and current release workflows.
- Distinguish observed repository behavior from proposed policy.
- Treat stable tags as immutable evidence.
- Avoid changing unrelated dirty files or concurrent work.
- Run the full release gates before tagging.
- Verify binary, tag, and tagged-source versions agree.
- Restore the chosen `X.Y.0-dev` development line after stable publication.
- Report workflow URLs and exact blockers rather than repeatedly relaunching unchanged jobs.

## Human checklist

For a routine stable release:

1. Decide the stable `X.Y.Z` from actual scope.
2. Complete the exact changelog section.
3. Run preflight, full tests, and clippy.
4. Stamp and tag `X.Y.Z` from trunk.
5. Publish and watch CI.
6. Restore the deliberate development line (`X.Y.0-dev`, not next-patch dev).
7. Verify artifacts, downstream workflows, and changelog truth.

For normal development and nightlies: leave `Cargo.toml` at the active `X.Y.0-dev` line encoding.

## Implementation status

The release tooling now implements this policy. `development-line-version`
derives `X.Y.0-dev` from any stable `X.Y.Z`, and `just publish` restores that
same development-line identity after publishing instead of inventing
`X.Y.(Z+1)-dev`.

The nightly workflow derives `X.Y` from `Cargo.toml`, stamps a dated
`X.Y.0-nightly.YYYYMMDD` tag, and does not alter `main`.
