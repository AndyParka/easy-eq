#[cfg(windows)]
fn main() {
    use easy_eq_bridge::windows::{SpectrumMapping, now_ms};
    let Some(path) = std::env::args_os().nth(1) else { eprintln!("Usage: inspect-spectrum <spectrum.bin> [--request]"); std::process::exit(2); };
    let mapping = SpectrumMapping::open(std::path::Path::new(&path)).unwrap_or_else(|e| { eprintln!("{e}"); std::process::exit(1) });
    if std::env::args().any(|a| a == "--request") {
        mapping.block().request(now_ms()); std::thread::sleep(std::time::Duration::from_millis(200));
    }
    if let Some((sequence, frame)) = mapping.block().read(0) {
        let peak = |samples: &[f32]| samples.iter().map(|v| v.abs()).fold(0.0f32, f32::max);
        println!("sequence={sequence} age_ms={} pid={} rate={} input_l={} input_r={} output_l={} output_r={}",
            now_ms().saturating_sub(frame.captured_ms), frame.pid, frame.rate,
            peak(&frame.input[0]), peak(&frame.input[1]), peak(&frame.output[0]), peak(&frame.output[1]));
    } else { eprintln!("No stable spectrum frame is available."); std::process::exit(1); }
}
#[cfg(not(windows))]
fn main() { std::process::exit(2); }
