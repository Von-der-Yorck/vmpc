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
