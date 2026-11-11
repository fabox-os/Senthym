# Senthym 1.1.1

**Your Music. Your Freedom.** A patch release that fixes signing in on Windows 11 (GitHub issue #1) and improves it on macOS and Linux.

## Packages
Linux (x86_64 and ARM64): `.deb`, `.rpm`, AppImage, Flatpak · Windows: `.exe` · macOS: universal `.dmg`

## What's new
✅ The system's default browser is detected and pre-selected, so "Connect Google account" works with one click
✅ Senthym looks for the browsers that are really installed (Chrome, Edge, Firefox, Brave, Vivaldi, Opera, Chromium; Safari on macOS) and lists only those
✅ Clear help when reading a browser's sign-in fails: close the browser completely if it keeps the sign-in locked, or pick Firefox when Chrome, Edge or Brave protect their cookies on Windows
✅ Developer check `--debug-browsers`

## Fixed
🛠️ Windows 11: the "Choose your browser" list on the welcome window did not open (it was drawn behind the window)
🛠️ Windows and macOS: every browser in the list read "not found" because only Linux locations were checked

## Known limitations
❌ Not yet tried on a real Windows 11 or macOS machine: the fix was checked on Linux and the Windows code was type-checked only. Please tell us on the issue if it works for you
❌ Chrome, Edge and Brave on Windows may still refuse to share their sign-in with other programs; Firefox is the reliable choice there
❌ Windows and macOS builds get less testing than Linux; the macOS app is not notarized (first launch: right-click, Open) — please open an issue for any bug
❌ The update steps for Windows, macOS, .deb, .rpm and Flatpak have not been tried on real installs yet (the AppImage one was)
❌ Browser sign-in import only finds the browsers listed above
❌ One silent crash seen once on Linux was never reproduced
