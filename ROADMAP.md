# Senthym — Roadmap

**Your Music. Your Freedom.**

Senthym is a Linux desktop music player written in Rust with eframe/egui. It streams
YouTube Music, plays local files, shows synced lyrics and music videos, and follows the look and feel of the
Spotify desktop app.

> Naming: the product, crate, binary (`senthym`), window title, app id and data folder
> (`~/.local/share/senthym`) are all **Senthym**. The folders of the earlier name are moved over on the first start.

**Version 1.0** (2026-11-08) — first release.

Legend: ✅ done · 🔧 in progress · ⬜ planned

---

## 1. Interface (Spotify-style)

- ✅ Home, search, library, artist, album, playlist and now-playing screens rebuilt after the Spotify desktop UI
- ✅ Top bar: history arrows, Home, pill search box, Browse button, account
- ✅ Search dropdown: recent searches with covers, artist / song matches, keyboard navigation, suggestions
- ✅ Picking an artist in the dropdown opens the artist page
- ✅ Recent searches saved and shown with the item's cover
- ✅ Browse all: genre and mood tiles with covers behind them
- ✅ Library rail (collapsed) and full library grid, "+ Create" menu, expand button, filters and search
- ✅ Followed artists and saved albums / playlists listed right after your own playlists (before synced lists)
- ✅ Your own playlists listed first, newest first; playlist cover and "Edit details" dialog (name, description, cover)
- ✅ Playlist rows: grey only on hover, no double highlight; "⋯" menu per song
- ✅ Hide a song in a playlist (greyed, skipped by playback, Undo)
- ✅ Equalizer bars and highlight for the playing song and the playing playlist in the rail
- ✅ Round "+" (add to Liked) with green check, sized for the player bar and for song rows
- ✅ Now-playing panel on the right: cover or video, related music videos with arrows, "In this video", About the artist, Credits
- ✅ Follow / Following button for every artist in Credits
- ✅ Artist page: face-cropped banner, "⋯" menu (Unfollow, Don't play this artist, Go to artist radio, Report, Share → Copy link / Embed)
- ✅ Lyrics button toggles back to the previous page
- ✅ Removed: sleep timer, arrow next to volume, library icon in the top right, "Ctrl L" hint in the search box

## 2. Playback speed

- ✅ Fast path: direct address from yt-dlp + parallel ranged downloads
- ✅ Plain request first, signed-in route (cookies + JS runtime) only as a fallback; the two best routes are raced
- ✅ Prefetch: hover, visible rows, next two songs, top search results; queue capped, paused while a song loads
- ✅ Searches and playlists open instantly with a loading shell and prefetch
- ✅ Audio version preferred over clips and VEVO uploads
- ⬜ Keep a short list of recent yt-dlp failures to skip bad routes faster

## 3. Music video

- ✅ Video-first playback with "Switch to audio" / "Switch to video"
- ✅ Official videos only (never lyric, audio, cover, karaoke or visualizer uploads), maximum 1080p
- ✅ Real upload title check (oEmbed) so official videos listed under a bare title are found
- ✅ Performance films (Acoustic Version, Live, Sessions) accepted; a different cut never matches the studio song
- ✅ Adaptive quality, quality menu (Auto, 1080p … 240p), seamless quality switching
- ✅ Mini player, fullscreen, expanded Now-playing view
- ✅ Cover shown while the picture loads; no leftover frame from the previous song
- ✅ Lyrics offset for video audio
- ⬜ Remember the quality choice per song

## 4. Accounts and library

- ✅ YouTube / Google sign-in through the app (fresh cookie file, removed on sign-out)
- ✅ Synced YouTube Music library (liked, playlists, albums, artists)
- ✅ Follow, like and save work signed in and signed out
- ⬜ Optional: push follows to the YouTube account (needs Google account permissions)
- ⬜ Spotify library: keep as optional

## 5. Security and stability

- ✅ Rename to Senthym with automatic move of the saved library, cookies, covers and cache

- ✅ yt-dlp updates verified with SHA-256 checksums; temp file cleanup
- ✅ ffmpeg only reads https addresses with a protocol whitelist; picture size capped at 1920x1080
- ✅ Picture memory capped (count and about 450 MB)
- ✅ Panic log in `~/.local/share/senthym/crash.log`
- ✅ Space toggles play once (no double toggle with a focused button)
- ⬜ Find and fix the silent crash reported on 2026-11-07 (no trace yet; crash log will show panics)
- ⬜ Reduce per-frame video texture cost at 1080p

## 6. Packaging

- ✅ Linux: .deb, .rpm, AppImage, Flatpak
- ✅ Windows: single .exe (fetches ffmpeg on first start)
- ✅ macOS: universal .dmg (Apple silicon + Intel)
- ⬜ Apple notarization, Windows code signing
- ⬜ Linux ARM packages

## Next

1. ⬜ Test and polish the Windows and macOS builds
2. ⬜ Per-song video quality memory
3. ⬜ Follow sync to the YouTube account
4. ⬜ Crash investigation and long-run performance tests
