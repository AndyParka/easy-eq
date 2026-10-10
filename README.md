# Easy EQ

A focused headphone EQ with a native Rust egui/eframe editor. The Windows
prototype has shared EQ followed by independent left/right bell and shelf filters, manual gain,
stereo balance, profiles and an editable response graph. The app runs without
a webview, Node process or development server.

## Run the Windows prototype

~~~powershell
./scripts/build-desktop.ps1
./crates/easy-eq-desktop/target/debug/easy-eq.exe
~~~

Create or update your desktop shortcut to the executable above. Close the app
from its tray menu before rebuilding.

The editor follows the Windows default output. Profiles are remembered per
output by default; Settings can switch to one shared profile. New outputs start
flat. **Both ears** edits the shared first stage; **Left ear** and **Right ear**
add individual correction afterward. Use **Add band**, then drag nodes or edit
frequency, gain and Q. Left/right curves can be shown together, and edits can be
linked or copied between ears. Shared and individual filters can total up to
31 bands in each processing channel.

Controls apply automatically and are saved locally. Undo/redo covers profile
edits. Import/export uses native file dialogs and the Easy EQ JSON format.
Output gain is manual and common to both ears. Balance attenuates one side
without mixing stereo channels. Beneath the EQ controls, **Original frequencies**
and **New frequencies** display live left/right spectra measured before and after
EQ, gain and balance. Both use the same logarithmic frequency axis and dBFS scale.
Bypass shows the actual pass-through output. Comfort-tuning tools are deferred.

**Done / Close** hides the editor and keeps EQ running. **EQ off** bypasses
processing. Tray **Quit** revokes the audio lease and exits; a killed or hung
host expires its lease and fades to bypass. Sign-in startup opens in the tray
and restores the intentional on/off state. Disable it in Settings.

## How Windows audio is processed

Easy EQ implements its own Rust DSP engine in crates/easy-eq-core. Equalizer APO
provides the Windows system-audio insertion point and loads the component in
crates/easy-eq-vst; crates/easy-eq-bridge transports prepared coefficients and the
bypass lease. The editor does not stream or buffer playback audio. The component
optionally copies short, paired input/output windows for the visualiser; FFT
analysis runs in the editor. Capture stops when the editor is hidden and never
delays playback. Updating the visualiser requires rebuilding the audio component
and installing it with `scripts/install-audio.ps1` as well as rebuilding the
editor. The installer uses versioned DLL paths so APO can reload an update without
overwriting a loaded component or restarting the audio service.

The engine uses minimum-phase IIR filters on the current sample and adds no
processing buffer. This does not remove operating-system, hardware or Bluetooth
latency, nor frequency-dependent filter phase/group delay. Device latency has
not yet been measured.

The component is installed on configured APO outputs on this machine. The UI
reports actual callback acknowledgements separately from discovery/installation.
Windows shared playback at 44.1 kHz on the stereo Steinberg UR22mkII has
acknowledged profile edits and on/off changes from this native app.

Coverage depends on the device and playback path. ASIO and exclusive/raw modes
can bypass APO. The Logitech headset's eight-channel shared format currently
receives front L/R processing; complete correction through its surround renderer
is unverified. Device registration, surround validation, reconnect stress and
installer/upgrade packaging remain work.

## Build and verify

~~~powershell
./scripts/rust.ps1 test --manifest-path crates/easy-eq-desktop/Cargo.toml
./scripts/rust.ps1 test --manifest-path crates/easy-eq-core/Cargo.toml
./scripts/rust.ps1 build --manifest-path crates/easy-eq-bridge/Cargo.toml --bin inspect-control
./scripts/rust.ps1 build --manifest-path crates/easy-eq-devices/Cargo.toml --bin exercise-output
./scripts/verify-native-audio.ps1
~~~

The live harness uses silent shared playback, checks editor submissions and real
callbacks, and restores the saved workspace. It covers window close, Off, Quit,
tray startup, single-instance behavior and host kill. It does not establish
audible behavior in every application or spatial mode.

Windows builds require Windows build tools and a current Rust toolchain.
The current editor build uses only Cargo. The earlier TypeScript/Tauri prototype
remains under src, src-tauri and public for reference.

macOS/Linux remain later targets. The shared Rust DSP and profile format are
portable; discovery adapters and a PipeWire export exist, but their complete
system-audio paths require implementation and hardware tests.

## Sources and decisions

Official source snapshots are stored locally under upstream/equalizer-apo and
upstream/peace, with versions/checksums in [docs/upstream.json](docs/upstream.json).
They are ignored by source control. Their licenses and notices remain intact;
new app source is GPL-2.0-or-later. See LICENSE, [decisions](docs/decisions.md),
[architecture](docs/architecture.md) and [validation](docs/validation.md).

## Rename compatibility

Easy EQ was previously called Peace 2. Package and crate names, the desktop
executable, editor branding and exported filenames now use Easy EQ. Existing
profile JSON remains compatible. For existing installations, the application ID
`technology.parkinson.peace2`, browser storage key `peace2-v1`, Windows startup
registration, single-instance events, `Peace2` audio state/config directories,
managed APO include marker and `peace2_audio` DLL identity intentionally remain
stable. These are compatibility identifiers, not the product name. Rebuild the
editor and update old shortcuts to `easy-eq.exe`. Historical validation notes
refer to the original Peace 2 builds and are not evidence of a new Windows test.
