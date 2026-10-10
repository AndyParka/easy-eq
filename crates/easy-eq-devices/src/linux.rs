use super::{Output, Report};
use serde_json::Value;
#[cfg(target_os = "linux")]
use std::{io::Read, process::{Command, Stdio}, time::{Duration, Instant}};

pub(super) fn parse(json: &str) -> Result<Report, String> {
    let values: Vec<Value> = serde_json::from_str(json).map_err(|e| format!("PipeWire returned invalid device data: {e}"))?;
    let default = values.iter().flat_map(|v| v["metadata"].as_array().into_iter().flatten())
        .find(|v| v["key"] == "default.audio.sink")
        .and_then(|v| v["value"].as_str()).and_then(|v| serde_json::from_str::<Value>(v).ok())
        .and_then(|v| v["name"].as_str().map(str::to_owned));
    let mut outputs = Vec::new();
    let mut diagnostics = Vec::new();
    for value in values.iter().filter(|v| v["type"] == "PipeWire:Interface:Node") {
        let props = &value["info"]["props"];
        if !props["media.class"].as_str().is_some_and(|v| v.starts_with("Audio/Sink")) { continue; }
        let Some(id) = props["node.name"].as_str() else { diagnostics.push("An output has no stable PipeWire node name.".into()); continue; };
        let name = props["node.description"].as_str().or_else(|| props["node.nick"].as_str()).unwrap_or(id);
        let state = if value["info"]["state"] == "error" { "error" } else { "available" };
        outputs.push(Output::detected(id.into(), name.into(), state, default.as_deref() == Some(id)));
    }
    outputs.sort_by(|a,b| b.is_default.cmp(&a.is_default).then_with(|| a.name.cmp(&b.name)));
    Ok(Report { platform: "linux".into(), backend: "PipeWire outputs".into(), outputs, diagnostics })
}
#[cfg(target_os = "linux")]
pub fn detect() -> Result<Report, String> {
    let mut child = Command::new("pw-dump").args(["--no-colors"]).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()
        .map_err(|e| format!("PipeWire output discovery needs pw-dump. Install PipeWire's tools and check that its audio service is running. ({e})"))?;
    let stdout = child.stdout.take().ok_or("PipeWire discovery has no output stream")?;
    let reader = std::thread::spawn(move || -> Result<String, String> {
        let mut json = String::new();
        stdout.take(8 * 1024 * 1024 + 1).read_to_string(&mut json).map_err(|e| e.to_string())?;
        if json.len() > 8 * 1024 * 1024 { return Err("PipeWire device data exceeds 8 MB".into()); }
        Ok(json)
    });
    let start = Instant::now();
    let exit = loop {
        match child.try_wait() {
            Ok(Some(exit)) => break Ok(exit),
            Ok(None) if start.elapsed() < Duration::from_secs(5) => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => break Err("PipeWire discovery timed out. Check that its audio service is running.".to_string()),
            Err(e) => break Err(format!("PipeWire discovery failed: {e}")),
        }
    };
    if exit.is_err() { let _ = child.kill(); let _ = child.wait(); }
    let json = reader.join().map_err(|_| "PipeWire device reader failed".to_string())??;
    if !exit?.success() { return Err("PipeWire could not list outputs. Check that its audio service is running and accessible.".into()); }
    parse(&json)
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn includes_virtual_and_error_outputs_but_excludes_inputs() {
        let report = parse(r#"[
          {"type":"PipeWire:Interface:Metadata","metadata":[{"key":"default.audio.sink","value":"{\"name\":\"usb\"}"}]},
          {"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Sink","node.name":"usb","node.description":"USB Headphones"},"state":"suspended"}},
          {"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Sink/Virtual","node.name":"virtual"},"state":"error"}},
          {"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Source","node.name":"mic"},"state":"running"}}
        ]"#).unwrap();
        assert_eq!(report.outputs.len(), 2);
        assert!(report.outputs[0].is_default);
        assert_eq!(report.outputs[0].support, "needsSetup");
        assert_eq!(report.outputs[1].support, "unavailable");
        assert!(parse("not json").is_err());
    }
}
