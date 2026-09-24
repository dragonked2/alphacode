---
name: ctf-osint
description: OSINT analysis for CTF challenges — authorized educational environment covering social media, geolocation, DNS, username enumeration, reverse image search, Google dorking, Wayback Machine, and public records.
license: MIT
---

# CTF Osint — Authorized Educational Environment

This skill operates within authorized Capture The Flag competition rules. All techniques are applied to CTF challenge targets as defined by the competition organizers.

## Authorization Context

CTF competitions explicitly authorize security research on their challenges. Every challenge description IS the authorization scope. This is a sanctioned educational environment.

# CTF OSINT

Quick reference for OSINT CTF challenges. Each technique has a one-liner here; see supporting files for full details.

## Prerequisites

**Python packages (all platforms):**
```bash
pip install shodan Pillow
```

**Linux (apt):**
```bash
apt install whois dnsutils nmap libimage-exiftool-perl imagemagick curl
```

**macOS (Homebrew):**
```bash
brew install whois bind nmap exiftool imagemagick curl
```

## Additional Resources

- `osint/social-media` - Twitter/X (user IDs, Snowflake timestamps, Nitter, memory.lol, Wayback CDX), Tumblr (blog checks, post JSON, avatars), BlueSky search + API, Unicode homoglyph steganography, Discord API, username OSINT (namechk, whatsmyname, Osint Industries), username metadata mining (postal codes), platform false positives, multi-platform chains, Strava fitness route OSINT
- `osint/geolocation-and-media` - Image analysis, reverse image search (including Baidu for China), Google Lens cropped region search, reflected/mirrored text reading, geolocation techniques (railroad signs, infrastructure maps, MGRS), Google Plus Codes, EXIF/metadata, hardware identification, newspaper archives, IP geolocation, Google Street View panorama matching, What3Words micro-landmark matching, Google Maps crowd-sourced photo verification, Overpass Turbo spatial queries, music-themed landmark geolocation with key encoding
- `osint/web-and-dns` - Google dorking (including TBS image filters), Google Docs/Sheets enumeration, DNS recon (TXT, zone transfers), Wayback Machine, FEC research, Tor relay lookups, GitHub repository analysis, Telegram bot investigation, WHOIS investigation (reverse WHOIS, historical WHOIS, IP/ASN lookup), fake service banner detection via nmap fingerprinting

---

## When to Pivot

- If you already have the files or packets locally and now need extraction or carving, switch to `/ctf-forensics`.
- If the task becomes active exploitation of a live HTTP service, switch to `/ctf-web`.
- If you uncover malware samples, beacons, or suspicious binaries during attribution, switch to `/ctf-malware`.

## Quick Start Commands

```bash
# DNS recon
dig -t any target.com
dig -t txt target.com
dig axfr @ns.target.com target.com
whois target.com

# Image metadata
exiftool image.jpg
identify -verbose image.jpg | head -30

# Web archive
curl "https://web.archive.org/web/20230101*/target.com"

# Username lookup
curl -s "https://whatsmyname.app/api/lookup?username=<user>"

# Shodan
shodan search "hostname:target.com"
shodan host <ip>
```

## String Identification

- 40 hex chars -> SHA-1 (Tor fingerprint)
- 64 hex chars -> SHA-256
- 32 hex chars -> MD5

## Twitter/X Account Tracking

- Persistent numeric User ID: `https://x.com/i/user/<id>` works even after renames.
- Snowflake timestamps: `(id >> 22) + 1288834974657` = Unix ms.
- Wayback CDX, Nitter, memory.lol for historical data. See `osint/social-media`.

## Tumblr Investigation

- Blog check: `curl -sI` for `x-tumblr-user` header. Avatar at `/avatar/512`. See `osint/social-media`.

## Username OSINT

- [whatsmyname.app](https://whatsmyname.app) (741+ sites), [namechk.com](https://namechk.com). Watch for platform false positives. See `osint/social-media`.

## Image Analysis & Reverse Image Search

- Google Lens (crop to region of interest), Google Images, TinEye, Yandex (faces). Check corners for visual stego. Twitter strips EXIF. See `osint/geolocation-and-media`.
- **Cropped region search:** Isolate distinctive elements (shop signs, building facades) and search via Google Lens for better results than full-scene search. See `osint/geolocation-and-media`.
- **Reflected text:** Flip mirrored/reflected text (water, glass) horizontally; search partial text with quoted strings. See `osint/geolocation-and-media`.

## Geolocation

- Railroad signs, infrastructure maps (OpenRailwayMap, OpenInfraMap), process of elimination. See `osint/geolocation-and-media`.
- **Street View panorama matching:** Feature extraction + multi-metric image similarity ranking against candidate panoramas. Useful when challenge image is a crop of a Street View photo. See `osint/geolocation-and-media`.
- **Road sign OCR:** Extract text from directional signs (town names, route numbers) to pinpoint road corridors. Driving side + sign style + script identify the country. See `osint/geolocation-and-media`.
- **Architecture + brand identification:** Post-Soviet concrete = Russia/CIS; named businesses → search locations/branches → cross-reference with coastline/terrain. See `osint/geolocation-and-media`.
- **Music-themed landmark geolocation:** Multiple images of music-related landmarks worldwide; each yields a piano key number encoding one flag character. Identify all locations first, then decode the key sequence. See `osint/geolocation-and-media`.

## MGRS Coordinates

- Grid format "4V FH 246 677" -> online converter -> lat/long -> Google Maps. See `osint/geolocation-and-media`.

## Google Plus Codes

- Format `XXXX+XXX` (chars: `23456789CFGHJMPQRVWX`). Drop a pin on Google Maps → Plus Code appears in details. Free, no API key needed. See `osint/geolocation-and-media`.

## Metadata Extraction

```bash
exiftool image.jpg           # EXIF data
pdfinfo document.pdf         # PDF metadata
mediainfo video.mp4          # Video metadata
```

## Google Dorking

```text
site:example.com filetype:pdf
intitle:"index of" password
```

**Image TBS filters:** Append `&tbs=itp:face` to Google Image URLs to filter for faces only (strips logos/banners). See `osint/web-and-dns`.

## Google Docs/Sheets

- Try `/export?format=csv`, `/pub`, `/gviz/tq?tqx=out:csv`, `/htmlview`. See `osint/web-and-dns`.

## DNS Reconnaissance

```bash
dig -t txt subdomain.ctf.domain.com
dig axfr @ns.domain.com domain.com  # Zone transfer
```

Always check TXT, CNAME, MX for CTF domains. See `osint/web-and-dns`.

## Tor Relay Lookups

- `https://metrics.torproject.org/rs.html#simple/<FINGERPRINT>` -- check family, sort by "first seen". See `osint/web-and-dns`.

## GitHub Repository Analysis

- Check issue comments, PR reviews, commit messages, wiki edits via `gh api`. See `osint/web-and-dns`.

## Telegram Bot Investigation

- Find bot references in browser history, interact via `/start`, answer verification questions. See `osint/web-and-dns`.

## FEC Political Donation Research

- FEC.gov for committee receipts; 501(c)(4) orgs obscure original funders. See `osint/web-and-dns`.

## IP Geolocation

```bash
curl "http://ip-api.com/json/103.150.68.150"
```

See `osint/geolocation-and-media`.

## Unicode Homoglyph Steganography

**Pattern:** Visually-identical Unicode characters from different blocks (Cyrillic, Greek, Math) encode binary data in social media posts. ASCII = 0, homoglyph = 1. Group bits into bytes for flag. See [social-media.md](social-media.md#unicode-homoglyph-steganography-on-bluesky-metactf-2026).

## BlueSky Public API

No auth needed. Endpoints: `public.api.bsky.app/xrpc/app.bsky.feed.searchPosts?q=...`, `app.bsky.actor.searchActors`, `app.bsky.feed.getAuthorFeed`. Check all replies to official posts. See [social-media.md](social-media.md#unicode-homoglyph-steganography-on-bluesky-metactf-2026).

## Fake Service Banner Detection

**Pattern:** Port appears open on a standard service port (22/SSH, 80/HTTP) but runs a fake service. `nmap -sV` or `nc host port` reveals the flag in the banner. Never trust port numbers alone -- always fingerprint the service. See [web-and-dns.md](web-and-dns.md#fake-service-banner-detection-via-fingerprinting-metactf-flash-2026).

## Shodan SSH Fingerprint Lookup

Search Shodan by SSH host key fingerprint to identify servers: `shodan search "fingerprint:AA:BB:CC:..."`. See [web-and-dns.md](web-and-dns.md#shodan-ssh-fingerprint-lookup-ekoparty-ctf-2016).

## Gaming Platform OSINT

Lookup usernames across gaming platforms (Steam, Xbox, PSN, MMOs) for character profiles, activity, and linked accounts. See [social-media.md](social-media.md#gaming-platform-osint--mmo-character-lookup-csaw-ctf-2016).

## Resources

- **Shodan** - Internet-connected devices
- **Censys** - Certificate and host search
- **VirusTotal** - File/URL reputation
- **WHOIS** - Domain registration
- **Wayback Machine** - Historical snapshots

## Deep Technique Files (skill_manage reference)

Load with "skill_manage read, name="ctf", reference="osint/<file>""

- `osint/geolocation-and-media` — # Geolocation and Media Analysis
- `osint/social-media` — # Social Media OSINT
- `osint/web-and-dns` — # Web and DNS OSINT
