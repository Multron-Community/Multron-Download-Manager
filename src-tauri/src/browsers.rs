use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::Serialize;
use tauri::{AppHandle, Manager as _};

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Browser {
    pub id: String,
    pub name: String,
    pub engine: String,
    pub path: String,
    pub extensions_page: String,
    pub flatpak: Option<String>,
    pub flatpak_autostart: bool,
    pub flatpak_grant: Option<String>,
}

#[allow(dead_code)]
struct Known {
    id: &'static str,
    name: &'static str,
    engine: &'static str,
    page: &'static str,
    windows: &'static [&'static str],
    mac: &'static str,
    linux: &'static [&'static str],
}

const FIREFOX_PAGE: &str = "about:debugging#/runtime/this-firefox";

const KNOWN: &[Known] = &[
    Known {
        id: "chrome",
        name: "Google Chrome",
        engine: "chromium",
        page: "chrome://extensions",
        windows: &[
            "%ProgramFiles%\\Google\\Chrome\\Application\\chrome.exe",
            "%ProgramFiles(x86)%\\Google\\Chrome\\Application\\chrome.exe",
            "%LOCALAPPDATA%\\Google\\Chrome\\Application\\chrome.exe",
            "@chrome.exe",
        ],
        mac: "Google Chrome.app/Contents/MacOS/Google Chrome",
        linux: &["google-chrome", "google-chrome-stable"],
    },
    Known {
        id: "edge",
        name: "Microsoft Edge",
        engine: "chromium",
        page: "edge://extensions",
        windows: &[
            "%ProgramFiles(x86)%\\Microsoft\\Edge\\Application\\msedge.exe",
            "%ProgramFiles%\\Microsoft\\Edge\\Application\\msedge.exe",
            "@msedge.exe",
        ],
        mac: "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        linux: &["microsoft-edge", "microsoft-edge-stable"],
    },
    Known {
        id: "brave",
        name: "Brave",
        engine: "chromium",
        page: "brave://extensions",
        windows: &[
            "%ProgramFiles%\\BraveSoftware\\Brave-Browser\\Application\\brave.exe",
            "%ProgramFiles(x86)%\\BraveSoftware\\Brave-Browser\\Application\\brave.exe",
            "%LOCALAPPDATA%\\BraveSoftware\\Brave-Browser\\Application\\brave.exe",
            "@brave.exe",
        ],
        mac: "Brave Browser.app/Contents/MacOS/Brave Browser",
        linux: &["brave-browser", "brave"],
    },
    Known {
        id: "opera",
        name: "Opera",
        engine: "chromium",
        page: "opera://extensions",
        windows: &[
            "%LOCALAPPDATA%\\Programs\\Opera\\opera.exe",
            "%ProgramFiles%\\Opera\\opera.exe",
        ],
        mac: "Opera.app/Contents/MacOS/Opera",
        linux: &["opera"],
    },
    Known {
        id: "opera-gx",
        name: "Opera GX",
        engine: "chromium",
        page: "opera://extensions",
        windows: &["%LOCALAPPDATA%\\Programs\\Opera GX\\opera.exe"],
        mac: "Opera GX.app/Contents/MacOS/Opera",
        linux: &[],
    },
    Known {
        id: "vivaldi",
        name: "Vivaldi",
        engine: "chromium",
        page: "vivaldi://extensions",
        windows: &[
            "%LOCALAPPDATA%\\Vivaldi\\Application\\vivaldi.exe",
            "%ProgramFiles%\\Vivaldi\\Application\\vivaldi.exe",
        ],
        mac: "Vivaldi.app/Contents/MacOS/Vivaldi",
        linux: &["vivaldi", "vivaldi-stable"],
    },
    Known {
        id: "chromium",
        name: "Chromium / Ungoogled Chromium",
        engine: "chromium",
        page: "chrome://extensions",
        windows: &["%LOCALAPPDATA%\\Chromium\\Application\\chrome.exe"],
        mac: "Chromium.app/Contents/MacOS/Chromium",
        linux: &["chromium", "chromium-browser"],
    },
    Known {
        id: "chrome-beta",
        name: "Google Chrome Beta",
        engine: "chromium",
        page: "chrome://extensions",
        windows: &[
            "%ProgramFiles%\\Google\\Chrome Beta\\Application\\chrome.exe",
            "%LOCALAPPDATA%\\Google\\Chrome Beta\\Application\\chrome.exe",
        ],
        mac: "Google Chrome Beta.app/Contents/MacOS/Google Chrome Beta",
        linux: &["google-chrome-beta"],
    },
    Known {
        id: "chrome-dev",
        name: "Google Chrome Dev",
        engine: "chromium",
        page: "chrome://extensions",
        windows: &[
            "%ProgramFiles%\\Google\\Chrome Dev\\Application\\chrome.exe",
            "%LOCALAPPDATA%\\Google\\Chrome Dev\\Application\\chrome.exe",
        ],
        mac: "Google Chrome Dev.app/Contents/MacOS/Google Chrome Dev",
        linux: &["google-chrome-unstable"],
    },
    Known {
        id: "chrome-canary",
        name: "Google Chrome Canary",
        engine: "chromium",
        page: "chrome://extensions",
        windows: &["%LOCALAPPDATA%\\Google\\Chrome SxS\\Application\\chrome.exe"],
        mac: "Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
        linux: &["google-chrome-canary"],
    },
    Known {
        id: "edge-beta",
        name: "Microsoft Edge Beta",
        engine: "chromium",
        page: "edge://extensions",
        windows: &[
            "%ProgramFiles(x86)%\\Microsoft\\Edge Beta\\Application\\msedge.exe",
            "%ProgramFiles%\\Microsoft\\Edge Beta\\Application\\msedge.exe",
        ],
        mac: "Microsoft Edge Beta.app/Contents/MacOS/Microsoft Edge Beta",
        linux: &["microsoft-edge-beta"],
    },
    Known {
        id: "edge-dev",
        name: "Microsoft Edge Dev",
        engine: "chromium",
        page: "edge://extensions",
        windows: &[
            "%ProgramFiles(x86)%\\Microsoft\\Edge Dev\\Application\\msedge.exe",
            "%ProgramFiles%\\Microsoft\\Edge Dev\\Application\\msedge.exe",
        ],
        mac: "Microsoft Edge Dev.app/Contents/MacOS/Microsoft Edge Dev",
        linux: &["microsoft-edge-dev"],
    },
    Known {
        id: "edge-canary",
        name: "Microsoft Edge Canary",
        engine: "chromium",
        page: "edge://extensions",
        windows: &["%LOCALAPPDATA%\\Microsoft\\Edge SxS\\Application\\msedge.exe"],
        mac: "Microsoft Edge Canary.app/Contents/MacOS/Microsoft Edge Canary",
        linux: &[],
    },
    Known {
        id: "brave-beta",
        name: "Brave Beta",
        engine: "chromium",
        page: "brave://extensions",
        windows: &[
            "%ProgramFiles%\\BraveSoftware\\Brave-Browser-Beta\\Application\\brave.exe",
            "%LOCALAPPDATA%\\BraveSoftware\\Brave-Browser-Beta\\Application\\brave.exe",
        ],
        mac: "Brave Browser Beta.app/Contents/MacOS/Brave Browser Beta",
        linux: &["brave-browser-beta"],
    },
    Known {
        id: "brave-nightly",
        name: "Brave Nightly",
        engine: "chromium",
        page: "brave://extensions",
        windows: &[
            "%ProgramFiles%\\BraveSoftware\\Brave-Browser-Nightly\\Application\\brave.exe",
            "%LOCALAPPDATA%\\BraveSoftware\\Brave-Browser-Nightly\\Application\\brave.exe",
        ],
        mac: "Brave Browser Nightly.app/Contents/MacOS/Brave Browser Nightly",
        linux: &["brave-browser-nightly"],
    },
    Known {
        id: "dia",
        name: "Dia",
        engine: "chromium",
        page: "chrome://extensions",
        windows: &["@Dia.exe"],
        mac: "Dia.app/Contents/MacOS/Dia",
        linux: &[],
    },
    Known {
        id: "arc",
        name: "Arc",
        engine: "chromium",
        page: "chrome://extensions",
        windows: &[
            "%LOCALAPPDATA%\\Microsoft\\WindowsApps\\Arc.exe",
            "@Arc.exe",
        ],
        mac: "Arc.app/Contents/MacOS/Arc",
        linux: &[],
    },
    Known {
        id: "yandex",
        name: "Yandex Browser",
        engine: "chromium",
        page: "browser://extensions",
        windows: &[
            "%LOCALAPPDATA%\\Yandex\\YandexBrowser\\Application\\browser.exe",
            "%ProgramFiles%\\Yandex\\YandexBrowser\\Application\\browser.exe",
            "%ProgramFiles(x86)%\\Yandex\\YandexBrowser\\Application\\browser.exe",
            "%LOCALAPPDATA%\\Programs\\Yandex\\YandexBrowser\\Application\\browser.exe",
        ],
        mac: "Yandex.app/Contents/MacOS/Yandex",
        linux: &["yandex-browser", "yandex-browser-stable"],
    },
    Known {
        id: "whale",
        name: "Naver Whale",
        engine: "chromium",
        page: "whale://extensions",
        windows: &[
            "%ProgramFiles%\\Naver\\Naver Whale\\Application\\whale.exe",
            "%ProgramFiles(x86)%\\Naver\\Naver Whale\\Application\\whale.exe",
        ],
        mac: "Whale.app/Contents/MacOS/Whale",
        linux: &["naver-whale-stable", "whale"],
    },
    Known {
        id: "thorium",
        name: "Thorium",
        engine: "chromium",
        page: "chrome://extensions",
        windows: &[
            "%LOCALAPPDATA%\\Thorium\\Application\\thorium.exe",
            "%ProgramFiles%\\Thorium\\Application\\thorium.exe",
        ],
        mac: "Thorium.app/Contents/MacOS/Thorium",
        linux: &["thorium-browser", "thorium"],
    },
    Known {
        id: "supermium",
        name: "Supermium",
        engine: "chromium",
        page: "chrome://extensions",
        windows: &[
            "%ProgramFiles%\\Supermium\\chrome.exe",
            "%ProgramFiles(x86)%\\Supermium\\chrome.exe",
            "%LOCALAPPDATA%\\Supermium\\chrome.exe",
        ],
        mac: "",
        linux: &[],
    },
    Known {
        id: "cent",
        name: "Cent Browser",
        engine: "chromium",
        page: "chrome://extensions",
        windows: &[
            "%LOCALAPPDATA%\\CentBrowser\\Application\\chrome.exe",
            "%ProgramFiles%\\CentBrowser\\Application\\chrome.exe",
        ],
        mac: "",
        linux: &[],
    },
    Known {
        id: "dragon",
        name: "Comodo Dragon",
        engine: "chromium",
        page: "chrome://extensions",
        windows: &[
            "%ProgramFiles%\\Comodo\\Dragon\\dragon.exe",
            "%ProgramFiles(x86)%\\Comodo\\Dragon\\dragon.exe",
            "%LOCALAPPDATA%\\Comodo\\Dragon\\dragon.exe",
        ],
        mac: "",
        linux: &[],
    },
    Known {
        id: "epic",
        name: "Epic Privacy Browser",
        engine: "chromium",
        page: "chrome://extensions",
        windows: &["%LOCALAPPDATA%\\Epic Privacy Browser\\Application\\epic.exe"],
        mac: "Epic.app/Contents/MacOS/Epic",
        linux: &[],
    },
    Known {
        id: "avast",
        name: "Avast Secure Browser",
        engine: "chromium",
        page: "chrome://extensions",
        windows: &[
            "%ProgramFiles%\\AVAST Software\\Browser\\Application\\AvastBrowser.exe",
            "%ProgramFiles(x86)%\\AVAST Software\\Browser\\Application\\AvastBrowser.exe",
        ],
        mac: "Avast Secure Browser.app/Contents/MacOS/Avast Secure Browser",
        linux: &[],
    },
    Known {
        id: "avg",
        name: "AVG Secure Browser",
        engine: "chromium",
        page: "chrome://extensions",
        windows: &[
            "%ProgramFiles%\\AVG\\Browser\\Application\\AVGBrowser.exe",
            "%ProgramFiles(x86)%\\AVG\\Browser\\Application\\AVGBrowser.exe",
        ],
        mac: "AVG Secure Browser.app/Contents/MacOS/AVG Secure Browser",
        linux: &[],
    },
    Known {
        id: "ccleaner",
        name: "CCleaner Browser",
        engine: "chromium",
        page: "chrome://extensions",
        windows: &[
            "%ProgramFiles%\\CCleaner Browser\\Application\\CCleanerBrowser.exe",
            "%ProgramFiles(x86)%\\CCleaner Browser\\Application\\CCleanerBrowser.exe",
        ],
        mac: "",
        linux: &[],
    },
    Known {
        id: "sidekick",
        name: "Sidekick",
        engine: "chromium",
        page: "chrome://extensions",
        windows: &["%LOCALAPPDATA%\\Sidekick\\Application\\sidekick.exe"],
        mac: "Sidekick.app/Contents/MacOS/Sidekick",
        linux: &[],
    },
    Known {
        id: "wavebox",
        name: "Wavebox",
        engine: "chromium",
        page: "chrome://extensions",
        windows: &["%LOCALAPPDATA%\\WaveboxApp\\Application\\wavebox.exe"],
        mac: "Wavebox.app/Contents/MacOS/Wavebox",
        linux: &["wavebox"],
    },
    Known {
        id: "firefox",
        name: "Mozilla Firefox",
        engine: "firefox",
        page: FIREFOX_PAGE,
        windows: &[
            "%ProgramFiles%\\Mozilla Firefox\\firefox.exe",
            "%ProgramFiles(x86)%\\Mozilla Firefox\\firefox.exe",
            "@firefox.exe",
        ],
        mac: "Firefox.app/Contents/MacOS/firefox",
        linux: &["firefox", "firefox-esr"],
    },
    Known {
        id: "firefox-dev",
        name: "Firefox Developer Edition",
        engine: "firefox",
        page: FIREFOX_PAGE,
        windows: &[
            "%ProgramFiles%\\Firefox Developer Edition\\firefox.exe",
            "%LOCALAPPDATA%\\Firefox Developer Edition\\firefox.exe",
        ],
        mac: "Firefox Developer Edition.app/Contents/MacOS/firefox",
        linux: &["firefox-developer-edition", "firefox-dev"],
    },
    Known {
        id: "firefox-nightly",
        name: "Firefox Nightly",
        engine: "firefox",
        page: FIREFOX_PAGE,
        windows: &[
            "%ProgramFiles%\\Firefox Nightly\\firefox.exe",
            "%LOCALAPPDATA%\\Firefox Nightly\\firefox.exe",
        ],
        mac: "Firefox Nightly.app/Contents/MacOS/firefox",
        linux: &["firefox-nightly"],
    },
    Known {
        id: "librewolf",
        name: "LibreWolf",
        engine: "firefox",
        page: FIREFOX_PAGE,
        windows: &[
            "%ProgramFiles%\\LibreWolf\\librewolf.exe",
            "%LOCALAPPDATA%\\LibreWolf\\librewolf.exe",
            "@librewolf.exe",
        ],
        mac: "LibreWolf.app/Contents/MacOS/librewolf",
        linux: &["librewolf"],
    },
    Known {
        id: "waterfox",
        name: "Waterfox",
        engine: "firefox",
        page: FIREFOX_PAGE,
        windows: &["%ProgramFiles%\\Waterfox\\waterfox.exe", "@waterfox.exe"],
        mac: "Waterfox.app/Contents/MacOS/waterfox",
        linux: &["waterfox"],
    },
    Known {
        id: "floorp",
        name: "Floorp",
        engine: "firefox",
        page: FIREFOX_PAGE,
        windows: &[
            "%ProgramFiles%\\Ablaze Floorp\\floorp.exe",
            "%LOCALAPPDATA%\\Ablaze Floorp\\floorp.exe",
            "@floorp.exe",
        ],
        mac: "Floorp.app/Contents/MacOS/floorp",
        linux: &["floorp"],
    },
    Known {
        id: "zen",
        name: "Zen Browser",
        engine: "firefox",
        page: FIREFOX_PAGE,
        windows: &[
            "%ProgramFiles%\\Zen Browser\\zen.exe",
            "%LOCALAPPDATA%\\Zen Browser\\zen.exe",
            "@zen.exe",
        ],
        mac: "Zen.app/Contents/MacOS/zen",
        linux: &["zen-browser", "zen"],
    },
    Known {
        id: "mullvad",
        name: "Mullvad Browser",
        engine: "firefox",
        page: FIREFOX_PAGE,
        windows: &[
            "%LOCALAPPDATA%\\Mullvad\\MullvadBrowser\\Release\\mullvadbrowser.exe",
            "%LOCALAPPDATA%\\Mullvad\\MullvadBrowser\\Release\\Browser\\mullvadbrowser.exe",
            "%ProgramFiles%\\MullvadBrowser\\mullvadbrowser.exe",
            "%ProgramFiles%\\Mullvad\\MullvadBrowser\\Release\\mullvadbrowser.exe",
        ],
        mac: "Mullvad Browser.app/Contents/MacOS/mullvadbrowser",
        linux: &["mullvad-browser"],
    },
];

pub fn detect() -> Vec<Browser> {
    let mut found: Vec<Option<PathBuf>> = KNOWN.iter().map(find).collect();
    for (i, k) in KNOWN.iter().enumerate() {
        if found[i].is_none() {
            found[i] = find_registered(k).filter(|p| !found.contains(&Some(p.clone())));
        }
    }
    let mut list: Vec<Browser> = KNOWN
        .iter()
        .zip(found)
        .filter_map(|(k, path)| {
            path.map(|path| Browser {
                id: k.id.into(),
                name: k.name.into(),
                engine: k.engine.into(),
                path: path.to_string_lossy().into_owned(),
                extensions_page: k.page.into(),
                ..Default::default()
            })
        })
        .collect();
    for extra in registered_browsers() {
        let same = |b: &Browser| b.path.eq_ignore_ascii_case(&extra.path);
        if !list.iter().any(same) {
            list.push(extra);
        }
    }
    if cfg!(target_os = "linux") {
        if let Some(home) = crate::flatpak::home() {
            list.extend(flatpak_browsers(&home, &crate::flatpak::roots(&home)));
        }
    }
    list
}

fn flatpak_browsers(home: &Path, roots: &[PathBuf]) -> Vec<Browser> {
    crate::flatpak::installed(roots)
        .into_iter()
        .filter_map(|app| {
            let k = KNOWN.iter().find(|k| k.id == app.browser)?;
            Some(Browser {
                id: format!("flatpak:{}", app.id),
                name: format!("{} (Flatpak)", app.name.unwrap_or(k.name)),
                engine: k.engine.into(),
                path: format!("flatpak run {}", app.id),
                extensions_page: k.page.into(),
                flatpak: Some(app.id.into()),
                flatpak_autostart: crate::flatpak::can_start_host(home, roots, app.id),
                flatpak_grant: Some(crate::flatpak::grant_command(app.id)),
            })
        })
        .collect()
}

#[cfg(not(windows))]
fn registered_browsers() -> Vec<Browser> {
    Vec::new()
}

#[cfg(windows)]
fn registered_browsers() -> Vec<Browser> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut out: Vec<Browser> = Vec::new();
    for hive in ["HKCU", "HKLM"] {
        let root = format!("{hive}\\SOFTWARE\\Clients\\StartMenuInternet");
        let Ok(listing) = Command::new("reg")
            .args(["query", &root])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        else {
            continue;
        };
        for key in String::from_utf8_lossy(&listing.stdout)
            .lines()
            .map(str::trim)
            .filter(|l| {
                l.len() > root.len()
                    && l.to_ascii_lowercase().starts_with(&format!(
                        "hkey_{}",
                        if hive == "HKCU" {
                            "current_user"
                        } else {
                            "local_machine"
                        }
                    ))
            })
        {
            let read = |sub: &str| -> Option<String> {
                let path = if sub.is_empty() {
                    key.to_string()
                } else {
                    format!("{key}\\{sub}")
                };
                let o = Command::new("reg")
                    .args(["query", &path, "/ve"])
                    .creation_flags(CREATE_NO_WINDOW)
                    .output()
                    .ok()?;
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .find_map(|l| {
                        l.split("REG_SZ")
                            .nth(1)
                            .or_else(|| l.split("REG_EXPAND_SZ").nth(1))
                    })
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
            };
            let Some(command) = read("shell\\open\\command") else {
                continue;
            };
            let Some(exe) = command_exe(&command) else {
                continue;
            };
            let name = read("")
                .unwrap_or_else(|| key.rsplit('\\').next().unwrap_or("Browser").to_string());
            if name.to_ascii_lowercase().contains("internet explorer")
                || exe.to_ascii_lowercase().ends_with("iexplore.exe")
            {
                continue;
            }
            let Some(engine) = engine_of(Path::new(&exe)) else {
                continue;
            };
            if out.iter().any(|b| b.path.eq_ignore_ascii_case(&exe)) {
                continue;
            }
            let id = format!(
                "registered-{}",
                name.chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .collect::<String>()
                    .to_ascii_lowercase()
            );
            out.push(Browser {
                id,
                name,
                engine: engine.into(),
                path: exe,
                extensions_page: if engine == "firefox" {
                    FIREFOX_PAGE.into()
                } else {
                    "chrome://extensions".into()
                },
                ..Default::default()
            });
        }
    }
    out
}

#[cfg(windows)]
fn command_exe(command: &str) -> Option<String> {
    let c = command.trim();
    let exe = if let Some(rest) = c.strip_prefix('"') {
        rest.split('"').next()?.to_string()
    } else {
        c.split(".exe").next().map(|p| format!("{p}.exe"))?
    };
    let exe = expand_env(&exe).unwrap_or(exe);
    Path::new(&exe).is_file().then_some(exe)
}

#[cfg(windows)]
fn engine_of(exe: &Path) -> Option<&'static str> {
    let dir = exe.parent()?;
    if dir.join("xul.dll").is_file() {
        return Some("firefox");
    }
    let chromium = |d: &Path| -> bool {
        std::fs::read_dir(d)
            .map(|rd| {
                rd.flatten().any(|e| {
                    let n = e.file_name().to_string_lossy().to_ascii_lowercase();
                    n == "chrome.dll"
                        || n == "msedge.dll"
                        || n.ends_with(".pak")
                        || n == "chrome_elf.dll"
                })
            })
            .unwrap_or(false)
    };
    if chromium(dir) {
        return Some("chromium");
    }
    let nested = std::fs::read_dir(dir).ok()?.flatten().find(|e| {
        e.path().is_dir()
            && e.file_name()
                .to_string_lossy()
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
    });
    nested.filter(|e| chromium(&e.path())).map(|_| "chromium")
}

#[cfg(not(windows))]
fn find_registered(_: &Known) -> Option<PathBuf> {
    None
}

#[cfg(windows)]
fn find(k: &Known) -> Option<PathBuf> {
    k.windows
        .iter()
        .filter(|c| !c.starts_with('@'))
        .filter_map(|c| expand_env(c).map(PathBuf::from))
        .find(|p| p.is_file())
}

#[cfg(windows)]
fn find_registered(k: &Known) -> Option<PathBuf> {
    k.windows
        .iter()
        .filter_map(|c| c.strip_prefix('@'))
        .filter_map(app_path)
        .find(|p| p.is_file())
}

#[cfg(windows)]
fn expand_env(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find('%')?;
        out.push_str(&std::env::var(&after[..end]).ok()?);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

#[cfg(windows)]
fn app_path(exe: &str) -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    for hive in ["HKCU", "HKLM"] {
        let key = format!("{hive}\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\{exe}");
        let Ok(out) = Command::new("reg")
            .args(["query", &key, "/ve"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        else {
            continue;
        };
        let text = String::from_utf8_lossy(&out.stdout);
        if let Some(value) = text.lines().find_map(|l| l.split("REG_SZ").nth(1)) {
            let value = value.trim().trim_matches('"');
            if !value.is_empty() {
                return Some(PathBuf::from(value));
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn find(k: &Known) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    [
        Some(PathBuf::from("/Applications")),
        home.map(|h| h.join("Applications")),
    ]
    .into_iter()
    .flatten()
    .map(|dir| dir.join(k.mac))
    .find(|p| p.is_file())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn find(k: &Known) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    k.linux.iter().find_map(|name| {
        std::env::split_paths(&path)
            .map(|dir| dir.join(name))
            .find(|p| p.is_file())
    })
}

pub fn open_page(browser: &Browser) -> Result<()> {
    let mut cmd = match &browser.flatpak {
        Some(id) => {
            let mut c = Command::new("flatpak");
            c.args(["run", id]);
            c
        }
        None => Command::new(&browser.path),
    };
    cmd.arg(&browser.extensions_page)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(DETACHED_PROCESS);
    }
    cmd.spawn()
        .with_context(|| format!("could not start {}", browser.name))?;
    Ok(())
}

fn plain_path(path: PathBuf) -> PathBuf {
    let plain = path
        .to_str()
        .and_then(|t| t.strip_prefix(r"\\?\"))
        .filter(|rest| rest.as_bytes().get(1) == Some(&b':'))
        .map(PathBuf::from);
    plain.unwrap_or(path)
}

fn bundled_extension(app: &AppHandle, firefox: bool) -> Option<PathBuf> {
    let folder = if firefox {
        "browser-extension-firefox"
    } else {
        "browser-extension"
    };
    let bundled = app
        .path()
        .resource_dir()
        .ok()
        .map(|d| plain_path(d).join(folder));
    if let Some(dir) = bundled.filter(|d| d.join("manifest.json").is_file()) {
        return Some(dir);
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join(if firefox {
        "../extension-build/firefox"
    } else {
        "../extension"
    });
    (cfg!(debug_assertions) && repo.join("manifest.json").is_file()).then_some(repo)
}

pub fn extension_build(app: &AppHandle) -> Option<String> {
    static BUILD: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    BUILD
        .get_or_init(|| {
            let dir = bundled_extension(app, false)?;
            let text = std::fs::read_to_string(dir.join("build.json")).ok()?;
            let v: serde_json::Value = serde_json::from_str(&text).ok()?;
            v.get("build")?.as_str().map(str::to_string)
        })
        .clone()
}

pub fn extension_folder(app: &AppHandle, firefox: bool) -> Result<PathBuf> {
    let src = bundled_extension(app, firefox)
        .context("the browser extension is not bundled with this build")?;
    if std::env::var_os("APPIMAGE").is_none() {
        return Ok(src);
    }
    let dest = app
        .path()
        .app_data_dir()
        .context("no app data dir")?
        .join(if firefox {
            "browser-extension-firefox"
        } else {
            "browser-extension"
        });
    copy_dir(&src, &dest)?;
    Ok(dest)
}

pub fn extension_folder_for(app: &AppHandle, browser: &Browser) -> Result<PathBuf> {
    let firefox = browser.engine == "firefox";
    let Some(id) = &browser.flatpak else {
        return extension_folder(app, firefox);
    };
    let src = bundled_extension(app, firefox)
        .context("the browser extension is not bundled with this build")?;
    let home = crate::flatpak::home().context("HOME is not set")?;
    let dest = crate::flatpak::data_dir(&home, id).join(if firefox {
        "browser-extension-firefox"
    } else {
        "browser-extension"
    });
    copy_dir(&src, &dest)?;
    Ok(dest)
}

pub fn refresh_copies(app: &AppHandle) {
    let mut bases = Vec::new();
    if std::env::var_os("APPIMAGE").is_some() {
        bases.extend(app.path().app_data_dir().ok());
    }
    if cfg!(target_os = "linux") {
        if let Some(home) = crate::flatpak::home() {
            bases.extend(
                crate::flatpak::APPS
                    .iter()
                    .map(|a| crate::flatpak::data_dir(&home, a.id)),
            );
        }
    }
    for base in bases {
        for (folder, firefox) in [
            ("browser-extension", false),
            ("browser-extension-firefox", true),
        ] {
            let dest = base.join(folder);
            if !dest.join("manifest.json").is_file() {
                continue;
            }
            if let Some(src) = bundled_extension(app, firefox) {
                if let Err(e) = copy_dir(&src, &dest) {
                    tracing::warn!("could not refresh {}: {e:#}", dest.display());
                }
            }
        }
    }
}

fn copy_dir(src: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else if let Err(e) = std::fs::copy(entry.path(), &to) {
            if !to.exists() {
                return Err(e)
                    .with_context(|| format!("could not copy {}", entry.path().display()));
            }
            tracing::warn!("kept the existing {}: {e}", to.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_the_verbatim_prefix_from_local_paths() {
        assert_eq!(
            plain_path(PathBuf::from(r"\\?\C:\Users\me\Desktop\Multron")),
            PathBuf::from(r"C:\Users\me\Desktop\Multron")
        );
        assert_eq!(
            plain_path(PathBuf::from(r"\\?\UNC\server\share")),
            PathBuf::from(r"\\?\UNC\server\share")
        );
    }

    #[cfg(windows)]
    #[test]
    fn expands_environment_variables() {
        std::env::set_var("MULTRON_TEST_DIR", "C:\\x");
        assert_eq!(
            expand_env("%MULTRON_TEST_DIR%\\a.exe").as_deref(),
            Some("C:\\x\\a.exe")
        );
        assert_eq!(expand_env("%MULTRON_MISSING_VAR%\\a.exe"), None);
        assert_eq!(expand_env("plain").as_deref(), Some("plain"));
    }

    #[test]
    fn lists_flatpak_browsers() {
        let home = std::env::temp_dir().join(format!("mdm-browsers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let root = home.join(".local/share/flatpak");
        for id in [
            "org.mozilla.firefox",
            "io.github.ungoogled_software.ungoogled_chromium",
        ] {
            std::fs::create_dir_all(root.join("app").join(id).join("current")).unwrap();
        }
        let list = flatpak_browsers(&home, &[root]);
        let summary: Vec<_> = list
            .iter()
            .map(|b| {
                (
                    b.id.as_str(),
                    b.name.as_str(),
                    b.engine.as_str(),
                    b.path.as_str(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                (
                    "flatpak:io.github.ungoogled_software.ungoogled_chromium",
                    "Ungoogled Chromium (Flatpak)",
                    "chromium",
                    "flatpak run io.github.ungoogled_software.ungoogled_chromium"
                ),
                (
                    "flatpak:org.mozilla.firefox",
                    "Mozilla Firefox (Flatpak)",
                    "firefox",
                    "flatpak run org.mozilla.firefox"
                ),
            ]
        );
        assert!(list.iter().all(|b| !b.flatpak_autostart));
        assert_eq!(
            list[1].flatpak_grant.as_deref(),
            Some("flatpak override --user --talk-name=org.freedesktop.Flatpak org.mozilla.firefox")
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn detected_browsers_have_distinct_paths() {
        let list = detect();
        eprintln!("{list:#?}");
        let mut paths: Vec<_> = list.iter().map(|b| b.path.to_lowercase()).collect();
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), list.len());
    }

    #[test]
    fn every_browser_has_an_extensions_page() {
        for k in KNOWN {
            assert!(
                k.page.contains("://") || k.page.starts_with("about:"),
                "{}",
                k.id
            );
        }
    }
}

#[cfg(all(windows, test))]
mod detection_tests {
    use super::*;

    #[test]
    fn recognizes_unknown_browsers_by_their_files() {
        let root = std::env::temp_dir().join(format!("mdm-browsers-{}", uuid::Uuid::new_v4()));
        let dia = root.join("Dia");
        std::fs::create_dir_all(dia.join("142.0.7000.1")).unwrap();
        std::fs::write(dia.join("Dia.exe"), b"").unwrap();
        std::fs::write(dia.join("142.0.7000.1").join("resources.pak"), b"").unwrap();
        let fox = root.join("Fox");
        std::fs::create_dir_all(&fox).unwrap();
        std::fs::write(fox.join("fox.exe"), b"").unwrap();
        std::fs::write(fox.join("xul.dll"), b"").unwrap();
        let other = root.join("Other");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("app.exe"), b"").unwrap();
        assert_eq!(engine_of(&dia.join("Dia.exe")), Some("chromium"));
        assert_eq!(engine_of(&fox.join("fox.exe")), Some("firefox"));
        assert_eq!(engine_of(&other.join("app.exe")), None);
        let exe = dia.join("Dia.exe").to_string_lossy().into_owned();
        assert_eq!(
            command_exe(&format!("\"{exe}\" -- \"%1\"")),
            Some(exe.clone())
        );
        assert_eq!(command_exe(r#""C:\nope\x.exe""#), None);
        std::fs::remove_dir_all(&root).ok();
    }
}
