use std::path::{Path, PathBuf};

pub enum Hosts {
    Chromium(&'static str),
    Firefox(&'static [&'static str]),
}

pub struct App {
    pub id: &'static str,
    pub browser: &'static str,
    pub name: Option<&'static str>,
    pub hosts: Hosts,
}

pub const APPS: &[App] = &[
    App {
        id: "com.google.Chrome",
        browser: "chrome",
        name: None,
        hosts: Hosts::Chromium("google-chrome"),
    },
    App {
        id: "com.google.ChromeDev",
        browser: "chrome-dev",
        name: None,
        hosts: Hosts::Chromium("google-chrome-unstable"),
    },
    App {
        id: "com.microsoft.Edge",
        browser: "edge",
        name: None,
        hosts: Hosts::Chromium("microsoft-edge"),
    },
    App {
        id: "com.brave.Browser",
        browser: "brave",
        name: None,
        hosts: Hosts::Chromium("BraveSoftware/Brave-Browser"),
    },
    App {
        id: "com.opera.Opera",
        browser: "opera",
        name: None,
        hosts: Hosts::Chromium("opera"),
    },
    App {
        id: "com.vivaldi.Vivaldi",
        browser: "vivaldi",
        name: None,
        hosts: Hosts::Chromium("vivaldi"),
    },
    App {
        id: "org.chromium.Chromium",
        browser: "chromium",
        name: None,
        hosts: Hosts::Chromium("chromium"),
    },
    App {
        id: "io.github.ungoogled_software.ungoogled_chromium",
        browser: "chromium",
        name: Some("Ungoogled Chromium"),
        hosts: Hosts::Chromium("chromium"),
    },
    App {
        id: "com.github.Eloston.UngoogledChromium",
        browser: "chromium",
        name: Some("Ungoogled Chromium"),
        hosts: Hosts::Chromium("chromium"),
    },
    App {
        id: "ru.yandex.Browser",
        browser: "yandex",
        name: None,
        hosts: Hosts::Chromium("yandex-browser"),
    },
    App {
        id: "org.mozilla.firefox",
        browser: "firefox",
        name: None,
        hosts: Hosts::Firefox(&[".mozilla"]),
    },
    App {
        id: "io.gitlab.librewolf-community",
        browser: "librewolf",
        name: None,
        hosts: Hosts::Firefox(&[".librewolf", ".mozilla"]),
    },
    App {
        id: "net.waterfox.waterfox",
        browser: "waterfox",
        name: None,
        hosts: Hosts::Firefox(&[".waterfox", ".mozilla"]),
    },
    App {
        id: "one.ablaze.floorp",
        browser: "floorp",
        name: None,
        hosts: Hosts::Firefox(&[".floorp", ".mozilla"]),
    },
    App {
        id: "app.zen_browser.zen",
        browser: "zen",
        name: None,
        hosts: Hosts::Firefox(&[".zen", ".mozilla"]),
    },
];

const HOST_ACCESS: &str = "org.freedesktop.Flatpak";

pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

pub fn roots(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".local/share/flatpak"),
        PathBuf::from("/var/lib/flatpak"),
    ]
}

pub fn installed(roots: &[PathBuf]) -> Vec<&'static App> {
    APPS.iter()
        .filter(|app| {
            roots
                .iter()
                .any(|r| r.join("app").join(app.id).join("current").exists())
        })
        .collect()
}

#[cfg(test)]
pub fn find(id: &str) -> Option<&'static App> {
    APPS.iter().find(|a| a.id == id)
}

fn bus_policy(text: &str) -> Option<bool> {
    text.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        (name.trim() == HOST_ACCESS).then(|| value.trim() == "talk" || value.trim() == "own")
    })
}

pub fn can_start_host(home: &Path, roots: &[PathBuf], id: &str) -> bool {
    let read = |p: PathBuf| std::fs::read_to_string(p).ok();
    let overrides = [
        home.join(".local/share/flatpak/overrides").join(id),
        home.join(".local/share/flatpak/overrides/global"),
        PathBuf::from("/var/lib/flatpak/overrides").join(id),
        PathBuf::from("/var/lib/flatpak/overrides/global"),
    ];
    for file in overrides {
        if let Some(allowed) = read(file).as_deref().and_then(bus_policy) {
            return allowed;
        }
    }
    roots
        .iter()
        .filter_map(|r| read(r.join("app").join(id).join("current/active/metadata")))
        .find_map(|m| bus_policy(&m))
        .unwrap_or(false)
}

pub fn grant_command(id: &str) -> String {
    format!("flatpak override --user --talk-name={HOST_ACCESS} {id}")
}

pub fn data_dir(home: &Path, id: &str) -> PathBuf {
    home.join(".var/app")
        .join(id)
        .join("data/multron-download-manager")
}

pub fn wrapper_path(home: &Path, id: &str) -> PathBuf {
    data_dir(home, id).join("native-host.sh")
}

pub fn wrapper_script(exe: &Path) -> String {
    let exe = exe.to_string_lossy().replace('\'', r"'\''");
    format!("#!/bin/sh\nexec flatpak-spawn --host '{exe}' \"$@\"\n")
}

pub fn manifest_paths(home: &Path, app: &App, file: &str) -> Vec<PathBuf> {
    let base = home.join(".var/app").join(app.id);
    match app.hosts {
        Hosts::Chromium(dir) => vec![base
            .join("config")
            .join(dir)
            .join("NativeMessagingHosts")
            .join(file)],
        Hosts::Firefox(dirs) => dirs
            .iter()
            .map(|d| base.join(d).join("native-messaging-hosts").join(file))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mdm-flatpak-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn finds_installed_flatpak_browsers() {
        let home = temp("installed");
        let user = home.join(".local/share/flatpak");
        let system = home.join("system");
        std::fs::create_dir_all(user.join("app/org.mozilla.firefox/current")).unwrap();
        std::fs::create_dir_all(system.join("app/com.brave.Browser/current")).unwrap();
        std::fs::create_dir_all(system.join("app/org.gnome.Calculator/current")).unwrap();
        let ids: Vec<_> = installed(&[user, system]).iter().map(|a| a.id).collect();
        assert_eq!(ids, vec!["com.brave.Browser", "org.mozilla.firefox"]);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn reads_the_host_access_permission() {
        let home = temp("permission");
        let root = home.join("system");
        let meta = root.join("app/com.google.Chrome/current/active");
        std::fs::create_dir_all(&meta).unwrap();
        std::fs::write(
            meta.join("metadata"),
            "[Session Bus Policy]\norg.freedesktop.Notifications=talk\n",
        )
        .unwrap();
        let roots = [root.clone()];
        assert!(!can_start_host(&home, &roots, "com.google.Chrome"));
        let overrides = home.join(".local/share/flatpak/overrides");
        std::fs::create_dir_all(&overrides).unwrap();
        std::fs::write(
            overrides.join("com.google.Chrome"),
            "[Session Bus Policy]\norg.freedesktop.Flatpak=talk\n",
        )
        .unwrap();
        assert!(can_start_host(&home, &roots, "com.google.Chrome"));
        std::fs::write(
            overrides.join("com.google.Chrome"),
            "[Session Bus Policy]\norg.freedesktop.Flatpak=none\n",
        )
        .unwrap();
        assert!(!can_start_host(&home, &roots, "com.google.Chrome"));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn places_native_host_files_inside_the_sandbox() {
        let home = Path::new("/home/u");
        let file = "com.multron.downloadmanager.json";
        let chrome = find("com.google.Chrome").unwrap();
        assert_eq!(
            manifest_paths(home, chrome, file),
            vec![home.join(
                ".var/app/com.google.Chrome/config/google-chrome/NativeMessagingHosts/com.multron.downloadmanager.json"
            )]
        );
        let firefox = find("org.mozilla.firefox").unwrap();
        assert_eq!(
            manifest_paths(home, firefox, file),
            vec![home.join(
                ".var/app/org.mozilla.firefox/.mozilla/native-messaging-hosts/com.multron.downloadmanager.json"
            )]
        );
        assert_eq!(
            wrapper_path(home, "org.mozilla.firefox"),
            home.join(".var/app/org.mozilla.firefox/data/multron-download-manager/native-host.sh")
        );
    }

    #[test]
    fn quotes_the_app_path_in_the_wrapper() {
        let script = wrapper_script(Path::new("/home/u/Apps/it's here.AppImage"));
        assert_eq!(
            script,
            "#!/bin/sh\nexec flatpak-spawn --host '/home/u/Apps/it'\\''s here.AppImage' \"$@\"\n"
        );
        assert_eq!(
            grant_command("org.mozilla.firefox"),
            "flatpak override --user --talk-name=org.freedesktop.Flatpak org.mozilla.firefox"
        );
    }
}
