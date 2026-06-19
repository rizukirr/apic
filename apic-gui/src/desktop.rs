//! `apic-gui --desktop-entry`: register the running binary in the Linux
//! application launcher (per-user, no root). A no-op-with-message on other
//! platforms, which are served by Homebrew / winget / the release artifacts.

#[cfg(target_os = "linux")]
use std::path::Path;

/// Builds the `.desktop` file body, with `Exec` pointing at `exec`.
#[cfg(target_os = "linux")]
fn desktop_entry(exec: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=apic\n\
         GenericName=API Contract Explorer\n\
         Comment=Browse and edit Git-friendly API contracts\n\
         Exec={exec}\n\
         Icon=apic-gui\n\
         Terminal=false\n\
         Categories=Development;\n\
         Keywords=api;contract;rest;json;\n\
         StartupWMClass=apic-gui\n"
    )
}

/// Writes the icon and `.desktop` file under `data_dir` (an XDG data dir such as
/// `~/.local/share`), with `Exec` = `exec`. Returns a human-readable summary.
#[cfg(target_os = "linux")]
fn install_to(data_dir: &Path, exec: &str) -> Result<String, String> {
    let icon_dir = data_dir.join("icons/hicolor/256x256/apps");
    let apps_dir = data_dir.join("applications");
    std::fs::create_dir_all(&icon_dir)
        .map_err(|e| format!("create {}: {e}", icon_dir.display()))?;
    std::fs::create_dir_all(&apps_dir)
        .map_err(|e| format!("create {}: {e}", apps_dir.display()))?;

    let icon_path = icon_dir.join("apic-gui.png");
    std::fs::write(&icon_path, include_bytes!("../assets/icon.png"))
        .map_err(|e| format!("write {}: {e}", icon_path.display()))?;

    let desktop_path = apps_dir.join("apic-gui.desktop");
    std::fs::write(&desktop_path, desktop_entry(exec))
        .map_err(|e| format!("write {}: {e}", desktop_path.display()))?;

    Ok(format!(
        "Installed launcher entry:\n  {}\n  {}\nSearch \"apic\" in your application launcher.",
        desktop_path.display(),
        icon_path.display(),
    ))
}

/// Refreshes the desktop/icon caches. Best-effort: failures are ignored because
/// the entry already works without them.
#[cfg(target_os = "linux")]
fn refresh_caches(data_dir: &Path) {
    use std::process::Command;
    let _ = Command::new("update-desktop-database")
        .arg(data_dir.join("applications"))
        .status();
    let _ = Command::new("gtk-update-icon-cache")
        .args(["-f", "-t"])
        .arg(data_dir.join("icons/hicolor"))
        .status();
}

/// Entry point for the `--desktop-entry` flag.
#[cfg(target_os = "linux")]
pub fn install_desktop_entry() -> Result<String, String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("cannot resolve the running binary path: {e}"))?;
    let data_dir = dirs::data_dir().ok_or("cannot resolve the XDG data directory")?;
    let summary = install_to(&data_dir, &exe.to_string_lossy())?;
    refresh_caches(&data_dir);
    Ok(summary)
}

/// Non-Linux: launcher integration is handled by platform package managers.
#[cfg(not(target_os = "linux"))]
pub fn install_desktop_entry() -> Result<String, String> {
    Err("--desktop-entry is Linux-only. On macOS install via Homebrew (or use the .app from Releases); on Windows use winget (or the .exe from Releases).".to_string())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn desktop_entry_has_exec_and_wmclass() {
        let body = desktop_entry("/home/u/.cargo/bin/apic-gui");
        assert!(body.contains("Exec=/home/u/.cargo/bin/apic-gui\n"));
        assert!(body.contains("Icon=apic-gui\n"));
        assert!(body.contains("StartupWMClass=apic-gui\n"));
        assert!(body.starts_with("[Desktop Entry]\n"));
    }

    #[test]
    fn install_to_writes_desktop_and_icon() {
        let tmp = std::env::temp_dir().join(format!("apic-desk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        install_to(&tmp, "/fake/apic-gui").unwrap();

        let desktop = tmp.join("applications/apic-gui.desktop");
        let icon = tmp.join("icons/hicolor/256x256/apps/apic-gui.png");
        assert!(desktop.is_file());
        assert!(icon.is_file());
        let body = std::fs::read_to_string(&desktop).unwrap();
        assert!(body.contains("Exec=/fake/apic-gui\n"));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
