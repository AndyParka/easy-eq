use super::{Output, Report};
use objc2_core_audio::*;
use objc2_core_foundation::{CFString, CFRetained};
use std::{mem::size_of, ptr::{self, NonNull}};

fn address(selector: u32, scope: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress { mSelector: selector, mScope: scope, mElement: kAudioObjectPropertyElementMain }
}
fn status(code: i32) -> Result<(), String> {
    if code == 0 { Ok(()) } else { Err(format!("Core Audio property query failed ({code})")) }
}
// These queries run on the control thread and read only HAL metadata.
unsafe fn scalar<T: Copy>(object: u32, selector: u32, initial: T) -> Result<T, String> {
    let mut key = address(selector, kAudioObjectPropertyScopeGlobal);
    let mut value = initial;
    let mut bytes = size_of::<T>() as u32;
    status(AudioObjectGetPropertyData(object, NonNull::from(&mut key), 0, ptr::null(),
        NonNull::from(&mut bytes), NonNull::from(&mut value).cast()))?;
    if bytes as usize != size_of::<T>() { return Err("Core Audio property has an unexpected size".into()); }
    Ok(value)
}
unsafe fn ids(object: u32, selector: u32, scope: u32) -> Result<Vec<u32>, String> {
    let mut key = address(selector, scope);
    // A device can reconnect between the size and data queries; retry a bounded number of times.
    for _ in 0..3 {
        let mut bytes = 0;
        status(AudioObjectGetPropertyDataSize(object, NonNull::from(&mut key), 0, ptr::null(), NonNull::from(&mut bytes)))?;
        if bytes == 0 { return Ok(Vec::new()); }
        if bytes > 1024 * 1024 || bytes as usize % size_of::<u32>() != 0 { return Err("Core Audio device data has an invalid size".into()); }
        let mut values = vec![0u32; bytes as usize / size_of::<u32>()];
        let capacity = bytes;
        let code = AudioObjectGetPropertyData(object, NonNull::from(&mut key), 0, ptr::null(),
            NonNull::from(&mut bytes), NonNull::new(values.as_mut_ptr()).unwrap().cast());
        if code == 0 && bytes <= capacity && bytes as usize % size_of::<u32>() == 0 {
            values.truncate(bytes as usize / size_of::<u32>()); return Ok(values);
        }
    }
    Err("Audio outputs changed during discovery. Refresh the device list.".into())
}
unsafe fn string(object: u32, selector: u32) -> Result<String, String> {
    let raw: *mut CFString = scalar(object, selector, ptr::null_mut())?;
    let pointer = NonNull::new(raw).ok_or("Core Audio returned an empty device property")?;
    // Name/UID properties transfer ownership of their CFString to the caller.
    let owned = CFRetained::from_raw(pointer);
    Ok(owned.to_string())
}
pub fn detect() -> Result<Report, String> {
    unsafe {
        let system = kAudioObjectSystemObject as u32;
        let default = scalar(system, kAudioHardwarePropertyDefaultOutputDevice, 0u32).ok();
        let mut outputs = Vec::new(); let mut diagnostics = Vec::new();
        for object in ids(system, kAudioHardwarePropertyDevices, kAudioObjectPropertyScopeGlobal)? {
            let streams = match ids(object, kAudioDevicePropertyStreams, kAudioObjectPropertyScopeOutput) {
                Ok(streams) => streams,
                Err(error) => { diagnostics.push(format!("Could not inspect Core Audio device {object}: {error}")); continue; }
            };
            if streams.is_empty() { continue; }
            let id = string(object, kAudioDevicePropertyDeviceUID).unwrap_or_else(|error| {
                diagnostics.push(format!("Device {object} has no persistent UID: {error}")); format!("coreaudio-object:{object}")
            });
            let name = string(object, kAudioObjectPropertyName).unwrap_or_else(|_| id.clone());
            let state = if scalar(object, kAudioDevicePropertyDeviceIsAlive, 0u32).unwrap_or(0) != 0 { "available" } else { "notPresent" };
            outputs.push(Output::detected(id, name, state, default == Some(object)));
        }
        outputs.sort_by(|a,b| b.is_default.cmp(&a.is_default).then_with(|| a.name.cmp(&b.name)));
        Ok(Report { platform: "macos".into(), backend: "Core Audio outputs".into(), outputs, diagnostics })
    }
}
