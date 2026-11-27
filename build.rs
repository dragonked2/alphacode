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
    configure_bundled_skills(&repo_root);
    configure_gui_assets(&repo_root, &info);
    configure_windows_desktop(&repo_root, &info);
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

/// `(major, minor, patch)` of the shipped crate version for the Windows
/// VERSIONINFO resource. Numeric resource fields cannot carry a pre-release
/// or build suffix, so the leading `x.y.z` is all that is used.
fn release_version_triplet() -> (u64, u64, u64) {
    let version = package_version();
    let mut parts = version.split('.');
    let mut next = || -> u64 {
        parts
            .next()
            .and_then(|part| {
                let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
                digits.parse().ok()
            })
            .unwrap_or(0)
    };
    (next(), next(), next())
}

fn resolve_build_version(base: Version, repo_root: &Path) -> String {
    if let Some(version) = explicit_build_version() {
        return version;
    }

    // For release builds, derive the version from the annotated git tag rather
    // than commit-count arithmetic. The tag *is* the release version; commit
    // counting produces a phantom number (e.g. v1.0.34) that does not match
    // the tag the CI created (v1.0.27), which confuses users and breaks the
    // update check (update_semver ends up wrong).
    let is_release = env::var_os(ENV_RELEASE).is_some();
    if is_release && let Some(tag_version) = extract_version_from_release_tag(repo_root) {
        return tag_version;
    }

    let commits = commits_since_base_tag(base, repo_root).unwrap_or(0);

    base.with_patch_offset(commits).as_string()
}

/// When `ALPHACODE_RELEASE_BUILD` is set, try to extract the version from the
/// current git tag. Looks for tags matching `v*.*.*` at HEAD and returns the
/// version string (without the `v` prefix).
fn extract_version_from_release_tag(repo_root: &Path) -> Option<String> {
    // `git tag --points-at HEAD --list 'v*'` lists tags pointing at HEAD.
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["tag", "--points-at", "HEAD", "--list", "v*"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let tags = String::from_utf8_lossy(&output.stdout);
    // Find the first valid semver tag.
    for tag in tags.lines() {
        let tag = tag.trim();
        if let Some(version) = tag.strip_prefix('v')
            && Version::parse(version).is_some()
        {
            return Some(version.to_string());
        }
    }

    // Fallback: `git describe --exact-match` returns a tag only when one points
    // at HEAD. This used to be plain `--abbrev=0`, which returns the *nearest*
    // tag — possibly hundreds of commits behind — and the result was accepted
    // unconditionally despite the comment claiming otherwise. That shipped a
    // release whose `--version` reported an older tag than the crate it was
    // built from (observed: binary said v1.0.91 while Cargo.toml and the Windows
    // VERSIONINFO resource said 1.0.93). Without an exact match, fall through to
    // the caller's Cargo.toml-derived version, which is always correct.
    let describe = Command::new("git")
        .current_dir(repo_root)
        .args(["describe", "--tags", "--exact-match"])
        .output()
        .ok()?;

    if !describe.status.success() {
        return None;
    }

    let tag = String::from_utf8_lossy(&describe.stdout);
    let tag = tag.trim();
    let version = tag.strip_prefix('v')?;
    Version::parse(version)
        .is_some()
        .then(|| version.to_string())
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
    // is running, not just the package base tag. For release builds we use
    // the build_version (which now comes from the git tag via
    // extract_version_from_release_tag, or from ALPHACODE_BUILD_SEMVER in CI)
    // so v1.0.27 correctly identifies v1.0.28 as newer. For dev builds, the
    // binary already contains commits past the latest tagged release, so we
    // use build_version (base + commit count) and avoid the "already up to
    // date" bug.
    let update_semver = info.build_version.clone();

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
// bundled skills
// -----------------------------------------------------------------------------

/// Compress the bundled skill corpus into a single deflated blob.
///
/// The manifest is read from `src/alphacode_base/bundled_skills.rs`, packed by
/// `scripts/bundled_skills_manifest.py`, then deflated here and emitted as one
/// `include_bytes!` target. This replaces ~290 `include_str!` calls whose
/// contents were several megabytes of *plaintext documentation* inside the
/// shipped executable.
///
/// That plaintext was the single largest source of false antivirus verdicts:
/// generic heuristics and crowdsourced YARA rules matched inert methodology
/// text (browser-credential forensics recipes, `webhook.site/` callback
/// examples, event-log clearing commands, hypervisor MAC prefixes, tool-name
/// lists) and labelled the program a stealer/dropper. Nothing in that text was
/// ever executed, but a scanner cannot know that from a byte match.
///
/// Storing the corpus deflated removes every one of those literals from the
/// image while serving the model exactly the same bytes at runtime.
fn configure_bundled_skills(root: &Path) {
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let manifest = root
        .join("src")
        .join("alphacode_base")
        .join("bundled_skills.json");
    let packer = root.join("scripts").join("bundled_skills_manifest.py");

    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rerun-if-changed={}", packer.display());
    println!(
        "cargo:rerun-if-changed={}",
        root.join("src")
            .join("alphacode_base")
            .join("bundled_skills")
            .display()
    );

    let raw = out.join("bundled-skills.pack");
    let status = Command::new(python_interpreter())
        .arg(&packer)
        .arg(&raw)
        .status()
        .expect("run bundled-skills packer");
    assert!(
        status.success(),
        "bundled-skills packer failed: {}",
        packer.display()
    );

    let packed = fs::read(&raw).expect("read packed bundled skills");
    let target = out.join("bundled-skills.bin");
    // The packer already deflates the body data, so write its output as-is.
    fs::write(&target, packed).expect("write compressed bundled skills");
    println!(
        "cargo:rustc-env=ALPHACODE_BUNDLED_SKILLS_BYTES={}",
        target.display()
    );
}

/// Locate a Python interpreter for the packer.
///
/// `PYTHON` wins so a caller can pin one; otherwise try the names each platform
/// actually ships (`python` on Windows and most Unix images, `python3`
/// elsewhere). A missing interpreter is a hard error rather than a silent
/// fallback: shipping uncompressed documentation is the defect this prevents.
fn python_interpreter() -> String {
    if let Some(explicit) = env::var_os("PYTHON") {
        return explicit.to_string_lossy().into_owned();
    }
    for candidate in ["python", "python3", "py"] {
        let found = Command::new(candidate)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if matches!(found, Ok(status) if status.success()) {
            return candidate.to_string();
        }
    }
    panic!(
        "no Python interpreter found for scripts/bundled_skills_manifest.py; \
         set PYTHON to one that works in this build environment"
    );
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
// GUI static assets
// -----------------------------------------------------------------------------

const ENV_REQUIRE_GUI_ASSETS: &str = "ALPHACODE_REQUIRE_GUI_ASSETS";

/// Compress built frontend assets (`src/cli/gui/ui/dist`) with deterministic
/// gzip and generate `OUT_DIR/gui_embedded_assets.rs`.
///
/// On release builds (`ALPHACODE_RELEASE_BUILD=1`) or when
/// `ALPHACODE_REQUIRE_GUI_ASSETS=1` is set, missing or incomplete frontend
/// assets are a hard build error so a release can never ship an empty GUI.
/// Local developer builds without `ui/dist` still compile cleanly with an
/// empty asset table.
fn configure_gui_assets(root: &Path, info: &BuildInfo) {
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let packer = root.join("scripts").join("gui_assets_pack.py");
    let dist_dir = root
        .join("src")
        .join("cli")
        .join("gui")
        .join("ui")
        .join("dist");

    println!("cargo:rerun-if-env-changed={ENV_REQUIRE_GUI_ASSETS}");
    println!("cargo:rerun-if-changed={}", packer.display());
    println!("cargo:rerun-if-changed={}", dist_dir.display());
    if dist_dir.is_dir() {
        watch_gui_dist_files(&dist_dir);
    }

    let require_assets = info.release
        || env::var(ENV_REQUIRE_GUI_ASSETS)
            .ok()
            .is_some_and(|value| parse_bool(&value));

    let mut cmd = Command::new(python_interpreter());
    cmd.arg(&packer).arg(&dist_dir).arg(&out);
    if require_assets {
        cmd.arg("--require");
    }
    let output = cmd.output().expect("run GUI asset packer");
    assert!(
        output.status.success(),
        "GUI asset packer failed ({}): {} {}",
        packer.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn watch_gui_dist_files(dir: &Path) {
    println!("cargo:rerun-if-changed={}", dir.display());
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            watch_gui_dist_files(&path);
        } else if path.is_file() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
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

    // Deliberately NOT watching `index`.
    //
    // The stamp embeds `HEAD`, the ref, and a dirty flag derived from
    // `git status`. `.git/index` is a *stat cache*: ordinary read-only commands
    // (`git status`, `git diff`, editor git integrations) rewrite it whenever
    // they refresh file metadata, without changing any commit, ref, or tracked
    // content. Watching it meant that merely looking at the repository caused
    // build-script rerun and a partial recompile of every crate that depends on
    // the stamp — a recurring cost that made "I only ran git status" feel like
    // a rebuild.
    //
    // Correctness is preserved because the value derived from the index is the
    // dirty flag, and a dirty flag can only change when tracked *content*
    // changes — which is already covered by watching the worktree inputs that
    // matter here (`HEAD`, the checked-out ref) and by Cargo's own source
    // change detection. `git describe`/`rev-parse` results are unaffected by
    // the index.

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

/// Embed the application icon and compile the Windows desktop host. The host
/// embeds its WPF script so an installed binary needs no adjacent source files.
fn configure_windows_desktop(root: &Path, info: &BuildInfo) {
    let target = env::var("TARGET").unwrap_or_default();
    if !target.contains("windows") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let icon = root.join("assets").join("app-icons").join("alphacode.ico");
    let manifest = root
        .join("assets")
        .join("app-icons")
        .join("alphacode.manifest");
    let host_source = root.join("src").join("cli").join("gui").join("GuiHost.cs");
    let diagnostics = root
        .join("src")
        .join("cli")
        .join("gui")
        .join("GuiDiagnostics.cs");
    let ui = root.join("src").join("cli").join("gui").join("gui.ps1");
    for path in [&icon, &manifest, &host_source, &diagnostics, &ui] {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let host = out.join("AlphaCode.Desktop.exe");
    let legacy_host = out.join("AlphaCode.Legacy.exe");
    if !cfg!(windows) {
        // Cross-builds cannot use Windows PowerShell's .NET Framework host.
        for path in [&host, &legacy_host] {
            fs::write(path, []).expect("write unavailable GUI host marker");
        }
        println!("cargo:warning=Windows GUI host requires a native Windows build");
    } else {
        let windir = PathBuf::from(env::var_os("WINDIR").expect("WINDIR"));
        let compiler = windir
            .join("Microsoft.NET")
            .join("Framework64")
            .join("v4.0.30319")
            .join("csc.exe");
        let automation = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "[System.Management.Automation.PSObject].Assembly.Location",
            ])
            .output()
            .expect("find Windows PowerShell assembly");
        assert!(
            automation.status.success(),
            "could not find Windows PowerShell assembly"
        );
        let assembly = String::from_utf8(automation.stdout).expect("PowerShell assembly path");
        let identity = format!(
            "{} ({}{})",
            info.build_version,
            info.git_hash,
            if info.git_dirty { ", dirty" } else { "" }
        );
        let (major, minor, patch) = release_version_triplet();
        let host_specs = [
            (
                "AlphaCode.Desktop.exe",
                "AlphaCode Desktop",
                "GuiBuildInfo.cs",
                false,
            ),
            (
                "AlphaCode.Legacy.exe",
                "AlphaCode Legacy GUI",
                "GuiLegacyBuildInfo.cs",
                true,
            ),
        ];
        for (exe_name, title, meta_file, legacy_default) in host_specs {
            let output_exe = out.join(exe_name);
            compile_windows_gui_host(
                &compiler,
                assembly.trim(),
                &out,
                &output_exe,
                &icon,
                &manifest,
                &host_source,
                &diagnostics,
                &ui,
                &identity,
                (major, minor, patch),
                exe_name,
                title,
                meta_file,
                legacy_default,
            );
        }
    }
    println!(
        "cargo:rustc-env=ALPHACODE_DESKTOP_HOST_BYTES={}",
        host.display()
    );
    // The host is *built* either way; only the embedding is opt-in.
    //
    // Shipping it as a sibling file (and embedding nothing) removes the
    // strongest generic-verdict signal from the main executable: an unsigned PE
    // that carries a second, complete, unsigned PE in its data section and later
    // writes it to disk and launches it is exactly the packer/dropper shape
    // heuristic engines score. BitDefender's `Gen:Variant.Barys` (also surfaced
    // as ALYac/Arcibit/eScan/Emsisoft/GData/VIPRE) flagged the 1.0.92 Windows
    // release for this, not for anything the program does.
    //
    // The `embedded-gui-host` cargo feature selects the legacy behaviour:
    //   enabled  -> `include_bytes!` embeds the host (single-file release)
    //   disabled -> the host ships as a sibling `AlphaCode.Desktop.exe`
    // `src/cli/gui.rs` reads the feature, so the env var is only a path hint.
    let embed_host = env::var_os("CARGO_FEATURE_EMBEDDED_GUI_HOST").is_some();
    if embed_host {
        println!(
            "cargo:warning=embedded-gui-host enabled: the release binary contains a nested PE"
        );
    }
    // Publish the built host next to the binary so the release archive and a
    // local `cargo build` both end up with the sibling the launcher expects.
    // `OUT_DIR` is `<profile>/build/<pkg>-<hash>/out`, so the third ancestor is
    // the profile directory that also holds `alphacode.exe`.
    if cfg!(windows)
        && let Some(profile_dir) = out.ancestors().nth(3)
    {
        for (src_host, exe_name) in [
            (&host, "AlphaCode.Desktop.exe"),
            (&legacy_host, "AlphaCode.Legacy.exe"),
        ] {
            let sibling = profile_dir.join(exe_name);
            let current = std::fs::read(&sibling).ok();
            let built = std::fs::read(src_host).ok();
            if current.as_deref() != built.as_deref()
                && let Err(error) = std::fs::copy(src_host, &sibling)
            {
                println!(
                    "cargo:warning=could not stage {}: {error}",
                    sibling.display()
                );
            }
        }
    }
    if target.contains("msvc") {
        let source = out.join("alphacode-icon.rc");
        let resource = out.join("alphacode-icon.res");
        let icon_path = icon.to_string_lossy().replace('\\', "\\\\");
        // Ship the version resource too, not just the icon. Without a
        // VERSIONINFO block Windows reports FileVersion 0.0.0.0 and empty
        // CompanyName/FileDescription for the executable, which is the
        // shape of an "unattributed, version-less PE" heuristic AV engines
        // score as suspicious, and it leaves Explorer's Properties dialog
        // useless to anyone verifying a downloaded release.
        //
        // Numeric fields derive from the crate version so they cannot
        // drift from the shipped build.
        let (v_major, v_minor, v_patch) = release_version_triplet();
        let version_csv = format!("{v_major},{v_minor},{v_patch},0");
        let version_dotted = format!("{v_major}.{v_minor}.{v_patch}");
        let mut rc = String::new();
        rc.push_str(&format!("1 ICON \"{icon_path}\"\n"));
        rc.push_str("1 VERSIONINFO\n");
        rc.push_str(&format!("FILEVERSION {version_csv}\n"));
        rc.push_str(&format!("PRODUCTVERSION {version_csv}\n"));
        rc.push_str("FILEOS 0x40004\nFILETYPE 1\nBEGIN\n");
        rc.push_str("BLOCK \"StringFileInfo\"\nBEGIN\nBLOCK \"040904b0\"\nBEGIN\n");
        rc.push_str("VALUE \"FileDescription\", \"AlphaCode - Terminal coding agent\\0\"\n");
        rc.push_str("VALUE \"ProductName\", \"AlphaCode\\0\"\n");
        rc.push_str("VALUE \"CompanyName\", \"AlphaCode\\0\"\n");
        // Empty LegalCopyright is itself an "unattributed binary" signal, and it
        // leaves Explorer's Properties dialog incomplete for anyone verifying a
        // downloaded release. scripts/verify-pe-metadata.py fails the build if a
        // shipped PE is missing it.
        rc.push_str("VALUE \"LegalCopyright\", \"Copyright (c) Ali Essam\\0\"\n");
        rc.push_str("VALUE \"OriginalFilename\", \"alphacode.exe\\0\"\n");
        rc.push_str(&format!("VALUE \"FileVersion\", \"{version_dotted}\\0\"\n"));
        rc.push_str(&format!(
            "VALUE \"ProductVersion\", \"{version_dotted}\\0\"\n"
        ));
        rc.push_str("END\nEND\nBLOCK \"VarFileInfo\"\nBEGIN\n");
        rc.push_str("VALUE \"Translation\", 0x409, 1200\nEND\nEND\n");
        fs::write(&source, rc).expect("write icon resource source");
        let result = Command::new(windows_resource_compiler())
            .arg("/nologo")
            .arg(format!("/fo{}", resource.display()))
            .arg(&source)
            .output();
        match result {
            Ok(result) if result.status.success() => {
                println!("cargo:rustc-link-arg-bin=alphacode={}", resource.display())
            }
            Ok(result) => panic!(
                "icon resource build failed: {}",
                String::from_utf8_lossy(&result.stderr)
            ),
            Err(error) => panic!(
                "Windows resource compiler not found: {error}. Load the MSVC build environment before building."
            ),
        }
    }
}

/// Compile one Windows GUI host executable (.NET Framework `csc.exe`) with embedded WPF script,
/// icon, manifest, and `.rsrc` VERSIONINFO metadata (declared via C# assembly attributes so no PE
/// overlay is appended).
#[allow(clippy::too_many_arguments)]
fn compile_windows_gui_host(
    compiler: &Path,
    automation_assembly: &str,
    out: &Path,
    output_exe: &Path,
    icon: &Path,
    manifest: &Path,
    host_source: &Path,
    diagnostics: &Path,
    ui: &Path,
    identity: &str,
    version: (u64, u64, u64),
    exe_name: &str,
    title: &str,
    meta_file: &str,
    legacy_default: bool,
) {
    let (major, minor, patch) = version;
    let literal = |value: &str| value.replace('"', "\"\"");
    let metadata_source = out.join(meta_file);
    // csc.exe synthesises the whole VERSIONINFO resource from these assembly
    // attributes and links it into `.rsrc`, avoiding a PE overlay and ensuring
    // Windows reports ProductName/CompanyName/LegalCopyright cleanly.
    fs::write(
        &metadata_source,
        format!(
            "[assembly: System.Reflection.AssemblyTitle(\"{}\")]\n\
             [assembly: System.Reflection.AssemblyProduct(\"AlphaCode\")]\n\
             [assembly: System.Reflection.AssemblyCompany(\"AlphaCode\")]\n\
             [assembly: System.Reflection.AssemblyCopyright(\"Copyright (c) Ali Essam\")]\n\
             [assembly: System.Reflection.AssemblyFileVersion(\"{major}.{minor}.{patch}.0\")]\n\
             [assembly: System.Reflection.AssemblyVersion(\"{major}.{minor}.{patch}.0\")]\n\
             [assembly: System.Reflection.AssemblyInformationalVersion(@\"{}\")]\n",
            literal(title),
            literal(identity)
        ),
    )
    .expect("write GUI build identity");
    let mut cmd = Command::new(compiler);
    cmd.args(["/nologo", "/target:winexe"])
        .arg(format!("/out:{}", output_exe.display()))
        .arg(format!("/reference:{automation_assembly}"))
        .arg("/reference:System.Web.Extensions.dll")
        .arg("/reference:System.Core.dll")
        .arg(format!("/resource:{},AlphaCode.UI", ui.display()))
        .arg(format!("/win32icon:{}", icon.display()))
        .arg(format!("/win32manifest:{}", manifest.display()));
    if legacy_default {
        cmd.arg("/define:LEGACY_DEFAULT");
    }
    let result = cmd
        .arg(host_source)
        .arg(diagnostics)
        .arg(&metadata_source)
        .output()
        .expect("start desktop host compiler");
    assert!(
        result.status.success(),
        "{exe_name} build failed: {} {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

/// Cargo can find MSVC without vcvars being loaded; find an installed SDK's
/// resource compiler as well so ordinary Windows builds can embed the icon.
fn windows_resource_compiler() -> PathBuf {
    let sdk = env::var_os("WindowsSdkDir").map(PathBuf::from).or_else(|| {
        env::var_os("ProgramFiles(x86)")
            .map(PathBuf::from)
            .map(|path| path.join("Windows Kits").join("10"))
    });
    if let Some(sdk) = sdk {
        let bin = sdk.join("bin");
        if let Ok(entries) = fs::read_dir(bin) {
            let mut candidates: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
            candidates.sort();
            for version in candidates.into_iter().rev() {
                for arch in ["x64", "x86", "arm64"] {
                    let compiler = version.join(arch).join("rc.exe");
                    if compiler.is_file() {
                        return compiler;
                    }
                }
            }
        }
    }
    PathBuf::from("rc.exe")
}
