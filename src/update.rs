use crate::net;
use serde_json::Value;
use std::path::PathBuf;

const LATEST: &str = "https://api.github.com/repos/patvuk/warlogger/releases/latest";
const HEADERS: [(&str, &str); 2] = [("User-Agent", "warlogger"), ("Accept", "application/vnd.github+json")];

const ASSET: &str = match (cfg!(windows), cfg!(feature = "omarchy")) {
    (true, _) => "-windows-x86_64.exe",
    (false, true) => "-linux-x86_64-omarchy",
    (false, false) => "-linux-x86_64",
};

pub struct Release {
    pub version: String,
    url: String,
    size: usize,
}

fn parts(version: &str) -> Vec<u64> {
    version
        .trim_start_matches('v')
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

pub fn check() -> Result<Option<Release>, String> {
    let response = net::send("GET", LATEST, &HEADERS, Vec::new())?;
    if response.status == 404 {
        return Err("no public release found on GitHub".into());
    }
    if response.status != 200 {
        return Err(format!("GitHub answered {} for the latest release", response.status));
    }

    let release: Value = serde_json::from_slice(&response.body).map_err(|e| e.to_string())?;
    let version = release["tag_name"].as_str().unwrap_or_default().trim_start_matches('v');
    if parts(version) <= parts(env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }

    let asset = release["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|a| a["name"].as_str().is_some_and(|n| n.ends_with(ASSET)))
        .ok_or(format!("release {version} has no {ASSET} download"))?;

    Ok(Some(Release {
        version: version.to_owned(),
        url: asset["browser_download_url"].as_str().unwrap_or_default().to_owned(),
        size: asset["size"].as_u64().unwrap_or(0) as usize,
    }))
}

fn download(mut url: String) -> Result<Vec<u8>, String> {
    for _ in 0..5 {
        let response = net::send("GET", &url, &[("User-Agent", "warlogger")], Vec::new())?;
        match (response.status, response.location) {
            (200, _) => return Ok(response.body),
            (300..400, Some(next)) => url = next,
            (status, _) => return Err(format!("download failed with status {status}")),
        }
    }
    Err("too many redirects".into())
}

pub fn install(release: &Release) -> Result<PathBuf, String> {
    let bytes = download(release.url.clone())?;
    if bytes.len() != release.size {
        return Err(format!("download is {} bytes, expected {}", bytes.len(), release.size));
    }

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let fresh = exe.with_extension("new");
    let fail = |e: std::io::Error| format!("{}: {e}", exe.display());
    std::fs::write(&fresh, bytes).map_err(fail)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fresh, std::fs::Permissions::from_mode(0o755)).map_err(fail)?;
    }
    #[cfg(windows)]
    {
        let old = exe.with_extension("old");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&exe, &old).map_err(fail)?;
    }

    std::fs::rename(&fresh, &exe).map_err(fail)?;
    Ok(exe)
}

#[cfg(windows)]
pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("old"));
    }
}
