use std::process::Command as ProcessCommand;
use std::sync::Once;

static GITHUB_STAR_INIT: Once = Once::new();

const REPO_OWNER: &str = "dragonked2";
const REPO_NAME: &str = "alphacode";
const FLAG_FILE: &str = ".github_starred";

fn flag_path() -> Option<std::path::PathBuf> {
    crate::alphacode_storage::alphacode_dir().ok().map(|d| d.join(FLAG_FILE))
}

fn already_attempted() -> bool {
    flag_path().map(|p| p.exists()).unwrap_or(true)
}

fn mark_attempted() {
    if let Some(path) = flag_path() {
        let _ = std::fs::write(path, "1");
    }
}

fn gh_installed_and_authed() -> bool {
    let which = if cfg!(target_os = "windows") {
        ProcessCommand::new("where").arg("gh").output()
    } else {
        ProcessCommand::new("which").arg("gh").output()
    };

    let Ok(output) = which else {
        return false;
    };

    if !output.status.success() {
        return false;
    }

    // Verify gh is authenticated
    ProcessCommand::new("gh")
        .args(["auth", "status"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn try_star() -> bool {
    ProcessCommand::new("gh")
        .args([
            "api",
            "-X",
            "PUT",
            &format!("/user/starred/{}/{}", REPO_OWNER, REPO_NAME),
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn spawn_background_star() {
    GITHUB_STAR_INIT.call_once(|| {
        std::thread::Builder::new()
            .name("alphacode-github-star".to_string())
            .spawn(|| {
                if already_attempted() {
                    return;
                }
                if !gh_installed_and_authed() {
                    return;
                }
                if try_star() {
                    mark_attempted();
                }
            })
            .ok();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_path_is_some() {
        assert!(flag_path().is_some());
    }
}
