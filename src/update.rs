use makepad_widgets::*;

/// GitHub repo (owner/name) hosting releases; release tag = v{version}.
/// TODO(发布前): 替换为实际的仓库。
pub const UPDATE_REPO: &str = "OWNER/REPO";

/// Release asset name for the current platform (release convention:
/// understand-everything-{linux|macos|windows}-{x86_64|aarch64}).
pub fn asset_name() -> String {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    if cfg!(target_os = "windows") {
        format!("understand-everything-{os}-{arch}.exe")
    } else {
        format!("understand-everything-{os}-{arch}")
    }
}

/// Latest-release check; the response arrives via
/// `MatchEvent::handle_http_response` keyed by `request_id`.
pub fn check_update(cx: &mut Cx, request_id: LiveId) {
    let url = format!("https://api.github.com/repos/{UPDATE_REPO}/releases/latest");
    let mut http = HttpRequest::new(url, HttpMethod::GET);
    http.set_header("User-Agent".to_string(), "understand-everything".to_string());
    http.set_header("Accept".to_string(), "application/vnd.github+json".to_string());
    cx.http_request(request_id, http);
}

pub struct UpdateInfo {
    pub tag: String,
    pub asset_url: Option<String>,
    pub newer: bool,
}

/// Parse the releases/latest JSON body: tag, this platform's asset URL, and
/// whether the tag is newer than the running version.
pub fn parse_latest(body: &str, running: &str) -> Option<UpdateInfo> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let tag = v.get("tag_name")?.as_str()?.to_string();
    let asset_url = v
        .get("assets")?
        .as_array()?
        .iter()
        .find(|a| a.get("name").and_then(|n| n.as_str()) == Some(asset_name().as_str()))
        .and_then(|a| a.get("browser_download_url")?.as_str().map(String::from));
    let newer = newer_than(&tag, running);
    Some(UpdateInfo { tag, asset_url, newer })
}

/// "v0.2.0" vs "0.1.0": strip a "v" prefix, compare numeric parts.
fn newer_than(tag: &str, running: &str) -> bool {
    fn parts(v: &str) -> Vec<u32> {
        v.trim_start_matches('v')
            .split('.')
            .map(|p| p.trim_end_matches(|c: char| !c.is_ascii_digit()).parse().unwrap_or(0))
            .collect()
    }
    let t = parts(tag);
    let r = parts(running);
    t.iter()
        .zip(r.iter())
        .find(|(a, b)| a != b)
        .map_or(t.len() > r.len(), |(a, b)| a > b)
}

/// Download the release asset; the body arrives via `handle_http_response`
/// keyed by `request_id`.
pub fn download(cx: &mut Cx, request_id: LiveId, url: &str) {
    let mut http = HttpRequest::new(url.to_string(), HttpMethod::GET);
    http.set_header("User-Agent".to_string(), "understand-everything".to_string());
    cx.http_request(request_id, http);
}

/// Replace the running binary with `new` and relaunch. On Windows the exe
/// is locked while running, so a helper .bat swaps after exit. Returns Err
/// on failure (the app keeps running).
pub fn apply(new: &std::path::Path) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    if cfg!(target_os = "windows") {
        let bat = new.with_extension("bat");
        let text = format!(
            "@echo off\r\ntimeout /t 1 /nobreak >nul\r\nmove /y \"{}\" \"{}\"\r\nstart \"\" \"{}\"\r\ndel \"%~f0\"\r\n",
            new.display(),
            exe.display(),
            exe.display()
        );
        std::fs::write(&bat, text)?;
        std::process::Command::new("cmd")
            .args(["/c", bat.to_str().unwrap_or_default()])
            .spawn()?;
        Ok(())
    } else {
        std::fs::rename(new, &exe)?;
        std::process::Command::new(&exe).spawn()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_name_matches_convention() {
        let n = asset_name();
        let (os, arch) = if cfg!(target_os = "windows") {
            ("windows", if cfg!(target_arch = "aarch64") { "aarch64" } else { "x86_64" })
        } else if cfg!(target_os = "macos") {
            ("macos", if cfg!(target_arch = "aarch64") { "aarch64" } else { "x86_64" })
        } else {
            ("linux", if cfg!(target_arch = "aarch64") { "aarch64" } else { "x86_64" })
        };
        assert!(n.starts_with(&format!("understand-everything-{os}-{arch}")));
    }

    #[test]
    fn newer_than_compares_numeric_parts() {
        assert!(newer_than("v0.2.0", "0.1.9"));
        assert!(newer_than("v0.10.0", "0.9.0"));
        assert!(!newer_than("v0.1.0", "0.1.0"));
        assert!(!newer_than("v0.1.1", "0.2.0"));
        assert!(!newer_than("v0.9.0", "v0.10.0"));
    }

    #[test]
    fn parse_latest_extracts_tag_and_asset() {
        let asset = asset_name();
        let json = format!(
            r#"{{"tag_name":"v0.2.0","assets":[{{"name":"understand-everything-windows-x86_64.exe","browser_download_url":"https://example/windows"}},{{"name":"{asset}","browser_download_url":"https://example/me"}}]}}"#
        );
        let info = parse_latest(&json, "0.1.0").unwrap();
        assert_eq!(info.tag, "v0.2.0");
        assert!(info.newer);
        assert_eq!(info.asset_url.as_deref(), Some("https://example/me"));
    }

    #[test]
    fn parse_latest_is_same_or_older() {
        let json = r#"{"tag_name":"v0.1.0","assets":[]}"#;
        let info = parse_latest(json, "0.1.0").unwrap();
        assert!(!info.newer);
    }

    #[test]
    fn parse_latest_rejects_garbage() {
        assert!(parse_latest("not json", "0.1.0").is_none());
        assert!(parse_latest("{}", "0.1.0").is_none());
    }
}
