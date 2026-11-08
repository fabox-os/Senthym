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
    static P: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    P.get_or_init(|| {
        let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
        let own = crate::store::data_dir().join("bin").join(name);
        if own.exists() {
            return own;
        }
        if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
            let local = dir.join(name);
            if local.exists() {
                return local;
            }
            // macOS app bundle: Contents/MacOS/senthym with Contents/Resources/ffmpeg beside it.
            let res = dir.join("../Resources").join(name);
            if res.exists() {
                return res;
            }
        }
        PathBuf::from("ffmpeg")
    })
    .clone()
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
