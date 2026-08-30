# Repair extension composition evidence

## Intent

Restore trust in the composition and optional-domain gates after codescan moved
from an in-process service to a release-coupled native extension.

## Scope

Align runtime identity, release fixtures, archive inventory validation, optional
domain evidence, and required CI tests. This change repairs current evidence. It
does not add a generic extension harness or new artifact profiles.

## Success criteria

- Source, linked, and release composition checks agree on codescan ownership.
- A normal archive containing the codescan sidecar passes positive and negative inventory tests.
- Optional-domain isolation describes and verifies the current extension boundary.
- Composition and release-script Python tests run as required pull-request gates.
