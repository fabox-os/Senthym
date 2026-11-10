# Changelog

All notable changes to Senthym. Newest first within each group.

## 1.1 — 2026-11-10

### Added
- Linux packages for **ARM64** (aarch64) next to the x86_64 ones: `.deb`, `.rpm`, AppImage and Flatpak, built on ARM runners; the updater picks the package that matches the computer.
- Updates from inside Senthym: **Settings → Updates → Check for updates**, and a notice card ("Senthym X is available" with Update / Later) when a newer release is on https://github.com/fabox-os/Senthym/releases, checked when the app starts (can be turned off). It downloads the package that matches how this copy was installed (Flatpak, AppImage, .deb, .rpm, Windows .exe, macOS .dmg), checks it against the checksum GitHub publishes for it, installs it, closes Senthym and opens the new version. Your library, sign-in and settings live in the data folder, not in the program, so the new version reads them as they were; a copy of `library.json` is kept as `library.json.before-update` first. Windows swaps the .exe in place, macOS copies the app over the old one, the AppImage replaces itself, .deb / .rpm ask for your password (polkit) and Flatpak installs through the host's `flatpak`; when a kind can't be installed automatically the package is opened with the system installer instead.
- Autoplay now follows the person's taste, the way Spotify's does: when the queue runs out (or when "Next" is pressed enough to show a direction was wrong), the songs added are blended from the radios around the last few songs the person actually chose — not just the one playing — newest choice weighing most. Listening to Kygo keeps tropical house coming; switching to rock swings the blend to rock within a couple of songs, without throwing the earlier taste away outright. Skipping an autoplay song in its first 45 seconds puts that artist aside for 45 minutes; choosing it again brings it straight back. The last few choices are saved, so autoplay picks up the same taste next time the app opens. A new developer check, `--debug-taste "song one|song two|..."`, prints the blend for a given taste.
- A song with no studio music video now shows the artist's official performance film instead (a live, acoustic or sessions clip, like Kygo & Sigrid's "The Feeling (Live From The Troll's Tongue)"), so more songs have a video version. Official music videos still come first, and remixes and other songs never stand in.
- Playing a video from search (for example a live film on the artist's own channel) now always shows that video; before, videos published on artist channels instead of by the label were wrongly treated as audio-only.
- When a song can't be fetched from its source, Senthym tries its other sources at once instead of skipping it: the song's copy on SoundCloud, another upload of it on YouTube Music, and the full download, all at the same time; the first that works is played and a short message says where it came from. A SoundCloud link that won't play falls back to its YouTube Music match, and Spotify / Deezer songs with no playable match fall back to SoundCloud.
- Search results no longer list the same song twice: results from YouTube Music, Deezer and SoundCloud are merged when title, main artist and length (within 3 seconds) agree. Other versions (remix, live, acoustic, covers) stay.
- Screenshots (Home with video, Browse all, synced lyrics) in the README.
- Developer checks: `--debug-update-check <kind>` (with `SENTHYM_FAKE_VERSION`, `--download`), `--test-update`, `--debug-loudness`, `--debug-fallback`, `--debug-dedupe`, and `--play-seconds` / `--test-quality` for testing quality changes and seeks while a song plays.

### Changed
- The quality menu shows a change in progress (for example "480p → 1080p…").
- The welcome window's heading is now "Your Music. Your Freedom."
- Video version volume: the volume leveller that adjusted the level while a song played is gone (it made the sound swell and dip). A song now keeps its own audio under the picture, so its volume never moves with the video's mix. A loudness-matching helper (`audio::loudness`, one fixed gain per file, checked with `--debug-loudness`) is kept for when the video's own sound is brought back.

### Fixed
- A song no longer waits for its video before it starts playing: the video check used to run before the song's own audio was considered ready (joined on, even though the two looked independent), so the heavier the check got over today's fixes, the longer every song took to start. The audio now always starts the moment it is ready; the video is looked up fully in the background and only attaches its picture once found, never delaying playback. The trade-off: the video's own distinct soundtrack is no longer swapped in automatically (a song now keeps playing its own audio once the picture appears, rather than the video's separately mixed one) — a deliberate choice for speed; "Switch to video" from a video someone picked explicitly is unaffected.
- A person's own, explicit pick of a video — from search, a related video, a playlist row — now always shows that exact video. Before, picking one still ran it through the "is this official" check meant for deciding whether to attach a video automatically to a plain song, which rejected anything not literally labelled an official video (a lyric video, an "(Audio)" upload, most re-uploads) and silently played audio only instead of the video picked.
- A Spotify or Deezer library song now gets the same video treatment as a YouTube song: before, only songs played directly from YouTube Music were ever checked for an official video or performance film — a Spotify song's YouTube Music match was never cross-checked for one at all, so it could only ever play as audio, video or not. The check now runs against whichever catalogue the song plays from, with the same artist-and-title verification either way, and the video's own sound plays with its picture exactly as it does for a YouTube song.
- A video or channel name spelled with a different letter (VÆB / VAEB, Björk / Bjork, and the like) is now recognised as the same artist, and "Artist: Title" uploads (a colon, not a dash) are recognised the same as "Artist - Title" ones — both only widen what counts as the same name or title; the artist and title must still both be confirmed, so a wrong video still never shows.
- Automatic radio can no longer repeat music that is already in the queue: besides the exact id, the same recording under another upload or source (same title, artist and length) is now skipped too, as are songs the radio list itself carries twice.
- The video lookup got cheaper: list data decides the artist and title checks when it can, YouTube's upload title is only asked for when it can't (once per video, remembered), and at most six label videos and six channel videos are examined per song.
- The picture never fast-forwards to catch up with the song any more — neither after "Switch to video", nor after seeking forwards, nor when the picture starts while the song is already playing. Frames behind the playing position are dropped instead of shown, a forward seek starts the picture straight at the new position, and a picture started while the song is still loading now begins where playback will continue instead of at 0:00. The picture simply appears at the part that is playing.
- A song can no longer get another artist's video or a third-party re-upload: every video pick must now name one of the song's credited artists in its artist line, upload title or channel name, and carry the song's title; a video whose artist can't be confirmed is never shown. Unit tests pin these rules down.
- Changing video quality is now near-instant (about 0.1 to 0.4 s instead of 2 to 8 s): one lookup now returns the picture addresses of all qualities, and the picture starts by fetching the right piece of the file directly instead of letting the decoder open the address and seek.
- Video quality changes never took effect while a picture was showing: the picture at the new quality was started but never took over, and it then blocked every later change. This is why the video stayed blurry in the Now Playing view and in full screen, and why picking a quality in the menu often did nothing. The new picture now takes over with its first frame (about 2 to 4 seconds), is retried up to three times, and a change that can't start no longer blocks the next one.
- Seeking forwards, or coming back to a hidden window, was mistaken for a slow connection and lowered the Auto quality by one step for 30 seconds. The picture is now picked up at the same quality.
- Windows: the video decoder downloaded on first start is now picked up without restarting the app, and checking for it no longer flashes a console window.

### Known limitations
- The update steps for Windows, macOS, .deb, .rpm and Flatpak have not been tried on real installs (the AppImage one was); Flatpak updates need the host's `flatpak` and otherwise open the system installer.
- Windows and macOS builds are made by CI and get less testing than Linux (the 1.0 limits about the macOS app not being notarized and the browser sign-in import being Linux-only still apply).
- Deezer can't supply the sound for a fallback (it only gives 30-second previews without an account), and SoundCloud refuses many label uploads (DRM), so the fallback often ends on another YouTube Music upload.
- The fallback takes a few seconds (it has to find and download the other copy).

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
