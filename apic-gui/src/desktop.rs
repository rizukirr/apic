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

/// Filenames of other `apic` launcher entries we might collide with: the
/// package builds (AUR/COPR) ship `apic-gui.desktop`; the Flatpak build ships
/// one named after its app id. All carry `Name=apic`.
#[cfg(target_os = "linux")]
const OTHER_DESKTOP_NAMES: [&str; 2] = ["apic-gui.desktop", "io.github.rizukirr.apic.desktop"];

/// Scans the XDG data dirs (`data_dirs`, colon-separated) for another `apic`
/// launcher entry — e.g. one dropped by a distro package like the AUR `apic-bin`
/// or a Flatpak install. They share `Name=apic`, so a launcher that does not
/// de-dup by desktop-file-id shows two "apic" apps. Skips only the exact file we
/// write (so a user-scoped Flatpak entry under the same data root still counts).
/// Returns the first such path found, so the caller can warn.
#[cfg(target_os = "linux")]
fn find_system_entry(data_dirs: &str, own_data_dir: &Path) -> Option<std::path::PathBuf> {
    let own = own_data_dir.join("applications/apic-gui.desktop");
    for dir in data_dirs.split(':').filter(|s| !s.is_empty()) {
        for name in OTHER_DESKTOP_NAMES {
            let candidate = Path::new(dir).join("applications").join(name);
            if candidate == own {
                continue;
            }
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
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
    let mut summary = install_to(&data_dir, &exe.to_string_lossy())?;
    refresh_caches(&data_dir);

    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    if let Some(other) = find_system_entry(&data_dirs, &data_dir) {
        summary.push_str(&format!(
            "\n\nNote: a system-wide launcher entry already exists at\n  {}\n\
             You may see two \"apic\" entries. Remove the system package (e.g.\n\
             `sudo pacman -R apic-bin`) or that file to de-duplicate.",
            other.display(),
        ));
    }
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

    #[test]
    fn find_system_entry_detects_and_skips_own_dir() {
        let base = std::env::temp_dir().join(format!("apic-sysentry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let sys = base.join("usr/share");
        let own = base.join("home/.local/share");
        // A user-scoped Flatpak export lives *under* the same data root as our
        // own entry, so the skip must be the exact file, not the whole subtree.
        let flatpak = own.join("flatpak/exports/share");
        std::fs::create_dir_all(sys.join("applications")).unwrap();
        std::fs::create_dir_all(own.join("applications")).unwrap();
        std::fs::create_dir_all(flatpak.join("applications")).unwrap();

        let dirs = format!("{}:{}:{}", own.display(), flatpak.display(), sys.display());

        // Nothing installed elsewhere yet -> no warning.
        assert!(find_system_entry(&dirs, &own).is_none());

        // Our own per-user entry must never count as a duplicate.
        std::fs::write(own.join("applications/apic-gui.desktop"), "x").unwrap();
        assert!(find_system_entry(&dirs, &own).is_none());

        // A user-scoped Flatpak entry (Name=apic, different filename, under our
        // own data root) is still reported.
        let fp_desktop = flatpak.join("applications/io.github.rizukirr.apic.desktop");
        std::fs::write(&fp_desktop, "x").unwrap();
        assert_eq!(find_system_entry(&dirs, &own), Some(fp_desktop));

        // A genuine system package entry is reported too.
        std::fs::remove_file(flatpak.join("applications/io.github.rizukirr.apic.desktop")).unwrap();
        let sys_desktop = sys.join("applications/apic-gui.desktop");
        std::fs::write(&sys_desktop, "x").unwrap();
        assert_eq!(find_system_entry(&dirs, &own), Some(sys_desktop));

        let _ = std::fs::remove_dir_all(&base);
    }
}
