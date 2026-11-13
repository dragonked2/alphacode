# Provider request budgets

Provider admission now combines the existing in-process concurrency gate with
rolling request budgets and persistent cooldowns shared by local Alphacode
processes. The ledger lives under the OS local cache directory at
`alphacode/provider-limits`. Files contain timestamps, with hashed endpoint
origins as filenames; API keys and prompts are never stored there.

TokenHarbor defaults to 50 requests per minute and 1,500 per hour, leaving
headroom below its documented free-account limits. Limits cover all model
requests using the shared transport, including memory work, compaction and
retries. An origin is shared conservatively across credentials on this machine.
Other origins default to no proactive rate budget, but still retain server
cooldowns. This does not coordinate machines or other applications using the
same account, so HTTP 429 responses remain authoritative.

| Environment variable | Default | Meaning |
| --- | --- | --- |
| `ALPHACODE_PROVIDER_REQUESTS_PER_MINUTE` | 50 for TokenHarbor; 0 elsewhere | Rolling minute allowance per origin |
| `ALPHACODE_PROVIDER_REQUESTS_PER_HOUR` | 1500 for TokenHarbor; 0 elsewhere | Rolling hour allowance per origin |
| `ALPHACODE_MEMORY_REQUESTS_PER_MINUTE` | 12 | Additional shared allowance for optional memory work |
| `ALPHACODE_MEMORY_REQUESTS_PER_HOUR` | 120 | Additional shared allowance for optional memory work |

Zero disables that proactive window, not server cooldowns. Paid TokenHarbor
accounts can explicitly disable its proactive windows. Existing concurrency
and contention-pacing environment settings still apply.

Explicit `Retry-After` hints retain the full delay, including HTTP dates and
positive body fields. Hints beyond 180 seconds stop automatic recovery instead
of shortening the provider's deadline. A 100-year tracking horizon bounds
malformed overflowing values; these also stop automatic recovery. Cooldowns
survive process restarts and a shorter later hint cannot reduce them. Shorter
waits are cancellable, release the concurrency slot, and recheck admission.

Memory calls skip optional work when their shared allowance or two-call
in-process concurrency ceiling is exhausted. Foreground requests use the
provider allowance but do not consume the separate memory allowance. Ranking
retains configured consensus voting. Extraction checks up to five possible
contradictions in one model call per new memory, instead of up to five calls.
Error classification inspects the full error chain and preserves a provider's
longer cooldown in the memory circuit breaker.

Regression checks use local HTTP servers, injected ledger timestamps and a
child test process. They do not call public providers or require API keys.
