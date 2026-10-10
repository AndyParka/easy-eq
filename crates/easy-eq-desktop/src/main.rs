#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(not(windows))] compile_error!("The new desktop UI currently targets Windows; shared DSP remains portable.");
mod controller;
mod editor;
mod graph;
mod instance;
mod spectrum;
mod ui;
use eframe::egui;
pub fn log(message:&str) {
    use std::io::Write;
    let directory=controller::data_dir();let _=std::fs::create_dir_all(&directory);
    if let Ok(mut file)=std::fs::OpenOptions::new().create(true).append(true).open(directory.join("native.log")){let _=writeln!(file,"{message}");}
    eprintln!("{message}");
}
fn main() {
    let instance=match instance::Instance::acquire(){Ok(i)=>i,Err(e)=>{log(&e);return;}};
    let args:Vec<_>=std::env::args().collect();
    let command=if args.iter().any(|a|a=="--test-tray-off"){1}else if args.iter().any(|a|a=="--test-tray-on"){2}else if args.iter().any(|a|a=="--test-tray-quit"){3}else if args.iter().any(|a|a=="--test-close-editor"){4}else if cfg!(debug_assertions) && args.iter().any(|a|a=="--test-controls"){5}else if cfg!(debug_assertions) && args.iter().any(|a|a=="--test-restore-controls"){6}else{0};
    if !instance.primary {if command!=0 || !args.iter().any(|a|a=="--tray"){instance.signal(command);}return;}
    if command==3 {return;}
    let hidden=args.iter().any(|a|a=="--tray");
    let screenshot=args.iter().position(|a|a=="--screenshot").and_then(|i|args.get(i+1)).map(std::path::PathBuf::from);
    let compact=args.iter().any(|a|a=="--compact");
    let size=if compact {[840.0,860.0]}else{[1000.0,940.0]};
    let native=eframe::NativeOptions {viewport:egui::ViewportBuilder::default().with_title("Easy EQ").with_inner_size(size).with_min_inner_size([800.0,620.0]).with_visible(!hidden),
        renderer:eframe::Renderer::Glow,centered:true,persist_window:false,..Default::default()};
    log(&format!("Starting native Rust/egui window. Tray startup: {hidden}"));
    let result=eframe::run_native("Easy EQ",native,Box::new(move |cc|Ok(Box::new(ui::Desktop::new(cc,instance,hidden,screenshot)))));
    if let Err(error)=result {log(&format!("Native window could not start: {error}"));}
}
