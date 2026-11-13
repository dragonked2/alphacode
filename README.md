<a id="top"></a>

<div align="center">

<img src="logo.png" alt="AlphaCode logo" width="640">

# AlphaCode

### The open-source AI coding agent for your terminal

**Built in Rust. Understands your codebase, edits files, runs commands and tests, drives Firefox, controls desktop apps, and verifies its own work.**

Works with **Claude · GPT · Gemini · GitHub Copilot · OpenRouter · Ollama · LM Studio** and more.
**Start free. No API key required.**

<br>

[![GitHub stars](https://img.shields.io/github/stars/dragonked2/alphacode?style=for-the-badge&label=Stars&labelColor=1a1a2e&color=FFD34E)](https://github.com/dragonked2/alphacode/stargazers)
[![Latest release](https://img.shields.io/github/v/release/dragonked2/alphacode?style=for-the-badge&label=Release&labelColor=1a1a2e&color=D7A93F)](https://github.com/dragonked2/alphacode/releases)
[![MIT license](https://img.shields.io/github/license/dragonked2/alphacode?style=for-the-badge&label=License&labelColor=1a1a2e&color=F5A623)](LICENSE)
[![Open issues](https://img.shields.io/github/issues/dragonked2/alphacode?style=for-the-badge&label=Issues&labelColor=1a1a2e&color=FF6B6B)](https://github.com/dragonked2/alphacode/issues)

![Windows](https://img.shields.io/badge/Windows-supported-2CBB5D?style=flat-square&logo=windows&logoColor=white&labelColor=1a1a2e)
![macOS](https://img.shields.io/badge/macOS-supported-2CBB5D?style=flat-square&logo=apple&logoColor=white&labelColor=1a1a2e)
![Linux](https://img.shields.io/badge/Linux-supported-2CBB5D?style=flat-square&logo=linux&logoColor=white&labelColor=1a1a2e)
![Rust](https://img.shields.io/badge/Rust-native-DEA584?style=flat-square&logo=rust&logoColor=white&labelColor=1a1a2e)
![40+ tools](https://img.shields.io/badge/agent%20tools-40%2B-D7A93F?style=flat-square&labelColor=1a1a2e)
![Free to start](https://img.shields.io/badge/free%20to%20start-no%20API%20key-2CBB5D?style=flat-square&labelColor=1a1a2e)

<br>

[**⚡ Quick start**](#quick-start) ·
[**🧠 Architecture**](#brain) ·
[Demo](#demo) ·
[Features](#features) ·
[Providers](#providers) ·
[Docs](docs) ·
[Website](https://alphacli.github.io/) ·
[Releases](https://github.com/dragonked2/alphacode/releases) ·
[Firefox Agent](https://addons.mozilla.org/en-US/firefox/addon/alphacode-browser-agent/) ·
[Report a bug](https://github.com/dragonked2/alphacode/issues)

<br>

<img src="assets/alphacode-brain.svg" alt="AlphaCode brain architecture: agent loop, tools, swarm, senses, model providers, memory, safety, skills, core and interface" width="100%">

<sub>The architecture of AlphaCode drawn as a brain: an executive loop, motor tools, real-world senses, a swappable language engine, persistent memory, protective instincts, and a lightweight Rust core.</sub>

</div>

---

<a id="install"></a>
## ⚡ Install in 30 seconds

**macOS / Linux**

```bash
curl -fsSL https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/install.sh | bash
```

**Windows (PowerShell)**

```powershell
irm https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/install.ps1 | iex
```

Then verify your setup and launch:

```bash
alphacode doctor
alphacode
```

On Windows, `alphacode --gui` opens the desktop interface. A full walkthrough is in the [Quick start](#quick-start).

> [!TIP]
> In security-conscious environments, download and inspect the installer before running it. See [Installation options](#installation-options) for version pinning, PATH control, and source builds.

---

<a id="find"></a>
## 🧭 Find what you need

| I want to… | Go to |
|---|---|
| Try AlphaCode in under two minutes | [Quick start](#quick-start) |
| See it in action | [Demo](#demo) |
| Understand how it is put together | [The brain of AlphaCode](#brain) |
| Understand what it can do | [Features](#features) |
| Use Claude, GPT, Gemini, or a local model | [Models and providers](#providers) |
| Automate a website in Firefox | [Firefox browser automation](#firefox) |
| Control a desktop application | [Desktop automation](#desktop) |
| Run several agents in parallel | [Swarm mode](#swarm) |
| Resume earlier work | [Sessions and memory](#sessions) |
| Extend it with skills or MCP | [Tools, skills, and MCP](#tools) |
| Fix an install or setup problem | [Troubleshooting](#troubleshooting) |
| Remove AlphaCode cleanly | [Uninstall](#uninstall) |
| Help improve the project | [Contributing](#contributing) |

<details>
<summary><b>📚 Full table of contents</b></summary>

1. [What is AlphaCode?](#what-is-alphacode)
2. [Demo](#demo)
3. [Quick start](#quick-start)
4. [The brain of AlphaCode](#brain)
5. [Features](#features)
6. [Firefox browser automation](#firefox)
7. [Desktop automation](#desktop)
8. [Swarm mode](#swarm)
9. [Sessions and memory](#sessions)
10. [Models and providers](#providers)
11. [Built-in tools, skills, and MCP](#tools)
12. [Security testing and bug bounty workflows](#security-testing)
13. [Safety and reliability](#safety)
14. [Benchmarks](#benchmarks)
15. [Installation options](#installation-options)
16. [Configuration](#configuration)
17. [Commands and shortcuts](#commands)
18. [Prompting tips and recipes](#recipes)
19. [Troubleshooting](#troubleshooting)
20. [Uninstall](#uninstall)
21. [FAQ](#faq)
22. [Use cases](#use-cases)
23. [Repository layout](#layout)
24. [Documentation and project links](#links)
25. [Security reporting](#security-reporting)
26. [Contributing](#contributing)
27. [Glossary](#glossary)
28. [Support AlphaCode](#support)

</details>

---

<a id="what-is-alphacode"></a>
## 🤖 What is AlphaCode?

**AlphaCode is an open-source AI coding agent written in Rust that runs in your terminal.** Instead of only suggesting code in a chat window, it works directly in your development environment. It can inspect a repository, plan a task, edit files, run commands and tests, search the web, operate a real Firefox session, interact with native desktop applications, and report what it verified.

It belongs to the same category as terminal coding agents such as Claude Code, Codex CLI, Aider, and OpenCode, with a focus on a **small memory footprint**, **real-browser and desktop automation**, and **model freedom**.

```mermaid
flowchart LR
    A["🎯 Describe a goal"] --> B["🔍 Understand the project"]
    B --> C["🗺️ Plan the work"]
    C --> D["⚙️ Act: code, web, browser, desktop"]
    D --> E["✅ Verify with tests and evidence"]
    E --> F["📋 Report what changed"]
```

The workflow is **understand, plan, execute, observe, verify**, and it ends with a report of what changed, what was checked, and what remains.

### Why developers choose AlphaCode

| | |
|---|---|
| ⚡ **Lightweight** | A native Rust binary with a small memory footprint (see [Benchmarks](#benchmarks)) |
| 🔌 **Model-agnostic** | Claude, GPT, Gemini, Copilot, Cursor, Bedrock, Azure, OpenRouter, or any OpenAI-compatible endpoint |
| 🌐 **Beyond the editor** | Drives a real Firefox session and native desktop apps, not only files and shells |
| ✅ **Verifies its work** | Runs tests, reviews its own diff, and reports evidence |
| ♻️ **Resumable** | Sessions survive terminal restarts, crashes, and dropped SSH connections |
| 🧩 **Extensible** | Skills, MCP servers, hooks, and multi-agent swarms |
| 🆓 **Free to start** | A built-in free AI lane works without an API key |
| 📜 **Open source** | MIT licensed |

### Example tasks

Use plain English or precise technical instructions.

**🐛 Debug and fix a regression**

```text
Find why the app crashes when users upload a PDF.
Fix the root cause, add a regression test, run the relevant tests, and review the final diff.
```

**🧪 Test a web application**

```text
Open the application in Firefox, go to the account settings page,
check that the new feature is visible, and capture a screenshot.
```

**🔐 Investigate a security issue**

```text
Trace untrusted input to sensitive operations, validate likely findings,
and report which issues are reproducible in this authorized test environment.
```

Advanced workflows can combine multiple agents, reusable skills, and MCP-connected tools.

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="demo"></a>
## 🎬 Demo

A walkthrough from installation to a first completed task. Click the image to open the SVG at full size.

[![AlphaCode demo: terminal AI coding agent going from install to a first completed task](alphacode-demo.svg)](alphacode-demo.svg "Open the AlphaCode demo at full size")

If the image does not render in a third-party viewer, [open the SVG directly](alphacode-demo.svg). A video version is available as [`alphacode-demo.mp4`](alphacode-demo.mp4).

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="quick-start"></a>
## 🚀 Quick start

Four steps from zero to your first task.

### 1. Install

Use the one-line installer from [Install in 30 seconds](#install).

> [!TIP]
> In security-conscious environments, download and inspect the installer before running it. See [Installation options](#installation-options) for version pinning, PATH control, and source builds.

> [!IMPORTANT]
> On Windows, run AlphaCode from **Windows Terminal or PowerShell**. Do not double-click `alphacode.exe`; the temporary console closes as soon as the process exits.

### 2. Check your setup and launch

```bash
alphacode doctor
alphacode
```

`alphacode doctor` checks your environment and reports common setup issues. The built-in free AI lane lets you start without configuring an API key. Connected providers may require their own credentials and may incur charges.

The first launch includes a short onboarding flow for preferences such as telemetry, model defaults, and key bindings. Press `Esc` to skip any step.

### 3. Give it a task

Type a request in the terminal UI:

```text
Explain this project and identify its main entry points.
```

Or ask it to find and repair a bug:

```text
Find the highest-impact bug, fix it with the smallest reasonable change, and run the relevant tests.
```

To run a task without opening the interactive UI:

```bash
alphacode run "fix the failing test"
```

### 4. Optional: connect Firefox

Install the [AlphaCode Browser Agent extension](https://addons.mozilla.org/en-US/firefox/addon/alphacode-browser-agent/), then run:

```bash
alphacode browser setup
alphacode browser status
```

The status command reports whether the browser bridge is available and responding.

### Your first five minutes

```mermaid
flowchart TD
    I["Install"] --> D["alphacode doctor"]
    D --> L["alphacode"]
    L --> T["Ask: Explain this project"]
    T --> S["Try a small, low-risk fix"]
    S --> R["Review with /diff"]
    R --> X["Optional: Firefox, swarm, skills"]
```

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="brain"></a>
## 🧠 The brain of AlphaCode

AlphaCode is organized like a mind: an **executive** that plans, **hands** that act, **senses** that perceive the real world, a **memory** that persists, **instincts** that protect, **reflexes** you can extend, and a **brainstem** that keeps everything alive.

<div align="center">
<img src="assets/alphacode-brain.svg" alt="AlphaCode brain architecture diagram" width="100%">
</div>

### Region map

| Region | AlphaCode component | What it does |
|---|---|---|
| 🟡 **Prefrontal Cortex** | **Agent loop** | Understands goals, plans, acts, observes, verifies, and reports. Follows the [coding-quality contract](#quality-contract). |
| 🟠 **Motor Cortex** | **40+ tools** | Files, search, shell execution, web, agent workflow, and utilities. |
| 🟣 **Parietal Lobe** | **Swarm mode** | A planner splits a goal across parallel agents; a coordinator merges and a review step finalizes. |
| 🔵 **Occipital Lobe** | **Senses** | Firefox through native messaging; native desktop apps through accessibility APIs. |
| 🩷 **Temporal Lobe** | **Model providers** | Built-in free lane, hosted providers, OpenAI-compatible endpoints, and local models. |
| 🟢 **Hippocampus** | **Sessions and memory** | Persistent transcripts, crash-tolerant resume, and project memory. |
| 🔴 **Amygdala** | **Safety guardrails** | Permission prompts, destructive-target blocking, SSRF and credential-leak heuristics, desktop emergency stop. |
| 🟦 **Cerebellum** | **Skills, MCP, hooks** | Reusable workflows, external tool connections, lifecycle hooks. |
| ⚪ **Brainstem** | **Rust core runtime** | Configuration, logs, health monitor, `doctor`, `update`. |
| 🔷 **Spinal Cord** | **Interfaces** | Terminal UI, CLI, REPL, and the Windows desktop GUI. |

> [!NOTE]
> The diagram is a conceptual map of AlphaCode's capabilities, not a module-for-module view of the source tree. See [Repository layout](#layout) for the directories and the [source-grounded architecture map](docs/architecture.md) for the package and runtime call paths.

### How a request flows through the brain

```mermaid
sequenceDiagram
    autonumber
    participant You
    participant UI as Interface
    participant Loop as Agent loop
    participant LLM as Model provider
    participant Mem as Memory and sessions
    participant Safe as Safety guardrails
    participant Tools as Tools and senses

    You->>UI: Describe a goal
    UI->>Loop: Task
    Loop->>Mem: Recall project context
    Loop->>LLM: Reason about the next step
    LLM-->>Loop: Proposed action
    Loop->>Safe: Is this action allowed?
    Safe-->>Loop: Allowed or ask for permission
    Loop->>Tools: Edit, run, browse, or click
    Tools-->>Loop: Observation and evidence
    Loop->>Loop: Verify: tests, diff review
    Loop->>Mem: Persist transcript
    Loop-->>You: Report: changed, verified, remaining
```

### Subsystem overview

```mermaid
flowchart TB
    subgraph Interface["🔷 Interfaces"]
        TUI["Terminal UI"]
        CLI["CLI: run, repl"]
        GUI["Desktop GUI (Windows)"]
    end

    subgraph Executive["🟡 Executive"]
        LOOP["Agent loop"]
        SWARM["Swarm coordinator"]
    end

    subgraph Language["🩷 Language engine"]
        FREE["Built-in free lane"]
        HOSTED["Claude · GPT · Gemini · Copilot · more"]
        LOCAL["Ollama · LM Studio · OpenAI-compatible"]
    end

    subgraph Action["🟠 Action and senses"]
        FILES["Files and search"]
        EXEC["Shell execution"]
        WEB["Web tools"]
        FF["Firefox bridge"]
        DESK["Desktop automation"]
    end

    subgraph Persist["🟢 Persistence"]
        SESS["Sessions"]
        MEM["Project memory"]
    end

    subgraph Guard["🔴 Protection"]
        PERM["Permissions"]
        NET["SSRF and credential heuristics"]
        ESTOP["Emergency stop"]
    end

    subgraph Extend["🟦 Extensibility"]
        SKILLS["Skills"]
        MCP["MCP servers"]
        HOOKS["Hooks"]
    end

    subgraph Core["⚪ Rust core"]
        CFG["Config"]
        HEALTH["Health monitor"]
        DOC["doctor and update"]
    end

    Interface --> LOOP
    LOOP <--> Language
    LOOP --> Action
    LOOP <--> Persist
    LOOP <--> SWARM
    Guard -. guards .-> Action
    Extend --> Action
    Core -. supervises .-> Executive
```

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="features"></a>
## ✨ Features

| Capability | What it does |
|---|---|
| **AI coding agent** | Understands natural-language tasks, plans multi-step work, edits code, and checks results |
| **Code and file tools** | Reads and writes files, applies targeted edits and patches, and searches code |
| **Terminal execution** | Runs commands, batches operations, and supports background jobs with safety controls |
| **Web tools** | Searches, fetches, and analyzes web content and HTTP flows |
| **Firefox automation** | Works with pages, DOM, forms, tabs, frames, screenshots, downloads, and existing sessions |
| **Desktop automation** | Uses platform accessibility APIs to interact with native applications |
| **Swarm mode** | Coordinates multiple agents on work that can be split into independent tasks |
| **Persistent sessions** | Resumes work across terminal restarts |
| **Project memory** | Supports continuity during longer workflows |
| **Skills and MCP** | Adds reusable workflows and connections to external tools and services |
| **Model choice** | Supports multiple hosted providers and OpenAI-compatible endpoints |
| **Cross-platform** | Supports Windows, macOS, and Linux |

### Coding and repository work

Explore an unfamiliar codebase, trace a bug, refactor a module, add a feature, run a build, create regression tests, review a diff, or investigate a failure.

### Web workflows and quality assurance

Combine web tools with a real browser to inspect dynamic pages, fill forms, validate user flows, capture screenshots, and support web application QA. Browser automation is especially useful when a page depends on JavaScript or an existing authenticated session.

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="firefox"></a>
## 🌐 Firefox browser automation

The [AlphaCode Browser Agent](https://addons.mozilla.org/en-US/firefox/addon/alphacode-browser-agent/) is a Firefox extension that connects AlphaCode to a real Firefox session through native messaging. AlphaCode handles reasoning and orchestration, and Firefox provides the live browser environment.

```mermaid
flowchart LR
    U["You"] --> A["AlphaCode agent"]
    A --> B["Browser tool"]
    B --> C["Native messaging"]
    C --> D["Browser Agent extension"]
    D --> F["Firefox"]
    F --> G["Page DOM, forms, frames, UI"]
    G --> D
    D --> A
    A --> R["Reported result"]
```

Browser automation helps with:

- JavaScript-heavy sites and dynamic interfaces
- DOM inspection, form entry, and interactive controls
- Tabs, frames, and embedded content
- Existing authenticated sessions
- Scrolling, navigation, screenshots, and downloads
- Browser-based testing, QA, and web application debugging
- Security testing of systems you own or are explicitly authorized to assess

Example instruction:

```text
Open the application, test the login flow, navigate to the dashboard,
check the form validation, and report reproducible problems.
```

### Setup

```bash
alphacode browser setup     # register the native messaging bridge
alphacode browser status    # confirm the bridge is available and responding
```

Install the extension from [addons.mozilla.org](https://addons.mozilla.org/en-US/firefox/addon/alphacode-browser-agent/). The repository also includes a packaged build, `AlphaCode-Browser-Agent-1.6.4.xpi`.

> [!WARNING]
> **Privacy note:** Browser automation acts inside a real browser session. Review requested actions carefully, and treat page content as untrusted input.

If the agent is not responding, see [Troubleshooting](#troubleshooting).

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="desktop"></a>
## 🖥️ Desktop automation

AlphaCode interacts with native applications through platform accessibility APIs rather than relying only on fixed screen coordinates.

| Platform | Accessibility backend |
|---|---|
| Windows | UI Automation |
| macOS | Accessibility API |
| Linux | AT-SPI2 |

**Supported actions:**

| Action | Action | Action | Action |
|---|---|---|---|
| `list_windows` | `snapshot` | `find` | `click` |
| `type` | `press` | `scroll` | `focus` |
| `screenshot` | `toggle` | `expand` | `collapse` |

Example:

```text
Open Calculator, calculate 123 × 456, and read the result.
```

The expected result is `56088`. Actual desktop interaction depends on a working accessibility backend and on the target application being available. If it does not work, see [Desktop automation is unavailable](#troubleshooting).

### Permissions at a glance

| OS | What to check |
|---|---|
| macOS | Grant **Accessibility** and **Screen Recording** under *System Settings → Privacy & Security* |
| Linux | Confirm **AT-SPI2** is available |
| Windows | Elevated applications may require AlphaCode to run with appropriate privileges |

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="swarm"></a>
## 🐝 Swarm mode

Swarm mode splits a large goal into parallel or semi-independent tasks and coordinates the results.

```mermaid
flowchart TD
    G["Large goal"] --> P["Planner"]
    P --> A["Agent A: code analysis"]
    P --> B["Agent B: security review"]
    P --> C["Agent C: tests"]
    P --> D["Agent D: documentation"]
    A --> M["Coordinator"]
    B --> M
    C --> M
    D --> M
    M --> R["Review"]
    R --> F["Final result"]
```

Example command inside AlphaCode:

```text
/swarm "Analyze this application from architecture, security, testing, and performance perspectives."
```

> [!NOTE]
> Swarm mode works best when a task can be divided so that agents do not modify the same files at the same time.

### When to use a swarm

| Good fit | Poor fit |
|---|---|
| Independent analyses of the same codebase (architecture, security, tests, docs) | Several agents editing the same file |
| Broad audits that can be split by area | A small, single-file fix |
| Work where results can be merged and reviewed at the end | Tightly sequential steps that each depend on the last |

Manage agent behavior with `/agents` and the `[agents]` section of your [configuration](#configuration).

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="sessions"></a>
## 💾 Sessions and memory

Persistent sessions preserve context across terminal restarts and support longer debugging, research, security-review, and refactoring workflows.

```bash
alphacode sessions list      # List saved sessions
alphacode --resume           # Resume the latest session
alphacode --resume <id>      # Resume a specific session
```

Project memory supports continuity during longer workflows. Use `/memory` inside AlphaCode to manage or inspect it.

### Resilience

Crashes, signals, or dropped SSH connections can mark a session as `Crashed` and provide a resume command. Transcripts are persisted to disk rather than kept entirely in process memory. See [Session resilience](#session-resilience).

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="providers"></a>
## 🔌 Models and providers

AlphaCode is model-agnostic. Start with the built-in free AI lane, or connect a provider using the authentication method it supports.

| Provider | Authentication or requirement | Notes |
|---|---|---|
| **Built-in free AI lane** | Built in | Designed to work without an API key |
| **Anthropic (Claude)** | OAuth or API key | Provider support is built in |
| **OpenAI (GPT and o-series)** | OAuth, API key, or supported sign-in flow | Reasoning, vision, and tools depend on the selected model and flow |
| **Google Gemini** | OAuth or API key | |
| **GitHub Copilot** | OAuth | Requires an eligible account or subscription |
| **Cursor** | OAuth | Reuses a Cursor session where supported |
| **AWS Bedrock** | AWS credentials | Build with `--features bedrock` when required |
| **Azure** | Azure AD or API key | Build with `--features azure-auth` when required |
| **OpenRouter** | API key | Access to a wide range of models |
| **OrcaRouter** | API key or browser login (OAuth 2.0 + PKCE) | One gateway for many models. The model list is read from `GET /v1/models`, so the picker excludes models advertised only for non-chat tasks |
| **OpenAI-compatible endpoint** | Endpoint-specific key or configuration | Add with `alphacode provider add <name>` |

### Provider and model commands

```bash
alphacode login                    # Open the provider sign-in flow
alphacode login --provider openai  # Sign in with a specific provider
alphacode provider list            # List configured providers
alphacode provider add <name>      # Add an OpenAI-compatible endpoint
alphacode provider current         # Show the active provider and model
alphacode model list               # List available models
alphacode model use <model>        # Select a model
```

In the terminal UI, type `/model` to open the model and provider selector. `Shift+Tab` cycles favorited models, and `Ctrl+Y` accepts the suggested fallback model after an error.

### Local models (Ollama, LM Studio, and other OpenAI-compatible servers)

**LM Studio**

1. Load a chat model in LM Studio.
2. Start **Local Server** on its default port (`1234`).
3. Run:

```bash
alphacode --provider lmstudio
```

AlphaCode asks `http://localhost:1234/v1/models` for the currently available models and picks the first chat model. To choose one explicitly, use the exact ID shown by LM Studio:

```bash
alphacode --provider lmstudio --model <model-id>
```

**Ollama**

Start the server, then run:

```bash
alphacode --provider ollama
alphacode --provider ollama --model <model-id>    # optional explicit model
```

AlphaCode uses the context window reported by the server when available. For local models with up to 32K context, it uses a shorter task prompt and a bounded set of relevant tool schemas. Simple greetings may not need tool schemas at all.

> [!NOTE]
> This reduces prompt overhead but does **not** increase the model server's configured context window. Set the context length in the server itself, and leave enough room for output.

### Authentication notes

Many providers use environment variables such as `ANTHROPIC_API_KEY` or `OPENAI_API_KEY`. You can also run `alphacode login` from your shell, or `/login` inside AlphaCode.

**OpenAI Codex OAuth:** AlphaCode uses a local callback at `http://localhost:1455/auth/callback` during sign-in. Make sure port `1455` is free. If the login flow times out, check whether another process is listening on that port, then retry `/login`. See [OpenAI login times out](#troubleshooting).

See the [documentation](docs) and [`OAUTH.md`](OAUTH.md) for provider-specific setup and troubleshooting.

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="tools"></a>
## 🧰 Built-in tools, skills, and MCP

AlphaCode includes **40+ tools**. Names and availability can vary with the build and enabled features.

| Category | Examples |
|---|---|
| **Files** | `read`, `write`, `edit`, `multiedit`, `patch`, `apply_patch`, `ls` |
| **Search** | `alphagrep`, `session_search`, `conversation_search` |
| **Execution** | `bash`, `batch`, `bg` |
| **Web and browser** | `browser`, `webfetch`, `websearch`, `scrapling`, `httpflow`, `open` |
| **Desktop** | `desktop`, `macos_computer_use` |
| **Agent workflow** | `memory`, `initiative`, `todo`, `plan`, `swarm` |
| **Utilities** | `doctor`, `self_improve`, `selfdev`, `cron`, `schedule`, `jwt`, `clipboard`, `side_panel`, `discover_tools` |

### Skills

Skills add reusable, specialized workflows without changing the core agent. Browse them with `/skills`. Examples from the project include `/bugbounty`, `/meme-coin-audit`, and `/frontend-design`.

### Model Context Protocol (MCP)

MCP support lets you connect external tools, services, and custom integrations. See the project [documentation](docs) for the configuration each server requires.

### Hooks

Lifecycle hooks run at defined points in a session and are configured in the `[hooks]` section of your [configuration](#configuration). Hooks inherit the attached client's terminal environment, so they target the pane a session is actually shown in.

### Extension points at a glance

| Mechanism | Best for | Changes the core? |
|---|---|---|
| **Skills** | Repeatable, specialized workflows | No |
| **MCP servers** | Connecting external tools and services | No |
| **Hooks** | Reacting to lifecycle events | No |
| **OpenAI-compatible providers** | Adding model endpoints | No |

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="security-testing"></a>
## 🛡️ Security testing and bug bounty workflows

Code analysis, HTTP tools, browser automation, terminal execution, memory, and multi-agent workflows can support repeatable application-security reviews.

```text
Review this API for insecure direct object references and provide reproducible evidence.
```

```text
Trace this parameter through the application and determine whether it reaches a dangerous sink.
```

```text
Use the authorized test environment in Firefox to test input validation and document reproducible findings.
```

> [!CAUTION]
> **Authorization required.** Test only systems you own or are explicitly authorized to assess.

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="safety"></a>
## 🔒 Safety and reliability

An agent that can change files, execute commands, and operate applications needs careful oversight. The project describes these safeguards:

| Safeguard | Applies to |
|---|---|
| Blocking certain destructive filesystem and device targets | Files and shell |
| Permission prompts for risky actions | All tools |
| Safety controls for shell execution | Terminal |
| SSRF and credential-leak heuristics | Network operations |
| Tracking interrupted sessions rather than silently losing them | Sessions |
| Timeouts and emergency-stop handling | Desktop automation |
| Input-state cleanup when operations fail | Desktop automation |
| Treating browser and desktop content as potentially untrusted | Senses |

> [!WARNING]
> These safeguards reduce risk; they do not guarantee that every action is safe or correct. **Review important diffs before merging or deploying.** Use `/diff` to inspect changes.

<a id="session-resilience"></a>
### Session resilience

Sessions are resumable and designed to recover from interruptions. Crashes, signals, or dropped SSH connections can mark a session as `Crashed` and provide a resume command. Transcripts are persisted to disk rather than kept entirely in process memory.

<a id="quality-contract"></a>
### Coding-quality contract

For code-changing tasks, AlphaCode is instructed to follow these guardrails:

1. **Small changes:** avoid bundling unrelated edits, and report adjacent issues separately.
2. **Regression awareness:** rerun relevant tests and investigate new warnings or failures.
3. **Self-review:** check that the objective is covered, evidence is available, regressions are considered, the diff is scoped, and edge cases are reviewed.
4. **Structured reporting:** state what changed, what was verified, and what remains.

The agent's instructions are not a substitute for independently reviewing the implementation and test results.

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="benchmarks"></a>
## 📊 Benchmarks

AlphaCode is written in Rust and designed to keep runtime overhead modest.

> [!NOTE]
> These are **historical measurements**, not a current independent benchmark or a guarantee of performance on every machine or workload. Competing tools change quickly, so re-measure before drawing conclusions.

### Memory usage: one active session

| Tool | RAM | Relative to AlphaCode |
|---|---:|---:|
| **AlphaCode** | **27.8 MB** | **1.0×** |
| Codex CLI | 140.0 MB | 5.0× |
| pi | 144.4 MB | 5.2× |
| Cursor Agent | 214.9 MB | 7.7× |
| Antigravity CLI | 243.7 MB | 8.8× |
| GitHub Copilot CLI | 333.3 MB | 12.0× |
| OpenCode | 371.5 MB | 13.4× |
| Claude Code | 386.6 MB | 13.9× |

### Memory usage: ten concurrent sessions

| Tool | RAM | Relative to AlphaCode |
|---|---:|---:|
| **AlphaCode** | **117.0 MB** | **1.0×** |
| Codex CLI | 334.8 MB | 2.9× |
| pi | 833.0 MB | 7.1× |
| Antigravity CLI | 1,021.2 MB | 8.7× |
| Cursor Agent | 1,632.4 MB | 14.0× |
| GitHub Copilot CLI | 1,756.5 MB | 15.0× |
| Claude Code | 2,300.6 MB | 19.7× |
| OpenCode | 3,237.2 MB | 27.7× |

### Reproducing a comparison

Compare tools under the same conditions and record:

- AlphaCode version or commit
- OS, CPU, and RAM
- Build profile and enabled features
- Number of concurrent sessions
- Measurement method and warm/cold state
- The exact workload

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="installation-options"></a>
## 📦 Installation options

The quick-start commands use the latest installer. This section covers optional flags and source builds. Inspect remote scripts before running them if your environment requires manual review.

### Windows

```powershell
irm https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/install.ps1 | iex
```

The installer:

- detects your CPU architecture and downloads a release;
- verifies the release's SHA-256 checksum;
- installs `alphacode.exe` under `%LOCALAPPDATA%\Programs\alphacode\bin\`;
- adds that folder to the current user's `Path` and activates it in the current shell;
- needs **no administrator rights** for the user-level install.

<details>
<summary><b>How the Windows PATH change works</b></summary>

The installer writes the user-level `HKCU\Environment\Path` rather than the machine-wide path. It appends its entry, avoids duplicates on re-run, and broadcasts `WM_SETTINGCHANGE` so new terminals can pick up the change. Existing shell-only PATH entries are preserved.

</details>

To pass installer parameters, load the script as a script block instead of piping directly to `iex`:

| Parameter | Purpose |
|---|---|
| `-Version vX.Y.Z` | Pin a release version |
| `-FromSource` | Build from source instead of downloading a release |
| `-PathDryRun` | Preview the PATH change without writing it |
| `-NoPath` | Do not modify PATH |
| `-Prefix <dir>` | Choose a custom installation prefix |

<details>
<summary><b>Windows examples with parameters</b></summary>

```powershell
$installer = [scriptblock]::Create((irm https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/install.ps1))

# Pin a release version
& $installer -Version vX.Y.Z

# Build from source instead of downloading a release
& $installer -FromSource

# Preview the PATH change without writing it
& $installer -PathDryRun

# Do not modify PATH
& $installer -NoPath

# Choose a custom installation prefix
& $installer -Prefix "$env:LOCALAPPDATA\Programs\alphacode"
```

Run one parameter at a time, or combine them in a single call such as `& $installer -Version vX.Y.Z -NoPath`. Replace `vX.Y.Z` with a real tag from the [releases page](https://github.com/dragonked2/alphacode/releases).

</details>

### macOS and Linux

```bash
curl -fsSL https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/install.sh | bash
```

The installer downloads a release, verifies its checksum, installs `alphacode` under `~/.local/bin`, and updates a supported shell profile. It is documented as supporting bash, zsh, fish, nushell, csh, and ksh, and is safe to re-run without duplicating its profile entry.

| Option | Purpose |
|---|---|
| `--version vX.Y.Z` | Pin a release version |
| `--no-path` | Do not modify PATH |
| `--link` | Link into `/usr/local/bin` |
| `--prefix <dir>` | Choose a custom installation prefix |

<details>
<summary><b>macOS / Linux examples with options</b></summary>

```bash
# Pin a release version
curl -fsSL https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/install.sh | bash -s -- --version vX.Y.Z

# Do not modify PATH
curl -fsSL https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/install.sh | bash -s -- --no-path

# Link into /usr/local/bin
curl -fsSL https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/install.sh | bash -s -- --link

# Choose a custom installation prefix
curl -fsSL https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/install.sh | bash -s -- --prefix ~/.local
```

</details>

### Verify installation

```bash
alphacode --version
alphacode doctor
```

On Windows you can also check command resolution:

```powershell
Get-Command alphacode
```

### Build from source

```bash
git clone https://github.com/dragonked2/alphacode.git
cd alphacode
cargo build --release
./target/release/alphacode --version
```

The project pins Rust edition 2024 in [`rust-toolchain.toml`](rust-toolchain.toml), and a C toolchain for your platform is also required.

The default build omits some heavier optional stacks. Enable them when you need them:

```bash
cargo build --release --features bedrock,embeddings,pdf,renderer
```

For Azure AD authentication, add `azure-auth`, for example `--features bedrock,azure-auth,embeddings,pdf,renderer`.

| Cargo feature | Enables |
|---|---|
| `bedrock` | AWS Bedrock provider |
| `azure-auth` | Azure AD authentication |
| `embeddings` | Optional embeddings stack |
| `pdf` | Optional PDF stack |
| `renderer` | Optional renderer stack |

### Update

```bash
alphacode update
```

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="configuration"></a>
## ⚙️ Configuration

AlphaCode stores its configuration, sessions, and logs outside the project directory.

| OS | Configuration | Sessions | Logs |
|---|---|---|---|
| Linux | `~/.config/alphacode/` | `~/.local/share/alphacode/sessions/` | `~/.local/share/alphacode/logs/` |
| macOS | `~/Library/Application Support/alphacode/` | `~/Library/Application Support/alphacode/` | `~/Library/Application Support/alphacode/` |
| Windows | `%APPDATA%\alphacode\` | `%LOCALAPPDATA%\alphacode\sessions\` | `%LOCALAPPDATA%\alphacode\logs\` |

A health monitor records memory use, slow operations, error rates, and subsystem liveness in `health.json`.

| Section | Purpose |
|---|---|
| `[provider]` | Provider and model selection |
| `[features]` | Feature flags |
| `[display]` | UI preferences |
| `[websearch]` | Search settings |
| `[agents]` | Agent and swarm settings |
| `[hooks]` | Lifecycle hooks |
| `[safety]` | Notifications and safety controls |
| `[compaction]` | Context management |
| `[power]` | Power-management behavior |
| `[gateway]` | Remote gateway settings |

### Themes

Royal Gold is the default terminal look, with a matching palette for light terminal backgrounds. Use `/theme` to browse other themes, `/theme royal-gold` to select it, or `/theme off` to return to the default. Your saved theme and individual `/colors` overrides are preserved.

See [`docs/configuration.md`](docs/configuration.md) for the full reference.

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="commands"></a>
## ⌨️ Commands and shortcuts

### CLI commands

| Command | Purpose |
|---|---|
| `alphacode` | Launch the interactive terminal UI |
| `alphacode doctor` | Check setup and common environment issues |
| `alphacode login` | Authenticate with a provider |
| `alphacode run "<task>"` | Run a task non-interactively |
| `alphacode repl` | Start the simple REPL without the TUI |
| `alphacode provider add <name>` | Add an OpenAI-compatible endpoint |
| `alphacode provider list` | List configured providers |
| `alphacode provider current` | Show the active provider and model |
| `alphacode model list` | List available models |
| `alphacode model use <model>` | Select a model |
| `alphacode browser setup` | Configure the browser bridge |
| `alphacode browser status` | Check browser connectivity |
| `alphacode sessions list` | List saved sessions |
| `alphacode --resume [id]` | Resume the latest session, or a specific one |
| `alphacode --version` | Show the installed version |
| `alphacode update` | Update AlphaCode |

### Slash commands

| Command | Purpose |
|---|---|
| `/help` | Show help |
| `/login` | Sign in to a provider from inside AlphaCode |
| `/agents` | Manage agents |
| `/swarm "<goal>"` | Split a large goal across multiple agents |
| `/compact` | Compact context |
| `/memory` | Manage or inspect project memory |
| `/skills` | Browse skills |
| `/diff` | Review changes |
| `/poke` | Trigger an automatic follow-up |
| `/screenshot-mode` | Configure screenshot capture |
| `/exit` | Exit while preserving the session |

### Keyboard shortcuts

| Shortcut | Action |
|---|---|
| `F1` | Show shortcut reference |
| `Ctrl+T` | Toggle queue mode |
| `Ctrl+Y` | Accept the suggested fallback model after an error |
| `Ctrl+C` | Interrupt the current turn (quit when idle) |
| `Esc` | Close a dialog or go back |
| `/model` | Open the model and provider picker |
| `Shift+Tab` | Cycle favorited models |

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="recipes"></a>
## 🍳 Prompting tips and recipes

Good prompts state the goal, the scope, and how success is checked.

| Do | Example |
|---|---|
| Name the outcome | "Fix the failing upload test." |
| Bound the change | "Use the smallest reasonable change." |
| Ask for verification | "Run the relevant tests and review the final diff." |
| State authorization for security work | "In this authorized test environment…" |

**Understand a project**

```text
Explain this project and identify its main entry points.
```

**Fix with evidence**

```text
Find the highest-impact bug, fix it with the smallest reasonable change, and run the relevant tests.
```

**Headless run**

```bash
alphacode run "fix the failing test"
```

**Parallel review**

```text
/swarm "Analyze this application from architecture, security, testing, and performance perspectives."
```

**Browser QA**

```text
Open the application, test the login flow, navigate to the dashboard,
check the form validation, and report reproducible problems.
```

**Desktop check**

```text
Open Calculator, calculate 123 × 456, and read the result.
```

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="troubleshooting"></a>
## 🩺 Troubleshooting

Start with `alphacode doctor`, which catches most setup problems. Then find your symptom below.

<details>
<summary><b><code>alphacode</code> command not found</b></summary>

Open a **new** terminal first so it can read the updated PATH. If it is still not found, confirm the install location and PATH settings.

**bash:**

```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

**zsh (default on modern macOS):**

```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

**fish:**

```fish
fish_add_path $HOME/.local/bin
```

**Windows:** re-run the installer (unless you used `-NoPath`), or add `%LOCALAPPDATA%\Programs\alphacode\bin` to your user PATH and open a new PowerShell window.

</details>

<details>
<summary><b>The Firefox Browser Agent is not responding</b></summary>

```bash
alphacode browser status
alphacode browser setup
```

Then confirm that the [AlphaCode Browser Agent extension](https://addons.mozilla.org/en-US/firefox/addon/alphacode-browser-agent/) is installed and enabled in Firefox.

</details>

<details>
<summary><b>Desktop automation is unavailable</b></summary>

- **macOS:** grant Accessibility and Screen Recording permissions under *System Settings → Privacy & Security*.
- **Linux:** check that AT-SPI2 is available.
- **Windows:** elevated applications may require AlphaCode to run with appropriate privileges.

</details>

<details>
<summary><b>OpenAI login times out</b></summary>

Port `1455` on localhost may already be in use. Check for another process listening on that port, stop it if appropriate, and retry `/login`. See the [documentation](docs) for provider-specific steps.

</details>

<details>
<summary><b>The demo image does not display</b></summary>

The demo asset is `alphacode-demo.svg`, not a video. Confirm that the file is committed at the repository root and that its filename capitalization matches the link. You can also [open the SVG directly](alphacode-demo.svg).

</details>

Still stuck? Run `alphacode doctor` and [open an issue](https://github.com/dragonked2/alphacode/issues) with reproducible steps and relevant logs.

> [!WARNING]
> Remove secrets and tokens from diagnostic output before sharing it.

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="uninstall"></a>
## 🗑️ Uninstall

Close active AlphaCode sessions first. On Windows, a running `alphacode.exe` cannot be removed until its process exits.

### Recommended: uninstall script

**macOS / Linux**

```bash
curl -fsSL https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/uninstall.sh | bash
```

**Windows (PowerShell)**

```powershell
irm https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/uninstall.ps1 | iex
```

By default the scripts remove the installed executable and leave your configuration and saved sessions in place.

### Remove all local data

> [!CAUTION]
> Purging is **permanent**. It may delete configuration, saved sessions, logs, and locally stored sign-in data. Back up anything you need first.

**macOS / Linux**

```bash
curl -fsSL https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/uninstall.sh | bash -s -- --purge
```

Equivalent environment-variable form:

```bash
curl -fsSL https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/uninstall.sh | ALPHACODE_PURGE=1 bash
```

**Windows (PowerShell)**

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/uninstall.ps1))) -Purge
```

### Manual uninstall

| Item | Linux | macOS | Windows |
|---|---|---|---|
| Binary | `~/.local/bin/alphacode` | `~/.local/bin/alphacode` | `%LOCALAPPDATA%\Programs\alphacode\` |
| Config | `~/.config/alphacode/` | `~/Library/Application Support/alphacode/` | `%APPDATA%\alphacode\` |
| Sessions | `~/.local/share/alphacode/sessions/` | Application Support directory | `%LOCALAPPDATA%\alphacode\sessions\` |
| Logs | `~/.local/share/alphacode/logs/` | Application Support directory | `%LOCALAPPDATA%\alphacode\logs\` |

Remove only the binary to keep your settings and sessions. Delete the data directories only if you intend to remove local state as well.

**Linux**

```bash
rm -f ~/.local/bin/alphacode
# Optional: remove configuration, sessions, and logs
rm -rf ~/.config/alphacode ~/.local/share/alphacode
```

**macOS**

```bash
rm -f ~/.local/bin/alphacode
# Optional: remove configuration and local data
rm -rf "$HOME/Library/Application Support/alphacode"
```

**Windows (PowerShell)**

```powershell
Remove-Item -Recurse -Force "$env:LOCALAPPDATA\Programs\alphacode"
# Optional: remove configuration and local data
Remove-Item -Recurse -Force "$env:APPDATA\alphacode", "$env:LOCALAPPDATA\alphacode"
```

If you installed with `cargo install --path .`, run `cargo uninstall alphacode`. If you built from a clone, delete the cloned repository separately.

### Additional cleanup

- **PATH entry:** remove the AlphaCode entry from your shell profile or user PATH. On Windows, use the Environment Variables dialog so `%USERPROFILE%`-style entries are not expanded unintentionally.
- **Firefox extension:** open `about:addons`, select *AlphaCode Browser Agent*, and choose **Remove**.
- **Native messaging registration:** if you ran `alphacode browser setup`, remove the AlphaCode registration after checking its exact name:
  - Linux: files in `~/.mozilla/native-messaging-hosts/`
  - macOS: files in `~/Library/Application Support/Mozilla/NativeMessagingHosts/`
  - Windows: a **registry key** under `HKCU\Software\Mozilla\NativeMessagingHosts` (remove it with Registry Editor or `Remove-Item -Path "HKCU:\Software\Mozilla\NativeMessagingHosts\<name>"`)
- **Credentials:** remove exported `*_API_KEY` variables, and revoke provider API keys or OAuth grants you no longer need.
- **Scheduled jobs:** remove any recurring tasks created with `cron` or `schedule` **before** deleting local application data.

### Verify removal

```bash
command -v alphacode
```

On Windows:

```powershell
Get-Command alphacode -ErrorAction SilentlyContinue
```

If the shell still finds the command, open a new terminal or refresh the shell's command cache.

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="faq"></a>
## ❓ FAQ

<details>
<summary><b>Is AlphaCode free?</b></summary>

AlphaCode is open source under the MIT License and includes a built-in free AI lane for getting started without an API key. Third-party model providers may impose their own usage limits, terms, or charges.

</details>

<details>
<summary><b>Do I need an API key?</b></summary>

Not to get started with the built-in free AI lane. Other providers may require sign-in, API keys, or additional configuration.

</details>

<details>
<summary><b>What is the best way to try AlphaCode?</b></summary>

Install it, run `alphacode doctor`, launch `alphacode` in a project folder, and ask it to explain the project. Then try a small, low-risk fix such as a failing test.

</details>

<details>
<summary><b>Which operating systems are supported?</b></summary>

Windows, macOS, and Linux. Individual features and desktop automation backends vary by operating system and build configuration.

</details>

<details>
<summary><b>Can AlphaCode use Claude, GPT, Gemini, or local models like Ollama?</b></summary>

Yes. It supports multiple providers and any OpenAI-compatible endpoint, including local servers such as Ollama and LM Studio. Authentication methods and model capabilities depend on your selected provider and model. See [Models and providers](#providers).

</details>

<details>
<summary><b>Can AlphaCode automate websites?</b></summary>

Yes. The Firefox integration supports real-browser workflows such as navigation, DOM and form interaction, tabs, frames, screenshots, and downloads. It requires a correctly configured browser bridge and extension.

</details>

<details>
<summary><b>Can AlphaCode control desktop applications?</b></summary>

Yes, through platform accessibility APIs on Windows, macOS, and Linux. Required permissions and the level of support depend on the operating system and target application.

</details>

<details>
<summary><b>Is AlphaCode suitable for beginners?</b></summary>

You can start with plain-English instructions. Review code changes and actions carefully, particularly before deploying software or running sensitive operations.

</details>

<details>
<summary><b>Can I use AlphaCode for security testing?</b></summary>

Yes, for authorized security research and testing. Only assess systems you own or have explicit permission to test. See [Security testing and bug bounty workflows](#security-testing).

</details>

<details>
<summary><b>Does AlphaCode remember previous work?</b></summary>

Persistent sessions and project memory support longer workflows across terminal restarts. Use `alphacode sessions list` and `alphacode --resume` to continue saved work.

</details>

<details>
<summary><b>How do I update AlphaCode?</b></summary>

Run `alphacode update`.

</details>

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="use-cases"></a>
## 🎯 Use cases

| Area | Examples |
|---|---|
| **Software development** | Debugging, refactoring, feature work, test generation, code review, repository exploration, documentation, build troubleshooting |
| **Web development** | Frontend and backend changes, API testing, browser-based testing, UI validation |
| **Application security** | Authorized penetration testing, bug bounty work, code audits, HTTP analysis, reproducible vulnerability reports |
| **QA and testing** | Regression testing, form validation, UI verification, reproducibility checks |
| **Automation** | Browser workflows, desktop applications, repetitive tasks, supported data-collection workflows |

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="layout"></a>
## 🗂️ Repository layout

| Path | Contents |
|---|---|
| [`src/`](src) | Rust source |
| [`alphagrep/`](alphagrep) | Code-search component |
| [`tests/`](tests) | Tests |
| [`docs/`](docs) | Documentation, including the [configuration reference](docs/configuration.md) |
| [`scripts/`](scripts) | Install and uninstall scripts |
| [`assets/`](assets) | App icons and the brain diagram |
| [`.github/`](.github) | GitHub configuration |
| [`AGENTS.md`](AGENTS.md) | Agent instructions |
| [`OAUTH.md`](OAUTH.md) | OAuth notes |
| [`CHANGELOG.md`](CHANGELOG.md) | Release history |
| [`Cargo.toml`](Cargo.toml), [`build.rs`](build.rs), [`Makefile`](Makefile) | Build configuration |
| [`rust-toolchain.toml`](rust-toolchain.toml) | Pinned Rust toolchain (edition 2024) |
| `AlphaCode-Browser-Agent-1.6.4.xpi` | Packaged Firefox extension |

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="links"></a>
## 🔗 Documentation and project links

- [Documentation index](docs)
- [Configuration reference](docs/configuration.md)
- [Changelog](CHANGELOG.md)
- [Contributing guide](CONTRIBUTING.md)
- [Code of conduct](CODE_OF_CONDUCT.md)
- [Security policy](SECURITY.md)
- [Official website](https://alphacli.github.io/)
- [GitHub releases](https://github.com/dragonked2/alphacode/releases)
- [Firefox Browser Agent](https://addons.mozilla.org/en-US/firefox/addon/alphacode-browser-agent/)

---

<a id="security-reporting"></a>
## 🚨 Security reporting

Report vulnerabilities **privately** using the instructions in [`SECURITY.md`](SECURITY.md). For issues specific to the Firefox extension, Mozilla's add-on reporting tools may also be appropriate.

---

<a id="contributing"></a>
## 🤝 Contributing

Issues, reproducible bug reports, documentation updates, and pull requests are welcome.

```bash
git clone https://github.com/dragonked2/alphacode.git
cd alphacode
cargo build --release
cargo test --lib
cargo clippy --lib -- -D warnings
```

Before opening a pull request, check that:

- [ ] The release build succeeds
- [ ] Relevant tests pass
- [ ] New behavior includes tests where appropriate
- [ ] Clippy passes
- [ ] Public APIs are documented
- [ ] New dependencies are justified
- [ ] User-visible changes are documented

Read [`CONTRIBUTING.md`](CONTRIBUTING.md) before submitting a change.

### Contributors

Thanks to everyone who has contributed code, tests, or reviews to AlphaCode.

- [@levz0r](https://github.com/levz0r): lifecycle hooks now inherit the attached client's terminal env ([#20](https://github.com/dragonked2/alphacode/issues/20)), so hooks target the pane a session is actually shown in.

<div align="right"><a href="#top">⬆ Back to top</a></div>

---

<a id="glossary"></a>
## 📖 Glossary

| Term | Meaning |
|---|---|
| **Agent loop** | The understand, plan, execute, observe, verify cycle |
| **Free AI lane** | The built-in model path that works without an API key |
| **Swarm** | Several agents working on independent parts of one goal |
| **Skill** | A reusable workflow added without changing the core agent |
| **MCP** | Model Context Protocol, for connecting external tools and services |
| **Hook** | A lifecycle callback configured in `[hooks]` |
| **Browser bridge** | Native messaging link between AlphaCode and the Firefox extension |
| **Accessibility backend** | The OS API used for desktop automation (UI Automation, Accessibility API, AT-SPI2) |
| **Session** | A resumable, persisted transcript of work |
| **Compaction** | Context management, configured in `[compaction]` |
| **Doctor** | `alphacode doctor`, the environment checker |

---

<a id="support"></a>
## 💛 Support AlphaCode

If AlphaCode saves you time, star the repository, report reproducible bugs, suggest improvements, improve the docs, test releases, or contribute code. Sharing it with a teammate helps too.

<div align="center">

[![Star AlphaCode on GitHub](https://img.shields.io/badge/Star%20AlphaCode%20on%20GitHub-FFD34E?style=for-the-badge&labelColor=1a1a2e&logo=github&logoColor=black)](https://github.com/dragonked2/alphacode)
[![Get the Firefox Browser Agent](https://img.shields.io/badge/Get%20the%20Firefox%20Browser%20Agent-FF7139?style=for-the-badge&logo=firefoxbrowser&logoColor=white)](https://addons.mozilla.org/en-US/firefox/addon/alphacode-browser-agent/)
[![Support the project](https://img.shields.io/badge/Support%20the%20Project-FFDD00?style=for-the-badge&logo=buymeacoffee&logoColor=black)](https://www.buymeacoffee.com/dragonked2)

<br>

**Built with Rust by [Ali Essam](https://github.com/dragonked2) · MIT licensed · Open source**

<a href="#top">⬆ Back to top</a>

</div>