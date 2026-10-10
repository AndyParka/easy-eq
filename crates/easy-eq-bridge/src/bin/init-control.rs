#[cfg(windows)]
fn main() {
    let spectrum = std::env::args_os().nth(1).is_some_and(|a| a == "--spectrum");
    let Some(path)=std::env::args_os().nth(if spectrum { 2 } else { 1 }) else { eprintln!("Usage: init-control [--spectrum] <new mapping path>");std::process::exit(2); };
    let result = if spectrum { easy_eq_bridge::windows::SpectrumMapping::initialize(std::path::Path::new(&path)) }
        else { easy_eq_bridge::windows::Mapping::initialize(std::path::Path::new(&path)) };
    if let Err(error)=result { eprintln!("{error}");std::process::exit(1); }
}
#[cfg(not(windows))]
fn main() { eprintln!("This installer helper is for Windows only.");std::process::exit(2); }
