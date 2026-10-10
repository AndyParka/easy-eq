use tauri::{Emitter,Manager};
#[cfg(windows)]
use easy_eq_bridge::windows::{AudioStatus,Host};
#[derive(Default)]
pub struct AudioRuntime {
    #[cfg(windows)] pub host:std::sync::Mutex<Host>,
    #[cfg(windows)] pub statuses:std::sync::Mutex<Vec<AudioStatus>>,
}
pub fn bypass(app:&tauri::AppHandle) {
    #[cfg(windows)]
    if let Ok(host)=app.state::<AudioRuntime>().host.lock() { host.bypass(); }
    #[cfg(not(windows))] let _=app;
}
pub fn augment(app:tauri::AppHandle,mut report:easy_eq_devices::Report)->easy_eq_devices::Report {
    #[cfg(windows)]
    if let Ok(statuses)=app.state::<AudioRuntime>().statuses.lock() {
        for output in &mut report.outputs {
            if let Some(status)=statuses.iter().find(|s| s.output_id==output.id) {
                output.support=status.state.clone(); output.reason=status.reason.clone();
            }
        }
    }
    #[cfg(not(windows))] let _=app;
    report
}
pub fn start(app:tauri::AppHandle) {
    #[cfg(windows)]
    std::thread::spawn(move || {
        let mut previous=String::new();
        while !app.state::<super::Runtime>().quitting.load(std::sync::atomic::Ordering::Relaxed) {
            let snapshot=app.state::<super::Runtime>().controller.lock().ok().map(|c| (c.workspace.clone(),c.report.clone()));
            if let Some((workspace,Some(report)))=snapshot {
                let outputs:Vec<_>=report.outputs.iter().map(|o| (o.id.clone(),o.state.clone(),o.channels)).collect();
                if let Ok(mut host)=app.state::<AudioRuntime>().host.lock() {
                    let statuses=host.update(workspace.as_ref(),&outputs);
                    let json=serde_json::to_string(&statuses).unwrap_or_default();
                    if let Ok(mut current)=app.state::<AudioRuntime>().statuses.lock() { *current=statuses.clone(); }
                    if json!=previous {
                        previous=json; let _=app.emit("audio-updated",&statuses);
                        let _=app.emit("outputs-updated",augment(app.clone(),report));
                        #[cfg(debug_assertions)]
                        if let Some(default)=outputs.iter().find(|(id,_,_)| workspace.as_ref().and_then(|w| w.settings.last_output_id.as_ref())==Some(id)) {
                            if let Some(status)=statuses.iter().find(|s| s.output_id==default.0) { eprintln!("System audio: {} — {}",status.state,status.reason); }
                        }
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        bypass(&app);
    });
    #[cfg(not(windows))] let _=app;
}
