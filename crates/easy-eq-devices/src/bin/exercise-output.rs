//! Send zero samples through the current shared output to inspect the real
//! Windows effects path without emitting a tone or changing output selection.
#[cfg(windows)]
fn run() -> windows::core::Result<()> {
    use windows::Win32::{Media::Audio::*,System::Com::*};
    unsafe {
        CoInitializeEx(None,COINIT_MULTITHREADED).ok()?;
        struct Apartment;impl Drop for Apartment { fn drop(&mut self) { unsafe { CoUninitialize(); } } }
        let _apartment=Apartment;
        let enumerator:IMMDeviceEnumerator=CoCreateInstance(&MMDeviceEnumerator,None,CLSCTX_ALL)?;
        let device=enumerator.GetDefaultAudioEndpoint(eRender,eMultimedia)?;
        let client:IAudioClient=device.Activate(CLSCTX_ALL,None)?;
        let format=client.GetMixFormat()?;
        let rate=(*format).nSamplesPerSec;let channels=(*format).nChannels;let frame_bytes=(*format).nBlockAlign as usize;
        let result=client.Initialize(AUDCLNT_SHAREMODE_SHARED,0,1_000_000,0,format,None);
        CoTaskMemFree(Some(format.cast()));result?;
        let render:IAudioRenderClient=client.GetService()?;let capacity=client.GetBufferSize()?;
        let buffer=render.GetBuffer(capacity)?;std::ptr::write_bytes(buffer,0,capacity as usize*frame_bytes);
        // Valid zero frames ensure effects see a normal stream. Nothing audible
        // is generated; no volume or enhancement settings are changed.
        render.ReleaseBuffer(capacity,0)?;client.Start()?;
        let seconds=std::env::args().nth(1).and_then(|s|s.parse::<u64>().ok()).filter(|n| (1..=15).contains(n)).unwrap_or(4);
        let started=std::time::Instant::now();
        while started.elapsed()<std::time::Duration::from_secs(seconds) {
            let available=capacity.saturating_sub(client.GetCurrentPadding()?);
            if available>0 { let buffer=render.GetBuffer(available)?;std::ptr::write_bytes(buffer,0,available as usize*frame_bytes);render.ReleaseBuffer(available,0)?; }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        client.Stop()?;println!("Exercised the default shared Windows output at {rate} Hz, {channels} channels, with zero samples.");Ok(())
    }
}
#[cfg(windows)]
fn main() { if let Err(error)=run() {eprintln!("Shared-output check failed: {error}");std::process::exit(1);} }
#[cfg(not(windows))]
fn main() { eprintln!("The shared-output check currently targets Windows.");std::process::exit(2); }
