# Senthym 1.0

**Your Music. Your Freedom.** First release of Senthym, a Spotify-style music player for Linux, Windows and macOS.

## Packages
Linux: `.deb`, `.rpm`, AppImage, Flatpak · Windows: `.exe` · macOS: universal `.dmg`

## What's new
✅ Spotify-style interface with a baby-blue accent and a modern custom window frame
✅ Search and stream from YouTube Music, Deezer and SoundCloud with no API keys; optional Spotify setup
✅ Your own files, downloads, queue, history, crossfade and synced lyrics with translation
✅ Official music videos up to 1080p, quality menu (Auto by default), mini player, full screen, Switch to audio
✅ Related music videos over the Now Playing picture; black bars in clips are cropped
✅ Automatic volume leveller for music-video sound
✅ Playlists with your own cover, Liked Songs, followed artists, Follow button in Credits
✅ "Hide in this playlist": hidden songs stay greyed and are skipped when playing, with Undo
✅ Fast playback with prefetching, panic log, picture-memory cap
✅ Packages for Linux (.deb, .rpm, AppImage, Flatpak), Windows (.exe) and macOS (universal)

## Fixed
🛠️ Videos dropping to 240p on fast connections (a slow start was mistaken for a slow connection)
🛠️ Video decoding can no longer make the sound stutter (lower priority, limited threads)
🛠️ Leftover picture or cover from the previous song
🛠️ Pause not sticking (Space toggled twice)
🛠️ Followed artists missing from the library when signed in
🛠️ Songs with an official video but a bare title showed only the cover
🛠️ Studio song matching the acoustic film of the same name
🛠️ Possible panic in title cleanup on non-English titles
🛠️ Hover card in the collapsed library wrapped one word per line

## Known limitations
❌ Windows and macOS builds are made by CI and get less testing than Linux — please open an issue for any bug
❌ The macOS app is not notarized (first launch: right-click, Open)
❌ Follow is stored in Senthym only; it does not subscribe on your online account yet
❌ Importing a sign-in from an installed browser only finds Linux browsers; sign in through the app elsewhere
❌ One silent crash seen once on Linux was never reproduced
❌ Linux packages are for x86_64 only
