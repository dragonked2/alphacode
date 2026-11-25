# Windows code signing (SignPath Foundation)

Signing is the last structural input to Windows reputation remaining after the
1.1.22 fixes. `docs/engineering/windows-av-false-positive.md` closes with
"unsigned binaries... the remaining human step"; this document is that step.

## Status

| Item | State |
|---|---|
| SignPath Foundation OSS application | **accepted** |
| SignPath account + organization | **done** (`f351ad44-52ab-4630-9179-ad04db6dd730`) |
| Project `alphacode` (slug `alphacode`) | **VALID**, repo URL set |
| Signing policies | `test-signing` **VALID**; `release-signing` **CSR PENDING** |
| Artifact configuration `windows-release` | **created, VALID, default** |
| CI user + API token | **created**, token in `SIGNPATH_API_TOKEN` |
| GitHub.com trusted build system | linked to the project |
| Repository variables | `SIGNPATH_ORGANIZATION_ID`, `SIGNPATH_PROJECT_SLUG` set; policy var **intentionally unset** |
| Production certificate | **pending SignPath** (CSR PENDING) -- not something the repo can finish |
| First signed release | blocked on the production certificate |
| API signing request, small artifact | **accepted (HTTP 201)** -- token, slug and config are proven correct |

The production certificate is the blocker for a *signed* 1.1.22. Until SignPath
imports it, `release-signing` is INVALID and the release pipeline publishes
unsigned (with a warning). Do **not** set `SIGNPATH_SIGNING_POLICY_SLUG=test-signing`
to force a signature: the pipeline refuses that value, and a test signature is
not trusted by Windows.

### Tooling limitation observed while validating

Submitting the 32 MB release archive to SignPath from the maintainer's Windows
machine fails after the body is fully sent: curl reports `sent=<full size>
recv=0` and then times out (`schannel` and OpenSSL/Python paths both stall;
chunked encoding stalls too). A ~250-byte artifact submitted from the same host
succeeds in under a second (HTTP 201), and cross-checking the SignPath API shows
the large request never reached the server. That isolates it to large multi-MB
TLS uploads from this network, not to the API shape, token, slugs, or artifact
configuration. GitHub-hosted runners do not share that network path, so the
pipeline is unaffected; it just could not be exercised locally against the real
32 MB archive.

## Portal setup

The Project you created is empty. Five things go in it. (Menu labels track the
current SignPath portal; if a label has moved, the SignPath docs at
<https://docs.signpath.io> are the authority and this list still names the
object you are creating.)

### 1. Signing policies

Under the project, add the signing policies the project will expose. You need
at least two:

- `test-signing` - uses the self-signed test certificate SignPath provides. Use
  this only for the manual/local rehearsal below; a release never uses it.
- `release-signing` - the policy that will use the production OV certificate.
  This is the one the release pipeline signs with.

`test-signing` exists so the whole loop can be exercised before the production
certificate arrives. It is deliberately rejected by the release pipeline: a
test-signed binary shows "Unknown publisher" exactly like an unsigned one, so
shipping it would gain nothing while looking signed.

### 2. Artifact configuration

**Artifact Configurations → Add → Custom**, then paste the contents of
`scripts/signpath/artifact-configuration.xml`. Name it `windows-release`
(that slug is what `-ArtifactConfigurationSlug windows-release` refers to) or
leave it as the project default and omit the slug at call sites.

The configuration is a `<zip-file>` root with two `<pe-file>` entries,
`alphacode.exe` and `AlphaCode.Desktop.exe`, each `<authenticode-sign/>`. It
matches the exact two-file layout the Windows release archive has had since
1.1.22 (see `resolve_desktop_host` and the packaging step in
`.github/workflows/release.yml`).

> If the portal offers **Update from an artifact sample**, you may instead
> upload `dist/alphacode-windows-x86_64.zip` and let SignPath generate the
> configuration, then replace the generated paths with the explicit two-file
> version. The explicit list is deliberate: a future change that re-embeds or
> adds a binary fails the signing request rather than silently signing the wrong
> artifact.

### 3. Trusted build system (GitHub)

SignPath → the organization's **Trusted Build Systems** → add **GitHub.com**
(predefined). Link it to this project. This is what lets the connector verify
that a signing request came from a real GitHub Actions run instead of anyone
holding the API token.

Then install the **SignPath GitHub App** (<https://github.com/apps/signpath>)
and grant it access to the `dragonked2/alphacode` repository. This is required
for origin/audit-log evaluation for OSS projects.

### 4. CI user and API token

Create a CI user in the organization (SignPath's docs describe the dedicated
"CI user" flow) and give it **submitter** permission on the project. Generate
an API token for it. That token is the one value that becomes the
`SIGNPATH_API_TOKEN` repository secret; nothing else in this list is secret.

Watch for the second welcome email: SignPath sends an invitation to the OSS
organization *and* a confirmation request for the CI user's email address. You
must accept the organization invitation **before** confirming the CI user
address, or the confirmation fails.

### 5. Repository configuration on GitHub

Settings → Secrets and variables → Actions:

| Kind | Name | Value |
|---|---|---|
| Secret | `SIGNPATH_API_TOKEN` | the CI user's API token |
| Variable | `SIGNPATH_ORGANIZATION_ID` | `f351ad44-52ab-4630-9179-ad04db6dd730` |
| Variable | `SIGNPATH_PROJECT_SLUG` | the project slug shown in the portal URL |
| Variable | `SIGNPATH_SIGNING_POLICY_SLUG` | the **production** policy (`release-signing`) once SignPath issues the certificate. Leave it unset until then: a release is never signed with `test-signing`, and the pipeline refuses to ship a test signature. |
| Variable | `SIGNPATH_ARTIFACT_CONFIGURATION_SLUG` | optional; names the artifact configuration if you do not use the project default |

The slug is whatever the portal shows after `/Web/<org-id>/Projects/`. It is not
guessed here on purpose.

## Test the loop with the self-signed certificate

### Option A: locally, without GitHub

```powershell
$env:SIGNPATH_API_TOKEN        = '<token>'
$env:SIGNPATH_ORGANIZATION_ID  = 'f351ad44-52ab-4630-9179-ad04db6dd730'
$env:SIGNPATH_PROJECT_SLUG     = '<project-slug>'
./scripts/signpath/submit-signing-request.ps1 `
    -ArtifactPath dist/alphacode-windows-x86_64.zip `
    -SigningPolicySlug test-signing
```

This calls SignPath's REST API directly (`SubmitWithArtifact`, then polls
`Status`, then downloads `SignedArtifact`). It writes
`dist/alphacode-windows-x86_64-signed.zip` and prints the
`Get-AuthenticodeSignature` status of both binaries inside it. Under the
self-signed test certificate the status is not `Valid` because the chain is
untrusted; a non-empty signer is the success condition at this phase.

### Option B: the release pipeline

Signing is a set of steps inside `.github/workflows/release.yml`, between
**Reject nested-executable layouts** and **Upload Windows release assets**.
Every one of them is gated on `SIGNPATH_API_TOKEN`, so a release built before
the token exists publishes an unsigned archive exactly as before.

Once the secret and variables are set, push a version tag. The pipeline will:

1. build and structurally verify the unsigned Windows archive;
2. upload it as a GitHub Actions artifact and submit it to SignPath through the
   GitHub connector (`scripts/signpath/artifact-configuration.xml` signs both
   `alphacode.exe` and `AlphaCode.Desktop.exe`);
3. re-run the structural guards on the *signed* bytes;
4. assert an Authenticode signature is present on both binaries;
5. replace the archive with the signed one and recompute its `.sha256`;
6. publish it and merge `SHA256SUMS` from the recomputed sidecar.

Because `install.ps1` verifies the download against the sidecar and `SHA256SUMS`
and hard-fails on a mismatch, the recompute in step 5 is what keeps the signed
archive installable; it is not optional.

Rehearse with Option A first: it exercises the same SignPath project, artifact
configuration and signing policy without touching the release pipeline.

## After the production certificate is imported

Set the `SIGNPATH_SIGNING_POLICY_SLUG` variable to `release-signing` (or pass
`-SigningPolicySlug release-signing` to the local script), and confirm
`Get-AuthenticodeSignature` reports `Valid` with a public-CA signer.

## One code change signing required

`scripts/verify-pe-metadata.py` previously treated *all* bytes past the last
section as packer overlay. An Authenticode signature is exactly that: the
certificate table is a file offset (data directory 4) appended after the last
section. Left unfixed, every correctly signed binary would have failed the
guard.

The script now reads directory 4 and subtracts the certificate table before
measuring overlay, so genuine overlay still fails while a valid signature
passes. This is verified two ways:

- `python scripts/verify-pe-metadata.py --self-test` builds synthetic PE images
  and pins the behaviour in both directions (signed passes, real overlay fails,
  unsigned-plus-overlay fails, missing VERSIONINFO fails).
- A real Microsoft-signed binary (`C:\Windows\System32\curl.exe`, certificate
  table 10,056 bytes at offset 834,048) passes; before the fix its 10,056
  trailing bytes were reported as overlay.

## Limits

- The GitHub connector requires that every job leading up to the signing request
  run on GitHub-hosted runners for OSS projects. `release.yml` builds Windows on
  `windows-latest`, so this holds; a self-hosted runner would break it.
- `AlphaCode.Desktop.exe` is a .NET/WPF host that embeds `gui.ps1`. Signing it
  is correct and expected; it is a sibling of the CLI, not an embedded payload.
- The signing steps run in the same job that builds the archive, before the
  asset is uploaded, so a signed release has no unsigned window. This could not
  be exercised before the SignPath account was configured; the first tagged
  release after the token is set is the real test of the path.
- Signing adds bytes past the last section (the certificate table), so the
  archive hash changes on every re-sign. Anything that pins a release archive
  hash (a downstream package, a mirror) must be refreshed after signing.
