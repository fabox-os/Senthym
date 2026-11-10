# Senthym 1.1

**Your Music. Your Freedom.** Senthym is a Spotify-style music player for Linux, Windows and macOS.

## Packages
Linux (x86_64 and ARM64): `.deb`, `.rpm`, AppImage, Flatpak · Windows: `.exe` · macOS: universal `.dmg`

## What's new
✅ Update from inside Senthym: Settings → Updates, plus a notice card when a new release is out; it downloads the right package, checks its checksum, installs, closes and reopens the new version with your library and sign-in kept
✅ Linux ARM64 packages (.deb, .rpm, AppImage, Flatpak) next to the x86_64 ones
✅ Autoplay follows your taste: when the queue ends, songs are blended from the radios around what you actually chose
✅ More songs get a video: the artist's official performance film (live, acoustic, sessions) shows when there is no studio video
✅ A video you pick yourself (search, related videos, playlist rows) always shows exactly that video
✅ When a song can't be fetched, Senthym tries SoundCloud and another upload at once instead of skipping it
✅ Search shows each song once (YouTube Music, Deezer and SoundCloud results are merged; remixes, live and covers stay)
✅ Near-instant video quality changes (about 0.1 to 0.4 s instead of 2 to 8 s) and the quality menu shows a change in progress
✅ Songs keep their own audio under the picture, so the volume never moves with the video's mix
✅ Welcome window heading "Your Music. Your Freedom." and README screenshots

## Fixed
🛠️ Video quality changes never took effect while a picture was showing, which kept the Now Playing view and full screen blurry
🛠️ Seeking forwards or returning to a hidden window lowered the Auto video quality for 30 seconds
🛠️ A song no longer waits for its video before it starts playing
🛠️ A song can no longer get another artist's video or a third-party re-upload
🛠️ Videos from artist channels, Spotify / Deezer library songs and names spelled differently (VÆB / VAEB, Björk / Bjork) are now matched correctly
🛠️ Automatic radio no longer repeats music already in the queue
🛠️ The picture never fast-forwards to catch up with the song
🛠️ The video lookup got cheaper, so songs start sooner
🛠️ Windows: the video decoder downloaded on first start is used without restarting the app, with no console window flashing

## Known limitations
❌ The update steps for Windows, macOS, .deb, .rpm and Flatpak have not been tried on real installs (the AppImage one was); Flatpak updates need the host's `flatpak`, otherwise the system installer opens
❌ Windows and macOS builds get less testing than Linux; the macOS app is not notarized (first launch: right-click, Open) — please open an issue for any bug
❌ Importing a sign-in from an installed browser only finds Linux browsers; sign in through the app elsewhere
❌ Deezer can't supply sound for the fallback (30-second previews only), and SoundCloud refuses many label uploads, so the fallback often ends on another YouTube Music upload
❌ The fallback takes a few seconds to find and download the other copy
❌ One silent crash seen once on Linux was never reproduced
