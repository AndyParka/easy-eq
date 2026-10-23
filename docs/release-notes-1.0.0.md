# Easy EQ 1.0.0

The first packaged Windows x64 release of the native equalizer previously developed as Peace 2.

Easy EQ started from a practical need: simple, stable Windows headphone EQ with independent adjustment for each ear and no added processing buffer. It is an independent frontend for Equalizer APO's system-audio path, with its own Rust DSP adapter.

- Shared and independent left/right bell/shelf filters; editable response graph.
- Manual gain and balance, linked/copy edits, undo/redo and JSON profiles.
- Profiles remembered per output or shared across outputs; new outputs start flat.
- Live original/processed left/right spectra from real audio callbacks.
- Tray operation, crash/stall watchdog and sign-in/unlock/wake recovery.
- Windows installer with bundled Equalizer APO and guided output selection.
- Existing Peace 2 settings retained; APO configuration backed up and preserved.

Run the setup EXE normally. Audio setup requests administrator approval and may require a restart for a newly configured device. Disable filters from another EQ to avoid stacking corrections.

Windows shared stereo playback is the supported path. ASIO, exclusive/raw and some spatial modes can bypass APO. Full surround coverage and end-to-end latency remain unverified. No added processing buffer does not mean zero device latency. Actual hardware sleep/wake and clean-machine installation still need manual validation. The installer is unsigned.

Matching Equalizer APO source and SHA-256 checksums accompany the release. See README and docs/validation.md for evidence and scope.
