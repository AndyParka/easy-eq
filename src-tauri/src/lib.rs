pub use easy_eq_core as audio_core;
mod audio;

use easy_eq_core::workspace::Workspace;
use std::{fs, path::PathBuf, sync::{atomic::{AtomicBool, Ordering}, Mutex}};
use tauri::{Emitter, Manager, menu::{Menu, MenuItem}, tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent}};
use tauri_plugin_dialog::DialogExt;
#[cfg(not(windows))]
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;
use std::io::Read;

#[derive(Default)]
struct Controller { workspace: Option<Workspace>, output: Option<(String, String)>, report: Option<easy_eq_devices::Report> }
struct Runtime { controller: Mutex<Controller>, quitting: AtomicBool }
fn file_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let directory = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    Ok(directory.join("workspace.json"))
}
fn persist(app: &tauri::AppHandle, workspace: &Workspace) -> Result<String, String> {
    workspace.validate().map_err(|e| e.to_string())?;
    let json = serde_json::to_string(workspace).map_err(|e| e.to_string())?;
    let path = file_path(&app)?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, &json).map_err(|e| e.to_string())?;
    fs::rename(temporary, path).map_err(|e| e.to_string())?;
    Ok(json)
}
#[tauri::command]
fn load_workspace(app: tauri::AppHandle, state: tauri::State<'_, Runtime>) -> Result<Option<String>, String> {
    let mut controller = state.controller.lock().map_err(|e| e.to_string())?;
    if let Some(workspace) = &controller.workspace { return serde_json::to_string(workspace).map(Some).map_err(|e| e.to_string()); }
    let path = file_path(&app)?;
    if !path.exists() { return Ok(None); }
    let mut json = String::new();
    fs::File::open(path).map_err(|e| e.to_string())?.take(4 * 1024 * 1024 + 1).read_to_string(&mut json).map_err(|e| e.to_string())?;
    let workspace = Workspace::parse(&json).map_err(|e| e.to_string())?;
    let json = serde_json::to_string(&workspace).map_err(|e| e.to_string())?;
    controller.workspace = Some(workspace);
    Ok(Some(json))
}
#[tauri::command]
fn save_workspace(app: tauri::AppHandle, json: String, base_json: Option<String>, state: tauri::State<'_, Runtime>) -> Result<String, String> {
    let client = Workspace::parse(&json).map_err(|e| e.to_string())?;
    let base = base_json.map(|raw| Workspace::parse(&raw)).transpose().map_err(|e| e.to_string())?;
    let mut controller = state.controller.lock().map_err(|e| e.to_string())?;
    let mut next = if let (Some(current), Some(base)) = (&controller.workspace, base.as_ref()) {
        let mut next = current.clone(); next.merge_edits(client, base).map_err(|e| e.to_string())?; next
    } else { client };
    next.follow_output(controller.output.as_ref().map(|(id,name)| (id.as_str(), name.as_str()))).map_err(|e| e.to_string())?;
    next.revision = controller.workspace.as_ref().map_or(1, |w| w.revision.saturating_add(1));
    let raw = persist(&app, &next)?; controller.workspace = Some(next); Ok(raw)
}
#[tauri::command]
fn configure_startup(app: tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    #[cfg(windows)]
    let manager = {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        // Use a quoted executable and a current-user entry even if the editor
        // was launched elevated. auto-launch's builder leaves its path unquoted.
        let _ = app;
        auto_launch::AutoLaunch::new("Peace 2", &format!("\"{}\"", executable.display()),
            auto_launch::WindowsEnableMode::CurrentUser, &["--tray"])
    };
    #[cfg(not(windows))]
    let manager = app.autolaunch();
    if enabled { manager.enable() } else { manager.disable() }.map_err(|e| e.to_string())?;
    manager.is_enabled().map_err(|e| e.to_string())
}
fn tray_enabled(app: &tauri::AppHandle, enabled: bool) {
    let result = (|| -> Result<(), String> {
        let state = app.state::<Runtime>();
        let mut controller = state.controller.lock().map_err(|e| e.to_string())?;
        if let Some(current) = &controller.workspace {
            let mut next = current.clone(); next.enabled = enabled; next.revision = next.revision.saturating_add(1);
            let json = persist(app, &next)?; controller.workspace = Some(next);
            let _ = app.emit("workspace-updated", json);
        }
        let _ = app.emit("eq-intent", enabled); Ok(())
    })();
    if let Err(error) = result { eprintln!("Tray state could not save: {error}"); }
}
fn monitor_outputs(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut previous = String::new();
        while !app.state::<Runtime>().quitting.load(Ordering::Relaxed) {
            match easy_eq_devices::detect() {
                Ok(mut report) => {
                    let output = report.outputs.iter().find(|o| o.is_default).map(|o| (o.id.clone(),o.name.clone()));
                    let update = (|| -> Result<Option<String>, String> {
                        let state = app.state::<Runtime>();
                        let mut controller = state.controller.lock().map_err(|e| e.to_string())?;
                        controller.output = output;
                        controller.report = Some(report.clone());
                        let Some(workspace) = &controller.workspace else { return Ok(None); };
                        let mut next = workspace.clone();
                        if !next.follow_output(controller.output.as_ref().map(|(id,name)| (id.as_str(),name.as_str()))).map_err(|e| e.to_string())? { return Ok(None); }
                        next.revision = next.revision.saturating_add(1);
                        let json = persist(&app, &next)?; controller.workspace = Some(next); Ok(Some(json))
                    })();
                    match update {
                        Ok(Some(json)) => { let _ = app.emit("workspace-updated", json); },
                        Err(error) => report.diagnostics.push(error), _ => (),
                    }
                    if let Ok(json) = serde_json::to_string(&report) {
                        if previous != json { previous = json; let _ = app.emit("outputs-updated", audio::augment(app.clone(), report)); }
                    }
                },
                Err(error) => { let _ = app.emit("outputs-error", error); }
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    });
}
#[tauri::command]
async fn import_profile(app: tauri::AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(chosen) = app.dialog().file().set_title("Import Easy EQ profile")
            .add_filter("Easy EQ profile", &["json"]).blocking_pick_file() else { return Ok(None); };
        let path = chosen.into_path().map_err(|e| e.to_string())?;
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        let mut json = String::new();
        file.take(1024 * 1024 + 1).read_to_string(&mut json).map_err(|e| e.to_string())?;
        easy_eq_core::parse_profile(&json).map_err(|e| e.to_string())?;
        Ok(Some(json))
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
async fn save_export(app: tauri::AppHandle, name: String, content: String) -> Result<bool, String> {
    let extension = match name.as_str() {
        "easy-eq-profile.json" | "easy-eq-macos-profile.json" => "json",
        "easy-eq.txt" => "txt",
        "easy-eq-filter-chain.conf" => "conf",
        _ => return Err("Unknown export format".into()),
    };
    if content.len() > 1024 * 1024 { return Err("Export exceeds 1 MB".into()); }
    if extension == "json" { easy_eq_core::parse_profile(&content).map_err(|e| e.to_string())?; }
    tauri::async_runtime::spawn_blocking(move || {
        let Some(chosen) = app.dialog().file().set_title("Export Easy EQ settings")
            .set_file_name(name).add_filter("Easy EQ export", &[extension]).blocking_save_file() else { return Ok(false); };
        let path = chosen.into_path().map_err(|e| e.to_string())?;
        fs::write(path, content).map_err(|e| e.to_string())?;
        Ok(true)
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
async fn detect_outputs(app: tauri::AppHandle) -> Result<easy_eq_devices::Report, String> {
    let report=tauri::async_runtime::spawn_blocking(easy_eq_devices::detect).await.map_err(|e| e.to_string())??;
    Ok(audio::augment(app,report))
}
#[tauri::command]
fn editor_ready(window: tauri::WebviewWindow) -> Result<(), String> {
    let url = window.url().map_err(|e| e.to_string())?;
    #[cfg(debug_assertions)]
    eprintln!("Easy EQ editor initialized from {url}; native storage and IPC are ready. Editor visible: {}", window.is_visible().unwrap_or(false));
    #[cfg(not(debug_assertions))]
    let _ = url;
    Ok(())
}
#[tauri::command]
fn audio_status(app:tauri::AppHandle)->Result<serde_json::Value,String> {
    #[cfg(windows)] {
        let runtime=app.state::<audio::AudioRuntime>();
        let statuses=runtime.statuses.lock().map_err(|e|e.to_string())?;
        serde_json::to_value(&*statuses).map_err(|e|e.to_string())
    }
    #[cfg(not(windows))] {let _=app;Ok(serde_json::json!([]))}
}
pub fn check_embedded_assets() -> Result<usize, String> {
    let context: tauri::Context<tauri::Wry> = tauri::generate_context!();
    let assets = context.assets();
    for path in ["index.html", "eq-worklet.js"] {
        if assets.get(&path.into()).is_none() { return Err(format!("Missing embedded asset: {path}")); }
    }
    let paths: Vec<_> = assets.iter().map(|(path, _)| path.to_string()).collect();
    if !paths.iter().any(|path| path.ends_with(".css")) || !paths.iter().any(|path| path.trim_start_matches('/').starts_with("assets/") && path.ends_with(".js")) {
        return Err("Editor JavaScript or stylesheet is not embedded".into());
    }
    Ok(paths.len())
}
fn show_editor(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show(); let _ = window.unminimize(); let _ = window.set_focus();
    }
}
fn quit(app:&tauri::AppHandle) {
    app.state::<Runtime>().quitting.store(true,Ordering::Relaxed);
    audio::bypass(app);app.exit(0);
}
#[cfg(debug_assertions)]
fn debug_lifecycle_command(app:&tauri::AppHandle,args:&[String])->bool {
    if args.iter().any(|a|a=="--test-close-editor") {
        if let Some(window)=app.get_webview_window("main") { let _=window.close(); }
    } else if args.iter().any(|a|a=="--test-tray-off") {tray_enabled(app,false);}
    else if args.iter().any(|a|a=="--test-tray-on") {tray_enabled(app,true);}
    else if args.iter().any(|a|a=="--test-tray-quit") {quit(app);}
    else {return false;}
    true
}
fn tray_icon() -> tauri::image::Image<'static> {
    let mut pixels = vec![0u8; 32 * 32 * 4];
    for y in 0..32 { for x in 0..32 {
        let distance = (x as f64 - 15.5).powi(2) + (y as f64 - 15.5).powi(2);
        if distance < 200.0 {
            let i = (y * 32 + x) * 4;
            let color = if (x > 8 && x < 12 && y > 9 && y < 26) || (distance > 20.0 && distance < 70.0 && y < 22) { [121, 218, 189, 255] } else { [24, 33, 40, 255] };
            pixels[i..i + 4].copy_from_slice(&color);
        }
    }}
    tauri::image::Image::new_owned(pixels, 32, 32)
}
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            #[cfg(debug_assertions)]
            if debug_lifecycle_command(app,&args) {return;}
            if !args.iter().any(|arg| arg == "--tray") { show_editor(app); }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::Builder::new().args(["--tray"]).build())
        .manage(Runtime { controller: Mutex::new(Controller::default()), quitting: AtomicBool::new(false) })
        .manage(audio::AudioRuntime::default())
        .invoke_handler(tauri::generate_handler![save_workspace, load_workspace, import_profile, save_export, detect_outputs, editor_ready, configure_startup, audio_status])
        .setup(|app| {
            let show = MenuItem::with_id(app, "show", "Open Easy EQ", true, None::<&str>)?;
            let bypass = MenuItem::with_id(app, "bypass", "Turn EQ off", true, None::<&str>)?;
            let enable = MenuItem::with_id(app, "enable", "Turn EQ on", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit Easy EQ", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &bypass, &enable, &quit_item])?;
            TrayIconBuilder::new().icon(tray_icon()).tooltip("Easy EQ · background headphone EQ")
                .menu(&menu).show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_editor(app),
                    "bypass" => tray_enabled(app, false),
                    "enable" => tray_enabled(app, true),
                    "quit" => quit(app),
                    _ => (),
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(event, TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }) { show_editor(tray.app_handle()); }
                }).build(app)?;
            if !std::env::args().any(|arg| arg == "--tray") {
                if let Some(window) = app.get_webview_window("main") { let _ = window.show(); }
            }
            if let Err(error) = load_workspace(app.handle().clone(), app.state::<Runtime>()) { eprintln!("Saved workspace could not be read: {error}"); }
            monitor_outputs(app.handle().clone());
            audio::start(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if !window.app_handle().state::<Runtime>().quitting.load(Ordering::Relaxed) {
                    api.prevent_close(); let _ = window.hide();
                    #[cfg(debug_assertions)]
                    eprintln!("Editor close intercepted; native host remains active. Editor visible: {}",window.is_visible().unwrap_or(true));
                }
            }
        })
        .build(tauri::generate_context!()).expect("Easy EQ could not start");
    app.run(|app, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            if !app.state::<Runtime>().quitting.load(Ordering::Relaxed) { api.prevent_exit(); }
        }
    });
}
