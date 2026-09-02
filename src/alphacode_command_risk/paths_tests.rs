//! Coverage for the catastrophic-path policy.
//!
//! This is the one tier no model justification can unlock, so its behaviour is
//! pinned here directly rather than only through `assess()`. Two properties
//! matter equally and pull in opposite directions: protected paths must never
//! be reachable (a false negative destroys user data), and routine paths must
//! never be denied (a false positive makes the gate useless and trains callers
//! to route around it).
//!
//! Paths are built with [`abs`] rather than written as literals because
//! `Path::is_absolute` is false for `/foo` on Windows, which would silently
//! reroute `expand` through the working-directory branch and make these tests
//! assert something different per platform.

use super::{
    ProtectedPaths, RiskContext, RiskLevel, classify_target, expand, is_catastrophic_target,
};
use std::path::{Path, PathBuf};

/// Build a genuinely host-absolute path, so `expand` takes the same branch on
/// Windows and Unix.
fn abs(parts: &[&str]) -> PathBuf {
    let mut path = if cfg!(windows) {
        PathBuf::from("C:\\")
    } else {
        PathBuf::from("/")
    };
    for part in parts {
        path.push(part);
    }
    path
}

fn home() -> PathBuf {
    abs(&["home", "u"])
}

fn cwd() -> PathBuf {
    abs(&["home", "u", "proj"])
}

fn ctx() -> RiskContext {
    RiskContext {
        working_dir: Some(cwd()),
        home_dir: Some(home()),
    }
}

// ---------------------------------------------------------------------------
// is_catastrophic_target
// ---------------------------------------------------------------------------

#[test]
fn the_home_directory_itself_is_catastrophic() {
    assert!(is_catastrophic_target(&home(), &ctx()));
}

#[test]
fn every_declared_system_root_is_catastrophic() {
    for path in ProtectedPaths::system_paths() {
        assert!(
            is_catastrophic_target(Path::new(path), &ctx()),
            "declared system root {path} is not actually protected"
        );
    }
}

/// For the paths whose contents are as unrecoverable as the directory, a single
/// file inside must be denied too.
#[test]
fn contents_of_recursively_protected_system_paths_are_catastrophic() {
    for path in ProtectedPaths::recursive_system_paths() {
        let nested = Path::new(path).join("nested").join("file");
        assert!(
            is_catastrophic_target(&nested, &ctx()),
            "{} should be protected by the recursive rule on {path}",
            nested.display()
        );
    }
}

#[test]
fn credential_stores_are_protected_all_the_way_down() {
    for sub in ProtectedPaths::credential_subpaths() {
        let key = home().join(sub).join("nested").join("id_rsa");
        assert!(
            is_catastrophic_target(&key, &ctx()),
            "{} should be protected as a credential store",
            key.display()
        );
    }
}

/// Config and document roots are matched exactly: the directory is protected,
/// the files inside it are edited and deleted routinely.
#[test]
fn home_roots_are_protected_but_their_files_are_not() {
    for sub in ProtectedPaths::home_subpaths() {
        let root = home().join(sub);
        assert!(
            is_catastrophic_target(&root, &ctx()),
            "{} should be protected",
            root.display()
        );

        // `child.toml` deliberately avoids colliding with a longer entry in the
        // list (`.local` vs `.local/share`), which would be protected in its
        // own right.
        let child = root.join("child.toml");
        assert!(
            !is_catastrophic_target(&child, &ctx()),
            "{} is an ordinary file and should not be denied",
            child.display()
        );
    }
}

#[test]
fn a_project_directory_is_not_catastrophic() {
    assert!(!is_catastrophic_target(&cwd(), &ctx()));
    assert!(!is_catastrophic_target(&cwd().join("target"), &ctx()));
}

/// `..` must be resolved before comparison, or traversal walks straight past
/// the protected set.
#[test]
fn parent_traversal_cannot_escape_the_protected_set() {
    // Resolves to the home directory.
    assert!(is_catastrophic_target(&cwd().join(".."), &ctx()));
    // Resolves back onto a credential store.
    assert!(is_catastrophic_target(
        &home().join(".ssh").join("..").join(".ssh"),
        &ctx()
    ));
    // Rooted literal with no drive prefix on either platform, so this exercises
    // `..` resolution rather than platform path syntax: it collapses to `/`.
    assert!(is_catastrophic_target(
        Path::new("/etc/nested/../.."),
        &ctx()
    ));
}

/// Without a known home directory the home rules cannot apply, but the system
/// rules must still hold — this is the configuration a bare `RiskContext` has.
#[test]
fn system_protection_survives_a_missing_home_dir() {
    let ctx = RiskContext::default();
    assert!(is_catastrophic_target(Path::new("/etc/passwd"), &ctx));
    assert!(!is_catastrophic_target(&home(), &ctx));
}

// ---------------------------------------------------------------------------
// Device sinks
// ---------------------------------------------------------------------------

/// `cmd > /dev/null` is one of the most common shell idioms there is. Denying
/// it would make the gate an obstacle rather than a safeguard, so the three
/// standard sinks are exempt from the recursive `/dev` rule.
#[test]
fn standard_device_sinks_are_not_flagged() {
    for sink in ["/dev/null", "/dev/stdout", "/dev/stderr"] {
        assert!(
            !is_catastrophic_target(Path::new(sink), &ctx()),
            "{sink} is a routine write target"
        );
        assert_eq!(
            classify_target(Path::new(sink), sink, false, &ctx()),
            None,
            "{sink} should not produce a finding at all"
        );
    }
}

#[test]
fn real_device_nodes_are_still_catastrophic() {
    for device in ["/dev/sda", "/dev/disk0", "/dev/nullX"] {
        let finding = classify_target(Path::new(device), device, false, &ctx())
            .unwrap_or_else(|| panic!("{device} should be flagged"));
        assert_eq!(
            finding.level,
            RiskLevel::Catastrophic,
            "{device} must not be writable"
        );
    }
}

// ---------------------------------------------------------------------------
// classify_target
// ---------------------------------------------------------------------------

/// No single resolved path in `/etc/*` is protected, but the effect is the
/// same as destroying `/etc`.
#[test]
fn a_glob_over_a_protected_directory_is_catastrophic() {
    let etc = classify_target(Path::new("/etc/*"), "/etc/*", true, &ctx())
        .expect("/etc/* should be flagged");
    assert_eq!(etc.level, RiskLevel::Catastrophic);

    let home_glob = home().join("*");
    let flagged = classify_target(&home_glob, "~/*", true, &ctx()).expect("~/* should be flagged");
    assert_eq!(flagged.level, RiskLevel::Catastrophic);
}

/// An ordinary glob is unknowable, not unacceptable — it earns a reflection
/// turn, not an absolute deny.
#[test]
fn an_ordinary_glob_is_confirm_not_deny() {
    let finding = classify_target(&cwd().join("*.log"), "*.log", false, &ctx())
        .expect("a glob should always be flagged");
    assert_eq!(finding.level, RiskLevel::Confirm);
}

#[test]
fn deletes_inside_the_working_directory_are_routine() {
    assert_eq!(
        classify_target(&cwd().join("notes.txt"), "notes.txt", false, &ctx()),
        None
    );

    let recursive = classify_target(&cwd().join("build"), "build", true, &ctx())
        .expect("a recursive delete should be recorded");
    assert_eq!(recursive.level, RiskLevel::Low);
}

#[test]
fn deletes_outside_the_working_directory_need_confirmation() {
    let outside = abs(&["home", "u", "other-project"]);
    let finding = classify_target(&outside, "../other-project", false, &ctx())
        .expect("a path outside the cwd should be flagged");
    assert_eq!(finding.level, RiskLevel::Confirm);
}

#[test]
fn temp_directories_are_disposable() {
    for temp in ["/tmp/build", "/var/tmp/cache", "/private/tmp/x"] {
        assert_eq!(
            classify_target(Path::new(temp), temp, true, &ctx()),
            None,
            "{temp} is conventionally disposable"
        );
    }
}

/// An unexpanded variable is unknowable, so it cannot be cleared in advance.
#[test]
fn runtime_computed_targets_need_confirmation() {
    for raw in ["$TARGET", "`cat which-dir`"] {
        let finding = classify_target(&PathBuf::from(raw), raw, true, &ctx())
            .unwrap_or_else(|| panic!("{raw} should be flagged"));
        assert_eq!(finding.level, RiskLevel::Confirm);
    }
}

/// `$HOME` resolves to a protected path, so it must be denied outright rather
/// than merely queried as an unknown substitution.
#[test]
fn a_variable_that_resolves_to_home_is_denied_not_queried() {
    let expanded = expand("$HOME", &ctx());
    let finding =
        classify_target(&expanded, "$HOME", true, &ctx()).expect("$HOME should be denied");
    assert_eq!(finding.level, RiskLevel::Catastrophic);
}

// ---------------------------------------------------------------------------
// expand
// ---------------------------------------------------------------------------

#[test]
fn expand_resolves_the_home_shorthands() {
    assert_eq!(expand("~", &ctx()), home());
    assert_eq!(expand("$HOME", &ctx()), home());
    assert_eq!(expand("${HOME}", &ctx()), home());
    assert_eq!(expand("~/.ssh", &ctx()), home().join(".ssh"));
}

#[test]
fn expand_resolves_relative_paths_against_the_working_dir() {
    assert_eq!(expand("build", &ctx()), cwd().join("build"));
    assert_eq!(expand("./build", &ctx()), cwd().join("build"));
    assert_eq!(expand("..", &ctx()), home());
}
