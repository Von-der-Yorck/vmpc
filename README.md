# VMPC — Video · Media · Player · Converter

A lightweight VLC-style media player and universal media converter with a
clean, Claude-style UI in VLC orange, written entirely in **Rust**. Runs on **Windows** and **Linux**.

<img src="assets/vmpc-256.png" width="96" alt="VMPC logo">

## How it works

The layout follows the Claude app:

1. **Home page:** drop files or a folder anywhere, use *Open files* / *Open folder*, or paste a
   stream link into the box. Suggestions (*Convert to MP3*, *Make a GIF*, *Video → image sequence*,
   *Images → video*) open a file and preselect that conversion.
2. The file opens in the **preview**: videos play, images are shown, audio gets a level-reactive
   visual (or its cover art).
3. VMPC **detects what you opened**, and the slide-out **Convert** bar lists *everything* that fits,
   in one place, with the options for the selected target right below it:

   | Detected | Conversions offered |
   | --- | --- |
   | Video | Video formats · audio only (extract the soundtrack) · still image · **image sequence** (PNG/JPEG/WebP/BMP/TIFF/TGA/DPX/EXR frames) · **subtitles** (extract text tracks) · any other extension |
   | Audio | Audio formats · any other extension |
   | Image(s) | Image formats · **video from these images** (also from a single image) · any other extension |
   | Subtitles | SRT · WebVTT · ASS · SSA · TTML · LRC, with a timing shift |
   | Archive | Extract · repack as ZIP, 7Z, TAR.GZ, TAR.XZ, TAR.ZST, TAR.BZ2, TAR, ISO · play the media inside |
   | JSON / YAML / TOML / CSV / TSV | Convert into each other |

   Opening a folder of images preselects *video from N images* automatically.
4. The **sidebar** (click the logo or press Ctrl+B to collapse it) has the open actions, your
   library of loaded files, the conversion progress and Settings.

## Automatic updates

At every start VMPC asks GitHub whether a newer release exists. If so, it offers to update: it
downloads the installer for your system, verifies it against the release's `SHA256SUMS.txt`,
installs it silently and restarts. Without an internet connection the check is skipped silently,
with no "please connect" messages. Turn it off or check manually in **Settings → Updates**.

- Installed with the Windows setup (per user or for all users) or with `install-linux.sh`:
  updates install themselves.
- Portable zip / `.deb`: VMPC opens the release page instead.

## File types

VMPC knows **1,708 file types**: nearly the whole of Wikipedia's
[List of file formats](https://en.wikipedia.org/wiki/List_of_file_formats) plus common media,
subtitle, archive and data formats the list misses. Open **Supported formats** in the sidebar
to search them all and see what VMPC does with each:

| What | Formats (examples) | VMPC |
| --- | --- | --- |
| Video, audio, images | MP4, MKV, MOV, AVI, WebM, MXF, VOB, MP3, FLAC, Opus, DTS, PNG, JPEG, AVIF, JPEG XL, EXR, PSD… | Play / view and convert (86 output formats) |
| Subtitles | SRT, ASS, SSA, VTT, SUB, LRC, SBV, TTML, SMI… | View, convert, shift timing, extract from videos |
| Playlists | M3U, M3U8, PLS, XSPF, ASX, WPL | Load every entry into the library (HLS `.m3u8` streams play directly) |
| Archives | ZIP, 7Z, RAR, TAR, GZ, BZ2, XZ, ZST, ISO, CAB, CPIO, XAR, LHA, DEB, RPM, CBZ/CBR… | Browse, extract, repack, play media inside |
| Text and data | Source code, logs, Markdown, HTML, XML, INI; JSON, YAML, TOML, CSV, TSV | Text viewer; data formats convert into each other |
| Everything else | Documents, spreadsheets, CAD, 3D, databases, executables, fonts, VM images, game data… | Recognised by name and category, opened with the default app |

Output formats depend on the FFmpeg build: VMPC asks the engine which encoders it has and only
offers what it can really write (the bundled build has all but AMR-WB, Speex and GSM).
Archives use libarchive's `bsdtar`, built into Windows 10/11; on Linux install
`libarchive-tools` (the `.deb` recommends it).

## Features

### ▶ Player
- Opens practically every audio, video and image format: MP4, MKV, AVI, MOV, WebM, FLV, TS, WMV, MPEG,
  MP3, FLAC, WAV, OGG, Opus, AAC, M4A, PNG, JPG, GIF, WebP, TIFF and hundreds more.
- **Network streams**: http(s), HLS (`.m3u8`), RTSP, RTMP (logo menu → *Open network stream*).
- Drag & drop files or whole folders anywhere on the window, or pass files on the command line
  (`vmpc movie.mkv song.flac`).
- Library with next/previous, loop off / all / one, auto-advance.
- Seek bar, frame stepping, speed 0.25×–4× (pitch-corrected), volume / mute, fullscreen with
  auto-hiding controls, snapshot (full-resolution PNG to your *Pictures* folder).
- Audio-clock A/V sync; if a slow PC can't keep up, the video re-syncs automatically
  instead of drifting.
- Hardware-accelerated decoding (on by default), adjustable render size (720p … 4K) in Settings.

### ⇄ Convert
- Converts the file in the preview, or every loaded file of the same type at once.
- **Video:** MP4 (H.264 / H.265), MKV (H.264 / AV1 / FFV1 lossless), WebM (VP9), MOV (ProRes),
  AVI, MPEG-2, WMV, MPEG-TS, animated GIF (palette-optimised), animated WebP.
- **Audio:** MP3, M4A (AAC / ALAC), Opus, OGG Vorbis, FLAC, WAV, AIFF, WMA.
- **Image:** PNG, JPEG, WebP, BMP, TIFF (also grabs a frame from a video, or a song's cover art).
- **Other:** type any extension FFmpeg can write (`3gp`, `ogv`, `ac3`, `apng`, …).
- Quality slider (mapped to CRF / q-scale / bitrate per codec), resolution (never upscales),
  audio bitrate, trim start/end, **reverse** (play backwards), remove audio, output folder.
- Jobs run one after another in the background with progress, ETA, speed and cancel.

### ▦ Frames: video ⇄ image sequence (like `ffmpeg -vf fps=N`)
- **Video → images:** choose frames per second (0.01–240, or *every original frame*), format
  (PNG, JPEG, WebP, BMP, TIFF), quality, resolution, time range and file-name prefix.
  **Backwards** option numbers the frames in reverse (last frame = `000001`).
- **Images → video:** open a folder or several images (sorted naturally: `img2` before `img10`),
  set fps, output MP4 / WebM / MOV ProRes 4444 (keeps alpha) / FFV1 / GIF / animated WebP / H.265,
  quality, **Backwards** (reverse order) and **Boomerang** (forward then backward).
  Mixed image sizes are letterboxed automatically.

### Design
- Calm, flat design inspired by the Claude app (warm neutral surfaces, hairline borders, serif
  headlines in Source Serif 4, UI text in Inter) with **VLC orange** accents; matching light and dark themes
  (Settings → Appearance).
- Subtle animation: the logo's convert ring spins on hover, the sidebar and Convert bar
  slide, selections glide and media fades in.
  Animations only run while something moves, so an idle VMPC uses no CPU.
- All icons and the logo are drawn by VMPC itself.
- Logo: a macOS Big Sur–style "squircle" app icon in VLC orange, with a play symbol inside a
  circular *convert* arrow. (Apple's and VLC's own logos are trademarks, so VMPC uses its own
  mark in that style.) `vmpc --export-icon <dir>` writes it as PNG/ICO.

## Keyboard shortcuts

| Key | Action |
| --- | --- |
| Space / K | Play / pause |
| ← / → | Seek 5 s (Shift = 1 s, Ctrl = 30 s) |
| ↑ / ↓ | Volume |
| M | Mute |
| F / F11, Esc | Fullscreen, leave fullscreen (double-click the video also toggles) |
| N / P | Next / previous file |
| E | Next frame |
| S | Snapshot |
| L | Loop off / all / one |
| + / − | Speed |
| T | Show / hide the Convert bar |
| Ctrl+O | Open files |
| Ctrl+B | Collapse / expand the sidebar |

## Install

Download from the [Releases](../../releases/latest) page:

| Platform | File | Notes |
| --- | --- | --- |
| Windows 10/11 | `VMPC-Setup-<version>-x64.exe` | Full installer, works offline (recommended) |
| Windows 10/11 | `VMPC-WebSetup-<version>-x64.exe` | 2 MB web installer, downloads the app during setup |
| Windows 10/11 | `VMPC-<version>-windows-x64-portable.zip` | No installation: unzip, run `vmpc.exe` |
| Linux x86_64 | `install-linux.sh` | One-line web install (below) |
| Debian/Ubuntu | `vmpc_<version>-1_amd64.deb` | `sudo apt install ./vmpc_*.deb` |
| Linux x86_64 | `vmpc-linux-x86_64.tar.gz` | Unpack, run `./vmpc/install-linux.sh` |

Linux one-line install (replace `OWNER/REPO` with this repository):

```bash
curl -fsSL https://github.com/OWNER/REPO/releases/latest/download/install-linux.sh | sh
```

The Windows installers install per user without admin rights (or for all users), add Start-menu
and desktop shortcuts, add VMPC to *Open with* for media files and register a normal uninstaller
(*Settings → Apps*). The web installer verifies every download with SHA-256.

### The FFmpeg engine

VMPC runs the **FFmpeg** tools (`ffmpeg` + `ffprobe`) as its codec engine — the same codec
library family VLC is built on. The installers, the portable zip and the Linux tarball ship it,
so nothing else is needed. VMPC looks for it in this order:

1. the folder in the environment variable `VMPC_FFMPEG_DIR`
2. `ffmpeg/bin/`, `ffmpeg/` or the folder next to `vmpc(.exe)` (where the installers put it)
3. its own download folder (`%LOCALAPPDATA%\VMPC\ffmpeg`, `~/.local/share/vmpc/ffmpeg`)
4. your `PATH` (plus winget's install folder on Windows)

If none is found, VMPC offers **Download engine automatically** (≈100 MB, in the background).

Preferences are saved to `%APPDATA%\VMPC\settings.json` / `~/.config/vmpc/settings.json`.

## Building

You need Rust (https://rustup.rs).

### Windows

```bash
powershell -ExecutionPolicy Bypass -File scripts/build-windows.ps1
```

Result: `dist\vmpc.exe`. Add `-BundleFFmpeg` to also copy ffmpeg/ffprobe into `dist\ffmpeg\`
so the folder runs on any PC without installing anything.

Toolchain notes:
- **MSVC toolchain** (Rust default) needs the *Visual Studio Build Tools* (C++ workload).
- **GNU toolchain** (`rustup default stable-x86_64-pc-windows-gnu`) needs MinGW-w64 binutils
  on `PATH` (e.g. `winget install BrechtSanders.WinLibs.POSIX.UCRT`) for `dlltool`/`as`.

### Linux

```bash
sudo apt install build-essential pkg-config libasound2-dev ffmpeg
```

```bash
sh scripts/build-linux.sh
```

This builds and installs `~/.local/bin/vmpc` with an app-menu entry and icon
(see the script for Fedora/Arch package names). Runs on X11 and Wayland.

### Tests

```bash
cargo test --release
```

The integration tests generate a sample clip with FFmpeg and run **every conversion preset**,
trim/reverse/downscale, frame extraction (5 fps JPEG reversed, every-frame PNG) and image
sequence → MP4 / reversed MP4 / boomerang GIF / ProRes through the real job queue.
`cargo test --release -- --ignored` additionally tests the in-app engine download (network).

### Installers (local)

With [Inno Setup 6](https://jrsoftware.org/isinfo.php) installed and `dist\` prepared
(`build-windows.ps1 -BundleFFmpeg`):

```bash
iscc /DMyAppVersion=1.0.0 installer\vmpc.iss
```

The web installer needs the files of a published release; see `installer/vmpc.iss` and the
release workflow for the parameters.

## Publishing a release (GitHub Actions)

Everything is automated in `.github/workflows/release.yml`:

1. Set `version` in `Cargo.toml` (e.g. `1.0.1`) and commit.
2. Tag and push:

   ```bash
   git tag v1.0.1
   ```

   ```bash
   git push origin main --tags
   ```

3. GitHub builds and tests on Windows and Linux, downloads FFmpeg, builds both Windows
   installers, the portable zip, the `.deb`, the Linux tarball and `install-linux.sh`, and
   publishes them all as a Release with `SHA256SUMS.txt` (~15 minutes; watch the *Actions* tab).

The repository must be **public** so the web installers can download the release files.
`.github/workflows/ci.yml` builds and tests every push and pull request.

## Architecture

```
src/
  main.rs        window setup, app icon, --export-icon
  app.rs         shell: sidebar, home composer, preview stage, controls, Convert bar,
                 dialogs (settings, update), library, shortcuts, drag & drop
  player.rs      playback engine: ffmpeg → raw RGBA pipe (zero-copy into the texture),
                 ffmpeg → f32 PCM → cpal audio output, audio-master clock, seeking, resync
  converter.rs   conversion targets per media type, format options, queue list
  frames.rs      video → image sequence, image(s) → video (fps or seconds per image)
  formats.rs     format catalogue, quality → encoder argument mapping
  jobs.rs        background job queue (one ffmpeg at a time, -progress parsing, cancel)
  ffmpeg.rs      locating/downloading ffmpeg + ffprobe, probing, single-frame grabs
  settings.rs    saved preferences (JSON in the user config folder)
  updater.rs     GitHub release check, verified download, silent install + restart
  theme.rs       design system (tokens, fonts, buttons, sliders, toggles, segmented…)
  icons.rs       vector icons, logo (vector + software rasteriser), PNG/ICO writer
  util.rs        time parsing, natural sort, output paths
  catalog.rs     1,708-format catalogue, file classification (media/subtitle/playlist/archive/text)
  playlist.rs    M3U/PLS/XSPF/ASX/WPL parsing
  archive.rs     list / extract / repack / play-inside via libarchive bsdtar
  data.rs        JSON / YAML / TOML / CSV / TSV conversion
installer/vmpc.iss   Windows installer (full + web) for Inno Setup
scripts/             build scripts and the Linux installer
.github/workflows/   CI and automated releases
```

Performance: idle CPU ≈ 0 %. The UI only repaints when a video frame is due; 720p30 playback
uses roughly a quarter of one CPU core for VMPC plus the FFmpeg decoder.

## Limitations

- Documents, spreadsheets, CAD, 3D models, databases and executables are recognised but not
  converted: that needs their own applications (e.g. LibreOffice for documents).
- DRM-protected streams (Netflix, Spotify…) can't be played. Web pages such as YouTube links
  aren't media URLs: VMPC needs the direct stream URL.
- Subtitles are not rendered during playback yet (they're kept by the *Custom* converter).
- *Reverse* conversion buffers the whole clip in memory — trim long videos first.
