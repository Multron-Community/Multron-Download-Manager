use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sandbox {
    Flatpak(String),
    Snap(String),
}

pub fn current() -> Option<Sandbox> {
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
    detect(
        var("FLATPAK_ID").as_deref(),
        Path::new("/.flatpak-info").is_file(),
        var("SNAP_INSTANCE_NAME")
            .or_else(|| var("SNAP_NAME"))
            .as_deref(),
        var("SNAP").is_some(),
    )
}

fn detect(
    flatpak_id: Option<&str>,
    flatpak_info: bool,
    snap_name: Option<&str>,
    in_snap: bool,
) -> Option<Sandbox> {
    if let Some(id) = flatpak_id {
        return Some(Sandbox::Flatpak(id.to_string()));
    }
    if flatpak_info {
        return Some(Sandbox::Flatpak(String::new()));
    }
    match (snap_name, in_snap) {
        (Some(name), true) => Some(Sandbox::Snap(name.to_string())),
        _ => None,
    }
}

pub fn store_managed() -> bool {
    current().is_some()
}

pub fn snap_host_path(name: &str, path: &Path) -> PathBuf {
    let root = std::env::var_os("SNAP").map(PathBuf::from);
    snap_host_path_from(root.as_deref(), name, path)
}

fn snap_host_path_from(root: Option<&Path>, name: &str, path: &Path) -> PathBuf {
    match root.and_then(|r| path.strip_prefix(r).ok()) {
        Some(rest) => Path::new("/snap").join(name).join("current").join(rest),
        None => path.to_path_buf(),
    }
}

pub fn flatpak_launcher(id: &str, binary: &str) -> String {
    format!("#!/bin/sh\nexec flatpak run --command={binary} {id} \"$@\"\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_flatpak_and_snap() {
        assert_eq!(
            detect(Some("io.github.App"), false, None, false),
            Some(Sandbox::Flatpak("io.github.App".into()))
        );
        assert_eq!(
            detect(None, true, None, false),
            Some(Sandbox::Flatpak(String::new()))
        );
        assert_eq!(
            detect(None, false, Some("multron-download-manager"), true),
            Some(Sandbox::Snap("multron-download-manager".into()))
        );
        assert_eq!(detect(None, false, Some("leftover"), false), None);
        assert_eq!(detect(None, false, None, false), None);
    }

    #[test]
    fn points_browsers_at_the_stable_snap_folder() {
        let root = Path::new("/snap/multron-download-manager/42");
        let ext = root.join("usr/lib/Multron Download Manager/browser-extension");
        assert_eq!(
            snap_host_path_from(Some(root), "multron-download-manager", &ext),
            Path::new("/snap/multron-download-manager/current/usr/lib/Multron Download Manager/browser-extension")
        );
        let elsewhere = Path::new("/opt/other");
        assert_eq!(
            snap_host_path_from(Some(root), "multron-download-manager", elsewhere),
            elsewhere
        );
    }

    #[test]
    fn flatpak_launcher_runs_the_app_through_flatpak() {
        let script = flatpak_launcher("io.github.App", "multron-download-manager");
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(script
            .contains("exec flatpak run --command=multron-download-manager io.github.App \"$@\""));
    }
}
