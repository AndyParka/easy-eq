#[cfg(windows)]
fn main() {
    use std::sync::atomic::Ordering;
    use easy_eq_bridge::windows::{Mapping,now_ms};
    let Some(path)=std::env::args_os().nth(1) else {eprintln!("Usage: inspect-control <control.bin>");std::process::exit(2);};
    let mapping=Mapping::open(std::path::Path::new(&path)).unwrap();let b=mapping.block();let now=now_ms();
    println!("now_ms={now} lease_live={} enabled={} callback_age_ms={} callback_pid={} processing={} published={} applied={} rate={}",
        now<b.expires_ms.load(Ordering::Acquire),b.enabled.load(Ordering::Acquire),now.saturating_sub(b.callback_ms.load(Ordering::Acquire)),
        b.callback_pid.load(Ordering::Acquire),b.processing_enabled.load(Ordering::Acquire),b.sequence(),b.applied_sequence.load(Ordering::Acquire),f64::from_bits(b.device_rate.load(Ordering::Acquire)));
    if std::env::args().any(|a|a=="--response") {
        if let Some((_,graph))=b.read_graph(0) {
            let p=graph.parameters();
            let response=|ear:usize| 20.0*(p.gain*p.matrix[if ear==0{0}else{3}]).max(1e-12).log10()
                + p.coefficients[ear][..p.counts[ear]].iter().map(|c|c.magnitude_db(1000.0,p.rate)).sum::<f64>();
            println!("graph_rate={} left_filters={} right_filters={} left_1000_db={:.5} right_1000_db={:.5} cross_lr={} cross_rl={}",
                p.rate,p.counts[0],p.counts[1],response(0),response(1),p.matrix[1],p.matrix[2]);
        } else {eprintln!("No stable coefficient graph is available.");std::process::exit(1);}
    }
}
#[cfg(not(windows))]
fn main() {std::process::exit(2);}
