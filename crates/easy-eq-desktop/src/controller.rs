use easy_eq_core::{Band,FilterKind,Profile,workspace::Workspace};
use easy_eq_bridge::windows::{AudioStatus,Host};
use std::{fs,path::PathBuf,sync::{Arc,Mutex,mpsc},time::{Duration,Instant}};

#[derive(Clone)]
pub struct Snapshot {pub workspace:Workspace,pub outputs:Option<easy_eq_devices::Report>,pub audio:Vec<AudioStatus>,pub error:Option<String>,pub acknowledged:u64,pub saved_revision:u64}
pub enum Command {Edit {client:Workspace,base:Workspace,sequence:u64},Enabled(bool),Refresh,Quit}
pub struct Controller {pub shared:Arc<Mutex<Snapshot>>,pub sender:mpsc::Sender<Command>,thread:Option<std::thread::JoinHandle<()>>}
pub fn data_dir()->PathBuf {PathBuf::from(std::env::var_os("APPDATA").unwrap_or_else(|| ".".into())).join("technology.parkinson.peace2")}
pub fn flat(id:String,name:String)->Profile {
    let mut p=Profile::flat(id,name);
    p.common=flat_bands();
    p
}
pub fn flat_bands()->Vec<Band> {
    let mut bands=Vec::new();
    for (i,frequency) in [31.0,63.0,125.0,250.0,500.0,750.0,1000.0,1500.0,2000.0,4000.0,8000.0,12000.0,16000.0].into_iter().enumerate() {
        let band=Band {id:format!("band-{i}"),kind:FilterKind::Peak,frequency,gain:0.0,q:1.4,enabled:true};
        bands.push(band);
    }
    bands
}
fn load_workspace(raw:&str)->Result<Workspace,String> {
    let mut workspace=Workspace::parse(raw).map_err(|e|e.to_string())?;
    let document:serde_json::Value=serde_json::from_str(raw).map_err(|e|e.to_string())?;
    // Existing individual correction is retained. Only a missing shared stage
    // receives visible zero-gain controls; an intentionally empty stage stays empty.
    for profile in &mut workspace.profiles {
        let legacy=document["profiles"].as_array().is_some_and(|profiles|profiles.iter().any(|p|p["id"].as_str()==Some(&profile.id) && p.get("common").is_none()));
        if legacy {
            let room=easy_eq_core::MAX_BANDS-profile.ears.left.len().max(profile.ears.right.len());
            profile.common=flat_bands().into_iter().take(room).collect();
        }
    }
    workspace.validate().map_err(|e|e.to_string())?;
    Ok(workspace)
}
fn initial()->Workspace {
    let p=flat("default".into(),"Flat".into());
    Workspace {active_id:p.id.clone(),profiles:vec![p],enabled:true,view:"overlay".into(),settings:Default::default(),revision:0}
}
fn persist(workspace:&Workspace)->Result<(),String> {
    workspace.validate().map_err(|e|e.to_string())?;
    let directory=data_dir();fs::create_dir_all(&directory).map_err(|e|e.to_string())?;
    let path=directory.join("workspace.json");let temporary=directory.join("workspace.native.tmp");
    fs::write(&temporary,serde_json::to_vec(workspace).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    fs::rename(temporary,path).map_err(|e|e.to_string())
}
pub fn set_startup_preference(enabled:bool)->Result<(),String> {
    let path=data_dir().join("workspace.json");
    let mut workspace=if path.exists(){load_workspace(&fs::read_to_string(path).map_err(|e|e.to_string())?)?}else{initial()};
    workspace.settings.start_at_login=enabled;
    workspace.revision=workspace.revision.saturating_add(1);
    persist(&workspace)
}
pub fn configure_startup(enabled:bool)->Result<(),String> {
    let executable=std::env::current_exe().map_err(|e|e.to_string())?;
    use std::os::windows::process::CommandExt;
    let result=std::process::Command::new("powershell.exe")
        .args(["-NoProfile","-NonInteractive","-Command",include_str!("../../../scripts/configure-startup.ps1")])
        .env("PEACE2_EXECUTABLE",&executable).env("PEACE2_STARTUP",if enabled {"enable"}else{"disable"})
        .creation_flags(0x08000000).output().map_err(|e|e.to_string())?;
    if result.status.success(){Ok(())}else{Err(format!("Startup task configuration failed ({}): {} {}",result.status,String::from_utf8_lossy(&result.stdout).trim(),String::from_utf8_lossy(&result.stderr).trim()))}
}
impl Controller {
    pub fn start(ctx:eframe::egui::Context)->Self {
        let path=data_dir().join("workspace.json");
        let (workspace,error)=if path.exists() {
            match fs::read_to_string(&path).map_err(|e|e.to_string()).and_then(|raw|load_workspace(&raw)) {
                Ok(w)=>(w,None),Err(e)=>(initial(),Some(format!("Saved settings could not be read; the existing file is preserved: {e}"))),
            }
        } else {(initial(),None)};
        let saved_revision=workspace.revision;
        let shared=Arc::new(Mutex::new(Snapshot {workspace,outputs:None,audio:vec![],error,acknowledged:0,saved_revision}));
        let (sender,receiver)=mpsc::channel();let thread_shared=shared.clone();
        let thread=std::thread::spawn(move || {
            let mut host=Host::default();let mut snapshot=thread_shared.lock().unwrap().clone();
            let (discovery_request,discovery_commands)=mpsc::channel();
            let (report_sender,reports)=mpsc::channel();
            let discovery_stop=Arc::new(std::sync::atomic::AtomicBool::new(false));let stop=discovery_stop.clone();
            let discovery_thread=std::thread::spawn(move || {
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    let _=report_sender.send(easy_eq_devices::detect());
                    let _=discovery_commands.recv_timeout(Duration::from_secs(2));
                }
            });
            let mut saving=false;let mut last_save=Instant::now();
            let heartbeat=crate::supervisor::heartbeat().ok();
            let resume=crate::supervisor::resume_event().ok();
            let mut previous_tick=easy_eq_bridge::windows::now_ms();
            let read_only=snapshot.error.as_ref().is_some_and(|e|e.starts_with("Saved settings could not be read"));
            let mut startup=snapshot.workspace.settings.start_at_login;
            if let Err(e)=configure_startup(startup) {snapshot.error=Some(format!("Sign-in startup: {e}"));}
            loop {
                let now=easy_eq_bridge::windows::now_ms();
                let signalled=resume.as_ref().is_some_and(|e|e.drain());
                if signalled || now.saturating_sub(previous_tick)>2_000 {
                    host=Host::default();
                    let _=discovery_request.send(());
                    crate::log("Audio control resumed: reopened bridge mappings and requested fresh output discovery.");
                }
                previous_tick=now;
                if let Some(event)=&heartbeat {event.signal();}
                let mut changed=false;let mut quitting=false;
                for command in receiver.try_iter() {
                    match command {
                        Command::Edit {client,base,sequence}=>{
                            let mut next=snapshot.workspace.clone();
                            match next.merge_edits(client,&base) {
                                Ok(())=>{next.revision=next.revision.saturating_add(1);snapshot.workspace=next;saving=true;},
                                Err(e)=>snapshot.error=Some(format!("Settings rejected: {e}")),
                            }
                            snapshot.acknowledged=sequence;changed=true;
                        },
                        Command::Enabled(value)=>{snapshot.workspace.enabled=value;snapshot.workspace.revision=snapshot.workspace.revision.saturating_add(1);saving=true;changed=true;},
                        Command::Refresh=>{let _=discovery_request.send(());},
                        Command::Quit=>{quitting=true;break;},
                    }
                }
                if quitting {host.bypass();if saving && !read_only {let _=persist(&snapshot.workspace);}discovery_stop.store(true,std::sync::atomic::Ordering::Relaxed);let _=discovery_request.send(());let _=discovery_thread.join();break;}
                for report in reports.try_iter() {
                    match report {Ok(report)=>snapshot.outputs=Some(report),Err(e)=>snapshot.error=Some(e)}
                    changed=true;
                }
                if let Some(report)=&snapshot.outputs {
                    let default=report.outputs.iter().find(|o|o.is_default);
                    let previous_profiles=snapshot.workspace.profiles.len();
                    match snapshot.workspace.follow_output(default.map(|o|(o.id.as_str(),o.name.as_str()))) {
                        Ok(true)=>{snapshot.workspace.revision=snapshot.workspace.revision.saturating_add(1);saving=true;changed=true;},
                        Err(e)=>snapshot.error=Some(e.to_string()),_=>(),
                    }
                    for profile in snapshot.workspace.profiles.iter_mut().skip(previous_profiles) {profile.common=flat_bands();}
                    let outputs:Vec<_>=report.outputs.iter().map(|o|(o.id.clone(),o.state.clone(),o.channels)).collect();
                    let audio=host.update(Some(&snapshot.workspace),&outputs);
                    if serde_json::to_string(&audio).ok()!=serde_json::to_string(&snapshot.audio).ok() {
                        if let Some(default)=default {if let Some(s)=audio.iter().find(|s|s.output_id==default.id) {crate::log(&format!("System audio: {} — {}",s.state,s.reason));}}
                        snapshot.audio=audio;changed=true;
                    }
                }
                if startup!=snapshot.workspace.settings.start_at_login {
                    startup=snapshot.workspace.settings.start_at_login;
                    if let Err(e)=configure_startup(startup) {snapshot.error=Some(format!("Sign-in startup: {e}"));}changed=true;
                }
                if saving && last_save.elapsed()>=Duration::from_millis(180) {
                    // A corrupt pre-existing workspace requires an explicit
                    // recovery before persistence; never silently overwrite it.
                    if !read_only {
                        match persist(&snapshot.workspace) {
                            Ok(())=>{snapshot.saved_revision=snapshot.workspace.revision;if snapshot.error.as_ref().is_some_and(|e|e.starts_with("Could not save settings:")){snapshot.error=None;}},
                            Err(e)=>snapshot.error=Some(format!("Could not save settings: {e}")),
                        }
                    }
                    saving=false;last_save=Instant::now();changed=true;
                }
                if changed { *thread_shared.lock().unwrap()=snapshot.clone();ctx.request_repaint(); }
                std::thread::sleep(Duration::from_millis(60));
            }
        });
        Self {shared,sender,thread:Some(thread)}
    }
    pub fn snapshot(&self)->Snapshot {self.shared.lock().unwrap().clone()}
    pub fn stop(&mut self) {let _=self.sender.send(Command::Quit);if let Some(thread)=self.thread.take(){let _=thread.join();}}
}
impl Drop for Controller {fn drop(&mut self){self.stop();}}
