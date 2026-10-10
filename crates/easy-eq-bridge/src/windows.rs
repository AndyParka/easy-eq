use super::SharedBlock;
use easy_eq_core::{PreparedGraph, workspace::Workspace};
use std::{collections::BTreeMap, fs::{File, OpenOptions}, os::windows::{fs::OpenOptionsExt, io::AsRawHandle}, path::{Path,PathBuf}, ptr::NonNull, sync::atomic::Ordering};
use winreg::{RegKey, enums::HKEY_LOCAL_MACHINE};
use serde::Serialize;

#[link(name="kernel32")]
extern "system" {
    fn CreateFileMappingW(file: *mut std::ffi::c_void, attributes: *mut std::ffi::c_void, protection: u32, high: u32, low: u32, name: *const u16) -> *mut std::ffi::c_void;
    fn MapViewOfFile(mapping: *mut std::ffi::c_void, access: u32, high: u32, low: u32, length: usize) -> *mut std::ffi::c_void;
    fn UnmapViewOfFile(address: *const std::ffi::c_void) -> i32;
    fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    fn GetTickCount64() -> u64;
}
pub fn now_ms() -> u64 { unsafe { GetTickCount64() } }
pub const BLOCK_BYTES: usize = std::mem::size_of::<SharedBlock>();
pub struct Mapping { pointer: NonNull<SharedBlock>, mapping: *mut std::ffi::c_void, _file: File }
// Only atomic cells are accessible through this mapping. No mutable references
// or non-atomic payload accesses are used after initialization.
unsafe impl Send for Mapping {}
unsafe impl Sync for Mapping {}
impl Mapping {
    pub fn open(path: &Path) -> Result<Self,String> {
        let file = OpenOptions::new().read(true).write(true).share_mode(7).open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if file.metadata().map_err(|e| e.to_string())?.len() != BLOCK_BYTES as u64 { return Err("Incompatible Easy EQ control mapping".into()); }
        unsafe {
            let mapping = CreateFileMappingW(file.as_raw_handle(), std::ptr::null_mut(), 4, 0, 0, std::ptr::null());
            if mapping.is_null() { return Err(std::io::Error::last_os_error().to_string()); }
            let pointer = MapViewOfFile(mapping, 6, 0, 0, BLOCK_BYTES).cast::<SharedBlock>();
            let Some(pointer) = NonNull::new(pointer) else { CloseHandle(mapping); return Err(std::io::Error::last_os_error().to_string()); };
            let result = Self { pointer, mapping, _file:file };
            if !result.block().valid() { return Err("Invalid Easy EQ control protocol".into()); }
            Ok(result)
        }
    }
    /// Installation only. Never truncate a file used by a loaded audio instance.
    pub fn initialize(path: &Path) -> Result<(),String> {
        use std::io::Write;
        let block = SharedBlock::default();
        // Every field is an aligned AtomicU64; its initialized representation is
        // written once before any mapping is opened. Runtime access is atomic.
        let bytes = unsafe { std::slice::from_raw_parts((&block as *const SharedBlock).cast::<u8>(), BLOCK_BYTES) };
        OpenOptions::new().write(true).create_new(true).open(path).and_then(|mut f| f.write_all(bytes)).map_err(|e| e.to_string())
    }
    pub fn block(&self) -> &SharedBlock { unsafe { self.pointer.as_ref() } }
}
impl Drop for Mapping {
    fn drop(&mut self) { unsafe { UnmapViewOfFile(self.pointer.as_ptr().cast()); CloseHandle(self.mapping); } }
}
pub struct SpectrumMapping { pointer: NonNull<super::spectrum::SpectrumBlock>, mapping: *mut std::ffi::c_void, _file: File }
unsafe impl Send for SpectrumMapping {}
unsafe impl Sync for SpectrumMapping {}
impl SpectrumMapping {
    pub fn open(path: &Path) -> Result<Self,String> {
        let bytes = std::mem::size_of::<super::spectrum::SpectrumBlock>();
        let file = OpenOptions::new().read(true).write(true).share_mode(7).open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if file.metadata().map_err(|e| e.to_string())?.len() != bytes as u64 { return Err("Incompatible Easy EQ spectrum mapping".into()); }
        unsafe {
            let mapping = CreateFileMappingW(file.as_raw_handle(), std::ptr::null_mut(), 4, 0, 0, std::ptr::null());
            if mapping.is_null() { return Err(std::io::Error::last_os_error().to_string()); }
            let pointer = MapViewOfFile(mapping, 6, 0, 0, bytes).cast::<super::spectrum::SpectrumBlock>();
            let Some(pointer) = NonNull::new(pointer) else { CloseHandle(mapping); return Err(std::io::Error::last_os_error().to_string()); };
            let result = Self { pointer, mapping, _file: file };
            if !result.block().valid() { return Err("Invalid Easy EQ spectrum protocol".into()); }
            Ok(result)
        }
    }
    pub fn initialize(path: &Path) -> Result<(),String> {
        use std::io::Write;
        let block = super::spectrum::SpectrumBlock::default();
        let bytes = unsafe { std::slice::from_raw_parts((&block as *const super::spectrum::SpectrumBlock).cast::<u8>(), std::mem::size_of_val(&block)) };
        OpenOptions::new().write(true).create_new(true).open(path).and_then(|mut f| f.write_all(bytes)).map_err(|e| e.to_string())
    }
    pub fn block(&self) -> &super::spectrum::SpectrumBlock { unsafe { self.pointer.as_ref() } }
}
impl Drop for SpectrumMapping {
    fn drop(&mut self) { unsafe { UnmapViewOfFile(self.pointer.as_ptr().cast()); CloseHandle(self.mapping); } }
}
pub fn config_directory() -> Result<PathBuf,String> {
    RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey("SOFTWARE\\EqualizerAPO").and_then(|key| key.get_value::<String,_>("ConfigPath"))
        .map(PathBuf::from).map_err(|e| format!("Equalizer APO is not installed: {e}"))
}
pub fn state_directory() -> PathBuf {
    PathBuf::from(std::env::var_os("ProgramData").unwrap_or_else(|| "C:\\ProgramData".into())).join("Peace2")
}
pub fn endpoint_guid(id: &str) -> Option<&str> {
    let start = id.rfind('{')?; let guid = &id[start..];
    if guid.len() != 38 || !guid.ends_with('}') { return None; }
    let body = &guid[1..37];
    if body.bytes().enumerate().all(|(i,b)| if [8,13,18,23].contains(&i) { b==b'-' } else { b.is_ascii_hexdigit() }) { Some(guid) } else { None }
}
pub fn post_mix_installed(id: &str) -> bool {
    let Some(guid) = endpoint_guid(id) else { return false; };
    let path = format!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\MMDevices\\Audio\\Render\\{guid}\\FxProperties");
    let Ok(key) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(path) else { return false; };
    [2,6,7,14,15].iter().any(|index| {
        let name = format!("{{d04e05a6-594b-4fb6-a80d-01af5eed7d1d}},{index}");
        let expected = "{EC1CC9CE-FAED-4822-828A-82A81A6F018F}";
        key.get_value::<String,_>(&name).is_ok_and(|value| value.eq_ignore_ascii_case(expected))
            || key.get_value::<Vec<String>,_>(&name).is_ok_and(|values| values.iter().any(|value| value.eq_ignore_ascii_case(expected)))
    })
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all="camelCase")]
pub struct AudioStatus { pub output_id:String, pub state:String, pub reason:String, pub sample_rate:Option<f64>, pub added_buffer_frames:u32 }
struct Connection { mapping:Mapping, profile:Option<easy_eq_core::Profile>, rate:f64, published:u64 }
pub struct Host { directory:Option<PathBuf>, connections:BTreeMap<String,Connection> }
impl Default for Host {
    fn default() -> Self { Self { directory:config_directory().ok(), connections:BTreeMap::new() } }
}
impl Host {
    pub fn update(&mut self, workspace: Option<&Workspace>, outputs: &[(String,String,Option<u16>)]) -> Vec<AudioStatus> {
        let now = now_ms(); let mut statuses = Vec::new();
        for (id,state,channels) in outputs {
            let mut status = AudioStatus { output_id:id.clone(), state:"needsSetup".into(), reason:String::new(), sample_rate:None, added_buffer_frames:0 };
            if state != "available" { status.state="unavailable".into(); status.reason=format!("Output is {state}."); }
            else if !post_mix_installed(id) { status.reason="Equalizer APO is not installed in this output's final effects stage. Enable it in APO Device Selector; some drivers or playback modes bypass system effects.".into(); }
            else if let (Some(_directory), Some(guid)) = (&self.directory, endpoint_guid(id)) {
                if !self.connections.contains_key(id) {
                    match Mapping::open(&state_directory().join(guid).join("control.bin")) {
                        Ok(mapping) => { self.connections.insert(id.clone(), Connection { mapping, profile:None,rate:0.0,published:0 }); },
                        Err(error) => status.reason=format!("Easy EQ audio bridge needs setup: {error}"),
                    }
                }
                if let Some(connection) = self.connections.get_mut(id) {
                    let block = connection.mapping.block();
                    let rate = f64::from_bits(block.device_rate.load(Ordering::Acquire));
                    if rate != 0.0 && (!rate.is_finite() || !(8000.0..=768000.0).contains(&rate)) {
                        status.state="error".into();status.reason=format!("The audio path reports an unsupported sample rate ({rate}). This adapter accepts 8–768 kHz and passes this format through without EQ.");
                    }
                    let profile = workspace.and_then(|w| w.profile_for_output(id));
                    let enabled = workspace.is_some_and(|w| w.enabled) && profile.is_some();
                    if profile != connection.profile.as_ref() || rate != connection.rate {
                        block.bypass();
                        if rate.is_finite() && (8000.0..=768000.0).contains(&rate) {
                            let graph = profile.map(|p| PreparedGraph::compile(p, rate)).unwrap_or_else(|| PreparedGraph::identity(rate));
                            match graph {
                                Ok(graph) => { connection.published=block.publish(graph); connection.profile=profile.cloned();connection.rate=rate; },
                                Err(error) => { status.state="error".into();status.reason=error.to_string(); },
                            }
                        }
                    }
                    let prepared = connection.published != 0 && connection.rate == rate && profile == connection.profile.as_ref();
                    block.renew(enabled && prepared, now);
                    let callback = block.callback_ms.load(Ordering::Acquire);
                    let applied = block.applied_sequence.load(Ordering::Acquire);
                    if rate > 0.0 { status.sample_rate=Some(rate); }
                    if status.state != "error" {
                        if callback == 0 || now.saturating_sub(callback) > 2000 {
                            status.state="waiting".into();status.reason="Bridge is installed; waiting for system playback through this output. Effects must be enabled in Windows. ASIO, exclusive/raw and some spatial paths may bypass APO.".into();
                        } else if !enabled && block.processing_enabled.load(Ordering::Acquire)==0 {
                            status.state="bypassed".into();status.reason=if profile.is_some() { "EQ is off. System playback passes through.".into() } else { "No profile assigned to this output; system playback passes through.".into() };
                        } else if !enabled {
                            status.state="connecting".into();status.reason="Turning EQ off; waiting for the audio callback to acknowledge bypass.".into();
                        } else if prepared && applied == connection.published && block.processing_enabled.load(Ordering::Acquire) != 0 {
                            if channels.is_some_and(|count| count>2) {
                                status.state="partial".into();status.reason=format!("Native EQ is processing front L/R. The shared output format exposes {} channels; complete per-ear correction through its surround renderer is unverified. Target a stereo headphone/binaural route for complete per-ear EQ. No added EQ buffer; device latency is not measured.",channels.unwrap());
                            } else {
                                status.state="processing".into();status.reason="Native audio callback is processing this profile in the APO path. EQ adds no processing buffer; device latency is not measured.".into();
                            }
                        } else { status.state="connecting".into();status.reason="Waiting for the audio callback to acknowledge the current profile.".into(); }
                    }
                }
            } else { status.reason="Equalizer APO configuration directory is unavailable.".into(); }
            if state != "available" { if let Some(c) = self.connections.get(id) { c.mapping.block().bypass(); } }
            statuses.push(status);
        }
        self.connections.retain(|id,c| { let keep=outputs.iter().any(|(key,_,_)| key==id); if !keep { c.mapping.block().bypass(); } keep });
        statuses
    }
    pub fn bypass(&self) { for c in self.connections.values() { c.mapping.block().bypass(); } }
}
impl Drop for Host { fn drop(&mut self) { self.bypass(); } }
