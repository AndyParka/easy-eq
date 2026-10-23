# Easy EQ implementation decisions

The 1.0 Windows release adds a separate watchdog without a renderer. It owns the
editor child process, monitors an audio-control heartbeat, backs off crash loops,
and restores the editor in the tray. Resume scheduling gaps grant a fresh
heartbeat window and refresh device discovery and bridge mappings. Explicit Quit
signals the watchdog before stopping the worker, preserving intentional exit.

The per-user scheduled task starts at sign-in, unlock and wake, retries failed
watchdogs, and has no battery, idle, network or execution-time restriction.
Legacy profile, mapping, DLL and instance identifiers remain compatible with
the original prototype. Audio remains in the APO callback; the watchdog does
not buffer, capture or route playback.

## Scope and accepted preferences

All three desktop platforms are designed in from the start. Core controls are
per-ear parametric EQ, stereo balance suitable for positional games, gain
reduction, and profiles. The graph is the primary interface. A toggle shows both
curves or switches between independent ear graphs. Default profiles are flat;
no hearing-loss correction is invented or applied automatically. Hearing/comfort
tuning and extra effects are secondary.

## Audio path

The current Windows desktop UI is Rust egui/eframe in `crates/easy-eq-desktop`.
It uses a native window with GPU-drawn widgets, without a webview or Node server.
The earlier Tauri implementation remains in the repository as a reference.
The shared `easy-eq-core` crate is independent of Tauri and all renderer APIs.

```text
OS audio path → shared IIR EQ → independent ear correction → output
                          common gain and stereo balance
```

The Both ears graph edits the shared first stage. Left/right response curves
include that stage plus each ear's individual filters, before common preamp and
balance. Prepared coefficient chains contain the shared filters first in both
channels and preserve the existing fixed-size bridge protocol. Shared and
individual filters total at most 31 per channel. The native host uses the live
device rate and recompiles on rate changes. Manual gain is the
agreed output mode; no automatic attenuation or limiter is offered by the editor.

Rust owns future real-time processing, device lifecycle, profile activation, and
health. The renderer sends model changes, never audio blocks. DSP callbacks must
avoid allocations, locks, file operations, IPC waits, and renderer dependencies.
Use prebuilt graphs with bounded transitions when coefficients change. Analyzer
frames use a bounded latest-window mapping; drop visualization frames under pressure.

The audio component captures optional 4096-sample windows of the actual stereo
input and output into a separate atomic `spectrum.bin` mapping. Input is captured
before in-place output writes; output includes shared EQ, per-ear filters, gain,
balance and bypass transitions. No analysis or allocation runs in the callback.
Publication is capped at 30 windows per second and never waits for a reader.
The editor renews a 500 ms capture lease only while visible, reads at most two
times per tick, and computes Hann-windowed FFTs outside the audio thread. Paired
plots share a 20 Hz–20 kHz logarithmic axis (limited by Nyquist) and a -96–0 dBFS
scale. Device changes and stale frames clear the traces. The coefficient mapping
is unchanged, preserving compatibility with earlier installed components.

Current file audition uses an AudioWorklet implementing direct-form-II biquads.
New graphs crossfade over 240 samples without holding or delaying input samples.
This transition is not a linear-phase mode. It still needs listening and stress
tests; it is not yet suitable evidence for native low-latency operation.

## Platform adapters

The `easy-eq-devices` crate performs read-only discovery on the control thread:
Windows MMDevice includes all output states, Core Audio enumerates output streams
and persistent device UIDs, and Linux parses a bounded PipeWire state snapshot.
The editor displays all returned outputs, their default status, and a reason when
EQ is unavailable or still needs setup. Discovery never reports active effects.
Windows runtime discovery has been validated here; macOS/Linux runtime discovery
and their full audio paths require testing on those systems.

The native controller monitors output metadata every two seconds, outside the
audio callback and independently of renderer timers. Workspace routing follows
the system default. Per-output mode remembers stable endpoint/profile mappings;
shared mode retains one selected profile without discarding individual mappings.
New outputs start flat. Renderer edits merge against an acknowledged workspace
so a concurrent output switch does not discard a newly created native profile.
These policies now run in the desktop shell and drive the Windows APO bridge.
Sign-in startup uses the native autostart plugin with `--tray`. Saved on/off state
is restored; startup does not force EQ on. The startup setting is exposed in the UI.

**Windows:** The Rust `easy-eq-vst` component implements the small VST2-compatible
ABI used by Equalizer APO, based on APO's GPL VeSTige header. APO loads one owned
DLL per endpoint in its post-mix stage, selecting L/R. Its `initialDelay` is zero;
processing uses the current host buffers with no interleaving copy or extra
audio queue. The desktop host compiles the shared IIR graph at the sample rate
reported by the callback and publishes a fixed-size coefficient snapshot.

Control files are memory-mapped under ProgramData, outside APO's watched config
tree. Every live payload word is atomic. A sequence check makes at most two read
attempts; partial writes retain the previous valid graph without waiting. The
callback validates bounded counts and stable finite coefficients without allocating.
The native host refreshes a 1.5-second monotonic lease every 100 ms. Off and Quit
revoke it; kill/host hang expires it and the callback fades to identity. Only
callbacks running inside `audiodg.exe` report active processing.

Installation retains the existing config, backs up the root and atomically adds
an owned include. It does not register new APO endpoints or restart the service.
Discovery distinguishes configured endpoints from actual acknowledged callbacks.
Shared APO paths do not cover ASIO, exclusive WASAPI or every raw/offloaded/vendor
path. The current Logitech shared format has eight channels. L/R selection alone
does not establish complete per-ear correction through its surround renderer;
the UI exposes this as partial coverage. Validate the selected stereo/binaural or
vendor-surround route before claiming spatial support.

**Linux:** Implement a native PipeWire filter node/graph with independent ear
chains and a stable virtual sink. Preserve user routes, measure graph quantum,
handle reconnect/rate change, and persist chosen hardware endpoint. The config
export is a starting point and has not been executed on Linux.

**macOS:** Implement a Core Audio HAL virtual stereo output and a native companion
that delivers processed data to the selected physical device. The extra routing
hop can add latency. Clock drift, sample-rate conversion, device reconnects,
driver installation/signing, and current Core Audio system behavior need real Mac
validation. CPAL alone cannot transparently intercept all system playback.

## Lifetime contract

| Event | Intended production result |
| --- | --- |
| Done, close, minimize, hide | Hide editor; keep the audio host and current profile |
| Explicit off | Publish identity processing; retain settings |
| Explicit tray Quit | Publish identity; acknowledge bypass; stop background host |
| Host killed/crashes | Driver lease expires; identity processing |
| UI renderer crashes | Audio host continues its last valid profile |
| Sleep/device change | Reconnect and revalidate endpoint; expose actual state |
| Startup | Restore intentional state/profile without briefly applying wrong settings |

The native egui app intercepts close and hides the editor while its Windows control
host continues renewing the audio lease. Tray controls affect native EQ as well
as file audition. Quit revokes the lease before exiting and retains intentional
on/off state. This host currently lives in the desktop process; a separately
packaged operating-system service and its installer are not implemented.

**APO kill behavior requires more than writing text.** Existing APO latches its
configuration after its controller disappears. For the requested kill-to-bypass
behavior, the new narrowly scoped lease-aware APO component reads a lease: the native host refreshes
a monotonic lease; the audio callback reads it without waiting and transitions to
identity on expiry. Do not rely on UI heartbeats, process-exit callbacks, or a
watchdog that merely writes files and can itself die. The lease must distinguish
a renderer crash from the intended audio host being killed. Keep it out of the
audio buffering path. The shared Rust core now implements the expiry/bypass
policy against a supplied monotonic lease snapshot. The Windows component,
mapped transport and native heartbeat are implemented and exercised against live
callbacks. macOS/Linux adapters and their lifetime behavior remain incomplete.

## Build and validation gates

1. DSP: coefficient response, per-ear independence, finite values, impulse timing,
   headroom interactions, profile parsing, and bypass. Compare Rust and export
   responses against the shared reference across all supported rates.
2. Audio: actual selected-device routing and audible A/B, WASAPI shared/spatial
   scenarios, PipeWire and Core Audio playback; no fake active status.
3. Latency: same-device baseline vs enabled EQ measured by loopback; report added
   latency separately from OS/hardware latency. Exercise reconnects and CPU load.
4. Lifecycle: close/reopen, renderer failure, explicit bypass, tray quit, forced
   host kill, startup, sleep/resume and device change. Verify identity after kill.
5. Profiles: schema migration, atomic persistence, switching under playback,
   rollback after invalid data, and importing existing Peace/AutoEQ settings.

## Later comfort tuning

Prefer user-triggered, short per-ear tones or narrow-band noise, an always-visible
stop control, bounded output, and notes on uncomfortable frequencies. Treat this
as subjective profile tuning. An uncalibrated PC/headphone setup must not label
its readings as clinical hearing thresholds or automatically prescribe gain.
Keep test audio and all analysis outside the always-on production path unless the
user explicitly starts a tuning session. No medical workflow is implemented.

## Sources

- [Equalizer APO source](https://sourceforge.net/p/equalizerapo/code/)
- [APO configuration reference](https://sourceforge.net/p/equalizerapo/wiki/Configuration%20reference/)
- [Peace official source archive](https://sourceforge.net/projects/peace-equalizer-apo-extension/files/Source%20code/)
- [RBJ Audio EQ Cookbook](https://www.w3.org/TR/audio-eq-cookbook/)
- [Tauri tray API](https://v2.tauri.app/learn/system-tray/)
- [PipeWire filter chain](https://docs.pipewire.org/page_module_filter_chain.html)
- [Apple Audio Server Driver Plug-in](https://developer.apple.com/documentation/coreaudio/creating-an-audio-server-driver-plug-in)
