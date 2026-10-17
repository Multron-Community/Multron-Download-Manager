use serde::Serialize;
use tauri::window::{Color, Effect, EffectState, EffectsBuilder};
use tauri::{Theme, WebviewWindow};

use crate::settings::Settings;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Support {
    pub materials: Vec<&'static str>,
}

pub fn support() -> Support {
    Support {
        materials: materials(),
    }
}

#[cfg(windows)]
fn materials() -> Vec<&'static str> {
    match windows_build() {
        b if b >= 22000 => vec!["acrylic", "mica"],
        b if b >= 10240 => vec!["blur"],
        _ => Vec::new(),
    }
}

#[cfg(target_os = "macos")]
fn materials() -> Vec<&'static str> {
    vec!["vibrancy"]
}

#[cfg(all(unix, not(target_os = "macos")))]
fn materials() -> Vec<&'static str> {
    Vec::new()
}

#[cfg(windows)]
fn windows_build() -> u32 {
    use std::os::windows::process::CommandExt;
    use std::sync::OnceLock;
    static BUILD: OnceLock<u32> = OnceLock::new();
    *BUILD.get_or_init(|| {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("reg")
            .args([
                "query",
                "HKLM\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion",
                "/v",
                "CurrentBuildNumber",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .split_whitespace()
                    .last()
                    .and_then(|v| v.parse().ok())
            })
            .unwrap_or(0)
    })
}

fn pick_material(requested: &str) -> Option<&'static str> {
    let available = materials();
    available
        .iter()
        .copied()
        .find(|m| *m == requested)
        .or_else(|| available.first().copied())
}

pub fn apply(
    win: &WebviewWindow,
    theme: &str,
    transparency: bool,
    material: &str,
    tint: Option<[u8; 4]>,
) -> bool {
    let forced = match theme {
        "dark" => Some(Theme::Dark),
        "light" => Some(Theme::Light),
        _ => None,
    };
    if let Err(e) = win.set_theme(forced) {
        tracing::warn!("could not set the window theme: {e}");
    }

    let material = if transparency {
        pick_material(material)
    } else {
        None
    };
    let Some(material) = material else {
        let _ = win.set_effects(None);
        return false;
    };
    let effect = match material {
        "mica" => match forced {
            Some(Theme::Dark) => Effect::MicaDark,
            Some(Theme::Light) => Effect::MicaLight,
            _ => Effect::Mica,
        },
        "acrylic" => Effect::Acrylic,
        "blur" => Effect::Blur,
        _ => Effect::UnderWindowBackground,
    };
    let mut effects = EffectsBuilder::new()
        .effect(effect)
        .state(EffectState::FollowsWindowActiveState);
    if let Some([r, g, b, a]) = tint {
        effects = effects.color(Color(r, g, b, a));
    }
    match win.set_effects(effects.build()) {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("could not apply the {material} effect: {e}");
            let _ = win.set_effects(None);
            false
        }
    }
}

pub fn apply_settings(win: &WebviewWindow, s: &Settings) -> bool {
    apply(win, &s.theme, s.transparency, &s.backdrop_material, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_the_best_material() {
        let available = materials();
        assert_eq!(
            pick_material("no-such-material"),
            available.first().copied()
        );
        for m in &available {
            assert_eq!(pick_material(m), Some(*m));
        }
    }
}
