#!/usr/bin/env bash
# Full release build with CARGO_TARGET_DIR removed, so output lands in ./target.
# Emits progress lines the agent harness can parse while it runs.
set -uo pipefail

cd "$HOME/OneDrive/Desktop/alphacode" || exit 1

# The fix: drop the inherited override for this build. Persisted User/Machine
# scopes were already cleared, but the currently-running alphacode TUI still
# carries the old environment block, so every shell it spawns inherits it.
unset CARGO_TARGET_DIR

echo "ALPHACODE_PROGRESS {\"percent\":5,\"message\":\"Building release into ./target (CARGO_TARGET_DIR unset)\"}"

LOG="$HOME/OneDrive/Desktop/alphacode/.audit_tmp-build.log"
mkdir -p "$(dirname "$LOG")"

# `--message-format=json` gives us a machine-readable compile stream; we
# translate the important events (start/finish + warnings/errors) into progress.
cargo build --release --message-format=json 2>&1 | \
python - "$LOG" <<'PY'
import json, sys, os

log_path = sys.argv[1]
compiled = 0
warnings = 0
errors = 0
targets_done = 0

with open(log_path, "w", encoding="utf-8") as logf:
    for raw in sys.stdin:
        raw = raw.strip()
        if not raw:
            continue
        try:
            msg = json.loads(raw)
        except json.JSONDecodeError:
            # Non-JSON line: compiler/linker passthrough output.
            print(raw, flush=True)
            logf.write(raw + "\n")
            continue

        reason = msg.get("reason")

        if reason == "compiler-message":
            m = msg.get("message", {})
            level = m.get("level", "")
            if level in ("error", "warning"):
                rendered = m.get("rendered") or m.get("message", "")
                logf.write(f"[{level}] {rendered}\n")
                if level == "error":
                    errors += 1
                    print(f"ERROR: {m.get('message')}", flush=True)
                else:
                    warnings += 1
            if m.get("target"):
                compiled += 1
                pct = min(95, 5 + compiled // 2)
                print(
                    'ALPHACODE_PROGRESS {"percent":%d,"message":"Compiling %s (%d warnings, %d errors)"}'
                    % (pct, m["target"]["name"], warnings, errors),
                    flush=True,
                )

        elif reason == "compiler-artifact":
            if not msg.get("fresh"):
                continue
            targets_done += 1
            name = msg.get("target", {}).get("name", "?")
            filenames = msg.get("filenames") or []
            print(
                'ALPHACODE_PROGRESS {"percent":96,"message":"Linked %s"}' % name,
                flush=True,
            )
            if any(f.endswith((".exe", "alphacode")) and "release" in f for f in filenames):
                print("BINARY: " + ", ".join(filenames), flush=True)

        elif reason == "build-finished":
            ok = msg.get("success")
            print("BUILD_FINISHED success=%s" % ok, flush=True)
            logf.write("build-finished success=%s\n" % ok)
            sys.exit(0 if ok else 1)

print(
    'ALPHACODE_PROGRESS {"percent":98,"message":"Build done: %d warnings, %d errors"}'
    % (warnings, errors),
    flush=True,
)
PY

status=$?
echo "=== build exit: $status ==="
echo "ALPHACODE_CHECKPOINT {\"message\": \"cargo build --release finished (exit $status)\"}"
exit $status