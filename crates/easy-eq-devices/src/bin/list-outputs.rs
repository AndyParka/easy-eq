fn main() {
    match easy_eq_devices::detect() {
        Ok(report) => println!("{}", serde_json::to_string_pretty(&report).unwrap()),
        Err(error) => { eprintln!("{error}"); std::process::exit(1); }
    }
}
