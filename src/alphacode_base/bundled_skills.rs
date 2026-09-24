//! Bundled skills that ship inside the compiled alphacode binary.
//!
//! Each bundled skill lives as a directory tree of `SKILL.md` files
//! committed under `bundled_skills/<skill>/...`. The top-level
//! `<skill>/SKILL.md` defines the skill itself; nested subskill
//! `SKILL.md` files are merged into the parent as named references
//! (e.g. `hunt-sqli.md`, `recon.md`) so they are available contextually
//! when the skill is invoked, but they are not exposed as separate
//! slash commands. This keeps the slash-command surface focused on the
//! parent skill while preserving the detailed methodology content.
//!
//! Skills are embedded at compile time via [`include_str!`] so they are
//! always available regardless of working directory, `$HOME`, or any
//! on-disk install. Users can still override or shadow these with
//! skills placed in `~/.alphacode/skills/` or `./.alphacode/skills/` —
//! the project-local overlay always wins, mirroring the existing load
//! order.
//!
//! To add a new bundled skill:
//!  1. Create the directory tree under `bundled_skills/<name>/...`.
//!  2. Add an entry below in [`bundled_skills`] with the top-level
//!     `SKILL.md` body and a list of `(name, body)` reference pairs.

/// Top-level bundled skills: `(skill_name, top_level_body, references)`.
///
/// `references` are merged into the skill's `reference_files` map keyed
/// by the subskill directory name (e.g. `hunt-sqli`).
pub(super) const BUNDLED_SKILLS: &[BundledSkill] = &[
    BundledSkill {
        name: "bugbounty",
        body: include_str!("bundled_skills/bugbounty/SKILL.md"),
        references: &[
            (
                "advanced-techniques",
                include_str!("bundled_skills/bugbounty/advanced-techniques/SKILL.md"),
            ),
            (
                "client-reverse",
                include_str!("bundled_skills/bugbounty/client-reverse/SKILL.md"),
            ),
            (
                "credential-attack",
                include_str!("bundled_skills/bugbounty/credential-attack/SKILL.md"),
            ),
            (
                "hunt-api",
                include_str!("bundled_skills/bugbounty/hunt-api/SKILL.md"),
            ),
            (
                "hunt-graphql",
                include_str!("bundled_skills/bugbounty/hunt-graphql/SKILL.md"),
            ),
            (
                "hunt-idor",
                include_str!("bundled_skills/bugbounty/hunt-idor/SKILL.md"),
            ),
            (
                "hunt-memory",
                include_str!("bundled_skills/bugbounty/hunt-memory/SKILL.md"),
            ),
            (
                "hunt-oauth",
                include_str!("bundled_skills/bugbounty/hunt-oauth/SKILL.md"),
            ),
            (
                "hunt-sqli",
                include_str!("bundled_skills/bugbounty/hunt-sqli/SKILL.md"),
            ),
            (
                "hunt-ssrf",
                include_str!("bundled_skills/bugbounty/hunt-ssrf/SKILL.md"),
            ),
            (
                "hunt-xss",
                include_str!("bundled_skills/bugbounty/hunt-xss/SKILL.md"),
            ),
            (
                "llm-redteam",
                include_str!("bundled_skills/bugbounty/llm-redteam/SKILL.md"),
            ),
            (
                "recon",
                include_str!("bundled_skills/bugbounty/recon/SKILL.md"),
            ),
            (
                "recon-js",
                include_str!("bundled_skills/bugbounty/recon-js/SKILL.md"),
            ),
            (
                "scope",
                include_str!("bundled_skills/bugbounty/scope/SKILL.md"),
            ),
            (
                "report",
                include_str!("bundled_skills/bugbounty/report/SKILL.md"),
            ),
            (
                "security-arsenal",
                include_str!("bundled_skills/bugbounty/security-arsenal/SKILL.md"),
            ),
            (
                "web3-audit",
                include_str!("bundled_skills/bugbounty/web3-audit/SKILL.md"),
            ),
            (
                "pentest-ops",
                include_str!("bundled_skills/bugbounty/pentest-ops/SKILL.md"),
            ),
            (
                "knowledge-broker",
                include_str!("bundled_skills/bugbounty/knowledge-broker/SKILL.md"),
            ),
            (
                "findings-lifecycle",
                include_str!("bundled_skills/bugbounty/findings-lifecycle/SKILL.md"),
            ),
            (
                "evidence-locker",
                include_str!("bundled_skills/bugbounty/evidence-locker/SKILL.md"),
            ),
            (
                "tool-doctor",
                include_str!("bundled_skills/bugbounty/tool-doctor/SKILL.md"),
            ),
            (
                "runbook",
                include_str!("bundled_skills/bugbounty/runbook/SKILL.md"),
            ),
            (
                "network-cloud-triage",
                include_str!("bundled_skills/bugbounty/network-cloud-triage/SKILL.md"),
            ),
            (
                "redteam-ops",
                include_str!("bundled_skills/bugbounty/redteam-ops/SKILL.md"),
            ),
            (
                "hunt-cors",
                include_str!("bundled_skills/bugbounty/hunt-cors/SKILL.md"),
            ),
            (
                "hunt-ssrf-advanced",
                include_str!("bundled_skills/bugbounty/hunt-ssrf-advanced/SKILL.md"),
            ),
            (
                "hunt-race",
                include_str!("bundled_skills/bugbounty/hunt-race/SKILL.md"),
            ),
            (
                "hunt-desync",
                include_str!("bundled_skills/bugbounty/hunt-desync/SKILL.md"),
            ),
            (
                "llm-injection",
                include_str!("bundled_skills/bugbounty/llm-injection/SKILL.md"),
            ),
            (
                "hunt-subdomain-takeover",
                include_str!("bundled_skills/bugbounty/hunt-subdomain-takeover/SKILL.md"),
            ),
            (
                "hunt-cache-poisoning",
                include_str!("bundled_skills/bugbounty/hunt-cache-poisoning/SKILL.md"),
            ),
            (
                "hunt-ssti",
                include_str!("bundled_skills/bugbounty/hunt-ssti/SKILL.md"),
            ),
            (
                "hunt-deserialization",
                include_str!("bundled_skills/bugbounty/hunt-deserialization/SKILL.md"),
            ),
            (
                "hunt-websocket",
                include_str!("bundled_skills/bugbounty/hunt-websocket/SKILL.md"),
            ),
            (
                "hunt-headers",
                include_str!("bundled_skills/bugbounty/hunt-headers/SKILL.md"),
            ),
            (
                "hunt-apikey-leak",
                include_str!("bundled_skills/bugbounty/hunt-apikey-leak/SKILL.md"),
            ),
            (
                "hunt-open-redirect",
                include_str!("bundled_skills/bugbounty/hunt-open-redirect/SKILL.md"),
            ),
            (
                "hunt-csrf",
                include_str!("bundled_skills/bugbounty/hunt-csrf/SKILL.md"),
            ),
            (
                "hunt-jwt",
                include_str!("bundled_skills/bugbounty/hunt-jwt/SKILL.md"),
            ),
            (
                "hunt-xxe",
                include_str!("bundled_skills/bugbounty/hunt-xxe/SKILL.md"),
            ),
            (
                "hunt-crlf",
                include_str!("bundled_skills/bugbounty/hunt-crlf/SKILL.md"),
            ),
            (
                "hunt-prototype-pollution",
                include_str!("bundled_skills/bugbounty/hunt-prototype-pollution/SKILL.md"),
            ),
            (
                "hunt-path-traversal",
                include_str!("bundled_skills/bugbounty/hunt-path-traversal/SKILL.md"),
            ),
            (
                "scripts/install_bugbounty_tools.sh",
                include_str!("bundled_skills/bugbounty/scripts/install_bugbounty_tools.sh"),
            ),
            (
                "scripts/install_bugbounty_tools.ps1",
                include_str!("bundled_skills/bugbounty/scripts/install_bugbounty_tools.ps1"),
            ),
        ],
    },
    BundledSkill {
        name: "optimization",
        body: include_str!("bundled_skills/optimization/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "security-audit",
        body: include_str!("bundled_skills/security-audit/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "refactor",
        body: include_str!("bundled_skills/refactor/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "test-writer",
        body: include_str!("bundled_skills/test-writer/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "api-design",
        body: include_str!("bundled_skills/api-design/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "dependency-audit",
        body: include_str!("bundled_skills/dependency-audit/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "architecture-review",
        body: include_str!("bundled_skills/architecture-review/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "performance-profiling",
        body: include_str!("bundled_skills/performance-profiling/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "incident-response",
        body: include_str!("bundled_skills/incident-response/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "data-pipeline",
        body: include_str!("bundled_skills/data-pipeline/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "migration-planner",
        body: include_str!("bundled_skills/migration-planner/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "observability",
        body: include_str!("bundled_skills/observability/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "vuln-hunter",
        body: include_str!("bundled_skills/vuln-hunter/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "bug-bounty-methodology",
        body: include_str!("bundled_skills/bug-bounty-methodology/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "api-security",
        body: include_str!("bundled_skills/api-security/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "crypto-audit",
        body: include_str!("bundled_skills/crypto-audit/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "cloud-security",
        body: include_str!("bundled_skills/cloud-security/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "code-audit",
        body: include_str!("bundled_skills/code-audit/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "exploit-dev",
        body: include_str!("bundled_skills/exploit-dev/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "frontend-design",
        body: include_str!("bundled_skills/frontend-design/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "git-workflow",
        body: include_str!("bundled_skills/git-workflow/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "docker",
        body: include_str!("bundled_skills/docker/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "database",
        body: include_str!("bundled_skills/database/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "code-review",
        body: include_str!("bundled_skills/code-review/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "debug",
        body: include_str!("bundled_skills/debug/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "documentation",
        body: include_str!("bundled_skills/documentation/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "devops",
        body: include_str!("bundled_skills/devops/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "regex",
        body: include_str!("bundled_skills/regex/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "testing",
        body: include_str!("bundled_skills/testing/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "api-builder",
        body: include_str!("bundled_skills/api-builder/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "ctf",
        body: include_str!("bundled_skills/ctf/SKILL.md"),
        references: &[
            (
                "advanced-reasoning",
                include_str!("bundled_skills/ctf/ctf/advanced-reasoning/SKILL.md"),
            ),
            (
                "adversarial-thinking",
                include_str!("bundled_skills/ctf/ctf/adversarial-thinking/SKILL.md"),
            ),
            (
                "ai-llm",
                include_str!("bundled_skills/ctf/ctf/ai-llm/SKILL.md"),
            ),
            (
                "ai-ml",
                include_str!("bundled_skills/ctf/ctf/ai-ml/SKILL.md"),
            ),
            (
                "ai-ml/adversarial-ml",
                include_str!("bundled_skills/ctf/ctf/ai-ml/adversarial-ml.md"),
            ),
            (
                "ai-ml/llm-attacks",
                include_str!("bundled_skills/ctf/ctf/ai-ml/llm-attacks.md"),
            ),
            (
                "ai-ml/model-attacks",
                include_str!("bundled_skills/ctf/ctf/ai-ml/model-attacks.md"),
            ),
            (
                "api-security",
                include_str!("bundled_skills/ctf/ctf/api-security/SKILL.md"),
            ),
            (
                "cloud",
                include_str!("bundled_skills/ctf/ctf/cloud/SKILL.md"),
            ),
            (
                "crypto",
                include_str!("bundled_skills/ctf/ctf/crypto/SKILL.md"),
            ),
            (
                "crypto/advanced-math",
                include_str!("bundled_skills/ctf/ctf/crypto/advanced-math.md"),
            ),
            (
                "crypto/classic-ciphers",
                include_str!("bundled_skills/ctf/ctf/crypto/classic-ciphers.md"),
            ),
            (
                "crypto/dh-attacks",
                include_str!("bundled_skills/ctf/ctf/crypto/dh-attacks.md"),
            ),
            (
                "crypto/ecc-attacks",
                include_str!("bundled_skills/ctf/ctf/crypto/ecc-attacks.md"),
            ),
            (
                "crypto/exotic-crypto",
                include_str!("bundled_skills/ctf/ctf/crypto/exotic-crypto.md"),
            ),
            (
                "crypto/exotic-crypto-2",
                include_str!("bundled_skills/ctf/ctf/crypto/exotic-crypto-2.md"),
            ),
            (
                "crypto/historical",
                include_str!("bundled_skills/ctf/ctf/crypto/historical.md"),
            ),
            (
                "crypto/lattice-and-lwe",
                include_str!("bundled_skills/ctf/ctf/crypto/lattice-and-lwe.md"),
            ),
            (
                "crypto/modern-ciphers",
                include_str!("bundled_skills/ctf/ctf/crypto/modern-ciphers.md"),
            ),
            (
                "crypto/modern-ciphers-2",
                include_str!("bundled_skills/ctf/ctf/crypto/modern-ciphers-2.md"),
            ),
            (
                "crypto/modern-ciphers-3",
                include_str!("bundled_skills/ctf/ctf/crypto/modern-ciphers-3.md"),
            ),
            (
                "crypto/modern-ciphers-4",
                include_str!("bundled_skills/ctf/ctf/crypto/modern-ciphers-4.md"),
            ),
            (
                "crypto/post-quantum",
                include_str!("bundled_skills/ctf/ctf/crypto/post-quantum.md"),
            ),
            (
                "crypto/prng",
                include_str!("bundled_skills/ctf/ctf/crypto/prng.md"),
            ),
            (
                "crypto/prng-attacks",
                include_str!("bundled_skills/ctf/ctf/crypto/prng-attacks.md"),
            ),
            (
                "crypto/rsa-attacks",
                include_str!("bundled_skills/ctf/ctf/crypto/rsa-attacks.md"),
            ),
            (
                "crypto/rsa-attacks-2",
                include_str!("bundled_skills/ctf/ctf/crypto/rsa-attacks-2.md"),
            ),
            (
                "crypto/stream-ciphers",
                include_str!("bundled_skills/ctf/ctf/crypto/stream-ciphers.md"),
            ),
            (
                "crypto/zkp-and-advanced",
                include_str!("bundled_skills/ctf/ctf/crypto/zkp-and-advanced.md"),
            ),
            (
                "ctf/pwn/scripts/fmtstr_payload_suite.py",
                include_str!("bundled_skills/ctf/ctf/pwn/scripts/fmtstr_payload_suite.py"),
            ),
            (
                "ctf/pwn/scripts/ret2libc_two_stage.py",
                include_str!("bundled_skills/ctf/ctf/pwn/scripts/ret2libc_two_stage.py"),
            ),
            (
                "ctf/pwn/scripts/seccomp_orw_generator.py",
                include_str!("bundled_skills/ctf/ctf/pwn/scripts/seccomp_orw_generator.py"),
            ),
            (
                "ctf/pwn/scripts/shellcraft_asm.py",
                include_str!("bundled_skills/ctf/ctf/pwn/scripts/shellcraft_asm.py"),
            ),
            (
                "ctf/pwn/scripts/srop_execve.py",
                include_str!("bundled_skills/ctf/ctf/pwn/scripts/srop_execve.py"),
            ),
            (
                "ctf/web/scripts/async_fuzz.py",
                include_str!("bundled_skills/ctf/ctf/web/scripts/async_fuzz.py"),
            ),
            ("dfir", include_str!("bundled_skills/ctf/ctf/dfir/SKILL.md")),
            (
                "forensics",
                include_str!("bundled_skills/ctf/ctf/forensics/SKILL.md"),
            ),
            (
                "forensics/3d-printing",
                include_str!("bundled_skills/ctf/ctf/forensics/3d-printing.md"),
            ),
            (
                "forensics/disk-advanced",
                include_str!("bundled_skills/ctf/ctf/forensics/disk-advanced.md"),
            ),
            (
                "forensics/disk-and-memory",
                include_str!("bundled_skills/ctf/ctf/forensics/disk-and-memory.md"),
            ),
            (
                "forensics/disk-recovery",
                include_str!("bundled_skills/ctf/ctf/forensics/disk-recovery.md"),
            ),
            (
                "forensics/linux-forensics",
                include_str!("bundled_skills/ctf/ctf/forensics/linux-forensics.md"),
            ),
            (
                "forensics/network",
                include_str!("bundled_skills/ctf/ctf/forensics/network.md"),
            ),
            (
                "forensics/network-advanced",
                include_str!("bundled_skills/ctf/ctf/forensics/network-advanced.md"),
            ),
            (
                "forensics/peripheral-capture",
                include_str!("bundled_skills/ctf/ctf/forensics/peripheral-capture.md"),
            ),
            (
                "forensics/signals-and-hardware",
                include_str!("bundled_skills/ctf/ctf/forensics/signals-and-hardware.md"),
            ),
            (
                "forensics/steganography",
                include_str!("bundled_skills/ctf/ctf/forensics/steganography.md"),
            ),
            (
                "forensics/stego-advanced",
                include_str!("bundled_skills/ctf/ctf/forensics/stego-advanced.md"),
            ),
            (
                "forensics/stego-advanced-2",
                include_str!("bundled_skills/ctf/ctf/forensics/stego-advanced-2.md"),
            ),
            (
                "forensics/stego-image",
                include_str!("bundled_skills/ctf/ctf/forensics/stego-image.md"),
            ),
            (
                "forensics/windows",
                include_str!("bundled_skills/ctf/ctf/forensics/windows.md"),
            ),
            (
                "LICENSE.ctf-skills",
                include_str!("bundled_skills/ctf/LICENSE.ctf-skills"),
            ),
            (
                "malware-analysis",
                include_str!("bundled_skills/ctf/ctf/malware-analysis/SKILL.md"),
            ),
            (
                "malware-analysis/c2-and-protocols",
                include_str!("bundled_skills/ctf/ctf/malware-analysis/c2-and-protocols.md"),
            ),
            (
                "malware-analysis/pe-and-dotnet",
                include_str!("bundled_skills/ctf/ctf/malware-analysis/pe-and-dotnet.md"),
            ),
            (
                "malware-analysis/scripts-and-obfuscation",
                include_str!("bundled_skills/ctf/ctf/malware-analysis/scripts-and-obfuscation.md"),
            ),
            (
                "master-brain",
                include_str!("bundled_skills/ctf/ctf/master-brain/SKILL.md"),
            ),
            (
                "methodology",
                include_str!("bundled_skills/ctf/ctf/methodology/SKILL.md"),
            ),
            ("misc", include_str!("bundled_skills/ctf/ctf/misc/SKILL.md")),
            (
                "misc/bashjails",
                include_str!("bundled_skills/ctf/ctf/misc/bashjails.md"),
            ),
            (
                "misc/ctfd-navigation",
                include_str!("bundled_skills/ctf/ctf/misc/ctfd-navigation.md"),
            ),
            (
                "misc/dns",
                include_str!("bundled_skills/ctf/ctf/misc/dns.md"),
            ),
            (
                "misc/encodings",
                include_str!("bundled_skills/ctf/ctf/misc/encodings.md"),
            ),
            (
                "misc/encodings-advanced",
                include_str!("bundled_skills/ctf/ctf/misc/encodings-advanced.md"),
            ),
            (
                "misc/games-and-vms",
                include_str!("bundled_skills/ctf/ctf/misc/games-and-vms.md"),
            ),
            (
                "misc/games-and-vms-2",
                include_str!("bundled_skills/ctf/ctf/misc/games-and-vms-2.md"),
            ),
            (
                "misc/games-and-vms-3",
                include_str!("bundled_skills/ctf/ctf/misc/games-and-vms-3.md"),
            ),
            (
                "misc/games-and-vms-4",
                include_str!("bundled_skills/ctf/ctf/misc/games-and-vms-4.md"),
            ),
            (
                "misc/linux-privesc",
                include_str!("bundled_skills/ctf/ctf/misc/linux-privesc.md"),
            ),
            (
                "misc/pyjails",
                include_str!("bundled_skills/ctf/ctf/misc/pyjails.md"),
            ),
            (
                "misc/rf-sdr",
                include_str!("bundled_skills/ctf/ctf/misc/rf-sdr.md"),
            ),
            (
                "osint",
                include_str!("bundled_skills/ctf/ctf/osint/SKILL.md"),
            ),
            (
                "osint/geolocation-and-media",
                include_str!("bundled_skills/ctf/ctf/osint/geolocation-and-media.md"),
            ),
            (
                "osint/social-media",
                include_str!("bundled_skills/ctf/ctf/osint/social-media.md"),
            ),
            (
                "osint/web-and-dns",
                include_str!("bundled_skills/ctf/ctf/osint/web-and-dns.md"),
            ),
            ("pwn", include_str!("bundled_skills/ctf/ctf/pwn/SKILL.md")),
            (
                "pwn/advanced",
                include_str!("bundled_skills/ctf/ctf/pwn/advanced.md"),
            ),
            (
                "pwn/advanced-exploits",
                include_str!("bundled_skills/ctf/ctf/pwn/advanced-exploits.md"),
            ),
            (
                "pwn/advanced-exploits-2",
                include_str!("bundled_skills/ctf/ctf/pwn/advanced-exploits-2.md"),
            ),
            (
                "pwn/advanced-exploits-3",
                include_str!("bundled_skills/ctf/ctf/pwn/advanced-exploits-3.md"),
            ),
            (
                "pwn/advanced-exploits-4",
                include_str!("bundled_skills/ctf/ctf/pwn/advanced-exploits-4.md"),
            ),
            (
                "pwn/advanced-exploits-5",
                include_str!("bundled_skills/ctf/ctf/pwn/advanced-exploits-5.md"),
            ),
            (
                "pwn/field-notes",
                include_str!("bundled_skills/ctf/ctf/pwn/field-notes.md"),
            ),
            (
                "pwn/format-string",
                include_str!("bundled_skills/ctf/ctf/pwn/format-string.md"),
            ),
            (
                "pwn/heap-fsop",
                include_str!("bundled_skills/ctf/ctf/pwn/heap-fsop.md"),
            ),
            (
                "pwn/heap-techniques",
                include_str!("bundled_skills/ctf/ctf/pwn/heap-techniques.md"),
            ),
            (
                "pwn/heap-techniques-2",
                include_str!("bundled_skills/ctf/ctf/pwn/heap-techniques-2.md"),
            ),
            (
                "pwn/kernel",
                include_str!("bundled_skills/ctf/ctf/pwn/kernel.md"),
            ),
            (
                "pwn/kernel-bypass",
                include_str!("bundled_skills/ctf/ctf/pwn/kernel-bypass.md"),
            ),
            (
                "pwn/kernel-techniques",
                include_str!("bundled_skills/ctf/ctf/pwn/kernel-techniques.md"),
            ),
            (
                "pwn/overflow-basics",
                include_str!("bundled_skills/ctf/ctf/pwn/overflow-basics.md"),
            ),
            (
                "pwn/rop-advanced",
                include_str!("bundled_skills/ctf/ctf/pwn/rop-advanced.md"),
            ),
            (
                "pwn/rop-and-shellcode",
                include_str!("bundled_skills/ctf/ctf/pwn/rop-and-shellcode.md"),
            ),
            (
                "pwn/sandbox-escape",
                include_str!("bundled_skills/ctf/ctf/pwn/sandbox-escape.md"),
            ),
            ("rev", include_str!("bundled_skills/ctf/ctf/rev/SKILL.md")),
            (
                "rev/anti-analysis",
                include_str!("bundled_skills/ctf/ctf/rev/anti-analysis.md"),
            ),
            (
                "rev/anti-analysis-ctf",
                include_str!("bundled_skills/ctf/ctf/rev/anti-analysis-ctf.md"),
            ),
            (
                "rev/field-notes",
                include_str!("bundled_skills/ctf/ctf/rev/field-notes.md"),
            ),
            (
                "rev/languages",
                include_str!("bundled_skills/ctf/ctf/rev/languages.md"),
            ),
            (
                "rev/languages-compiled",
                include_str!("bundled_skills/ctf/ctf/rev/languages-compiled.md"),
            ),
            (
                "rev/languages-platforms",
                include_str!("bundled_skills/ctf/ctf/rev/languages-platforms.md"),
            ),
            (
                "rev/patterns",
                include_str!("bundled_skills/ctf/ctf/rev/patterns.md"),
            ),
            (
                "rev/patterns-ctf",
                include_str!("bundled_skills/ctf/ctf/rev/patterns-ctf.md"),
            ),
            (
                "rev/patterns-ctf-2",
                include_str!("bundled_skills/ctf/ctf/rev/patterns-ctf-2.md"),
            ),
            (
                "rev/patterns-ctf-3",
                include_str!("bundled_skills/ctf/ctf/rev/patterns-ctf-3.md"),
            ),
            (
                "rev/patterns-runtime",
                include_str!("bundled_skills/ctf/ctf/rev/patterns-runtime.md"),
            ),
            (
                "rev/platforms",
                include_str!("bundled_skills/ctf/ctf/rev/platforms.md"),
            ),
            (
                "rev/platforms-hardware",
                include_str!("bundled_skills/ctf/ctf/rev/platforms-hardware.md"),
            ),
            (
                "rev/tools",
                include_str!("bundled_skills/ctf/ctf/rev/tools.md"),
            ),
            (
                "rev/tools-advanced",
                include_str!("bundled_skills/ctf/ctf/rev/tools-advanced.md"),
            ),
            (
                "rev/tools-advanced-2",
                include_str!("bundled_skills/ctf/ctf/rev/tools-advanced-2.md"),
            ),
            (
                "rev/tools-dynamic",
                include_str!("bundled_skills/ctf/ctf/rev/tools-dynamic.md"),
            ),
            (
                "rev/tools-emulation",
                include_str!("bundled_skills/ctf/ctf/rev/tools-emulation.md"),
            ),
            (
                "rev/unicorn-emulation",
                include_str!("bundled_skills/ctf/ctf/rev/unicorn-emulation.md"),
            ),
            (
                "scripts/install_ctf_tools.sh",
                include_str!("bundled_skills/ctf/scripts/install_ctf_tools.sh"),
            ),
            ("siem", include_str!("bundled_skills/ctf/ctf/siem/SKILL.md")),
            (
                "solution-verifier",
                include_str!("bundled_skills/ctf/ctf/solution-verifier/SKILL.md"),
            ),
            (
                "solve-challenge",
                include_str!("bundled_skills/ctf/ctf/solve-challenge/SKILL.md"),
            ),
            (
                "toolkit",
                include_str!("bundled_skills/ctf/ctf/toolkit/SKILL.md"),
            ),
            ("web", include_str!("bundled_skills/ctf/ctf/web/SKILL.md")),
            (
                "web/auth-and-access",
                include_str!("bundled_skills/ctf/ctf/web/auth-and-access.md"),
            ),
            (
                "web/auth-and-access-2",
                include_str!("bundled_skills/ctf/ctf/web/auth-and-access-2.md"),
            ),
            (
                "web/auth-infra",
                include_str!("bundled_skills/ctf/ctf/web/auth-infra.md"),
            ),
            (
                "web/auth-jwt",
                include_str!("bundled_skills/ctf/ctf/web/auth-jwt.md"),
            ),
            (
                "web/client-side",
                include_str!("bundled_skills/ctf/ctf/web/client-side.md"),
            ),
            (
                "web/client-side-advanced",
                include_str!("bundled_skills/ctf/ctf/web/client-side-advanced.md"),
            ),
            (
                "web/cves",
                include_str!("bundled_skills/ctf/ctf/web/cves.md"),
            ),
            (
                "web/field-notes",
                include_str!("bundled_skills/ctf/ctf/web/field-notes.md"),
            ),
            (
                "web/node-and-prototype",
                include_str!("bundled_skills/ctf/ctf/web/node-and-prototype.md"),
            ),
            (
                "web/pat-reference",
                include_str!("bundled_skills/ctf/ctf/web/pat-reference.md"),
            ),
            (
                "web/python-requests",
                include_str!("bundled_skills/ctf/ctf/web/python-requests.md"),
            ),
            (
                "web/server-side",
                include_str!("bundled_skills/ctf/ctf/web/server-side.md"),
            ),
            (
                "web/server-side-2",
                include_str!("bundled_skills/ctf/ctf/web/server-side-2.md"),
            ),
            (
                "web/server-side-advanced",
                include_str!("bundled_skills/ctf/ctf/web/server-side-advanced.md"),
            ),
            (
                "web/server-side-advanced-2",
                include_str!("bundled_skills/ctf/ctf/web/server-side-advanced-2.md"),
            ),
            (
                "web/server-side-advanced-3",
                include_str!("bundled_skills/ctf/ctf/web/server-side-advanced-3.md"),
            ),
            (
                "web/server-side-advanced-4",
                include_str!("bundled_skills/ctf/ctf/web/server-side-advanced-4.md"),
            ),
            (
                "web/server-side-deser",
                include_str!("bundled_skills/ctf/ctf/web/server-side-deser.md"),
            ),
            (
                "web/server-side-exec",
                include_str!("bundled_skills/ctf/ctf/web/server-side-exec.md"),
            ),
            (
                "web/server-side-exec-2",
                include_str!("bundled_skills/ctf/ctf/web/server-side-exec-2.md"),
            ),
            (
                "web/sql-injection",
                include_str!("bundled_skills/ctf/ctf/web/sql-injection.md"),
            ),
            (
                "web/web3",
                include_str!("bundled_skills/ctf/ctf/web/web3.md"),
            ),
            ("web3", include_str!("bundled_skills/ctf/ctf/web3/SKILL.md")),
            (
                "writeup",
                include_str!("bundled_skills/ctf/ctf/writeup/SKILL.md"),
            ),
        ],
    },
    BundledSkill {
        name: "frontend-dev",
        body: include_str!("bundled_skills/frontend-dev/SKILL.md"),
        references: &[
            (
                "react",
                include_str!("bundled_skills/frontend-dev/react/SKILL.md"),
            ),
            (
                "nextjs",
                include_str!("bundled_skills/frontend-dev/nextjs/SKILL.md"),
            ),
            (
                "tailwind",
                include_str!("bundled_skills/frontend-dev/tailwind/SKILL.md"),
            ),
            (
                "components",
                include_str!("bundled_skills/frontend-dev/components/SKILL.md"),
            ),
            (
                "performance",
                include_str!("bundled_skills/frontend-dev/performance/SKILL.md"),
            ),
            (
                "testing",
                include_str!("bundled_skills/frontend-dev/testing/SKILL.md"),
            ),
        ],
    },
    BundledSkill {
        name: "backend-dev",
        body: include_str!("bundled_skills/backend-dev/SKILL.md"),
        references: &[
            (
                "nodejs",
                include_str!("bundled_skills/backend-dev/nodejs/SKILL.md"),
            ),
            (
                "python",
                include_str!("bundled_skills/backend-dev/python/SKILL.md"),
            ),
            (
                "rust",
                include_str!("bundled_skills/backend-dev/rust/SKILL.md"),
            ),
            (
                "database",
                include_str!("bundled_skills/backend-dev/database/SKILL.md"),
            ),
            (
                "auth",
                include_str!("bundled_skills/backend-dev/auth/SKILL.md"),
            ),
            (
                "caching",
                include_str!("bundled_skills/backend-dev/caching/SKILL.md"),
            ),
        ],
    },
    BundledSkill {
        name: "web3-security-research",
        body: include_str!("bundled_skills/web3-security-research/SKILL.md"),
        references: &[
            (
                "protocol-model",
                include_str!("bundled_skills/web3-security-research/protocol-model/SKILL.md"),
            ),
            (
                "defi-accounting",
                include_str!("bundled_skills/web3-security-research/defi-accounting/SKILL.md"),
            ),
            (
                "oracle-security",
                include_str!("bundled_skills/web3-security-research/oracle-security/SKILL.md"),
            ),
            (
                "lending-amm",
                include_str!("bundled_skills/web3-security-research/lending-amm/SKILL.md"),
            ),
            (
                "token-security",
                include_str!("bundled_skills/web3-security-research/token-security/SKILL.md"),
            ),
            (
                "bridge-security",
                include_str!("bundled_skills/web3-security-research/bridge-security/SKILL.md"),
            ),
            (
                "governance-proxy",
                include_str!("bundled_skills/web3-security-research/governance-proxy/SKILL.md"),
            ),
            (
                "signature-security",
                include_str!("bundled_skills/web3-security-research/signature-security/SKILL.md"),
            ),
            (
                "economic-attacks",
                include_str!("bundled_skills/web3-security-research/economic-attacks/SKILL.md"),
            ),
            (
                "exploit-chaining",
                include_str!("bundled_skills/web3-security-research/exploit-chaining/SKILL.md"),
            ),
            (
                "bytecode-analysis",
                include_str!("bundled_skills/web3-security-research/bytecode-analysis/SKILL.md"),
            ),
            (
                "known-incidents",
                include_str!("bundled_skills/web3-security-research/known-incidents/SKILL.md"),
            ),
            (
                "swarm-playbook",
                include_str!("bundled_skills/web3-security-research/swarm-playbook/SKILL.md"),
            ),
            (
                "finding-templates",
                include_str!("bundled_skills/web3-security-research/finding-templates/SKILL.md"),
            ),
        ],
    },
];

/// One bundled skill: a name, the top-level SKILL.md body, and any
/// nested subskill references that should be merged into the skill's
/// `reference_files`.
pub(super) struct BundledSkill {
    pub name: &'static str,
    pub body: &'static str,
    pub references: &'static [(&'static str, &'static str)],
}

/// Number of bundled top-level skills (parent skills only — subskill
/// references are part of the parent).
pub fn bundled_skill_count() -> usize {
    BUNDLED_SKILLS.len()
}

/// Public list of bundled skill names. Excludes subskill references so
/// callers (e.g. `/skills` introspection) only see parent skills.
pub fn bundled_skill_names() -> Vec<&'static str> {
    BUNDLED_SKILLS.iter().map(|s| s.name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_skill_bodies_and_references_are_non_empty() {
        for skill in BUNDLED_SKILLS {
            assert!(
                !skill.body.trim().is_empty(),
                "bundled skill '{}' has an empty SKILL.md body",
                skill.name
            );
            assert!(
                skill.body.contains("description:"),
                "bundled skill '{}' SKILL.md is missing YAML frontmatter description",
                skill.name
            );
            for (ref_name, ref_body) in skill.references {
                assert!(
                    !ref_body.trim().is_empty(),
                    "bundled skill '{}' reference '{}' is empty",
                    skill.name,
                    ref_name
                );
            }
        }
    }

    #[test]
    fn bundled_skill_names_are_unique_and_lowercase() {
        let names = bundled_skill_names();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            names.len(),
            sorted.len(),
            "duplicate bundled skill name registered"
        );
        for name in &names {
            assert_eq!(
                *name,
                name.to_lowercase(),
                "bundled skill name '{}' is not lowercase; slash invocation resolves lowercase",
                name
            );
        }
    }
}
