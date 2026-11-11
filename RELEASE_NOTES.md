# Senthym 1.1.2

**Your Music. Your Freedom.** A patch release that fixes "The Senthym streaming engine isn't installed yet" on a fresh install, seen on Windows 10 (GitHub issue #3).

## Packages
Linux (x86_64 and ARM64): `.deb`, `.rpm`, AppImage, Flatpak · Windows: `.exe` · macOS: universal `.dmg`

## What's new
✅ The streaming engine is set up when Senthym starts, so sign-in and search work right after installing
✅ Connect Google account and SoundCloud search set it up on demand if it is still missing

## Fixed
🛠️ "Connect Google account" failed with "isn't installed yet" on a fresh install (the engine was only set up on the first song played)
🛠️ Clear message when the engine can't be downloaded, or on Windows won't start (missing Visual C++ Redistributable)

## Known limitations
❌ Not yet tried on a real Windows 11 or macOS machine: the fix was checked on Linux and the Windows code was type-checked only. Please tell us on the issue if it works for you
❌ Chrome, Edge and Brave on Windows may still refuse to share their sign-in with other programs; Firefox is the reliable choice there
❌ Windows and macOS builds get less testing than Linux; the macOS app is not notarized (first launch: right-click, Open) — please open an issue for any bug
❌ The update steps for Windows, macOS, .deb, .rpm and Flatpak have not been tried on real installs yet (the AppImage one was)
❌ Browser sign-in import only finds the browsers listed above
❌ One silent crash seen once on Linux was never reproduced
