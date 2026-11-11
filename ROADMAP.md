# Senthym — Roadmap

**Your Music. Your Freedom.**

Senthym is a Linux desktop music player written in Rust with eframe/egui. It streams
YouTube Music, plays local files, shows synced lyrics and music videos, and follows the look and feel of the
Spotify desktop app.

> Naming: the product, crate, binary (`senthym`), window title, app id and data folder
> (`~/.local/share/senthym`) are all **Senthym**. The folders of the earlier name are moved over on the first start.

**Version 1.1.1** (2026-11-11). Version 1.1 came out on 2026-11-10 and 1.0 was the first release (2026-11-08). Everything is listed in the changelog.

Legend: ✅ done · 🔧 in progress · ⬜ planned

---

## 1. Interface (Spotify-style)

- ✅ Home, search, library, artist, album, playlist and now-playing screens rebuilt after the Spotify desktop UI
- ✅ Top bar: history arrows, Home, pill search box, Browse button, account
- ✅ Search dropdown: recent searches with covers, artist / song matches, keyboard navigation, suggestions
- ✅ Picking an artist in the dropdown opens the artist page
- ✅ Recent searches saved and shown with the item's cover
- ✅ Search shows each song once (the same song from YouTube Music, Deezer and SoundCloud is merged; other versions stay)
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
- ✅ A song that can't be fetched plays from another source at once (SoundCloud, another YouTube Music upload) instead of being skipped
- ✅ Audio version preferred over clips and VEVO uploads
- ⬜ Keep a short list of recent yt-dlp failures to skip bad routes faster

## 2b. Autoplay and taste

- ✅ Autoplay blends the radios of the last few songs chosen (not just the one playing), newest choice weighing most, so it follows the person's taste the way Spotify's does
- ✅ A style switch swings the blend within a couple of songs instead of staying locked to the first song of the session
- ✅ Skipping an autoplay song early (under 45 s) puts that artist's songs aside for 45 minutes; choosing it again (even from the queue) brings it straight back
- ✅ Several autoplay songs skipped in a row drops what autoplay queued ahead and blends again from the person's latest choice
- ✅ Taste (the last few choices) is saved and picked up again on the next run
- ✅ Developer check: `--debug-taste "song one|song two|..."` prints the blended autoplay for a given taste (`src/taste.rs`, unit tests included)

## 3. Music video

- ✅ Video-first playback with "Switch to audio" / "Switch to video"
- ✅ Official videos only (never lyric, audio, cover, karaoke or visualizer uploads), maximum 1080p
- ✅ Real upload title check (oEmbed) so official videos listed under a bare title are found
- ✅ Performance films (Acoustic Version, Live, Sessions) stand in when the song has no studio video; remixes and other cuts never match
- ✅ Every video pick verified against the song: artist named in the video's artist line, upload title or channel name, and the song's title present — a wrong artist's video can never show (unit tests in `ytm.rs`)
- ✅ Videos on the artist's own channel (UGC) found and shown, not only label (OMV) videos; a video played from search always shows its own picture
- ✅ A Spotify or Deezer library song is checked for a video too, against its YouTube Music match, the same verified way as a YouTube song — not only songs played directly from YouTube Music
- ✅ A different spelling of the same artist or title (VÆB / VAEB, a colon instead of a dash) still counts as a match; the artist and title are still both confirmed either way
- ✅ A video the person picked themselves (search, a related video, a playlist row) always shows that exact video; it is no longer re-run through the "is this official" check meant for deciding whether to attach a video automatically to a plain song
- ✅ A song's audio never waits for its video: the video is looked up fully in the background and only attaches its picture once found, so a song starts exactly as fast with the video setting on or off
- ✅ Adaptive quality, quality menu (Auto, 1080p … 240p), seamless quality switching
- ✅ Near-instant quality changes (all quality addresses from one lookup; the picture starts from the right piece of the file)
- ✅ Quality changes always land (the new picture takes over with its first frame); seeks and a hidden window no longer lower the quality
- ✅ Mini player, fullscreen, expanded Now-playing view
- ✅ Cover shown while the picture loads; no leftover frame from the previous song
- ✅ Lyrics offset for video audio
- ✅ No volume riding in the video version: the volume leveller was removed and the song keeps its own audio under the picture
- ✅ "Related music videos" over the picture; black bars in clips cropped; quality starts on Auto
- ⬜ The video's own, separately-mixed sound (gain-matched, `audio::engine::set_path_gain`) swapped in once found, without delaying the song's start — dropped for now so the video lookup never blocks playback; the song plays its own audio throughout in the meantime
- ⬜ Optional: studio sound under the picture when the video is the same cut as the song
- ⬜ Remember the quality choice per song

## 4. Accounts and library

- ✅ YouTube / Google sign-in through the app (fresh cookie file, removed on sign-out)
- ✅ Synced YouTube Music library (liked, playlists, albums, artists)
- ✅ Follow, like and save work signed in and signed out
- ✅ Browser sign-in on Windows, macOS and Linux: installed browsers detected, the default one pre-selected, the list opens on the welcome window, clear help when a browser keeps its sign-in locked
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
- ✅ Linux ARM64 packages (.deb, .rpm, AppImage, Flatpak) next to x86_64
- ✅ Update check and one-click update from GitHub releases (Settings and a notice card), keeping the library and sign-in
- ⬜ Update from inside a Flatpak without the host-access permission (today: through `flatpak-spawn`, else the system installer)
- ⬜ Test the Windows, macOS, .deb, .rpm and Flatpak update steps on real installs
- ⬜ Try the new browser detection and sign-in on real Windows 11 and macOS machines

## Next

1. ⬜ Test and polish the Windows and macOS builds
2. ⬜ Per-song video quality memory
3. ⬜ Follow sync to the YouTube account
4. ⬜ Crash investigation and long-run performance tests
5. ⬜ Faster fallback (remember sources that failed, try the next one sooner)
