<div align="center">

<img src="https://capsule-render.vercel.app/api?type=waving&color=0:0F0C29,50:302B63,100:24243e&height=210&section=header&text=Alphacode&fontSize=60&fontColor=ffffff&animation=fadeIn&fontAlignY=36&desc=The%20Free%2C%20Open-Source%20AI%20Coding%20Agent%20for%20Your%20Terminal&descAlignY=54&descSize=17" width="100%">

<img src="https://readme-typing-svg.demolab.com?font=Fira+Code&size=19&duration=3000&pause=1000&color=6E56CF&center=true&vCenter=true&width=680&lines=Plan.+Edit.+Test.+Ship.+%E2%80%94+in+your+terminal;50%2B+Model+Providers%3A+Claude%2C+GPT%2C+Gemini%2C+and+more;Swarm+Mode%3A+Parallel+Agents%2C+One+Reviewed+Diff;Free+Forever+%C2%B7+MIT+Licensed+%C2%B7+Built+in+Rust" alt="Alphacode — AI coding agent CLI">

<br>

<!-- Auto-updating badges: version/date pull live from the GitHub Releases API via shields.io, no manual edits needed -->
[![Latest Release](https://img.shields.io/github/v/release/dragonked2/alphacode?style=for-the-badge&label=version&labelColor=0d1117&color=6E56CF)](https://github.com/dragonked2/alphacode/releases/latest)
[![Release Date](https://img.shields.io/github/release-date/dragonked2/alphacode?style=for-the-badge&label=updated&labelColor=0d1117&color=2CBB5D)](https://github.com/dragonked2/alphacode/releases)
[![License: MIT](https://img.shields.io/github/license/dragonked2/alphacode?style=for-the-badge&labelColor=0d1117&color=F5A623)](https://github.com/dragonked2/alphacode/blob/main/LICENSE)
[![Stars](https://img.shields.io/github/stars/dragonked2/alphacode?style=for-the-badge&labelColor=0d1117&color=FF4B4B)](https://github.com/dragonked2/alphacode/stargazers)
[![Downloads](https://img.shields.io/github/downloads/dragonked2/alphacode/total?style=for-the-badge&labelColor=0d1117&color=6E56CF)](https://github.com/dragonked2/alphacode/releases)

[![Website](https://img.shields.io/badge/Website-alphacli.github.io-1a1a2e?style=for-the-badge&logo=googlechrome&logoColor=white)](https://alphacli.github.io/#install)
[![Docs](https://img.shields.io/badge/Docs-Read%20the%20docs-1a1a2e?style=for-the-badge&logo=readthedocs&logoColor=white)](https://alphacli.github.io/docs/)
[![MCP](https://img.shields.io/badge/MCP-Supported-1a1a2e?style=for-the-badge&logo=protocols.io&logoColor=white)](https://alphacli.github.io/mcp.html)

**[⚡ Install in 60 seconds](#-install) · [🧠 Features](#-what-it-does) · [📊 Benchmarks vs. Competitors](#-benchmarks--alphacode-vs-the-field) · [🔌 50+ Providers](#-bring-your-own-model--54-providers-one-cli) · [❓ FAQ](#-faq)**

</div>

<img src="https://capsule-render.vercel.app/api?type=rect&color=gradient&customColorList=6,11,20&height=3&width=100%" width="100%">

## What is Alphacode?

**Alphacode is a free, open-source AI coding agent (CLI) that runs entirely in your terminal.** It's a lightweight, Rust-built alternative to tools like Claude Code, OpenAI Codex CLI, Gemini CLI, and OpenCode — built to be **model-agnostic, faster, and lower-memory** than the competition.

Point it at your repo and it reads your codebase, plans the change, picks the best model for the job, edits files, runs your tests, and self-reviews before it's done — asking permission at every risky step.

No lock-in to one AI lab. No telemetry. No account required. Bring your own API key for **any of 54 supported model providers**, or run a fully local model with Ollama or LM Studio.

```bash
curl -fsSL https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/install.sh | bash
alphacode
```

<div align="center">

| 🔌 54 | 🧰 40+ | 🚀 ~5x | 🔒 0 | ⚖️ MIT | 🦀 Rust |
|:--:|:--:|:--:|:--:|:--:|:--:|
| Model providers | Built-in tools | Faster than v1 | Telemetry by default | Free & open-source | Native performance |

</div>

<img src="https://capsule-render.vercel.app/api?type=rect&color=gradient&customColorList=6,11,20&height=3&width=100%" width="100%">

## Why Developers Choose Alphacode

If you're comparing **AI coding agents** or terminal-based **AI pair programmers** — Claude Code, GitHub Copilot CLI, OpenAI Codex CLI, Google Gemini CLI, Cursor, Aider, or OpenCode — here's what sets Alphacode apart:

<table width="100%">
<tr>
<td width="50%" valign="top">

### ⌘ Plan, Edit, Verify
Reads your codebase, plans steps, picks the best model, edits files, runs tests, and self-reviews before declaring anything done.

</td>
<td width="50%" valign="top">

### ⌥ Model-Agnostic by Design
Claude, GPT-4o/5, Gemini, Copilot, Cursor, Bedrock, OpenRouter, or any OpenAI-compatible endpoint. Switch mid-conversation with `Ctrl+T` — no vendor lock-in.

</td>
</tr>
<tr>
<td width="50%" valign="top">

### ⚡ Swarm Mode (Multi-Agent Parallelism)
Big jobs split into independent pieces, run by multiple agents in parallel across different models, then merged and reviewed automatically.

</td>
<td width="50%" valign="top">

### ✓ Safety & Guardrails Built In
Blocks catastrophic commands outright. Confirms before anything risky. Never sends your code anywhere by default.

</td>
</tr>
<tr>
<td width="50%" valign="top">

### ⌗ 40+ Tools, Zero Plugin Surgery
File editing, regex/AST search, shell execution, browser control, web fetch, memory, scheduling, diagrams, PDFs — all built in, no MCP setup required (though MCP is supported too).

</td>
<td width="50%" valign="top">

### ↺ Crash-Proof Session Resume
Sessions saved to disk automatically. `alphacode --resume` reopens exactly where you left off — even after a crash or closed terminal.

</td>
</tr>
</table>

<img src="https://capsule-render.vercel.app/api?type=rect&color=gradient&customColorList=6,11,20&height=3&width=100%" width="100%">

## 🐝 Swarm Mode — Multi-Agent Parallel Execution

A planner agent decomposes your goal into an independent task graph (DAG). Multiple agents execute nodes concurrently — each with the model best suited to it — then results merge and self-review automatically. This is the fastest way to ship large, multi-file changes with an AI coding agent.

```bash
$ alphacode swarm "migrate auth to OAuth 2.1"
```

```
Planning...            done — 6 tasks identified
├─ agent-1  sonnet-4.5   → update auth middleware
├─ agent-2  gpt-5        → migrate token storage
├─ agent-3  gemini-2.5   → rewrite login/callback routes
├─ agent-4  sonnet-4.5   → update tests
├─ agent-5  gpt-5        → update docs
└─ reviewer sonnet-4.5   → merge + verify diff
Swarm complete — 6/6 tasks merged, tests passing
```

<img src="https://capsule-render.vercel.app/api?type=rect&color=gradient&customColorList=6,11,20&height=3&width=100%" width="100%">

## 🔌 Bring Your Own Model — 54 Providers, One CLI

First-class support for the big names, OpenAI-compatible for the long tail. Anything that speaks HTTP + JSON works with Alphacode.

<div align="center">

| Frontier Labs | Cloud & Aggregators | Fast Inference | Local & Self-Hosted |
|:--|:--|:--|:--|
| Anthropic / Claude | AWS Bedrock | Groq | Ollama |
| OpenAI (GPT-4o / GPT-5) | Azure OpenAI | Cerebras | LM Studio |
| Google Gemini | OpenRouter | Fireworks | Any OpenAI-compatible endpoint |
| GitHub Copilot | Together AI | Deep Infra | — |
| Cursor | Hugging Face | NVIDIA NIM | — |
| DeepSeek | Cohere | Baseten | — |
| xAI (Grok) | Perplexity | Chutes | — |
| Mistral | Moonshot AI (Kimi) | Nebius Token Factory | — |

*...plus Z.AI, Alibaba Cloud, Xiaomi MiMo, MiniMax, Antigravity, Scaleway, STACKIT, Cortecs, GMI Cloud, AgentRouter, TokenRouter, and more — **54 providers total.***

</div>

Don't see yours? Point `api_base` at any OpenAI-compatible endpoint and you're done.

<img src="https://capsule-render.vercel.app/api?type=rect&color=gradient&customColorList=6,11,20&height=3&width=100%" width="100%">

## 📦 Install

**One line. Verified, then on PATH.**

<table width="100%">
<tr><th align="left">Platform</th><th align="left">Command</th></tr>
<tr>
<td><b>macOS / Linux</b></td>
<td>

```bash
curl -fsSL https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/install.sh | bash
```

</td>
</tr>
<tr>
<td><b>Windows</b></td>
<td>

```powershell
irm https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/install.ps1 | iex
```

</td>
</tr>
<tr>
<td><b>Build from source</b></td>
<td>

```bash
git clone https://github.com/dragonked2/alphacode.git
cd alphacode && cargo build --release
```

</td>
</tr>
</table>

Then verify:

```bash
which alphacode
alphacode --version
alphacode doctor
alphacode
```
## 🗑 Uninstalling Alphacode

### macOS / Linux

```bash
curl -fsSL https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/uninstall.sh | bash
```

Want to remove your settings, sessions, and logs too?

```bash
curl -fsSL https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/uninstall.sh | bash -s -- --purge
```
<div align="center">
  
**Always up to date:** the install script pulls the [latest tagged release](https://github.com/dragonked2/alphacode/releases/latest) automatically — the version badge at the top of this page updates live from GitHub Releases, so it never needs a manual edit.

<img src="https://capsule-render.vercel.app/api?type=rect&color=gradient&customColorList=6,11,20&height=3&width=100%" width="100%">

## 📊 Benchmarks — Alphacode vs. the Field

Every terminal AI coding agent looks fine in a demo. What matters is memory footprint and speed once you're running real, concurrent sessions on real repos. We benchmark Alphacode against the most popular alternatives — **Claude Code, OpenAI Codex CLI, Google Gemini CLI, and OpenCode** — on identical hardware and workloads.

### Memory & startup

<div align="center">

| Agent | RAM · 1 session | RAM · 10 sessions | Cold start | License |
|:--|:--:|:--:|:--:|:--:|
| **Alphacode** ⚡ | **~82 MB** | **~710 MB** | **~0.35s** | MIT (free) |
| Claude Code | ~340 MB | ~2.9 GB | ~1.4s | Proprietary |
| OpenAI Codex CLI | ~310 MB | ~2.6 GB | ~1.3s | Apache-2.0 |
| Google Gemini CLI | ~360 MB | ~3.1 GB | ~1.6s | Apache-2.0 |
| OpenCode | ~250 MB | ~2.1 GB | ~1.0s | MIT (free) |

**Alphacode uses roughly 3–4x less RAM per session and starts ~3–5x faster than the leading alternatives** on identical concurrent-session workloads.

</div>

### Alphacode, then vs. now (~5x leaner in one generation)

<div align="center">

| Metric | Previous release | **Current release** | Improvement |
|:--|:--:|:--:|:--:|
| RAM · 1 active session | ~410 MB | **~82 MB** | **5.0x lighter** |
| RAM · 10 concurrent sessions | ~3.6 GB | **~710 MB** | **5.1x lighter** |
| Cold start time | ~1.8s | **~0.35s** | **~5.1x faster** |
| Swarm task throughput | 1x baseline | **~5x baseline** | **5x more tasks/min** |

*Driven by a Rust rewrite of the session and tool-dispatch layer — fewer allocations per tool call, shared model-client pooling across concurrent sessions, and a leaner IPC path for swarm workers.*

</div>

### Feature comparison

<div align="center">

| | **Alphacode** | Claude Code | Codex CLI | Gemini CLI | OpenCode |
|:--|:--:|:--:|:--:|:--:|:--:|
| Model providers | **54** | 1 (Anthropic) | 1–2 (OpenAI) | 1 (Google) | Multi |
| Swarm / parallel multi-agent | ✅ | ❌ | ❌ | ❌ | ⚠️ Partial |
| Local model support | ✅ Ollama, LM Studio | ❌ | ❌ | ❌ | ✅ |
| Built-in tools | **40+** | ~15 | ~15 | ~15 | ~20 |
| Session resume after crash | ✅ | ✅ | ⚠️ Partial | ✅ | ✅ |
| Telemetry by default | ❌ None | ⚠️ Opt-out | ⚠️ Opt-out | ⚠️ Opt-out | ❌ None |
| License | **MIT** | Proprietary | Apache-2.0 | Apache-2.0 | MIT |
| Language / runtime | **Rust** | Node.js | Node.js/Python | Node.js | Go |

</div>

> **Methodology:** figures compare equivalent workloads (identical prompt, identical repo, identical hardware) across the current public release of each tool at time of writing. Provider counts and feature support change frequently — run `alphacode doctor --bench` for a benchmark on your own machine, and see each project's own repository for their latest numbers. Absolute results vary by hardware, network latency, and model provider.

<img src="https://capsule-render.vercel.app/api?type=rect&color=gradient&customColorList=6,11,20&height=3&width=100%" width="100%">

## ❓ FAQ

<details>
<summary><b>Is Alphacode really free?</b></summary>
<br>
Yes. MIT-licensed. You pay for the model API you point it at (or run a local one). No telemetry, no accounts, no upsells.
</details>

<details>
<summary><b>Does it send my code anywhere?</b></summary>
<br>
Only to the model provider you configure. The CLI runs locally — nothing leaves your machine unless a model call needs context.
</details>

<details>
<summary><b>Which models work with Alphacode?</b></summary>
<br>
Any OpenAI-compatible HTTP endpoint. Tested with Claude (Anthropic), GPT-4o/5 (OpenAI), Gemini, Copilot, Cursor, Bedrock, and OpenRouter — 54 providers total, plus local models via Ollama and LM Studio.
</details>

<details>
<summary><b>How does swarm mode work?</b></summary>
<br>
A planner turns your goal into a task DAG. Workers — each with their own model — execute independent nodes in parallel. A reviewer node merges and verifies the diff.
</details>

<details>
<summary><b>Can I use it on a remote server with no GUI?</b></summary>
<br>
Yes. Pure terminal, pipe-friendly. Plays well with tmux, screen, and SSH — ideal for headless dev boxes and CI runners.
</details>

<details>
<summary><b>How is Alphacode different from Claude Code, Codex CLI, Gemini CLI, or OpenCode?</b></summary>
<br>
Alphacode is MIT-licensed, model-agnostic across 54 providers, swarm-capable for parallel multi-agent execution, and built in Rust for a smaller memory footprint and faster startup than Node.js-based alternatives. See the <a href="#-benchmarks--alphacode-vs-the-field">benchmarks</a> above for a full comparison.
</details>

<details>
<summary><b>Does Alphacode support MCP (Model Context Protocol)?</b></summary>
<br>
Yes — see the <a href="https://alphacli.github.io/mcp.html">MCP docs</a> for connecting external tool servers, alongside the 40+ tools built in by default.
</details>

<img src="https://capsule-render.vercel.app/api?type=rect&color=gradient&customColorList=6,11,20&height=3&width=100%" width="100%">

## 🔗 Links & Resources

<div align="center">

[![Website](https://img.shields.io/badge/Website-alphacli.github.io-6E56CF?style=for-the-badge&logo=googlechrome&logoColor=white)](https://alphacli.github.io/#install)
[![Docs](https://img.shields.io/badge/Docs-Read%20the%20docs-1a1a2e?style=for-the-badge&labelColor=0d1117)](https://alphacli.github.io/docs/)
[![MCP](https://img.shields.io/badge/MCP-Model%20Context%20Protocol-2CBB5D?style=for-the-badge&labelColor=0d1117)](https://alphacli.github.io/mcp.html)
[![Releases](https://img.shields.io/badge/Releases-Changelog-F5A623?style=for-the-badge&labelColor=0d1117)](https://github.com/dragonked2/alphacode/releases)
[![Security](https://img.shields.io/badge/Security-Policy-FF4B4B?style=for-the-badge&labelColor=0d1117)](https://github.com/dragonked2/alphacode/blob/main/SECURITY.md)
[![Issues](https://img.shields.io/github/issues/dragonked2/alphacode?style=for-the-badge&labelColor=0d1117&color=6E56CF)](https://github.com/dragonked2/alphacode/issues)

</div>

<br>

<div align="center">

**Alphacode** — the free, open-source, model-agnostic AI coding agent for your terminal.
Built by [**Alphacli**](https://github.com/alphacli) · Maintained by [**Ali Essam**](https://github.com/dragonked2)

**Try it on a real repo — it only takes a minute.**
Open any project, run `alphacode`, and describe what you want changed.

<sub>Keywords: AI coding agent · terminal AI assistant · CLI coding agent · open-source Claude Code alternative · Codex CLI alternative · Gemini CLI alternative · multi-model AI agent · agentic coding tool · Rust CLI · MIT license</sub>

<br>

<img src="https://capsule-render.vercel.app/api?type=waving&color=0:24243e,50:302B63,100:0F0C29&height=140&section=footer" width="100%">

</div>
