use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const UNKNOWN: &str = "unknown";

const ENV_RELEASE: &str = "ALPHACODE_RELEASE_BUILD";
const ENV_SEMVER: &str = "ALPHACODE_BUILD_SEMVER";
const ENV_GIT_HASH: &str = "ALPHACODE_BUILD_GIT_HASH";
const ENV_GIT_DATE: &str = "ALPHACODE_BUILD_GIT_DATE";
const ENV_GIT_DIRTY: &str = "ALPHACODE_BUILD_GIT_DIRTY";
const ENV_GIT_TAG: &str = "ALPHACODE_BUILD_GIT_TAG";
const ENV_CHANGELOG: &str = "ALPHACODE_BUILD_CHANGELOG_RAW";
const ENV_METADATA: &str = "ALPHACODE_BUILD_METADATA_FILE";

const ENV_MMDR_DISABLE: &str = "ALPHACODE_MMDR_SIZE_API_DISABLE";
const ENV_MMDR_AVAILABLE: &str = "ALPHACODE_MMDR_SIZE_API_AVAILABLE";

const CFG_MMDR_SIZE_API: &str = "mmdr_size_api_available";

#[derive(Debug, Clone, Copy)]
struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

impl Version {
    fn parse(value: &str) -> Option<Self> {
        let value = value
            .trim()
            .trim_start_matches('v')
            .split(['-', '+'])
            .next()?;

        let mut parts = value.split('.');

        Some(Self {
            major: parts.next()?.parse().ok()?,
            minor: parts.next()?.parse().ok()?,
            patch: parts.next()?.parse().ok()?,
        })
    }

    fn as_string(self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }

    fn tag(self) -> String {
        format!("v{}", self.as_string())
    }

    fn with_patch_offset(self, offset: u32) -> Self {
        Self {
            major: self.major,
            minor: self.minor,
            patch: self.patch.saturating_add(offset),
        }
    }
}

#[derive(Debug)]
struct BuildInfo {
    package_version: String,
    base_version: Version,
    build_version: String,

    git_hash: String,
    git_date: String,
    git_tag: String,
    git_dirty: bool,

    changelog: String,
    release: bool,
}

fn main() {
    let repo_root = manifest_dir();

    configure_rerun_rules(&repo_root);

    let metadata = BuildMetadata::load();

    let package_version = package_version();
    let base_version = Version::parse(&package_version).unwrap_or(Version {
        major: 0,
        minor: 0,
        patch: 0,
    });

    // Resolve all metadata fields in parallel. Each call shells out to git
    // exactly once and the four lookups (hash, date, tag, dirty, base-tag-count)
    // are independent, so we fan them out across threads and join.
    let (build_version, git_hash, git_date, git_tag, git_dirty) = std::thread::scope(|s| {
        let bv = s.spawn(|| resolve_build_version(base_version, &repo_root));
        let gh = s.spawn(|| resolve_git_hash(&repo_root, &metadata));
        let gd = s.spawn(|| resolve_git_date(&repo_root, &metadata));
        let gt = s.spawn(|| resolve_git_tag(&repo_root, &metadata));
        let gdt = s.spawn(|| resolve_git_dirty(&repo_root, &metadata));
        (
            bv.join()
                .unwrap_or_else(|_| default_build_version(base_version)),
            gh.join().unwrap_or_else(|_| UNKNOWN.to_string()),
            gd.join().unwrap_or_else(|_| UNKNOWN.to_string()),
            gt.join().unwrap_or_default(),
            gdt.join().unwrap_or(false),
        )
    });

    let info = BuildInfo {
        package_version,
        base_version,
        build_version,

        git_hash,
        git_date,
        git_tag,
        git_dirty,

        changelog: resolve_changelog(&repo_root, &metadata),

        release: env::var_os(ENV_RELEASE).is_some(),
    };

    emit_build_environment(&info);
    configure_mmdr();
}

fn default_build_version(base: Version) -> String {
    base.as_string()
}

// -----------------------------------------------------------------------------
// Build metadata
// -----------------------------------------------------------------------------

fn package_version() -> String {
    env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string())
}

fn resolve_build_version(base: Version, repo_root: &Path) -> String {
    if let Some(version) = explicit_build_version() {
        return version;
    }

    let commits = commits_since_base_tag(base, repo_root).unwrap_or(0);

    base.with_patch_offset(commits).as_string()
}

fn explicit_build_version() -> Option<String> {
    env::var(ENV_SEMVER)
        .ok()
        .map(|value| value.trim().trim_start_matches('v').to_string())
        .filter(|value| Version::parse(value).is_some())
}

fn commits_since_base_tag(base: Version, repo_root: &Path) -> Option<u32> {
    let tag = base.tag();

    git_output(repo_root, &["rev-list", "--count", &format!("{tag}..HEAD")])?
        .trim()
        .parse()
        .ok()
}

// -----------------------------------------------------------------------------
// Git metadata
// -----------------------------------------------------------------------------

fn resolve_git_hash(repo_root: &Path, metadata: &BuildMetadata) -> String {
    env_or_metadata_or_git(
        ENV_GIT_HASH,
        "git_hash",
        repo_root,
        metadata,
        &["rev-parse", "--short", "HEAD"],
    )
    .filter(|value| !value.is_empty())
    .unwrap_or_else(|| UNKNOWN.to_string())
}

fn resolve_git_date(repo_root: &Path, metadata: &BuildMetadata) -> String {
    env_or_metadata_or_git(
        ENV_GIT_DATE,
        "git_date",
        repo_root,
        metadata,
        &["log", "-1", "--format=%ci"],
    )
    .filter(|value| !value.is_empty())
    .unwrap_or_else(|| UNKNOWN.to_string())
}

fn resolve_git_tag(repo_root: &Path, metadata: &BuildMetadata) -> String {
    env_or_metadata_or_git(
        ENV_GIT_TAG,
        "git_tag",
        repo_root,
        metadata,
        &["describe", "--tags", "--always"],
    )
    .unwrap_or_default()
}

fn resolve_git_dirty(repo_root: &Path, metadata: &BuildMetadata) -> bool {
    if let Ok(value) = env::var(ENV_GIT_DIRTY) {
        return parse_bool(&value);
    }

    if let Some(value) = metadata.get("git_dirty") {
        return parse_bool(value);
    }

    git_output(repo_root, &["status", "--porcelain"])
        .map(|output| !output.trim().is_empty())
        .unwrap_or(false)
}

fn env_or_metadata_or_git(
    env_name: &str,
    metadata_key: &str,
    repo_root: &Path,
    metadata: &BuildMetadata,
    git_args: &[&str],
) -> Option<String> {
    env::var(env_name)
        .ok()
        .or_else(|| metadata.get(metadata_key).map(str::to_owned))
        .or_else(|| git_output(repo_root, git_args))
        .map(|value| value.trim().to_string())
}

fn git_output(repo_root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout).ok()
}

// -----------------------------------------------------------------------------
// Changelog
// -----------------------------------------------------------------------------

fn resolve_changelog(repo_root: &Path, metadata: &BuildMetadata) -> String {
    let raw = env::var(ENV_CHANGELOG)
        .ok()
        .or_else(|| metadata.get("changelog_raw").map(str::to_owned))
        .or_else(|| git_output(repo_root, &["log", "-700", "--format=%h|%ct|%D|%s"]))
        .unwrap_or_default();

    parse_changelog(&raw)
}

fn parse_changelog(raw: &str) -> String {
    raw.lines()
        .filter_map(parse_changelog_entry)
        .collect::<Vec<_>>()
        .join("\x1f")
}

fn parse_changelog_entry(line: &str) -> Option<String> {
    let mut parts = line.splitn(4, '|');

    let hash = parts.next()?;
    let timestamp = parts.next().unwrap_or_default();
    let decorations = parts.next().unwrap_or_default();
    let subject = parts.next()?;

    let tag = decorations
        .split(',')
        .map(str::trim)
        .find_map(|decoration| {
            decoration
                .strip_prefix("tag: v")
                .map(|version| format!("v{version}"))
        })
        .unwrap_or_default();

    Some(format!("{hash}\x1e{tag}\x1e{timestamp}\x1e{subject}"))
}

// -----------------------------------------------------------------------------
// Metadata file
// -----------------------------------------------------------------------------

#[derive(Debug, Default)]
struct BuildMetadata {
    values: std::collections::HashMap<String, String>,
}

impl BuildMetadata {
    fn load() -> Self {
        let Some(path) = env::var_os(ENV_METADATA) else {
            return Self::default();
        };

        let Ok(data) = fs::read_to_string(path) else {
            return Self::default();
        };

        Self {
            values: parse_metadata(&data),
        }
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
}

fn parse_metadata(data: &str) -> std::collections::HashMap<String, String> {
    let mut values = std::collections::HashMap::new();
    let mut lines = data.lines();

    while let Some(line) = lines.next() {
        if let Some((key, marker)) = line.split_once("<<") {
            let key = key.trim();

            let mut value = String::new();

            for value_line in lines.by_ref() {
                if value_line == marker {
                    break;
                }

                if !value.is_empty() {
                    value.push('\n');
                }

                value.push_str(value_line);
            }

            values.insert(key.to_string(), value);
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            values.insert(key.trim().to_string(), value.to_string());
        }
    }

    values
}

// -----------------------------------------------------------------------------
// Cargo output
// -----------------------------------------------------------------------------

fn emit_build_environment(info: &BuildInfo) {
    let version = if info.release {
        format!("v{} ({})", info.build_version, info.git_hash)
    } else if info.git_dirty {
        format!("v{}-dev ({}, dirty)", info.build_version, info.git_hash)
    } else {
        format!("v{}-dev ({})", info.build_version, info.git_hash)
    };

    let base_semver = info.base_version.as_string();

    // The update-comparison semver must reflect the *actual* code the user
    // is running, not just the package base tag. For a release build we use
    // the base semver so a v1.0.5 release correctly identifies v1.0.6 as
    // newer; for a dev build, the binary already contains commits past the
    // latest tagged release, so we use build_version (base + commit count)
    // and avoid the older "I am still on v1.0.9 even though I am 5 commits
    // past it" bug that made /update report "already up to date" against
    // a release older than the local checkout.
    let update_semver = if explicit_build_version().is_some() || !info.release {
        info.build_version.clone()
    } else {
        base_semver.clone()
    };

    emit_env("ALPHACODE_GIT_HASH", &info.git_hash);
    emit_env("ALPHACODE_GIT_DATE", &info.git_date);
    emit_env("ALPHACODE_VERSION", &version);
    emit_env("ALPHACODE_SEMVER", &info.build_version);
    emit_env("ALPHACODE_BASE_SEMVER", &base_semver);
    emit_env("ALPHACODE_UPDATE_SEMVER", &update_semver);
    emit_env("ALPHACODE_GIT_TAG", &info.git_tag);
    emit_env("ALPHACODE_CHANGELOG", &info.changelog);
    emit_env("ALPHACODE_PKG_VERSION", &info.package_version);

    if info.release {
        emit_env("ALPHACODE_RELEASE_BUILD", "1");
    }
}

fn emit_env(name: &str, value: &str) {
    println!("cargo:rustc-env={name}={value}");
}

// -----------------------------------------------------------------------------
// mmdr
// -----------------------------------------------------------------------------

fn configure_mmdr() {
    println!("cargo:rustc-check-cfg=cfg({CFG_MMDR_SIZE_API})");

    println!("cargo:rerun-if-env-changed={ENV_MMDR_DISABLE}");
    println!("cargo:rerun-if-env-changed={ENV_MMDR_AVAILABLE}");

    let disabled = env::var_os(ENV_MMDR_DISABLE).is_some();

    let explicitly_available = env::var(ENV_MMDR_AVAILABLE)
        .ok()
        .map(|value| parse_bool(&value))
        .unwrap_or(false);

    if !disabled && explicitly_available {
        println!("cargo:rustc-cfg={CFG_MMDR_SIZE_API}");
    }
}

// -----------------------------------------------------------------------------
// Cargo rerun rules
// -----------------------------------------------------------------------------

fn configure_rerun_rules(repo_root: &Path) {
    println!(
        "cargo:rerun-if-changed={}",
        repo_root.join("Cargo.toml").display()
    );

    for variable in [
        ENV_RELEASE,
        ENV_SEMVER,
        ENV_GIT_HASH,
        ENV_GIT_DATE,
        ENV_GIT_DIRTY,
        ENV_GIT_TAG,
        ENV_CHANGELOG,
        ENV_METADATA,
    ] {
        println!("cargo:rerun-if-env-changed={variable}");
    }

    // Git state can change without Cargo.toml changing.
    watch_git_state(repo_root);

    // If a metadata file exists, Cargo should rebuild when it changes.
    if let Some(path) = env::var_os(ENV_METADATA) {
        println!("cargo:rerun-if-changed={}", PathBuf::from(path).display());
    }
}

fn watch_git_state(repo_root: &Path) {
    let git_dir = match git_output(repo_root, &["rev-parse", "--git-dir"]) {
        Some(path) => {
            let path = PathBuf::from(path.trim());

            if path.is_absolute() {
                path
            } else {
                repo_root.join(path)
            }
        }
        None => return,
    };

    println!("cargo:rerun-if-changed={}", git_dir.join("HEAD").display());

    println!("cargo:rerun-if-changed={}", git_dir.join("index").display());

    if let Ok(head) = fs::read_to_string(git_dir.join("HEAD"))
        && let Some(reference) = head.trim().strip_prefix("ref: ")
    {
        println!(
            "cargo:rerun-if-changed={}",
            git_dir.join(reference).display()
        );
    }
}

// -----------------------------------------------------------------------------
// Utilities
// -----------------------------------------------------------------------------

fn manifest_dir() -> PathBuf {
    env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn parse_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on" | "dirty"
    )
}
