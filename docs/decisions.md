# Agreed product decisions

- Replace the rejected Tauri/webview interface with a fresh native Rust egui
  desktop UI. No webview, HTML/CSS or Node is part of the new UI.
- Focus on Windows native first. macOS/Linux are later targets; keep the shared
  profile format and DSP portable.
- Prioritize no added processing buffer. Minimum-phase per-ear IIR EQ, gain,
  and stereo balance are first. Effects and comfort/hearing tuning come later.
- Use stereo balance for spatial games; do not mix the left/right channels by
  default. Positional audio still needs testing with the actual game and device.
- Let the editor toggle between both curves and one ear at a time. Keep the EQ
  graph front and center with a live spectrum once the audio feed is available.
- Closing/hiding the editor must leave the audio host alive. Explicit Off, tray
  Quit, or a killed audio host must lead to identity/bypass processing.
- Use manual gain only. Apply one common gain setting to both ears; do not offer
  automatic headroom or a limiter in the first version.
- Discover every audio output, with support status and an actionable reason
  inside the UI. Device detection alone must never imply EQ is working. Do not
  restrict the app to a particular headset model or transport.

- Offer per-output profiles and one shared profile as selectable modes. Per-output
  profiles are the default, following the system default output and remembering
  each output's profile.
- Start in the tray at sign-in and restore the last on/off state.
- Use a compact control layout like the original Peace app, with an editable EQ
  graph and band controls. Prioritize a functioning Windows prototype.
- Show live original and processed frequency plots beneath the native EQ
  controls, with separate left/right traces and matching scales. Capture actual
  input/output from the audio component; pause capture when the editor is hidden.

Still awaiting user preferences: comfort-tuning
tools. Existing controls are editable prototype defaults, not settled decisions.
