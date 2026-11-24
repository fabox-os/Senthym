# Senthym 1.1.5

**Your Music. Your Freedom.** The next song now starts while the window is minimized, plus playlist, cover and menu fixes. Deezer is no longer a source.

## Packages
Linux (x86_64 and ARM64): `.deb`, `.rpm`, AppImage, Flatpak · Windows: `.exe` · macOS: universal `.dmg`

## What's new
✅ Playback goes on while the window is minimized (a background heartbeat starts the next song without a redraw)
✅ A playlist you make opens at once and is listed right after the fixed entries
✅ YouTube songs with no cover use the picture of their video instead of a letter

## Fixed
🛠️ Minimized window: the next song waited until the window was shown again
🛠️ A new playlist was listed last and was easy to miss; saving the library no longer loses changes when Windows has the file busy
🛠️ "Connect Spotify" removed from the library list (still under Settings)

## Removed
❌ Deezer is no longer a source (libraries with Deezer songs still load; those songs play from their YouTube Music match)

## Known limitations
❌ The macOS app is not notarized (first launch: right-click, Open), and its in-app update has not been tried on a real install yet
❌ Chrome, Edge and Brave on Windows may refuse to share their sign-in with other programs; Firefox is the reliable choice there
❌ Only YouTube Music songs can be liked on the account; local and Spotify songs stay on that computer
❌ The blue edge in full screen on Windows may come from the system and needs another look if it stays
❌ One silent crash seen once on Linux was never reproduced
