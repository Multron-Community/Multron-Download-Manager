use serde::{Deserialize, Serialize};

pub const REPO: &str = "Multron-Community/Multron-Download-Manager";

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdate {
    pub current: String,
    pub latest: Option<String>,
    pub has_update: bool,
    pub url: String,
    pub notes: Option<String>,
    pub published_at: Option<String>,
    pub error: Option<String>,
    pub asset: Option<ReleaseAsset>,
    #[serde(skip)]
    pub manifest_url: Option<String>,
    #[serde(skip)]
    pub signature_url: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseAsset {
    pub name: String,
    #[serde(alias = "browser_download_url")]
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    body: Option<String>,
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

fn parse_version(v: &str) -> Option<semver::Version> {
    semver::Version::parse(v.trim().trim_start_matches(['v', 'V'])).ok()
}

fn releases_api() -> String {
    #[cfg(debug_assertions)]
    if let Some(url) = std::env::var("MULTRON_UPDATE_API")
        .ok()
        .filter(|u| !u.trim().is_empty())
    {
        return url;
    }
    format!("https://api.github.com/repos/{REPO}/releases/latest")
}

pub fn test_mode() -> bool {
    cfg!(debug_assertions) && std::env::var_os("MULTRON_UPDATE_API").is_some()
}

pub async fn check_app(client: &reqwest::Client) -> AppUpdate {
    check_app_at(client, &releases_api(), env!("CARGO_PKG_VERSION")).await
}

pub async fn check_app_at(client: &reqwest::Client, api: &str, current: &str) -> AppUpdate {
    let current = current.to_string();
    let mut out = AppUpdate {
        current: current.clone(),
        url: format!("https://github.com/{REPO}/releases"),
        ..Default::default()
    };
    let resp = client
        .get(api)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await;
    let resp = match resp {
        Ok(r) if r.status() == reqwest::StatusCode::NOT_FOUND => return out,
        Ok(r) => r,
        Err(e) => {
            out.error = Some(e.to_string());
            return out;
        }
    };
    match resp.error_for_status() {
        Ok(r) => match r.json::<Release>().await {
            Ok(rel) => {
                out.has_update = match (parse_version(&rel.tag_name), parse_version(&current)) {
                    (Some(latest), Some(cur)) => latest > cur,
                    _ => false,
                };
                out.latest = Some(rel.tag_name.trim_start_matches(['v', 'V']).to_string());
                out.url = rel.html_url;
                out.notes = rel.body;
                out.published_at = rel.published_at;
                out.asset = crate::selfupdate::pick_asset(&rel.assets).cloned();
                out.manifest_url =
                    crate::selfupdate::asset_url(&rel.assets, crate::selfupdate::MANIFEST_NAME);
                out.signature_url =
                    crate::selfupdate::asset_url(&rel.assets, crate::selfupdate::SIGNATURE_NAME);
            }
            Err(e) => out.error = Some(e.to_string()),
        },
        Err(e) => out.error = Some(e.to_string()),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(parse_version("v1.2.10").unwrap() > parse_version("1.2.9").unwrap());
        assert!(parse_version("nightly").is_none());
    }
}
