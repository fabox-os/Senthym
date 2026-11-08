# Changelog

All notable changes to Senthym. Newest first within each group.

## 1.0 — 2026-11-08

First release.

### Added
- Packages: `.deb`, `.rpm`, AppImage and Flatpak for Linux, a single `.exe` for Windows and a universal `.dmg` for macOS, built by GitHub Actions; Windows and macOS support in the code.
- Automatic volume leveller for music-video sound (slow, so beats never pump the volume).
- "Related music videos" laid over the bottom of the Now Playing picture, with the picture running behind it.
- Black bars baked into a clip are cropped away so the picture fills the Now Playing view.
- Hover card in the collapsed library rail (name and kind on one line each).
- "Hide in this playlist" in the song menu (and "Show in this playlist"): the song stays in the list, greyed out, with a red − on hover to show it again; playback skips it (next, previous, end of song, shuffle, and when starting the playlist). An "Hidden in <playlist>. Undo" message appears after hiding. Saved per playlist; available in every playlist, Liked Songs and YouTube Music liked songs included.
- New app icon: the Senthym logo (`assets/icon*.png`) is compiled into the program as the window / taskbar icon; a launcher entry (`senthym.desktop`) and icons were installed in `~/.local/share` so the desktop shows it.
- Project renamed to Senthym everywhere: crate and binary (`senthym`), window title and app id, texts, log tag,
  `SENTHYM_HOME`, data folder `~/.local/share/senthym`, cache `~/.cache/senthym`, new downloads folder
  `~/Music/Senthym`. On the first start the data and cache folders of the earlier name are moved over (saved paths
  inside the library, such as playlist covers, are rewritten). Existing downloads and the download folder
  already chosen are untouched.
- Follow / Following button for every artist in a song's Credits (replaces the external-link icon). The main
  artist uses the known artist page; other credits are looked up by name on first click.
- `crash.log` in `~/.local/share/senthym`: every panic is written with location and backtrace.
- Picture memory budget (about 450 MB) on top of the 700-picture limit.
- Real upload-title check (YouTube oEmbed) for videos listed under a bare song title.
- Roadmap (`ROADMAP.md`) and this changelog.

### Changed
- Video quality starts on Auto; a fixed quality is only used (and saved) when you pick it in the menu.
- Settings: the music-source row is called "Senthym Music Library"; the streaming-engine row and the third-party tool's name are no longer shown.
- Accent colour changed from green to baby blue (#89CFF0): active labels and options, the playing song's title and equalizer, the liked check, the avatar when selected, selection colours and the Local files tile.
- Custom window buttons (minimize, maximize / restore, close) replace the system frame; the empty top bar drags the window and the edges resize it.
- Library lists followed artists and saved albums / playlists right after your own playlists; synced
  YouTube Music items come after and skip anything already followed or saved.
- Performance films (acoustic, live, session, performance, unplugged, stripped, piano version) now count as
  official videos.
- Song-title plus icon in the player bar is smaller (radius 9.5 → 7); song-row plus unchanged.
- Debug helpers: `--debug-audio` also prints video candidates; `--debug-search` prints radio covers.

### Fixed
- Videos dropping to 240p on fast connections: a slow start was mistaken for a slow connection.
- Video decoding now runs at lower priority and limited threads, so it cannot make the sound stutter.
- Related-video hover card wrapped one word per line.
- Video that can't keep up now really restarts at the audio position (one quality lower when automatic) and stays there for a while; the code that did this was never being called.
- Build is now free of warnings: explicit number types in drawing code, unused imports, variables, old helpers and the unused sleep-timer action removed.
- Followed artists missing from the library when signed in to YouTube (they were listed after hundreds of
  synced items).
- Songs with an official video but a bare title (for example "Chasing Paradise") showed only the cover.
- Studio song matching the acoustic film of the same name: a different cut never matches now.
- Leftover video frame or cover from the previous song: video shows only for the song that is playing and is
  dropped when the song's video changes.
- Pause not sticking: Space was toggled twice when a clicked button kept keyboard focus; now consumed once.
- Possible panic in title cleanup on non-English titles (ASCII-only lowercasing keeps byte positions valid).

### Known limitations
- Windows and macOS builds are made by CI and get less testing than Linux.
- macOS app is not notarized (first launch: right-click, Open).
- Follow is stored in Senthym only; it does not subscribe on your online account yet.
- Importing a sign-in from an installed browser only finds Linux browsers; sign in through the app elsewhere.
- One silent crash seen once on Linux was never reproduced (panics are logged to `crash.log`).
- Linux packages are for x86_64 only.

## Earlier work in this project

### Interface
- Rebuilt Home, search, library, artist, album, playlist and player screens after the Spotify desktop UI.
- Library: collapsible rail, full grid view, expand button, "+ Create" menu (centred), filters, search.
- Playlists: created playlists appear first in the library; Edit details dialog (name, description, cover
  picture with file picker), centred on screen; cover shown in rail, grid and page header.
- Search: pill box with Browse button, dropdown with recent searches (covers), artist and song matches,
  keyboard navigation; picking an artist opens the artist page; recents saved and resolved to covers.
- Browse all: tilted covers behind genre tiles.
- Now-playing right panel: cover or video, related music videos with arrows, "In this video", About the
  artist, Credits; expanded Now-playing page with video filling the view.
- Artist page: face-cropped banner using larger thumbnails, "⋯" menu (Unfollow, Don't play this artist,
  artist radio, Report, Share with Copy link and Embed), radio icon removed.
- Playlist rows: hover-only grey, equalizer on the playing row, "⋯" menu; row overlap and double-highlight
  fixed with a child layout and half-open hover test.
- Playing playlist highlighted in the rail with equalizer.
- Lyrics button toggles back to the previous page.
- Heart replaced by a circled plus (green check when liked).
- Removed library icon (top right), sleep timer and the arrow next to the volume; right-side icons moved in.

### Playback
- Fast YouTube path (yt-dlp direct address + parallel 1 MB ranged downloads), plain-first with signed-in
  fallback, raced routes, measured throughput.
- Prefetch of hovered, visible, next and top-result songs; searches and playlists open instantly.
- Audio version (ATV) preferred over music videos and uploads.
- YouTube sign-in fixes: JS runtime for yt-dlp, fresh cookie file instead of an empty one, file removed on
  sign-out.

### Music video
- Video-first playback with audio switch, hover controls, quality menu (Auto to 240p, max 1080p), HD badge,
  seamless quality switching, mini player, fullscreen (seek bar no longer overlaps the end time).
- Video no longer freezes when opening the expanded view; faster first picture with cached addresses.
- Official video clip chosen over lyric videos (for example Firestone); lyrics offset for video audio.

### Security and performance
- Checksum-verified yt-dlp updates, temp cleanup, https-only ffmpeg input, size caps, shared HTTP agent,
  background image worker queue with LRU textures.
- Search, results, play and next made as instant as the sources allow.
