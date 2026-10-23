//! A user-session watchdog. The graphical process may lose its GPU on resume;
//! this process has no window, GPU context, or audio-device COM dependency.
use crate::instance::{Event, NamedMutex};
use std::{process::{Child, Command}, time::Duration};
use std::os::windows::process::CommandExt;

const STOP: &str = "Local\\Peace2.Native.Supervisor.Stop";
const HEARTBEAT: &str = "Local\\Peace2.Native.Supervisor.Heartbeat";
const RESUME: &str = "Local\\Peace2.Native.Supervisor.Resume";
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn heartbeat() -> Result<Event, String> { Event::open(HEARTBEAT) }
pub fn resume_event() -> Result<Event, String> { Event::open(RESUME) }
pub fn request_stop() { if let Ok(event) = Event::open(STOP) { event.signal(); } }

#[derive(Default)]
struct Health { last_tick: u64, last_heartbeat: u64 }
impl Health {
    fn tick(&mut self, now: u64, heartbeat: bool) -> (bool, bool) {
        let resumed = self.last_tick != 0 && now.saturating_sub(self.last_tick) > 5_000;
        if self.last_tick == 0 || resumed || heartbeat { self.last_heartbeat = now; }
        self.last_tick = now;
        (resumed, now.saturating_sub(self.last_heartbeat) > 15_000)
    }
}

fn stop_child(child: &mut Child) {
    if let Ok(instance) = crate::instance::Instance::acquire() {
        if !instance.primary { instance.signal(3); }
    }
    for _ in 0..50 {
        if child.try_wait().ok().flatten().is_some() { return; }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub fn run(args: &[String]) -> Result<(), String> {
    let owner = NamedMutex::acquire("Local\\Peace2.Native.Supervisor")?;
    if !owner.primary {
        if !args.iter().any(|a| a == "--tray") {
            // The primary may still be creating the editor. Retain the show
            // request until that editor has acquired its instance events.
            for _ in 0..50 {
                let instance = crate::instance::Instance::acquire()?;
                if !instance.primary { instance.signal(0); break; }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        return Ok(());
    }
    let stop = Event::open(STOP)?;
    let beat = heartbeat()?;
    let resume = resume_event()?;
    stop.drain(); beat.drain(); resume.drain();
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let status_path = crate::controller::data_dir().join("supervisor.json");
    let mut retries = 0u64;
    loop {
        if stop.drain() { break; }
        let mut command = Command::new(&executable);
        command.arg("--worker").creation_flags(CREATE_NO_WINDOW);
        if retries == 0 { command.args(args.iter().skip(1)); }
        else { command.arg("--tray"); }
        let mut child = command.spawn().map_err(|e| format!("Could not start the editor: {e}"))?;
        let started = easy_eq_bridge::windows::now_ms();
        let _ = std::fs::create_dir_all(crate::controller::data_dir());
        let status = serde_json::json!({"supervisorPid":std::process::id(),"workerPid":child.id(),"restarts":retries});
        let _ = std::fs::write(&status_path, status.to_string());
        crate::log(&format!("Watchdog started editor PID {} (restart {retries}).", child.id()));
        let mut health = Health::default();
        loop {
            if stop.drain() {
                stop_child(&mut child);
                let _ = std::fs::remove_file(&status_path);
                crate::log("Watchdog stopped by explicit Quit.");
                return Ok(());
            }
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                crate::log(&format!("Editor exited ({status}); watchdog will restore it in the tray."));
                break;
            }
            let (resumed, stalled) = health.tick(easy_eq_bridge::windows::now_ms(), beat.drain());
            if resumed { resume.signal(); crate::log("Watchdog detected resume or a scheduling gap; refreshing audio with a recovery grace period."); }
            if stalled {
                crate::log("Audio control heartbeat stalled for 15 seconds; restarting the editor.");
                let _ = child.kill(); let _ = child.wait(); break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        // Bound crash loops and remain responsive to Quit during backoff.
        let stable = easy_eq_bridge::windows::now_ms().saturating_sub(started) > 60_000;
        retries = if stable { 1 } else { retries.saturating_add(1) };
        for _ in 0..(retries.min(15) * 5) {
            if stop.drain() { let _ = std::fs::remove_file(&status_path); return Ok(()); }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let _ = std::fs::remove_file(status_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Health;
    #[test]
    fn resume_grants_a_fresh_heartbeat_window_and_stalls_recover() {
        let mut health = Health::default();
        assert_eq!(health.tick(1_000, false), (false, false));
        for now in (1_200..17_000).step_by(200) { health.tick(now, false); }
        assert_eq!(health.tick(17_000, false), (false, true));
        assert_eq!(health.tick(17_200, true), (false, false));
        assert_eq!(health.tick(90_000, false), (true, false));
        assert_eq!(health.tick(90_200, true), (false, false));
    }
}
