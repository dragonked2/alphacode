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
| Production certificate | **not ordered yet** -- SignPath orders it after the setup check passes (see below) |
| First signed release | blocked on a successful test-signing request, then on the certificate |
| API signing request, small artifact | **accepted (HTTP 201)** -- token, slug and config are proven correct |
| Signing requests that succeeded | **none yet** -- the three on record are probes and all `Failed` |

### The certificate is not in a queue -- it has not been ordered yet

This is the part that was previously misread as "waiting on SignPath". The
Foundation program is a handshake, stated by `oss-support@signpath.org` on
2026-11-25:

> Once you're all set, we'll review the setup and **order the actual production
> certificate and import it into your organization.**

and, earlier the same day:

> After the technical setup and **origin verification** have been successfully
> completed, we can then **order and import** the production (release)
> certificate into your subscription.

So `CsrPending` is not SignPath slowly processing a request. It is the
placeholder that stays pending until *they* order the certificate, and they do
that only after the setup is demonstrably working. Waiting on it accomplishes
nothing; the prerequisite is a **successful signing request**.

Checklist state at that point (all verified against the API, not the UI):

| Step | State |
|---|---|
| Organization invitation accepted | done (notification 2026-11-25 11:04) |
| CI user notification address confirmed | done -- `confirmedEmail` set, `status: Active` |
| Project `alphacode` | `Valid` |
| Artifact configuration `windows-release` | `isDefault: true`, matches the repo XML |
| GitHub trusted build system linked | `Active` |
| GitHub Actions integration exercised | **not yet -- every signing request is `Failed`** |
| Origin verification | blocked on the above |
| Production certificate ordered and imported | blocked on origin verification |

The three failed requests on record are a 250-byte probe and two upload probes
of invalid PE data, run to prove token/slugs/configuration. None of them
contained the `alphacode.exe` + `AlphaCode.Desktop.exe` pair the artifact
configuration names, so none exercised the real path.

`.github/workflows/signpath-rehearsal.yml` exists to produce that missing
evidence. It is `workflow_dispatch`-only, runs a single `windows-latest` job
(so the connector's "all jobs on GitHub-hosted runners" rule holds), builds the
same two-file archive the release produces, submits it with `test-signing`, and
then re-runs every structural guard on the signed bytes. It never creates a
release, tags, or publishes anything.

The reason this has to run on CI at all is the upload ceiling measured below:
the maintainer's network cannot send the archive.

## Why the test certificate is not a stopgap

The obvious question while the production certificate is pending is whether the
test certificate can stand in for it. It cannot, for a reason that has nothing
to do with project policy:

**A code-signing certificate either chains to a trusted root or it does not,
and the signing key's location does not change that.** Windows shows the
publisher from the signing certificate only after building a chain to a root in
its trusted store. Anything else reads as "Unknown publisher". A certificate we
generate ourselves, or hire someone to generate, produces exactly that result no
matter where the key lives -- on a hardware token, in a SignPath HSM, or in a
local file. SignPath's own documentation states the same thing about its test
certificates: they "are not created by a commercial CA and are therefore not
trusted by operating systems or browsers."

So the test certificate yields no reputation gain over shipping unsigned. It
buys only the pipeline wiring and the proof that both binaries survive signing,
which are the two things already demonstrated by run `45809064986`. The
`test-signing` refusal in `release.yml` is therefore correct and should stay.

### Adding our own certificate to users' trusted stores

The one genuinely working variant: generate our own CA, sign with it, and add
the root to each user's Trusted Root store via the installer. That makes "Valid"
appear on the machine we modified, and only there.

It is not a substitute for a CA certificate:

- **It is a security downgrade.** It asks every user to trust a certificate
  whose private key we hold. If that key leaks, an attacker can sign *anything*
  -- including malware impersonating other vendors -- and every user who ran the
  installer trusts it. A CA-issued OV certificate carries no such power: the key
  signs only our own files, and revoking it affects only us.
- **Antivirus does not consult the local root store the way you would hope.**
  Engines score an unknown signer against reputation data they already have;
  `CN=Test certificate` with zero history is a *new* signal, not a removed one.
  The specific detections in this release (`TR/Dropper.Gen`,
  `MachineLearning/Anomalous.100%`, `Malicious.moderate.ml.score`) are
  behaviour and structure scores. Signing removes a couple of "no valid
  signature" heuristics; it does not clear those. See
  [windows-av-false-positive.md](windows-av-false-positive.md).
- **It breaks the release checksum contract.** The certificate table is appended
  bytes, so any signing -- ours included -- changes the archive hash. That is
  handled, but it is work done for no trust gain.
- **It teaches users the wrong lesson.** Shipping an installer that silently
  plants a root certificate is what actual malware does. It would be a worse
  reputation signal than being unsigned, and a fair reason for a security
  researcher to distrust the project.

### What is actually in our control

Nobody who is not a CA can make an Authenticode signature trusted. What we can
do, and have done, is remove the things that make an unsigned binary look
hostile:

| Signal | State |
|---|---|
| PE overlay (packer shape) | fixed -- VERSIONINFO is a real resource, not appended bytes |
| Home-directory paths in the PE (2,076 occurrences) | fixed via `cygpath`-mapped `--remap-path-prefix` |
| Known AV trigger strings | guarded by `verify-no-av-triggers.py`, run on signed bytes too |
| Nested/double-zipped archive | fixed -- binaries upload loose |
| Debug-looking metadata | `strip = true`, complete VERSIONINFO |

That is the direct fix for the 11/74 VirusTotal result. A trusted signature
layers on top of it once the certificate lands.

## Rehearsal result: signing works end to end (2026-11-26)

The first fully green run of `signpath-rehearsal.yml` (run `45809064986`,
source `25e21c7`) completed every step:

| Step | Result |
|---|---|
| Build Windows binaries | success |
| Package archive (same layout as the release) | success |
| Structural guards (unsigned bytes) | success |
| Upload unsigned binaries for signing | success |
| Submit signing request | success -- `c65ec8ac-4590-4636-91a8-1272f740b193` |
| Verify signed binaries | success |
| Assert Authenticode signatures are present | success |
| Upload + report | success |

Both signed binaries pass every structural guard *after* signing:

```
OK: signed-check/alphacode.exe has no overlay and complete VERSIONINFO
OK: signed-check/alphacode.exe contains 1 PE image
OK: alphacode.exe contains none of the known AV trigger strings
OK: signed-check/AlphaCode.Desktop.exe has no overlay and complete VERSIONINFO
OK: signed-check/AlphaCode.Desktop.exe contains 1 PE image
OK: AlphaCode.Desktop.exe contains none of the known AV trigger strings
```

and both report a signer:

```
alphacode.exe        : signer=CN=Test certificate for 'Alphacode [OSS]'
AlphaCode.Desktop.exe: signer=CN=Test certificate for 'Alphacode [OSS]'
```

`Get-AuthenticodeSignature` reports `UnknownError` rather than `Valid` because
the test certificate's chain is not trusted by Windows. That is the expected
success condition for the test certificate -- a signer is present, and shipping
it would be pointless, which is exactly why `release.yml` refuses
`test-signing`.

This is the "all set" evidence the Foundation asked for: a real GitHub Actions
build, origin-verified through the connector, signed successfully.

## Double-zip trap: `archive: false` does not do what it looks like

The first real submission through the connector (request `ee6df7d7`) was
accepted, passed the malware scan, then failed in processing:

> Expected path to match exactly 1 item, but found 0. [...] An error occurred
> while processing `rehearsal-unsigned-alphacode-windows-x86_64.zip` as
> specified in line 4, position 6 of your artifact configuration
> (`<pe-file path="alphacode.exe">`).

The upload step had used `actions/upload-artifact@v4` with `archive: false` and
`path: alphacode-windows-x86_64.zip`, on the assumption that this sends the zip
as-is. It does not. **GitHub always stores an artifact inside its own ZIP**, so
the artifact SignPath received was a zip containing one entry:

```
rehearsal-unsigned-alphacode-windows-x86_64.zip   <- GitHub's container = <zip-file> root
  alphacode-windows-x86_64.zip                    <- the archive we uploaded
    alphacode.exe
    AlphaCode.Desktop.exe
```

`<pe-file path="alphacode.exe">` is a root-level match, so it found 0 items.

How this was established rather than guessed: the artifact was downloaded back
from the run and its SHA-256 was **byte-identical** to the `8f379e6d...` that
SignPath recorded for the received artifact, and that download contained exactly
one entry, `alphacode-windows-x86_64.zip`.

**The fix is to upload the binaries loose**, and let GitHub's container be the
single `<zip-file>` root:

```yaml
- uses: actions/upload-artifact@v4
  with:
    name: unsigned-alphacode-windows-x86_64
    path: |
      target/<triple>/release/alphacode.exe
      target/<triple>/release/AlphaCode.Desktop.exe
```

That is exactly what SignPath's onboarding mail means by *"GitHub requires you
to put it in a zip file"*: you do not build the zip, GitHub does.

`skip-decompress: true` in the submit step is unrelated to this -- it controls
how the *signed* artifact is written back out (saved as a zip instead of being
extracted), not how the unsigned one is wrapped.

### This also affects `release.yml`

`release.yml` uses the same pattern (`archive: false` plus the pre-built
`.zip`), so the release pipeline would hit the identical failure the moment a
valid production policy made the signing steps run. It is inert today only
because `SIGNPATH_SIGNING_POLICY_SLUG` is unset, which short-circuits every
signing step. It must be corrected in the same change that turns signing on.

## State re-verified 2026-11-26 (read-only REST API)

Re-checked against the live organization rather than the portal UI, so this is
the server's own answer:

| Object | Server answer |
|---|---|
| `alphacode` project | `status: Valid`, repo URL set |
| `release-signing` policy | **`status: Invalid`** (certificate `CSR PENDING`) |
| `test-signing` policy | `status: Valid` |
| `windows-release` artifact configuration | `isDefault: true`, XML **byte-identical** to `scripts/signpath/artifact-configuration.xml` apart from comments |
| `release-signing` submitters / approvers | CI user `CI builds`; approver `Ali Essam`, `numberOfRequiredApprovals: 1` |
| `release-signing` origin checks | `verifiesTrustedBuildSystem: true`, `verifiesOriginPolicy: true` |
| GitHub trusted build system | linked to the project, `status: Active` |

Nothing in that list is actionable from the repository: the only red item is the
production certificate, which is SignPath's external step.

**Do not pre-set `SIGNPATH_SIGNING_POLICY_SLUG` while `release-signing` is
`Invalid`.** The submit step in `release.yml` has no `continue-on-error`, so a
request against an invalid policy fails the Windows build job and therefore
fails the whole release -- an unsigned archive is *not* published as the
fallback in that case. The gate protects against this only for the
`test-signing` and unset cases, not for "set to a policy that is still invalid".

### Large-upload throughput, measured

The earlier note said a 32 MB submit "stalls". It is a throughput ceiling on
this network path, not an API problem. Measured 2026-11-26 by submitting probe
archives of random bytes (each fails validation afterwards, which is the
expected outcome -- the point was the transfer):

| Artifact size | Result |
|---|---|
| 100 KB | **HTTP 201 in 0.9 s** |
| 1 MB | **HTTP 201 in 48.9 s** (about 20 KB/s) |
| 4 MB | write timeout after 20 s |
| 8 MB | write timeout after 20 s |
| 33 MB (real archive) | write timeout after 20 s |

So the ceiling is the client's send rate, roughly 20 KB/s to `app.signpath.io`,
and anything past a few MB cannot complete before the socket write times out.
Recompressing the archive does not help: the two binaries are already close to
incompressible (33,410,364 bytes from a 33,366,424-byte input).

What this means for this host: the artifact configuration **cannot** be
rehearsed locally against the real archive. The first genuine exercise of it is
the CI run after the certificate is imported, on a GitHub-hosted runner, which
does not share this path.

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
