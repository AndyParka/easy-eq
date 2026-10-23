# Easy EQ 1.0 release validation — 23 October 2026

The packaged native application now includes a user-session watchdog and a
Task Scheduler recovery task. The installed Windows x64 application passed:

- Hidden startup and duplicate-launch handling with one supervised editor.
- Forced editor termination followed by automatic tray recovery, retaining Off.
- Resume notification reopening mappings and refreshing output discovery.
- A six-second pause of both processes without an unnecessary watchdog restart.
- A stalled editor heartbeat causing automatic replacement after 15 seconds.
- Explicit Quit stopping both processes without a restart.
- Windows PowerShell 5.1 audio setup, upgrading the configured outputs through
  versioned DLL paths, with existing profiles preserved.
- Real shared-playback callbacks acknowledging the installed release's graph
  and enabled state at 48 kHz, with zero cross-channel mixing.

The core, bridge, device and editor suites and compiled-plugin integration test
cover 38 tests. The installer bundles the pinned official Equalizer APO 1.4.2
installer, carries dependency notices, and statically links Easy EQ's MSVC CRT.
Source archives and SHA-256 checksums accompany release binaries.

The original sleep-related disappearance has no conclusive crash record. The
checks above exercise recovery and simulated scheduling gaps; they do not
establish actual hardware sleep/wake, clean-machine installation, audible A/B,
full surround/spatial coverage or measured end-to-end latency. Those remain
manual validation items. The installer is unsigned.

## Prototype snapshot — 9 October 2026

The current Windows prototype uses a native Rust/egui editor and an independent
background audio host. The desktop shortcut targets the latest native executable
at `crates/easy-eq-desktop/target/debug/easy-eq.exe`. The former Tauri editor below is
a historical snapshot.

- 23 Rust core tests pass, including shared EQ followed by independent ear EQ,
  sample-by-sample comparison against sequential processing, backward-compatible
  profile loading, shared-first APO export, combined filter capacity, and ordered
  unacknowledged edits that reverse direction.
- Seven native editor tests pass for linked edits, independent reset behavior,
  shared-stage isolation, undo gestures, response graph coordinates, calibrated
  FFT tone/gain measurements, silence and clearing stale/device-switched traces.
- Four bridge tests pass, including paired original/processed stereo capture,
  bounded reads, capture lease expiry and discarding partial windows on rate changes.
  The compiled audio DLL test measures a real 9 dB left-ear cut in captured
  output samples while the right-ear input/output match. In-place buffers retain
  the original samples, common gain/balance is captured correctly, and disabling
  capture stops publication without affecting processing.
- Live visualiser capture has been verified on the default Steinberg UR22mkII
  at 44.1 kHz. The telemetry publisher PID matches the actual `audiodg.exe`
  callback. Hiding the editor stops the telemetry sequence; reopening resumes
  fresh frames. The two frequency plots beneath the EQ controls have been
  visually checked with real playback. See `artifacts/spectrum-validation.txt`
  and `artifacts/native-visualiser.png` for the local results.
- The live audio harness passes on the default stereo Steinberg UR22mkII at
  44.1 kHz. The real Windows audio callback acknowledges the graph published
  through the editor submission path: shared -3 dB at 1 kHz, left-only -6 dB,
  output gain -6 dB, and balance 25% right. Transport inspection confirms the
  expected left/right responses and zero crossfeed. The original profile is
  restored after the fixture.
- Real window close keeps processing active. Explicit Off bypasses; Quit revokes
  the lease; hidden startup restores saved Off; a secondary tray launch keeps
  one host; forced host termination bypasses after lease expiry. Startup checks
  inspect the actual Peace 2 editor window, excluding winit's helper window.
- The rendered native editor has been visually checked. Both ears, Left ear and
  Right ear have separate controls; band cards expose shape, frequency, gain, Q
  and enable state. Settings and Audio outputs have no collapse arrows.

The harness restores the exact saved workspace and reopens the editor. The app
and desktop shortcut are ready for interactive use, including the live visualiser.
Silent callback checks and offline DSP tests establish the current stereo shared
audio path; audible A/B, ASIO/exclusive paths, surround/spatial coverage, measured
latency, and installation/upgrade packaging remain separate validation work.

## Historical snapshot — 8 October 2026

The Windows development executable builds with embedded assets and launches a
Peace 2 desktop window. The renderer successfully initializes native storage and
IPC from Tauri's internal `http://tauri.localhost/` asset origin. The executable's
`--check-assets` command verifies its HTML, JavaScript, CSS, and audition worklet.
It does not need the Vite development server at runtime.
The running preview has a WebView2 child process, no Node child process, and no
listener on the development-server port 1420.
Tray startup reports the editor initially hidden through Tauri's window API.
A normal second launch exits successfully, shows the existing editor, and leaves
exactly one Peace 2 process running. The Windows sign-in entry uses a quoted
executable path with `--tray` and is registered for the current user.

- 14 TypeScript tests pass for DSP response, export behavior, manual gain,
  invalid profile rejection, stereo isolation, and worklet transitions.
- 19 Rust core tests pass, including 54 independent coefficient-reference cases,
  no added frame delay for identity processing, lease-expiry bypass, rapid profile
  changes, zero heap allocations while processing prepared graphs, per-output and
  shared profile routing, legacy settings migration, restoration of off state,
  and merging edits during an output switch.
- Two bridge tests validate coefficient transport and bounded rejection of
  interrupted or invalid publications. A compiled-DLL test covers in-place
  buffers, current-sample output, gain/balance, independent per-ear bell EQ, Off,
  expiry and unsupported-format bypass.
- 2 discovery tests pass for explicit support status and PipeWire snapshot
  parsing, including virtual outputs, input exclusion, and unavailable outputs.
- Native Windows discovery returns 28 output entries, including four currently
  available outputs and the default Logitech headset. No discovery errors were
  reported. The editor displays setup/unavailable reasons instead of claiming EQ
  is active.
- Discovery adapters type-check for Windows, Apple Silicon macOS, and Linux
  x86-64. macOS and Linux discovery have not been run on actual machines.

Existing Equalizer APO configuration still includes `peace.txt`; that file's
device selection is unchanged. A backed-up root config now includes the owned
Peace 2 native component for five already registered APO endpoints. No device
registrations or Windows audio-service restart were performed. Sign-in startup
can be controlled from Audio Outputs settings.

Silent shared playback on the default Logitech output runs at 48 kHz with eight
channels. The real `audiodg.exe` callback acknowledges the native coefficient
sequence and enabled state. Killing only the Peace 2 host leaves those callbacks
running and changes processing to bypass after the lease expires (observed after
1.7 seconds). Native CloseRequested hides the editor without stopping processing.
Explicit Off, Quit while enabled, and hidden startup restoring saved Off have
also passed live callback checks. The lifecycle harness restores the original
on/off preference. It exercises the native handlers; physical tray mouse clicks
and visual layout have not been tested by this harness.

Eight-channel shared format does not prove the final surround processing order.
The bridge selects front L/R, and the UI reports partial spatial coverage. Full
per-ear headphone correction across this surround path is not verified.

Still required: audible A/B and spatial-route validation, system processing on
macOS/Linux, measured added latency, live system spectrum, renderer-crash and
reconnect stress, installation/upgrade packaging and the compact Peace-style UI.
Silent callback checks and offline DSP tests do not establish every device,
application or audible spatial behavior.
