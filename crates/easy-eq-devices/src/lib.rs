use serde::Serialize;

#[cfg(windows)] mod windows;
#[cfg(target_os = "macos")] mod macos;
#[cfg(any(target_os = "linux", test))] mod linux;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    pub id: String,
    pub name: String,
    pub state: String,
    pub is_default: bool,
    pub support: String,
    pub reason: String,
    /// Shared playback format is a capability hint, not proof of the final
    /// speaker/headphone layout after vendor or spatial effects.
    pub channels: Option<u16>,
    pub sample_rate: Option<u32>,
}
impl Output {
    fn detected(id: String, name: String, state: &str, is_default: bool) -> Self {
        let (support, reason) = match state {
            "disabled" => ("unavailable", "This output is disabled in the operating system. Enable it to use EQ."),
            "unplugged" => ("unavailable", "This output is unplugged. Reconnect it to use EQ."),
            "notPresent" => ("unavailable", "This output is no longer present. Connect it or reinstall its device driver."),
            "error" => ("unavailable", "The audio service reports an output error. Check its connection and device settings."),
            _ => ("needsSetup", "Output detected. Easy EQ system audio integration is still in development; EQ has not been verified on this output."),
        };
        Self { id, name, state: state.into(), is_default, support: support.into(), reason: reason.into(),channels:None,sample_rate:None }
    }
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report { pub platform: String, pub backend: String, pub outputs: Vec<Output>, pub diagnostics: Vec<String> }

/// Read-only control-thread work. Never call discovery from an audio callback.
pub fn detect() -> Result<Report, String> {
    #[cfg(windows)] { windows::detect() }
    #[cfg(target_os = "macos")] { macos::detect() }
    #[cfg(target_os = "linux")] { linux::detect() }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))] { Err("This operating system has no audio adapter yet.".into()) }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test]
    fn detection_is_never_a_claim_that_eq_is_working() {
        let output = Output::detected("id".into(), "USB headphones".into(), "available", true);
        assert_eq!(output.support, "needsSetup");
        for state in ["disabled", "unplugged", "notPresent", "error"] {
            let output = Output::detected("id".into(), "Output".into(), state, false);
            assert_eq!(output.support, "unavailable");
            assert!(!output.reason.is_empty());
        }
    }
}
