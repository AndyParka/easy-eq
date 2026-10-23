# Third-party software

Easy EQ is an independent frontend for Windows system audio EQ. It is not an
official release of Peace or Equalizer APO and is not affiliated with their authors.

Equalizer APO 1.4.2 by Jonas Thedering supplies the system-audio insertion point.
The official, unmodified x64 installer is bundled for first-run setup. It is
licensed under GPL-2.0-or-later. Its license is shown by its own installer.

- Project and source: https://sourceforge.net/projects/equalizerapo/
- Matching source: https://sourceforge.net/projects/equalizerapo/files/1.4.2/EqualizerAPO-src-1.4.2.zip/download
- Installer SHA-256: `7403be7427bbe1936a40dded082829b6e217fc4f5990fee5cba501f0ae055afa`

Distribute the matching Equalizer APO source archive alongside Easy EQ's release
assets. Easy EQ's own source is provided in the same release and repository under
GPL-2.0-or-later (see LICENSE). Rust dependency versions are recorded in each
crate's Cargo.lock. Their individual copyright and license notices are in the
published crates and source; they are not relicensed by this project.

The original Peace interface by Peter Verbeek inspired this project's initial
exploration and working name, Peace 2. Easy EQ's native editor and DSP adapter
are separate implementations. The legacy storage directory and audio-bridge
identifiers are retained to preserve existing user profiles.
