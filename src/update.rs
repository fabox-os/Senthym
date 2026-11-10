//! Updates: look for a newer Senthym release on GitHub, pick the package that matches how this copy was
//! installed (Flatpak, AppImage, .deb, .rpm, Windows .exe, macOS .dmg), download it with its checksum
//! verified and hand it over to be installed.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const REPO: &str = "fabox-os/Senthym";
/// The version this program is.
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// The version to compare against (a developer can pretend to be older to try the update flow).
pub fn current() -> String {
    std::env::var("SENTHYM_FAKE_VERSION").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| CURRENT.to_string())
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Flatpak,
    AppImage,
    Deb,
    Rpm,
    Windows,
    MacOs,
    /// Built from source or run from some folder: no package fits it, the releases page is opened.
    Other,
}

impl Kind {
    /// How this copy of Senthym was installed.
    pub fn detect() -> Kind {
        if cfg!(windows) {
            return Kind::Windows;
        }
        if cfg!(target_os = "macos") {
            return Kind::MacOs;
        }
        if std::env::var_os("FLATPAK_ID").is_some() || std::path::Path::new("/.flatpak-info").exists() {
            return Kind::Flatpak;
        }
        if std::env::var_os("APPIMAGE").is_some() {
            return Kind::AppImage;
        }
        let exe = std::env::current_exe().ok();
        if exe.as_deref().map(|p| p.starts_with("/usr/bin")).unwrap_or(false) {
            let has = |tool: &str| crate::sys::cmd(tool).arg("--version").output().is_ok();
            // A package manager owns /usr/bin: whichever one is on this system.
            if std::path::Path::new("/usr/bin/dpkg").exists() {
                return Kind::Deb;
            }
            if has("rpm") {
                return Kind::Rpm;
            }
        }
        Kind::Other
    }

    /// The file name of this kind's package in a release (Linux packages exist for x86_64 and ARM64).
    pub fn asset_name(&self) -> Option<&'static str> {
        let arm = cfg!(target_arch = "aarch64");
        match self {
            Kind::Flatpak => Some(if arm { "Senthym-linux-aarch64.flatpak" } else { "Senthym-linux-x86_64.flatpak" }),
            Kind::AppImage => Some(if arm { "Senthym-linux-aarch64.AppImage" } else { "Senthym-linux-x86_64.AppImage" }),
            Kind::Deb => Some(if arm { "Senthym-linux-arm64.deb" } else { "Senthym-linux-amd64.deb" }),
            Kind::Rpm => Some(if arm { "Senthym-linux-aarch64.rpm" } else { "Senthym-linux-x86_64.rpm" }),
            Kind::Windows => Some("Senthym-windows.exe"),
            Kind::MacOs => Some("Senthym-macos-universal.dmg"),
            Kind::Other => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Kind::Flatpak => "Flatpak",
            Kind::AppImage => "AppImage",
            Kind::Deb => ".deb package",
            Kind::Rpm => ".rpm package",
            Kind::Windows => "Windows program",
            Kind::MacOs => "macOS app",
            Kind::Other => "this copy",
        }
    }
}

/// A newer release and the package for this copy.
#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub page: String,
    pub asset: Option<Asset>,
}

#[derive(Clone, Debug)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// Lowercase hex SHA-256 of the file, as GitHub states it.
    pub sha256: Option<String>,
}

/// Numbers of a version such as "1.2.0", "v1.2" or "1.0": compared piece by piece, missing pieces are 0.
fn parts(v: &str) -> Vec<u64> {
    v.trim().trim_start_matches(['v', 'V']).split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).map(|p| p.parse().unwrap_or(0)).collect()
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    let (a, b) = (parts(candidate), parts(current));
    if a.is_empty() {
        return false;
    }
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(10))
        .timeout_read(std::time::Duration::from_secs(30))
        .user_agent(&format!("Senthym/{CURRENT}"))
        .build()
}

/// The latest published release, if it is newer than this program. `Ok(None)`: up to date.
pub fn check(kind: &Kind) -> Result<Option<Release>, String> {
    let v: serde_json::Value = agent()
        .get(&format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("Couldn't reach GitHub ({e})"))?
        .into_json()
        .map_err(|e| e.to_string())?;
    let tag = v["tag_name"].as_str().ok_or("GitHub's answer had no version")?.to_string();
    if !is_newer(&tag, &current()) {
        return Ok(None);
    }
    let want = kind.asset_name();
    let asset = v["assets"].as_array().and_then(|a| {
        a.iter().find(|x| x["name"].as_str() == want).and_then(|x| {
            let url = x["browser_download_url"].as_str()?;
            // Only ever downloaded from this project's own releases.
            if !url.starts_with(&format!("https://github.com/{REPO}/releases/download/")) {
                return None;
            }
            Some(Asset {
                name: x["name"].as_str()?.to_string(),
                url: url.to_string(),
                size: x["size"].as_u64().unwrap_or(0),
                sha256: x["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(|h| h.to_lowercase()),
            })
        })
    });
    Ok(Some(Release {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        page: v["html_url"].as_str().unwrap_or(&format!("https://github.com/{REPO}/releases")).to_string(),
        asset,
    }))
}

/// The folder updates are saved in.
fn folder() -> PathBuf {
    if let Some(d) = std::env::var_os("SENTHYM_UPDATE_DIR") {
        return PathBuf::from(d);
    }
    dirs::download_dir().unwrap_or_else(|| crate::store::data_dir().join("updates"))
}

/// Download the package (progress is counted in `got`), check it against its checksum, and return where
/// it was saved. A file that doesn't match is deleted and never kept.
pub fn download(rel: &Release, got: Arc<AtomicU64>) -> Result<PathBuf, String> {
    let asset = rel.asset.as_ref().ok_or("This release has no package for how Senthym is installed here")?;
    let dir = folder();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest = dir.join(&asset.name);
    let tmp = dir.join(format!("{}.part", asset.name));
    let resp = agent()
        .get(&asset.url)
        .call()
        .map_err(|e| format!("Couldn't download the update ({e})"))?;
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
    let mut buf = vec![0u8; 256 * 1024];
    let limit = asset.size.max(1).saturating_add(1024 * 1024);
    let mut total = 0u64;
    loop {
        let n = std::io::Read::read(&mut reader, &mut buf).map_err(|e| format!("The download stopped ({e})"))?;
        if n == 0 {
            break;
        }
        total += n as u64;
        // Never more than the release said the file is (plus a little).
        if asset.size > 0 && total > limit {
            let _ = std::fs::remove_file(&tmp);
            return Err("The download is bigger than it should be, so it was thrown away".into());
        }
        sha2::Digest::update(&mut hasher, &buf[..n]);
        std::io::Write::write_all(&mut file, &buf[..n]).map_err(|e| e.to_string())?;
        got.store(total, Ordering::Relaxed);
    }
    drop(file);
    if asset.size > 0 && total != asset.size {
        let _ = std::fs::remove_file(&tmp);
        return Err("The download is incomplete, please try again".into());
    }
    let sum: String = sha2::Digest::finalize(hasher).iter().map(|b| format!("{b:02x}")).collect();
    match &asset.sha256 {
        Some(expected) if *expected != sum => {
            let _ = std::fs::remove_file(&tmp);
            return Err("The download doesn't match its checksum, so it was not kept".into());
        }
        _ => {}
    }
    let _ = std::fs::remove_file(&dest);
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    if asset.name.ends_with(".AppImage") {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755));
    }
    Ok(dest)
}

/// What installing did: what to tell the person, and what to start once Senthym has closed.
pub struct Outcome {
    pub text: String,
    /// The program (and arguments) to open once this one has shut down; `None` when the person has to
    /// finish the install themselves.
    pub relaunch: Option<(PathBuf, Vec<String>)>,
}

fn done(text: &str, program: PathBuf, args: &[&str]) -> Outcome {
    Outcome { text: text.to_string(), relaunch: Some((program, args.iter().map(|a| a.to_string()).collect())) }
}

fn manual(text: &str) -> Outcome {
    Outcome { text: text.to_string(), relaunch: None }
}

/// Run a command that may ask for the person's password (polkit); true when it succeeded.
fn ok(mut c: std::process::Command) -> bool {
    c.stdin(std::process::Stdio::null()).output().map(|o| o.status.success()).unwrap_or(false)
}

/// Install the downloaded package in place, the way each kind allows. The person's library, sign-in and
/// settings live in their own data folder, apart from the program, so the new version simply reads them.
/// A copy of the library is kept next to it first, in case anything goes wrong.
pub fn install(kind: &Kind, file: &std::path::Path) -> Result<Outcome, String> {
    let lib = crate::store::data_dir().join("library.json");
    if lib.exists() {
        let _ = std::fs::copy(&lib, lib.with_extension("json.before-update"));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let opened = |text: &str| {
        crate::sys::open_path(file);
        manual(text)
    };
    Ok(match kind {
        Kind::AppImage => {
            let Some(current) = std::env::var_os("APPIMAGE").map(PathBuf::from) else { return Ok(opened("Downloaded. Run the new AppImage to update.")) };
            let staged = current.with_extension("update");
            std::fs::copy(file, &staged).map_err(|e| format!("Couldn't replace the AppImage ({e})"))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755));
            }
            std::fs::rename(&staged, &current).map_err(|e| format!("Couldn't replace the AppImage ({e})"))?;
            done("Updated. Opening the new version…", current, &[])
        }
        Kind::Windows => {
            // A running .exe can be renamed but not overwritten: move it aside, put the new one in its place.
            let old = exe.with_extension("exe.old");
            let _ = std::fs::remove_file(&old);
            let swapped = std::fs::rename(&exe, &old).is_ok() && std::fs::copy(file, &exe).is_ok();
            if swapped {
                done("Updated. Opening the new version…", exe, &[])
            } else {
                if old.exists() && !exe.exists() {
                    let _ = std::fs::rename(&old, &exe);
                }
                opened("Downloaded. Close Senthym and run the new file to update.")
            }
        }
        Kind::MacOs => {
            // exe = Senthym.app/Contents/MacOS/senthym
            let bundle = exe.parent().and_then(|p| p.parent()).and_then(|p| p.parent()).map(|p| p.to_path_buf()).filter(|p| p.extension().map(|e| e == "app").unwrap_or(false));
            let Some(bundle) = bundle else { return Ok(opened("Downloaded. Drag the new Senthym over the old one, then open it.")) };
            let mount = std::env::temp_dir().join(format!("senthym-update-{}", std::process::id()));
            let _ = std::fs::create_dir_all(&mount);
            let attached = ok({
                let mut c = crate::sys::cmd("hdiutil");
                c.args(["attach", "-nobrowse", "-readonly", "-quiet", "-mountpoint"]).arg(&mount).arg(file);
                c
            });
            let copied = attached
                && ok({
                    let mut c = crate::sys::cmd("ditto");
                    c.arg(mount.join("Senthym.app")).arg(&bundle);
                    c
                });
            let _ = crate::sys::cmd("hdiutil").args(["detach", "-quiet", "-force"]).arg(&mount).output();
            let _ = std::fs::remove_dir(&mount);
            if copied {
                done("Updated. Opening the new version…", PathBuf::from("open"), &[&bundle.to_string_lossy()])
            } else {
                opened("Downloaded. Drag the new Senthym over the old one, then open it.")
            }
        }
        Kind::Deb => {
            let mut c = crate::sys::cmd("pkexec");
            c.args(["apt-get", "install", "-y", "--reinstall"]).arg(file);
            if ok(c) { done("Updated. Opening the new version…", exe, &[]) } else { opened("Downloaded. Finish the install in the window that opened, then open Senthym again.") }
        }
        Kind::Rpm => {
            let mut c = crate::sys::cmd("pkexec");
            c.args(["rpm", "-U", "--replacepkgs"]).arg(file);
            if ok(c) { done("Updated. Opening the new version…", exe, &[]) } else { opened("Downloaded. Finish the install in the window that opened, then open Senthym again.") }
        }
        Kind::Flatpak => {
            // From inside the sandbox the host's flatpak is reached through flatpak-spawn, when allowed.
            let mut c = crate::sys::cmd("flatpak-spawn");
            c.args(["--host", "flatpak", "install", "--user", "-y", "--noninteractive", "--reinstall"]).arg(file);
            if ok(c) {
                done("Updated. Opening the new version…", PathBuf::from("flatpak-spawn"), &["--host", "flatpak", "run", "io.github.fabox_os.Senthym"])
            } else {
                opened("Downloaded. Finish the install in the window that opened, then open Senthym again.")
            }
        }
        Kind::Other => opened("Downloaded."),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number() {
        assert!(is_newer("1.1", "1.0.0"));
        assert!(is_newer("v1.0.1", "1.0"));
        assert!(is_newer("2.0", "1.9.9"));
        assert!(is_newer("1.10", "1.9"));
        assert!(!is_newer("1.0", "1.0.0"));
        assert!(!is_newer("1.0.0", "1.0"));
        assert!(!is_newer("0.9", "1.0.0"));
        assert!(!is_newer("nightly", "1.0.0"));
    }
}
