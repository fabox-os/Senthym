# Senthym

**Your Music. Your Freedom.**  ·  Version 1.1

> **Platforms:** Senthym was designed and tested on **Linux (Ubuntu 26.04.1 LTS)**. It also works on
> **Windows** and **macOS**, but those versions get less testing. If you find a bug on Windows or macOS,
> please report it by **opening an issue**.

Senthym is a desktop music player for Linux, Windows and macOS. It is written in Rust and draws its own fast, modern
interface with [egui](https://github.com/emilk/egui). You search for any song, artist, album or playlist and
it plays right away, with no ads, no subscription and no tracking. It also plays your own music files, shows
synced lyrics and, when a song has an official music video, plays the video too.

Senthym uses a **Spotify-style interface** on purpose, so it is simple to use from the first minute: a
library on the left, the song bar at the bottom, the Now Playing panel on the right. If you have used
Spotify, you already know where everything is. It has its own dark look with a baby-blue accent (`#89CFF0`).

---

## Screenshots

**Home and Now Playing with the music video**

![Home and Now Playing with the music video](docs/screenshots/home-and-video.webp)

**Browse all**

![Browse all](docs/screenshots/browse-all.webp)

**Synced lyrics**

![Synced lyrics](docs/screenshots/lyrics.webp)

---

## How it works

Senthym comes **already set up with YouTube Music**. Open the app, type a song and press play. There is
nothing to configure, no account to create and no key to enter.

- **YouTube Music** is the main source: search, albums, artists, playlists, radio and related music videos.
- **Deezer** and **SoundCloud** are searched alongside it, so you also find songs, remixes and tracks that
  only exist there. Their results appear together with the others.
- When you press play, Senthym's built-in streaming engine fetches the song and plays it through its own
  audio player, which is why crossfade, volume, seeking and lyrics work the same for every source.
- If the song has an official music video, Senthym finds it and plays the picture in sync with the sound; you
  can switch between the video and the audio version at any time.

You can also sign in to bring in your own library, and optionally connect Spotify (see **Music sources**
below).

---

## What it does

### Listen to anything
- **Search** for songs, artists, albums and playlists, with suggestions as you type, recent searches with
  covers, and a Browse page of genres and moods.
- **Instant playback.** Songs start fast: Senthym looks ahead at what you hover over, what is on screen and
  what plays next, and gets it ready before you press play.
- **Your own files.** Play music from your computer alongside the online library.
- **Downloads.** Keep songs on your computer to play offline.
- **Queue, shuffle, repeat and history**, plus automatic radio when your queue runs out — it follows your taste as it shifts, blending songs from your last few choices (newest weighing most), the way Spotify's autoplay does.

### Music videos
- Songs with an official music video play with the picture, in the side panel or in the full Now Playing
  view, in mini-player mode or full screen.
- **Quality menu:** Auto (follows your connection and the size of the view) or a fixed 240p up to 1080p.
  Your choice is remembered.
- One click on **Switch to audio** (or **Switch to video**) changes version at the same moment of the song.
- **Related music videos** appear over the bottom of the Now Playing view.
- The song keeps its own audio under the picture, so switching between video and audio doesn't change
  the volume.

### Lyrics
- Synced, line-by-line lyrics that follow the song, with translation to your language.
- Lyrics stay in time with video versions, even when the video starts with a scene before the music.

### Your library
- **Playlists** you create, with your own name, description and cover picture.
- **Liked Songs**, followed artists and saved albums and playlists, all in one library, as a narrow rail or a
  full grid with filters and search.
- **Follow / Following** buttons for every artist credited on a song.
- **Hide in this playlist:** a song stays in the list, greyed out, but is skipped when it is time to play.
  Works in every playlist, Liked Songs included, with an Undo.
- Optional sign-in to bring in your account's playlists, liked songs and artists. Senthym also works fully
  without any account.

### Sound
- Crossfade between songs with an adjustable length.
- Volume, seek and keyboard shortcuts.

### The window
- A modern custom title bar with minimize, maximize and close buttons; drag the empty top bar to move the
  window and drag the edges to resize it.
- Now Playing panel with cover or video, "In this video", About the artist and Credits.

---

## Music sources

Senthym brings several services together in one place, and **none of them needs an API key**:

- **YouTube Music**
- **SoundCloud**
- **Deezer**

Search, albums, artists and playlists work out of the box. You don't have to create an account or register
anything.

**Spotify (optional):** if you want your Spotify library inside Senthym too, you can set up your own Spotify
API key (a client ID from the Spotify developer dashboard) in **Settings → Accounts → Spotify (optional)**. It is entirely optional;
everything else works without it.

---

## Updates

Senthym tells you when a new release is out and updates itself from
[the Releases page](https://github.com/fabox-os/Senthym/releases). Open **Settings → Updates** and press
**Check for updates**, or use the notice that appears when you start the app (it can be turned off in the
same place).

It downloads the package that matches how you installed Senthym (Flatpak, AppImage, `.deb`, `.rpm`, Windows
`.exe` or macOS `.dmg`), checks it against the checksum GitHub publishes, installs it, closes the app and
opens the new version. Your library, playlists, sign-in and settings stay as they were: they live in your
data folder, not in the program. Senthym keeps a copy of your library before every update.

---

## Keyboard shortcuts

| Keys | Action |
| --- | --- |
| Space | Play / pause |
| Ctrl + → / Ctrl + ← | Next / previous song |
| Ctrl + F or Ctrl + L | Focus the search box |
| Alt + ← / Alt + → | Back / forward |

---

## Download

Ready-made packages are on the [Releases page](https://github.com/fabox-os/Senthym/releases):

| System | Package |
| --- | --- |
| Debian / Ubuntu (x86_64) | `Senthym-linux-amd64.deb` |
| Debian / Ubuntu (ARM64) | `Senthym-linux-arm64.deb` |
| Fedora / openSUSE (x86_64) | `Senthym-linux-x86_64.rpm` (install `ffmpeg` too) |
| Fedora / openSUSE (ARM64) | `Senthym-linux-aarch64.rpm` (install `ffmpeg` too) |
| Any Linux (x86_64 / ARM64) | `Senthym-linux-x86_64.AppImage` / `Senthym-linux-aarch64.AppImage` (ffmpeg included) |
| Any Linux (x86_64 / ARM64) | `Senthym-linux-x86_64.flatpak` / `Senthym-linux-aarch64.flatpak` (ffmpeg included) |
| Windows | `Senthym-windows.exe` |
| macOS (Apple silicon + Intel) | `Senthym-macos-universal.dmg` |

--- | --- |
| Debian / Ubuntu | `Senthym-linux-amd64.deb` |
| Fedora / openSUSE | `Senthym-linux-x86_64.rpm` (install `ffmpeg` too) |
| Any Linux | `Senthym-linux-x86_64.AppImage` (ffmpeg included) |
| Any Linux | `Senthym-linux.flatpak` (ffmpeg included) |
| Windows | `Senthym-windows.exe` |
| macOS (Apple silicon + Intel) | `Senthym-macos-universal.dmg` |

---

## Building and running

Senthym runs on **Linux** (Wayland and X11), **Windows** and **macOS**.

- **Windows:** one file, `senthym.exe`. Double-click it. The first time, it fetches the video decoder
  (ffmpeg) by itself, checked against its published checksum.
- **macOS:** one universal app (`Senthym.app` in a `.dmg`) that runs natively on Apple silicon and Intel
  Macs. It is not notarized by Apple, so the first time use right-click → Open.
- **Linux:** build from source below.

The Windows and macOS packages are built by the GitHub Actions workflow in `.github/workflows/build.yml`
(run it from the Actions tab, or push a `v*` tag), and the results are downloadable from the run.

**To build from source you need:**

- Rust (stable) and Cargo
- `ffmpeg` on your `PATH` (Linux only; it decodes the music-video picture)
- Working sound output (ALSA, PulseAudio or PipeWire)

```bash
cd app
cargo build --release
./target/release/senthym
```

The first time you play an online song, Senthym sets up its streaming engine on its own and keeps it up to
date; you don't have to install anything else.

**Handy options for testing:**

```bash
# Draw the interface to a PNG and exit (pages: search, library, player, lyrics, queue, settings, liked, ...)
./target/release/senthym --screenshot out.png --page library

# Use a separate data folder, for example a copy of your library
SENTHYM_HOME=/tmp/senthym-test ./target/release/senthym
```

---

## Where your data lives

| What | Where |
| --- | --- |
| Library, playlists, settings | `~/.local/share/senthym` |
| Song and picture cache | `~/.cache/senthym` |
| Downloads | `~/Music/Senthym` (changeable in Settings) |
| Crash report (only if the app panics) | `~/.local/share/senthym/crash.log` |

If you used an earlier version under another name, your library and cache are moved over automatically the
first time Senthym starts.

---

## Privacy and safety

- No ads, no analytics, no tracking.
- Your library, settings and sign-in files stay on your computer.
- Downloads of the streaming engine are checked with a SHA-256 checksum before use.
- The video decoder only reads secure (`https`) addresses, and picture and memory sizes are capped.
- If something does go wrong, a crash report is written locally so it can be fixed.

---

## Project layout

```
app/
  src/main.rs         start-up, window, icon, crash log, screenshot mode
  src/app.rs          application state, actions and background-work messages
  src/store.rs        saved library and settings, data folders and migration
  src/audio/          audio engine (decks, crossfade, leveller) and decoding
  src/video.rs        music-video picture pipeline
  src/ui/             screens, widgets, player bar, video views
  src/images.rs       cover pictures, cache and memory limit
  assets/             app icons
  ROADMAP.md          what is done and what is planned
  CHANGELOG.md        what changed
```

---

## Roadmap

See [ROADMAP.md](ROADMAP.md) for finished work and what comes next (packaging as AppImage / .deb, per-song
video quality, account follow sync, long-run performance tests) and [CHANGELOG.md](CHANGELOG.md) for the
history of changes.

---

## License

Senthym is free software under the [GNU GPL v3](LICENSE).
