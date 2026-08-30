# Extension distribution runtime parity design

## Dependency

Implement `additive-extension-composition-ladder` first. Distribution tests will
execute its composition rows against installed layouts.

## Two-axis declaration

Each distribution declares both its compiled host artifact profile and installed
composition class for a target and version. The initial host profiles are
`full-product` and `kernel-host-v1`; the initial installation classes are
`full-product`, `kernel-only`, `kernel-plus-codescan-v1`, and `host-only`.
Artifact profile and installation class must not be inferred from one another.

The release policy records signed `core:*` product-component members separately
from independently managed SDK extensions. A host-only distribution must not
advertise full-product capability. Retired surfaces are explicitly unsupported
rather than assigned a misleading runtime class.

## Product-component authority

Codescan is `core:codescan`, a release-authorized product component whose wire
manifest identity remains `omegon-codescan`. Core authority comes from signed
composition evidence; an SDK manifest cannot self-promote into this class.

Generic and first-party independent SDK extensions remain operator-managed and
outside atomic host generation ownership. Provenance and support tier are
orthogonal to product-component class.

## Installed acceptance

The release tarball is the reference installed-layout test. It will be extracted
into a temporary prefix and exercised through the installed launcher or binary.
The host must discover the adjacent core component, run the conformance operation, and
settle the process tree. Other distribution tests reuse the same acceptance
driver when their environment can execute the target artifact.

## Artifact identity

Release-coupled product components require their own component ID, wire manifest
ID, manifest digest, executable digest, protocol range, target, fallback policy,
and signing identity. Component ownership
must not be represented as bytes resident in the host executable.

The component evidence is a separate canonical archive member at
`share/omegon/components/core-codescan.lock.json`. Runtime admission needs this
record after installation, so a package-manifest-only field is insufficient.
The signed package manifest repeats the exact typed record and binds the lock
member digest. The release manifest copies the verified package record for each
asset. Resident host locks remain limited to host-resident contributions.

## Atomic lifecycle

Install and update stage the host and required product-component set before activation.
Rollback restores one internally consistent generation. Operator-managed
extensions remain protected from silent replacement by release-coupled assets.

Profile disablement changes runtime eligibility only. It never changes package
inventory, lock validity, update ownership, or the distribution composition
class. Positive acceptance uses isolated default settings; negative acceptance
may prove a packaged component remains resident but disabled.
