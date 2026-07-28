# Maintainer continuity

AVPact currently has one repository owner and one release-capable maintainer,
`@yhay81`. This is an explicit continuity risk, not evidence of a second-person
review or a promise that private project state can be recovered.

## Public recovery boundary

The monthly `Maintainer continuity drill` workflow and
`scripts/continuity-drill.sh` prove that a person with no repository write
credential can recover and validate:

- a complete public Git mirror with a successful `git fsck`;
- every published `v*` tag against signing subkey fingerprint
  `0C153FFE2B0274365ACB1BF1AEFA86FA828C52C5`;
- every asset in the latest release against `SHA256SUMS`;
- GitHub build-provenance and CycloneDX SBOM attestations for the native
  archive selected for the drill host;
- the released native binary by extracting it and running `avpact --version`.

The script downloads the public key from the maintainer's GitHub identity but
accepts it only when the pinned signing fingerprint is present. A changed or
missing key fails closed.

Run the same drill locally from an authenticated GitHub CLI session:

```bash
GH_TOKEN="$(gh auth token)" \
  ./scripts/continuity-drill.sh yhay81/avpact avpact
```

The token needs public read access only. The script uses a dedicated temporary
directory and deletes it when the drill ends. The automated monthly drill runs
on Linux x86_64; local macOS drills select arm64 or x86_64 to match the host.

## State that public recovery does not restore

The drill cannot recover or transfer:

- control of the `yhay81` GitHub account or repository administration;
- private vulnerability reports, Actions secrets, environment approvals, or
  pending embargoed fixes;
- the maintainer's private signing key;
- package-registry ownership or credentials.

Loss of the private signing key requires a new key. A release manager must
publish the old and new fingerprints in a reviewed governance change, revoke
the old key when possible, update this document, and publish only a new patch
version. Existing tags and releases remain immutable.

Loss of repository control is not solved by a public clone. GitHub account
recovery must be attempted first. If control cannot be recovered, the public
mirror and verified releases permit a clearly named fork, but the fork must not
claim continuity of repository identity, private reports, or registry
ownership.

## v1.0 gate

Before v1.0, either:

1. a second trusted maintainer receives repository administration and release
   authority and independently exercises the release and incident runbooks; or
2. the single-maintainer risk remains prominently disclosed, this public drill
   is green, and repository-account plus package-registry recovery are tested
   and recorded without storing secrets in Git.

Until one path is evidenced, maintainer continuity remains an open v1.0 gate.
