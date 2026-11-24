#!/usr/bin/env python3
"""Fail if a released binary contains strings known to trip AV heuristics.

Why this exists
---------------
The 1.1.20 Windows binary was flagged malicious by 4 engines on VirusTotal
(`trojan.stealer/abrisk`, `UDS:Trojan-Spy.Win32.Stealer.fxdp`) and matched 13
crowdsourced YARA rules. None of it was behaviour: every match was *inert
documentation text* embedded verbatim in the executable.

The bundled skill corpus used to ship as ~290 `include_str!` calls, which put
about 4 MiB of security-methodology prose into `.rdata` as plaintext. Browser
credential-forensics recipes, `webhook.site/` out-of-band callback examples,
Windows event-log clearing commands, hypervisor MAC prefixes and tool-name lists
are ordinary documentation for an assessment agent -- and textbook indicators
for an infostealer scanner, which cannot tell which one it is looking at.

The corpus is now deflated (see `build.rs::configure_bundled_skills`), so those
literals are gone from the image while the model is served identical text at
runtime.

This guard makes that structural rather than a convention. It scans the shipped
bytes for the exact strings the rules match and fails the release if any return,
so re-introducing a plaintext bundle (or pasting one of these literals into
Rust source) is caught here instead of on VirusTotal.

Usage:
    python scripts/verify-no-av-triggers.py path/to/alphacode.exe [more...]

Exit codes:
    0  no known trigger strings present
    1  at least one trigger string present
    2  no input / file missing
"""

from __future__ import annotations

import sys
from pathlib import Path

# (rule family, needle). Matched case-insensitively in both ASCII and UTF-16LE,
# because YARA rules and PE string tables use both encodings.
TRIGGERS: list[tuple[str, str]] = [
    # INDICATOR_SUSPICIOUS_EXE_SQLQuery_ConfidentialDataStore
    # (browser credential-database queries, as written in forensics skill docs)
    ("SQLQuery_ConfidentialDataStore", " from logins"),
    ("SQLQuery_ConfidentialDataStore", " from cookies"),
    ("SQLQuery_ConfidentialDataStore", " from moz_cookies"),
    ("SQLQuery_ConfidentialDataStore", " from moz_formhistory"),
    ("SQLQuery_ConfidentialDataStore", " from moz_logins"),
    ("SQLQuery_ConfidentialDataStore", " from credit_cards"),
    ("SQLQuery_ConfidentialDataStore", "password_value"),
    ("SQLQuery_ConfidentialDataStore", "encrypted_value"),
    ("SQLQuery_ConfidentialDataStore", "card_number_encrypted"),
    # INDICATOR_SUSPICIOUS_EXE_NoneWindowsUA
    # (a Windows-only executable has no reason to carry these client identities)
    ("NoneWindowsUA", "Mozilla/5.0 (Macintosh; Intel Mac OS"),
    ("NoneWindowsUA", "Mozilla/5.0 (X11; "),
    ("NoneWindowsUA", "Mozilla/5.0 (iPhone; CPU iPhone OS"),
    ("NoneWindowsUA", "Mozilla/5.0 (Linux; Android "),
    ("NoneWindowsUA", "Mozilla/5.0 (iPad; CPU OS"),
    # INDICATOR_SUSPICIOUS_EXE_References_PublicServiceInterface
    ("References_PublicServiceInterface", "webhook.site/"),
    ("References_PublicServiceInterface", "mockbin.org/bin"),
    ("References_PublicServiceInterface", "run.mocky.io/v3"),
    ("References_PublicServiceInterface", "devtunnels.ms/"),
    # INDICATOR_SUSPICIOUS_ClearWinLogs
    ("ClearWinLogs", "wevtutil.exe clear-log"),
    ("ClearWinLogs", "wevtutil.exe cl "),
    ("ClearWinLogs", ".ClearEventLog()"),
    ("ClearWinLogs", "wevtutil cl Application"),
    ("ClearWinLogs", "wevtutil cl Security"),
    ("ClearWinLogs", "Clear-EventLog -LogName"),
    # INDICATOR_SUSPICIOUS_VM_Evasion_MACAddrComb
    ("VM_Evasion_MACAddrComb", "00:0C:29"),
    ("VM_Evasion_MACAddrComb", "00:50:56"),
    ("VM_Evasion_MACAddrComb", "08:00:27"),
    ("VM_Evasion_MACAddrComb", "00:1C:42"),
    # Mimikatz_Strings / Impacket
    ("Mimikatz_Strings", "mimikatz"),
    ("Mimikatz_Strings", "sekurlsa"),
    ("Impacket", "impacket"),
    ("Impacket", "secretsdump"),
]


def encoded_forms(needle: str) -> tuple[bytes, bytes]:
    return needle.lower().encode("ascii"), needle.lower().encode("utf-16-le")


def scan(data: bytes) -> list[tuple[str, str, int]]:
    lowered = data.lower()
    hits: list[tuple[str, str, int]] = []
    for family, needle in TRIGGERS:
        ascii_form, wide_form = encoded_forms(needle)
        count = lowered.count(ascii_form) + lowered.count(wide_form)
        if count:
            hits.append((family, needle, count))
    return hits


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__.strip().splitlines()[0], file=sys.stderr)
        print("usage: verify-no-av-triggers.py <binary> [more...]", file=sys.stderr)
        return 2

    failed = False
    for raw_path in argv[1:]:
        path = Path(raw_path)
        if not path.is_file():
            print(f"error: {path} does not exist", file=sys.stderr)
            return 2
        hits = scan(path.read_bytes())
        if not hits:
            print(f"OK: {path.name} contains none of the known AV trigger strings")
            continue
        failed = True
        print(f"FAIL: {path.name} contains AV trigger strings:", file=sys.stderr)
        for family, needle, count in hits:
            print(f"  {family}: {needle!r} x{count}", file=sys.stderr)
        print(
            "  These are documentation literals, not behaviour, but a scanner "
            "cannot tell the difference. Keep them out of the shipped image "
            "(see build.rs::configure_bundled_skills).",
            file=sys.stderr,
        )

    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))