//! Small per-system differences (Linux, Windows, macOS) in one place.

use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command;

/// Senthym draws its own title bar and window buttons, except on macOS where the system frame
/// (traffic-light buttons, edge resizing) is kept.
pub const CUSTOM_FRAME: bool = !cfg!(target_os = "macos");

/// File name of the streaming engine on this system.
pub const ENGINE_FILE: &str = if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp" };

/// Name of the engine's file in the project's release downloads (and its checksum list).
pub const ENGINE_ASSET: &str = if cfg!(windows) {
    "yt-dlp.exe"
} else if cfg!(target_os = "macos") {
    "yt-dlp_macos"
} else {
    "yt-dlp"
};

/// A command that never opens a console window on Windows.
pub fn cmd<S: AsRef<OsStr>>(program: S) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

/// ffmpeg: a copy placed next to the program wins (release packages bring one), else the one on PATH.
pub fn ffmpeg() -> PathBuf {
    // Only a copy that was really found is remembered: on Windows the app's own copy is downloaded
    // after start-up, and must be picked up without a restart.
    static FOUND: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);
    if let Some(p) = FOUND.lock().unwrap().clone() {
        return p;
    }
    let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    let mut places = vec![crate::store::data_dir().join("bin").join(name)];
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
        places.push(dir.join(name));
        // macOS app bundle: Contents/MacOS/senthym with Contents/Resources/ffmpeg beside it.
        places.push(dir.join("../Resources").join(name));
    }
    match places.into_iter().find(|p| p.exists()) {
        Some(p) => {
            *FOUND.lock().unwrap() = Some(p.clone());
            p
        }
        None => PathBuf::from("ffmpeg"),
    }
}

/// Open a web address in the default browser.
pub fn open_url(url: &str) {
    #[cfg(target_os = "linux")]
    let _ = cmd("xdg-open").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let _ = cmd("open").arg(url).spawn();
    #[cfg(windows)]
    let _ = cmd("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    let _ = url;
}

/// Open a file (or show it) with whatever the system uses for it: the installer for a package, the
/// disk image for macOS.
pub fn open_path(path: &std::path::Path) {
    #[cfg(target_os = "linux")]
    let _ = cmd("xdg-open").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let _ = cmd("open").arg(path).spawn();
    #[cfg(windows)]
    {
        // Show the file selected in Explorer: the running program can't be replaced from inside itself.
        let _ = cmd("explorer").arg(format!("/select,{}", path.display())).spawn();
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    let _ = path;
}

/// Remove the old program file an update left beside the new one (Windows keeps it until the next start).
pub fn clean_old_update() {
    #[cfg(windows)]
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("exe.old"));
    }
}

/// Open a program a moment from now, detached, so it starts after this one has finished closing (and
/// saving) and never reads files this one is still writing.
pub fn relaunch_later(program: &std::path::Path, args: &[String]) {
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg("sleep 2; exec \"$0\" \"$@\"")
            .arg(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
    #[cfg(windows)]
    {
        let quoted: Vec<String> = std::iter::once(program.display().to_string()).chain(args.iter().cloned()).map(|a| format!("\"{a}\"")).collect();
        let _ = cmd("cmd").args(["/C", &format!("ping -n 3 127.0.0.1 >nul & start \"\" {}", quoted.join(" "))]).spawn();
    }
}

/// The system's default web browser, as the name the helper uses ("chrome", "edge", "firefox"…), when it
/// is one of the browsers a sign-in can be read from.
pub fn default_browser() -> Option<&'static str> {
    #[cfg(windows)]
    {
        // The program the person chose for web links, from their own settings.
        let out = cmd("reg")
            .args(["query", r"HKCU\Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https\UserChoice", "/v", "ProgId"])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        let prog = text.lines().find(|l| l.contains("ProgId"))?.split_whitespace().last()?.to_string();
        return browser_from_text(&prog);
    }
    #[cfg(target_os = "macos")]
    {
        let out = cmd("defaults").args(["read", "com.apple.LaunchServices/com.apple.launchservices.secure", "LSHandlers"]).output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        for block in text.split('{') {
            if block.contains("LSHandlerURLScheme = https") {
                if let Some(line) = block.lines().find(|l| l.contains("LSHandlerRoleAll")) {
                    return browser_from_text(line);
                }
            }
        }
        return None;
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let out = cmd("xdg-settings").args(["get", "default-web-browser"]).output().ok()?;
        browser_from_text(&String::from_utf8_lossy(&out.stdout))
    }
}

/// Which known browser a system's name for it means (a registry ProgId, a bundle id, a .desktop file name).
pub fn browser_from_text(text: &str) -> Option<&'static str> {
    let t = text.to_lowercase();
    // Order matters: "edge" and "chromium" must be tried before the broader "chrome".
    [
        ("msedge", "edge"),
        ("microsoft-edge", "edge"),
        ("edgemac", "edge"),
        ("firefox", "firefox"),
        ("brave", "brave"),
        ("vivaldi", "vivaldi"),
        ("opera", "opera"),
        ("chromium", "chromium"),
        ("chromehtml", "chrome"),
        ("google-chrome", "chrome"),
        ("google.chrome", "chrome"),
        ("safari", "safari"),
    ]
    .iter()
    .find(|(needle, _)| t.contains(needle))
    .map(|(_, name)| *name)
}

#[cfg(test)]
mod tests {
    use super::browser_from_text as b;

    #[test]
    fn default_browser_names_from_each_system() {
        // Windows ProgIds
        assert_eq!(b("ChromeHTML"), Some("chrome"));
        assert_eq!(b("MSEdgeHTM"), Some("edge"));
        assert_eq!(b("FirefoxURL-308046B0AF4A39CB"), Some("firefox"));
        assert_eq!(b("BraveHTML"), Some("brave"));
        assert_eq!(b("VivaldiHTM.ABCDEF"), Some("vivaldi"));
        assert_eq!(b("OperaStable"), Some("opera"));
        // macOS bundle ids
        assert_eq!(b("LSHandlerRoleAll = \"com.google.chrome\";"), Some("chrome"));
        assert_eq!(b("LSHandlerRoleAll = \"com.microsoft.edgemac\";"), Some("edge"));
        assert_eq!(b("LSHandlerRoleAll = \"com.apple.safari\";"), Some("safari"));
        assert_eq!(b("LSHandlerRoleAll = \"org.mozilla.firefox\";"), Some("firefox"));
        // Linux .desktop names
        assert_eq!(b("google-chrome.desktop\n"), Some("chrome"));
        assert_eq!(b("chromium_chromium.desktop"), Some("chromium"));
        assert_eq!(b("firefox_firefox.desktop"), Some("firefox"));
        assert_eq!(b("microsoft-edge.desktop"), Some("edge"));
        assert_eq!(b("someunknownbrowser.desktop"), None);
    }
}
