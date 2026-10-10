#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
fn main() {
    if std::env::args().any(|arg| arg == "--check-assets") {
        match easy_eq_lib::check_embedded_assets() {
            Ok(count) => println!("Verified {count} embedded UI assets; no development server is required."),
            Err(error) => { eprintln!("{error}"); std::process::exit(1); }
        }
    } else { easy_eq_lib::run(); }
}
