"""Summarize opt-in provider dispatch JSONL files without reading prompts or keys."""
import argparse
from collections import Counter
from datetime import datetime, timezone
import json
from pathlib import Path


def summarize(events):
    events = sorted(events, key=lambda event: event["unix_ms"])
    prompts, sessions, minutes, kinds, origins = (Counter() for _ in range(5))
    left = peak = 0
    for right, event in enumerate(events):
        stamp = event["unix_ms"]
        minute = datetime.fromtimestamp(stamp / 1000, timezone.utc).strftime("%Y-%m-%dT%H:%MZ")
        minutes[minute] += 1
        kinds[event["kind"]] += 1
        origins[event["origin"]] += 1
        if event["prompt"] is not None:
            prompts[f'{event["session"]}/{event["prompt"]}'] += 1
        if event["session"] is not None:
            sessions[event["session"]] += 1
        while events[left]["unix_ms"] <= stamp - 60_000:
            left += 1
        peak = max(peak, right - left + 1)
    return {
        "metric": "instrumented client dispatch attempts (HTTP, WebSocket connect and inference; not server receipts)",
        "total": len(events),
        "http_dispatches": sum(event["method"] != "WS" for event in events),
        "websocket_inference_messages": sum(event["method"] == "WS" for event in events),
        "unattributed": sum(event["prompt"] is None for event in events),
        "by_prompt": dict(prompts),
        "by_session": dict(sessions),
        "by_kind": dict(kinds),
        "by_origin": dict(origins),
        "by_utc_calendar_minute": dict(minutes),
        "peak_rolling_60s": peak,
    }


def read_summary(directory):
    files = sorted(directory.glob("requests-*.jsonl"))
    if not files:
        return None
    events = []
    for path in files:
        with path.open(encoding="utf-8") as stream:
            for line_number, line in enumerate(stream, 1):
                try:
                    events.append(json.loads(line))
                except json.JSONDecodeError as error:
                    raise ValueError(f"{path}:{line_number}: invalid or incomplete record: {error}") from error
    return summarize(events)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    try:
        summary = read_summary(args.directory)
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.error(str(error))
    if summary is None:
        parser.error("no requests-*.jsonl files found; enable ALPHACODE_PROVIDER_AUDIT_DIR before starting Alphacode")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
