use super::{Output, Report};
use windows::{core::Result, Win32::{Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
    Media::Audio::{eRender, eMultimedia, MMDeviceEnumerator, IMMDevice, IMMDeviceEnumerator, DEVICE_STATE, DEVICE_STATEMASK_ALL,
        DEVICE_STATE_ACTIVE, DEVICE_STATE_DISABLED, DEVICE_STATE_UNPLUGGED, IAudioClient},
    System::Com::{CoInitializeEx, CoUninitialize, CoCreateInstance, CoTaskMemFree, COINIT_MULTITHREADED, CLSCTX_ALL, STGM_READ,
        StructuredStorage::{PropVariantClear, PropVariantToStringAlloc}}}};

struct ComApartment;
impl Drop for ComApartment { fn drop(&mut self) { unsafe { CoUninitialize(); } } }
unsafe fn id(device: &IMMDevice) -> Result<String> {
    let raw = device.GetId()?;
    let value = raw.to_string();
    CoTaskMemFree(Some(raw.0.cast()));
    Ok(value?)
}
unsafe fn name(device: &IMMDevice) -> Result<String> {
    let properties = device.OpenPropertyStore(STGM_READ)?;
    let mut value = properties.GetValue(&PKEY_Device_FriendlyName)?;
    let text = PropVariantToStringAlloc(&value);
    let _ = PropVariantClear(&mut value);
    let raw = text?;
    let value = raw.to_string();
    CoTaskMemFree(Some(raw.0.cast()));
    Ok(value?)
}
fn enumerate() -> Result<Report> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        let _apartment = ComApartment;
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let default = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia).ok().and_then(|d| id(&d).ok());
        let collection = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE(DEVICE_STATEMASK_ALL))?;
        let mut outputs = Vec::new();
        let mut diagnostics = Vec::new();
        for index in 0..collection.GetCount()? {
            let result = (|| -> Result<Output> {
                let device = collection.Item(index)?;
                let key = id(&device)?;
                let display = name(&device).unwrap_or_else(|_| key.clone());
                let state = device.GetState()?;
                let state = if state == DEVICE_STATE_ACTIVE { "available" }
                    else if state == DEVICE_STATE_DISABLED { "disabled" }
                    else if state == DEVICE_STATE_UNPLUGGED { "unplugged" } else { "notPresent" };
                let mut output=Output::detected(key.clone(), display, state, default.as_ref() == Some(&key));
                if state=="available" {
                    if let Ok(client)=device.Activate::<IAudioClient>(CLSCTX_ALL,None) {
                        if let Ok(format)=client.GetMixFormat() {
                            output.channels=Some((*format).nChannels);output.sample_rate=Some((*format).nSamplesPerSec);
                            CoTaskMemFree(Some(format.cast()));
                        }
                    }
                }
                Ok(output)
            })();
            match result { Ok(output) => outputs.push(output), Err(error) => diagnostics.push(format!("Could not inspect Windows output {index}: {error}")) }
        }
        outputs.sort_by(|a,b| b.is_default.cmp(&a.is_default).then_with(|| a.name.cmp(&b.name)));
        Ok(Report { platform: "windows".into(), backend: "Windows audio endpoints".into(), outputs, diagnostics })
    }
}
pub fn detect() -> std::result::Result<Report, String> {
    // A fresh apartment avoids interference with the UI's COM threading model.
    std::thread::spawn(enumerate).join().map_err(|_| "Windows output discovery failed".to_string())?
        .map_err(|e| format!("Windows could not enumerate audio outputs: {e}"))
}
