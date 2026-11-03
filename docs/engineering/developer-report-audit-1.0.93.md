# Developer report audit for 1.0.93

The supplied November 3 report concerns several independent failures. Source fixes and local fixtures do not prove that a third-party service or antivirus verdict has changed.

| Report | Assessment and change |
| --- | --- |
| Alphax Free HTTP 429 | Not established as permanently resolved. Authentication availability is not an inference smoke test. Free-pool cooldown/rotation and bounded retry handling exist; they cannot supply upstream capacity. The available local November 3 logs contain a structured 429 attributed to OpenAI, not an Alphax request-frequency series. That is insufficient to measure Alphax improvement or infer rate limit versus quota versus overload. No live paid inference was initiated in this audit. |
| `write` not allowed | This message is produced by the session tool allowlist, independently of the provider. Both turn paths now record a policy error as the tool result instead of aborting with an unpaired tool call that later becomes “session interrupted.” The error identifies the allowlist; provider fallback is suppressed for policy denials. Intentional allowlists and disabled tools remain enforced. The reporter's actual session configuration was not supplied, so enabling write indiscriminately would not establish the original cause. |
| Bash missing output | Current shell handling avoids Windows Store/WSL bash shims, captures output, and retains tool lifecycle diagnostics. A native `echo DIAGNOSTIC_TEST_OK` execution regression and denied-tool turn regressions cover concrete local paths. The remote reporter's interrupted process and exact configuration are unavailable; these changes do not prove a unique historical cause. |
| `tool_help` not registered | Current normal registry construction registers tool_help, including the progressive-disclosure inventory. Live inventory tests exercise both turn paths. Custom/empty registries deliberately contain only registered tools. Restart the daemon after upgrading so an old registry is not retained. |
| Missing security skills | security-research and finding-validation existed as files but were absent from the embedded catalog. Eight omitted entry points are now bundled, including security-research's handbook. A regression checks every source entry point is registered; existing reload tests require each bundle to parse and retain its references. |
| Browser open has no tab | Untargeted open now creates its own tab even if the model requested in-place navigation. Explicit/session-owned targets retain reuse. Other untargeted mutations remain refused. No real account/browser session was opened in this audit. |
| Protected-path command block | Curl URL/method/payload operands no longer masquerade as local executable names. Shell body markers are unwrapped so method loops reach the actual curl command. Curl output files and shell redirections still receive protected-path checks, and destructive commands in the same loop remain checked. The follow-up transcript confirms the same method-enumeration failure but omits its command. The parser now treats an ambiguous mentioned verb as advisory; resolvable nested protected-path destruction remains denied. |
| Avast on 1.0.90 | Default Windows builds now ship a separate desktop host and VERSIONINFO; release packaging checks that the CLI contains one PE image and includes the sibling host. Those changes remove concrete packaging defects; they do not prove an Avast false positive or a clean current verdict. No signing certificate is configured in repository secrets, so Authenticode signing and vendor clearance cannot honestly be claimed complete. No antivirus exclusions were added. The supplied old hashes were not independently re-downloaded/verified in this audit. |
| Fresh-session old tasks/plan | Todo storage/cache identity is per session. Client plan events now reject plans that do not include the attached remote session; server fanout/reconnect also requires participation. Session-change render caches are cleared so old todo/plan surfaces cannot survive a switch. Existing saved user work is preserved. |

## Follow-up transcript: premature hunt closure and /copy

The supplied Riot hunt transcript is not evidence of a complete assessment. A
second controlled actor was absent; invented UUID paths returned SPA documents;
three-bundle keyword searches cannot establish that RCE-relevant behavior does
not exist. The agent repeatedly restated those broad negatives during confidence
follow-ups. The security-research entry now explicitly distinguishes a smoke
test from a campaign, requires dispositions for observed features and blocked
actors, and directs new evidence or corrected task status instead of repeated
conclusions/percentage increases. The report checker flags the supplied forms
of class-wide absence claims as unverified. This does not guarantee a finding
or exhaustive coverage; model behavior still requires measured evaluation.

The /copy error is a newer client attached to a daemon without the
get_session_transcript request variant. Modern daemons export the full retained
session. A legacy variant error is now consumed as a copy-specific failure and
falls back to the saved local session plus the captured live tail, off the UI
thread. Its notice explicitly calls this a saved snapshot and asks for a daemon
restart for a live full export. If no local saved session exists, it reports an
actionable upgrade/restart message rather than substituting bounded viewport
history. Unrelated request errors are not consumed by this path.

## Release verification

Local and GitHub check outcomes are recorded in the delivery message after completion. GitHub historical failed runs remain historical; success is assessed on the corrected source and release run. The re-release keeps semantic version 1.0.93 but rebuilds all eight platform archives and a combined checksum manifest from the corrected tag.
