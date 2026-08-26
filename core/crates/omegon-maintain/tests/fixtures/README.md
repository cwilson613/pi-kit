# Release verifier fixture

`release-verifier-v1.tar.gz.b64` contains the exact archive, canonical package
manifest, and Sigstore bundle produced by the tag-bound `release.yml` fixture
job. The outer base64/gzip/tar container exists only so the binary evidence can
be reviewed and stored portably; tests verify the three signed inner operands.

- Workflow run: https://github.com/styrene-lab/omegon/actions/runs/32194265720
- Tag: `v0.29.0-dev-fixture.1`
- Commit: `d4ee9a6bfd500052fb52419e87af7b750321b35f`
- Cosign: `v3.0.6`
- Archive SHA-256: `77b590261b59f46d00abdb9f617e5bd460b0900a08263aefc69af94b4c9f4528`
- Manifest SHA-256: `bdbf81934318e8aa3a0d2bc9d463340b741bdda5b5c75f8a395fb29ff930eee8`
- Bundle SHA-256: `967618a1150859159356e448870bd595225e426607da576e70e10719a8c955a0`
