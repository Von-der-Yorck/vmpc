# Third-party notices

## FFmpeg

VMPC uses **FFmpeg** (https://ffmpeg.org) as its codec engine. FFmpeg is run as a separate
program (`ffmpeg` / `ffprobe`); it is not linked into VMPC.

The installers and the in-app downloader ship the static Windows/Linux builds published at
https://github.com/BtbN/FFmpeg-Builds ("gpl" variant). Those builds are licensed under the
**GNU General Public License version 3** (https://www.gnu.org/licenses/gpl-3.0.html) because they
include GPL components such as x264 and x265.

- FFmpeg source code: https://git.ffmpeg.org/ffmpeg.git (mirror: https://github.com/FFmpeg/FFmpeg)
- Build scripts and the exact source revisions of the bundled build:
  https://github.com/BtbN/FFmpeg-Builds

FFmpeg is a trademark of Fabrice Bellard, originator of the FFmpeg project.

## Rust crates

VMPC is built with open-source Rust crates, including egui/eframe (MIT/Apache-2.0),
cpal (Apache-2.0), rfd (MIT), serde/serde_json (MIT/Apache-2.0) and their dependencies.
Run `cargo tree` in the source tree for the complete list.

## Fonts

- **Inter** (UI text), Copyright 2020 The Inter Project Authors (https://github.com/rsms/inter)
- **Source Serif 4** (headlines), Copyright 2014 The Source Serif 4 Project Authors
  (https://github.com/adobe-fonts/source-serif)

Both are licensed under the SIL Open Font License 1.1; the full texts are in
`assets/fonts/Inter-OFL.txt` and `assets/fonts/SourceSerif4-OFL.txt`.

## File-format catalogue

`assets/formats.tsv` (the list of ~1,700 file formats VMPC recognises) is derived from the
Wikipedia article "List of file formats" (https://en.wikipedia.org/wiki/List_of_file_formats)
by its contributors, licensed under the Creative Commons Attribution-ShareAlike 4.0 License
(https://creativecommons.org/licenses/by-sa/4.0/). The derived file is distributed under the
same license; VMPC added entries for common media, subtitle, archive and data formats.

## Data and archive libraries

YAML, TOML and CSV support uses the serde_yaml (MIT/Apache-2.0), toml (MIT/Apache-2.0) and csv
(MIT/Unlicense) crates. Archives are handled by libarchive's `bsdtar` (BSD license), which ships
with Windows 10/11 and is available as `libarchive-tools` on Linux.
