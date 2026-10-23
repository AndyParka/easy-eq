# Easy EQ: origin and scope

Andrew Parkinson wanted a simple, stable Windows equalizer to balance different hearing loss in each ear. The original request on 30 September 2026 was for frequency-by-frequency adjustment of both ears, rather than only an overall volume balance.

Initial exploration used Peace and Equalizer APO. A neutral configuration warning and inconsistent left/right loudness led to inspecting the configuration. Two always-active profiles were stacking on top of the selected preset. That made predictable state and clear control over active correction central to the new design.

This account summarizes the relevant initial development chats and agreed decisions. It does not publish private transcripts, personal profiles, hearing measurements or machine logs.

The working name was Peace 2. The project explored a TypeScript/Tauri editor, then moved to native Rust egui/eframe. The Windows release is named Easy EQ and is independent of both Peace and Equalizer APO.

Easy EQ is a frontend for the Windows system-audio EQ path, with its own DSP adapter loaded by Equalizer APO. Shared tuning runs first; left and right corrections follow independently. There is no crossfeed, audio streaming through the editor, virtual-cable hop or extra processing buffer.

The first release covers manual bell/shelf EQ, shared and per-ear controls, gain and balance, response curves, per-device profiles, linked/copy editing, undo/redo, import/export, real before/after spectra, tray operation and process recovery. A Windows x64 installer packages the editor, adapter and guided Equalizer APO setup.

The latency ambition is zero additional processing-buffer latency. Current-sample processing and zero added delay frames have offline tests. End-to-end device latency has not been measured, and IIR phase/group delay still exists.

The compatibility ambition is broad Windows coverage. The implemented path is shared playback through configured APO outputs. ASIO and exclusive/raw modes may bypass it. Front L/R processing does not establish correct correction after every surround renderer.

Actual sleep/wake hardware behavior, a clean Windows installation, audible A/B, reconnect stress, surround/spatial coverage and measured latency remain explicit validation work. Automatic hearing tests, clinical correction, automatic gain/headroom, effects and macOS/Linux system processing are deferred.
