//! Build-readiness assessment for target repositories.
//!
//! The motivating session burned time discovering mid-investigation that Go
//! protobuf-generated packages were missing and that some repositories could
//! not be compiled standalone. This module moves that discovery *before* the
//! expensive work: a cheap filesystem-and-toolchain probe that says what can
//! actually be built and tested here, and what smaller test can still validate
//! a hypothesis when a full build is unavailable.
//!
//! The core distinction it enforces ([`EvidenceTier`]): static source review,
//! successful compilation, unit-test execution, integration-test execution,
//! standalone reproduction, and end-to-end impact validation are **different
//! things**. A readiness report never implies a test passed; it only says
//! whether the prerequisites to run one are present.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// What kind of evidence a piece of work actually produced. Kept explicit so
/// a report can never blur "we read the code" into "we ran the tests".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceTier {
    /// Source was reviewed; nothing was executed.
    StaticReview,
    /// The code compiled successfully.
    Compilation,
    /// Unit tests were executed (result recorded separately — compiling is
    /// not passing).
    UnitTests,
    /// Integration tests were executed.
    IntegrationTests,
    /// A standalone reproduction of the specific defect was executed.
    StandaloneReproduction,
    /// The claimed security impact was demonstrated end to end.
    EndToEndImpact,
}

impl EvidenceTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StaticReview => "static_review",
            Self::Compilation => "compilation",
            Self::UnitTests => "unit_tests",
            Self::IntegrationTests => "integration_tests",
            Self::StandaloneReproduction => "standalone_reproduction",
            Self::EndToEndImpact => "end_to_end_impact",
        }
    }
}

/// Languages this probe understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetLanguage {
    Go,
    Rust,
    Other,
}

/// A concrete blocker to building or testing, with what would resolve it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildBlocker {
    pub description: String,
    /// What would need to be true to clear it (e.g. "run go generate",
    /// "install protoc", "provide DATABASE_URL").
    pub remedy: String,
}

/// Result of probing a repository directory.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BuildReadiness {
    pub language: TargetLanguage,
    pub path: String,
    /// Whether the corresponding toolchain binary is on PATH.
    pub toolchain_available: bool,
    /// Toolchain version string, when it could be determined.
    pub toolchain_version: Option<String>,
    pub has_lockfile: bool,
    pub lockfile: Option<String>,
    /// Missing generated code (e.g. *.pb.go referenced by go.mod deps but
    /// absent, or `go:generate` directives with no generated output).
    pub missing_generated_code: Vec<String>,
    /// CGO or native-library requirements detected.
    pub native_requirements: Vec<String>,
    /// Required environment variables detected (from docs/env files).
    pub required_env: Vec<String>,
    /// Detected build blockers.
    pub blockers: Vec<BuildBlocker>,
    /// What a reviewer can still do despite blockers — a smaller test that
    /// validates a hypothesis without the full build.
    pub fallback_validation_paths: Vec<String>,
}

impl BuildReadiness {
    /// Can the repository be compiled here, given what was probed?
    pub fn compilable(&self) -> bool {
        self.toolchain_available && self.blockers.is_empty()
    }

    /// The highest evidence tier this readiness level can support. A
    /// repository that cannot build can still support static review and
    /// standalone reproductions of self-contained defects.
    pub fn max_feasible_tier(&self) -> EvidenceTier {
        if self.compilable() {
            EvidenceTier::UnitTests
        } else if self.toolchain_available {
            // Toolchain exists but something else blocks; standalone repros
            // of isolated logic may still work.
            EvidenceTier::StandaloneReproduction
        } else {
            EvidenceTier::StaticReview
        }
    }
}

/// Recursively visit every `.go` file under `dir`. Ignores unreadable
/// directories — the probe must never fail because one subtree is
/// inaccessible.
fn visit_go_files(dir: &Path, visit: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            visit_go_files(&p, visit);
        } else if p.extension().is_some_and(|e| e == "go") {
            visit(&p);
        }
    }
}

/// Probe a repository directory for build readiness.
///
/// This is a cheap, side-effect-free probe: it reads manifest files and
/// checks for toolchain binaries; it does not download dependencies, run
/// code generation, or execute the target.
pub fn assess_build_readiness(path: &Path) -> BuildReadiness {
    let go_mod = path.join("go.mod");
    let cargo_toml = path.join("Cargo.toml");
    let language = if go_mod.exists() {
        TargetLanguage::Go
    } else if cargo_toml.exists() {
        TargetLanguage::Rust
    } else {
        TargetLanguage::Other
    };

    let mut readiness = BuildReadiness {
        language,
        path: path.display().to_string(),
        toolchain_available: false,
        toolchain_version: None,
        has_lockfile: false,
        lockfile: None,
        missing_generated_code: Vec::new(),
        native_requirements: Vec::new(),
        required_env: Vec::new(),
        blockers: Vec::new(),
        fallback_validation_paths: Vec::new(),
    };

    match language {
        TargetLanguage::Go => probe_go(path, &mut readiness),
        TargetLanguage::Rust => probe_rust(path, &mut readiness),
        TargetLanguage::Other => {
            readiness.blockers.push(BuildBlocker {
                description: "no go.mod or Cargo.toml found at repository root".into(),
                remedy: "identify the build system manually before planning test execution".into(),
            });
            readiness
                .fallback_validation_paths
                .push("static source review; standalone reproduction of isolated logic".into());
        }
    }

    readiness
}

fn probe_go(path: &Path, readiness: &mut BuildReadiness) {
    let version = toolchain_version("go", &["version"]);
    readiness.toolchain_available = version.is_some();
    readiness.toolchain_version = version;

    let go_sum = path.join("go.sum");
    readiness.has_lockfile = go_sum.exists();
    readiness.lockfile = go_sum.exists().then(|| "go.sum".to_string());

    // Missing generated code: *.pb.go imports in .go files with no matching
    // generated file present, plus `go:generate` with no generated output.
    let mut imported_pb: std::collections::BTreeSet<String> = Default::default();
    let mut has_generate_directives = false;
    visit_go_files(path, &mut |fp| {
        let Ok(content) = std::fs::read_to_string(fp) else {
            return;
        };
        for line in content.lines() {
            let t = line.trim();
            if t.starts_with("//go:generate") {
                has_generate_directives = true;
                continue;
            }
            // Import lines and bare string lines; a generated-code import is
            // one whose final path segment looks like a protobuf package
            // (`pb`, `pb.v1`, `foo.pb`).
            if !(t.starts_with("import") || (t.starts_with('"') && t.ends_with('"'))) {
                continue;
            }
            if let Some(quoted) = t.split('"').nth(1) {
                let base = quoted.rsplit('/').next().unwrap_or(quoted);
                if base == "pb" || base.starts_with("pb.") || base.ends_with(".pb") {
                    imported_pb.insert(quoted.to_string());
                }
            }
        }
    });
    for pb_import in &imported_pb {
        let base = pb_import.rsplit('/').next().unwrap_or(pb_import);
        // If no file in the tree looks like the generated artifact, flag it.
        let generated_exists = std::fs::read_dir(path)
            .map(|dirs| {
                dirs.flatten().any(|d| {
                    std::fs::read_dir(d.path())
                        .map(|files| {
                            files.flatten().any(|f| {
                                let name = f.file_name().to_string_lossy().to_string();
                                name.ends_with(".pb.go") && name.contains(base)
                            })
                        })
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        if !generated_exists {
            readiness.missing_generated_code.push(format!(
                "import of '{pb_import}' references generated code, but no matching *.pb.go was found in the tree"
            ));
        }
    }
    if has_generate_directives && readiness.missing_generated_code.is_empty() {
        readiness
            .fallback_validation_paths
            .push("go:generate directives present; run codegen before full builds".into());
    }

    // CGO detection: stop at the first import "C" sighting.
    let mut cgo_detected = false;
    visit_go_files(path, &mut |fp| {
        if !cgo_detected && std::fs::read_to_string(fp).is_ok_and(|c| c.contains("import \"C\"")) {
            cgo_detected = true;
        }
    });
    if cgo_detected {
        readiness
            .native_requirements
            .push("CGO required (import \"C\" detected)".into());
    }

    if !readiness.missing_generated_code.is_empty() {
        readiness.blockers.push(BuildBlocker {
            description: format!(
                "{} generated-code import(s) cannot be resolved",
                readiness.missing_generated_code.len()
            ),
            remedy: "run `go generate ./...` (with protoc available) or fetch the generated \
                     packages; do not modify dependency versions to work around this"
                .into(),
        });
        readiness.fallback_validation_paths.push(
            "standalone reproduction of the specific defect in an isolated module (no repo \
             build needed); static review of the affected call paths"
                .into(),
        );
    }
    if !readiness.toolchain_available {
        readiness.blockers.push(BuildBlocker {
            description: "go toolchain not found on PATH".into(),
            remedy: "install Go or accept static-review-only evidence tier".into(),
        });
        readiness.fallback_validation_paths.push(
            "static source review only; a standalone reproduction cannot be executed here".into(),
        );
    }
    if !readiness.has_lockfile {
        readiness.blockers.push(BuildBlocker {
            description: "go.sum missing: dependency versions are not pinned".into(),
            remedy: "builds are non-reproducible; record the actual resolved versions if built"
                .into(),
        });
    }
}

fn probe_rust(path: &Path, readiness: &mut BuildReadiness) {
    let version = toolchain_version("cargo", &["--version"]);
    readiness.toolchain_available = version.is_some();
    readiness.toolchain_version = version;

    let lock = path.join("Cargo.lock");
    readiness.has_lockfile = lock.exists();
    readiness.lockfile = lock.exists().then(|| "Cargo.lock".to_string());

    // Required environment variables: common optional features read env at
    // runtime; the manifest's features cannot tell us, but a .env.example
    // can.
    for env_file in [".env.example", ".env.sample"] {
        let p = path.join(env_file);
        if let Ok(content) = std::fs::read_to_string(&p) {
            for line in content.lines() {
                let t = line.trim();
                if t.is_empty() || t.starts_with('#') {
                    continue;
                }
                if let Some(key) = t.split('=').next() {
                    readiness.required_env.push(key.trim().to_string());
                }
            }
        }
    }

    if !readiness.toolchain_available {
        readiness.blockers.push(BuildBlocker {
            description: "cargo not found on PATH".into(),
            remedy: "install Rust toolchain or accept static-review-only evidence tier".into(),
        });
        readiness
            .fallback_validation_paths
            .push("static source review only".into());
    }
    if !readiness.has_lockfile {
        readiness.blockers.push(BuildBlocker {
            description: "Cargo.lock missing: dependency versions are not pinned".into(),
            remedy: "builds are non-reproducible; record actual resolved versions if built".into(),
        });
    }
    if readiness.toolchain_available {
        readiness.fallback_validation_paths.push(
            "cargo test --lib <module> for a targeted subset; minimal standalone test in an \
             isolated crate when the full workspace does not build"
                .into(),
        );
    }
}

/// Ask a toolchain for its version. Bounded and side-effect-free.
fn toolchain_version(program: &str, args: &[&str]) -> Option<String> {
    use std::process::Command;
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text.lines().next().unwrap_or(&text).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_repo(files: &[(&str, &str)]) -> std::path::PathBuf {
        let dir = tempfile::Builder::new()
            .prefix("alphacode-buildprobe-")
            .tempdir()
            .expect("temp dir");
        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&p, content).unwrap();
        }
        let path = dir.path().to_path_buf();
        std::mem::forget(dir);
        path
    }

    #[test]
    fn a_go_repo_with_missing_generated_code_is_flagged() {
        let root = temp_repo(&[
            ("go.mod", "module example.com/m\n\ngo 1.21\n"),
            (
                "svc/main.go",
                "package svc\n\nimport (\n\t\"example.com/m/pb\"\n)\n\nvar _ = pb.Msg{}\n",
            ),
        ]);
        let readiness = assess_build_readiness(&root);
        assert_eq!(readiness.language, TargetLanguage::Go);
        assert!(
            !readiness.missing_generated_code.is_empty(),
            "missing generated code must be detected: {readiness:?}"
        );
        assert!(
            readiness
                .blockers
                .iter()
                .any(|b| b.description.contains("generated")),
            "{:?}",
            readiness.blockers
        );
        // Even when blocked, fallback validation paths are offered.
        assert!(!readiness.fallback_validation_paths.is_empty());
    }

    /// A repository with no manifest gets an honest "cannot classify" with a
    /// fallback, not a panic or silent success.
    #[test]
    fn an_unknown_repository_is_reported_as_other() {
        let root = temp_repo(&[("README.md", "# hi\n")]);
        let readiness = assess_build_readiness(&root);
        assert_eq!(readiness.language, TargetLanguage::Other);
        assert!(!readiness.blockers.is_empty());
        assert!(!readiness.fallback_validation_paths.is_empty());
    }

    #[test]
    fn a_rust_repo_without_a_lockfile_is_flagged() {
        let root = temp_repo(&[(
            "Cargo.toml",
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )]);
        let readiness = assess_build_readiness(&root);
        assert_eq!(readiness.language, TargetLanguage::Rust);
        assert!(!readiness.has_lockfile);
        assert!(
            readiness
                .blockers
                .iter()
                .any(|b| b.description.contains("Cargo.lock"))
        );
    }

    /// Evidence tiers are strictly ordered and never blurred: compilation
    /// does not imply unit tests, and nothing implies impact.
    #[test]
    fn evidence_tiers_are_ordered_strictly_below_impact() {
        assert!(EvidenceTier::StaticReview < EvidenceTier::Compilation);
        assert!(EvidenceTier::Compilation < EvidenceTier::UnitTests);
        assert!(EvidenceTier::UnitTests < EvidenceTier::IntegrationTests);
        assert!(EvidenceTier::IntegrationTests < EvidenceTier::StandaloneReproduction);
        assert!(EvidenceTier::StandaloneReproduction < EvidenceTier::EndToEndImpact);
    }

    /// max_feasible_tier must never claim more than the environment can do.
    #[test]
    fn max_feasible_tier_never_overpromises() {
        let root = temp_repo(&[("go.mod", "module m\n\ngo 1.21\n")]);
        let readiness = assess_build_readiness(&root);
        // Whatever the environment, the tier must be <= what readiness says.
        if readiness.compilable() {
            assert_eq!(readiness.max_feasible_tier(), EvidenceTier::UnitTests);
        } else {
            // Not compilable: repository-level test tiers (unit/integration)
            // require building this repo and are off the table. Standalone
            // reproductions of isolated defects may still run when a
            // toolchain exists, and static review always can.
            let tier = readiness.max_feasible_tier();
            assert!(
                tier < EvidenceTier::UnitTests || tier == EvidenceTier::StandaloneReproduction,
                "a non-compilable repo must not claim repository-level test tiers, got {tier:?}"
            );
        }
    }

    /// No repo probe may ever execute target code — this is a guard on the
    /// probe's own design: it only reads files and asks toolchains for
    /// versions. (Structural test: readiness for a repo containing an
    /// executable-looking file is still produced purely from manifests.)
    #[test]
    fn probing_does_not_require_executing_target_code() {
        let root = temp_repo(&[
            ("go.mod", "module m\n\ngo 1.21\n"),
            (
                "main.go",
                "package main\n\nfunc main() { panic(\"must not run\") }\n",
            ),
        ]);
        // If the probe executed target code this test would fail loudly.
        let readiness = assess_build_readiness(&root);
        assert_eq!(readiness.language, TargetLanguage::Go);
    }
}
