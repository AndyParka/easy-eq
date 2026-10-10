#![cfg(windows)]
use peace2_audio::{AEffect,HostCallback};
use easy_eq_bridge::windows::{Mapping,SpectrumMapping,now_ms};
use easy_eq_core::{PreparedGraph,Profile,Band,FilterKind};
use std::sync::atomic::Ordering;

unsafe extern "C" fn host(_effect:*mut AEffect,opcode:i32,_index:i32,_value:isize,_pointer:*mut std::ffi::c_void,_opt:f32)->isize {
    if opcode==1 {2400} else {0}
}
#[test]
fn native_plugin_handles_in_place_buffers_per_ear_gain_off_and_expiry() {
    // The fixture gets its own GUID and does not use any installed device's
    // mapping/configuration. Override ProgramData only in this test process.
    let root=std::env::temp_dir().join(format!("peace2-vst-test-{}",std::process::id()));
    let guid="{00000000-0000-0000-0000-000000000042}";
    let dll_dir=root.join("dll").join(guid);std::fs::create_dir_all(&dll_dir).unwrap();
    let state_dir=root.join("Peace2").join(guid);std::fs::create_dir_all(&state_dir).unwrap();
    std::env::set_var("ProgramData",&root);
    let source=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/peace2_audio.dll");
    let dll=dll_dir.join("peace2_audio.dll");std::fs::copy(source,&dll).unwrap();
    let control=state_dir.join("control.bin");Mapping::initialize(&control).unwrap();let mapping=Mapping::open(&control).unwrap();
    let spectrum_path=state_dir.join("spectrum.bin");SpectrumMapping::initialize(&spectrum_path).unwrap();let spectrum=SpectrumMapping::open(&spectrum_path).unwrap();
    spectrum.block().request(now_ms());
    let mut profile=Profile::flat("fixture","Fixture");profile.gain_db=-6.0;profile.pan=0.5;
    mapping.block().publish(PreparedGraph::compile(&profile,48000.0).unwrap());mapping.block().renew(true,now_ms());
    unsafe {
        let library=libloading::Library::new(&dll).unwrap();
        let entry:libloading::Symbol<unsafe extern "C" fn(Option<HostCallback>)->*mut AEffect>=library.get(b"VSTPluginMain\0").unwrap();
        let effect=entry(Some(host));assert!(!effect.is_null());assert_eq!((*effect).initial_delay,0);assert_eq!((*effect).inputs,2);assert_eq!((*effect).outputs,2);
        assert_eq!(std::mem::size_of::<AEffect>(),192);
        ((*effect).dispatcher)(effect,0,0,0,std::ptr::null_mut(),0.0);
        ((*effect).dispatcher)(effect,10,0,0,std::ptr::null_mut(),48000.0);
        assert_eq!(f64::from_bits(mapping.block().device_rate.load(Ordering::Acquire)),48000.0);
        let mut left=[0.25f32;512];let mut right=[0.125f32;512];
        let inputs=[left.as_ptr(),right.as_ptr()];let outputs=[left.as_mut_ptr(),right.as_mut_ptr()];
        ((*effect).process_replacing)(effect,inputs.as_ptr(),outputs.as_ptr(),512);
        assert_eq!(left[0],0.25,"The current input sample must not be delayed");
        let gain=10f32.powf(-6.0/20.0);
        assert!((left[511]-0.25*gain*0.5).abs()<1e-6);assert!((right[511]-0.125*gain).abs()<1e-6);
        for _ in 0..8 {
            left.fill(0.25);right.fill(0.125);spectrum.block().request(now_ms());
            ((*effect).process_replacing)(effect,inputs.as_ptr(),outputs.as_ptr(),512);
        }
        let (_, captured)=spectrum.block().read(0).unwrap();
        assert_eq!(captured.rate,48000.0);assert_eq!(captured.input[0][4095],0.25);assert_eq!(captured.input[1][4095],0.125);
        assert!((captured.output[0][4095]-0.25*gain*0.5).abs()<1e-6);
        assert!((captured.output[1][4095]-0.125*gain).abs()<1e-6);
        // Process only left: the right channel must remain silent.
        left.fill(0.25);right.fill(0.0);((*effect).process_replacing)(effect,inputs.as_ptr(),outputs.as_ptr(),512);assert!(right.iter().all(|&v|v==0.0));
        mapping.block().bypass();left.fill(0.25);right.fill(0.125);
        ((*effect).process_replacing)(effect,inputs.as_ptr(),outputs.as_ptr(),512);assert_eq!(left[511],0.25);assert_eq!(right[511],0.125);
        // Verify actual biquad processing through the DLL entry point, not just
        // common gain. A center-frequency cut applies to only the left ear.
        let mut eq=Profile::flat("eq","Per-ear EQ");
        eq.ears.left.push(Band {id:"peak".into(),kind:FilterKind::Peak,frequency:1500.0,gain:-9.0,q:0.707,enabled:true});
        mapping.block().publish(PreparedGraph::compile(&eq,48000.0).unwrap());mapping.block().renew(true,now_ms());
        // Start a fresh capture and allow two full windows. The final window
        // measures the settled filter, with no transition samples mixed in.
        spectrum.block().stop();
        ((*effect).process_replacing)(effect,inputs.as_ptr(),outputs.as_ptr(),512);
        for block in 0..16 {
            spectrum.block().request(now_ms());
            for i in 0..512 {let sample=(2.0*std::f32::consts::PI*1500.0*(block*512+i) as f32/48000.0).sin()*0.1;left[i]=sample;right[i]=sample;}
            let last_input=left[511];((*effect).process_replacing)(effect,inputs.as_ptr(),outputs.as_ptr(),512);
            if block==15 {assert!((left[511]-last_input*10f32.powf(-9.0/20.0)).abs()<1e-5);assert_eq!(right[511],last_input);}
        }
        let (captured_sequence,captured)=spectrum.block().read(0).unwrap();
        let rms=|samples:&[f32]|(samples.iter().map(|v|(*v as f64).powi(2)).sum::<f64>()/samples.len() as f64).sqrt();
        let measured=20.0*(rms(&captured.output[0])/rms(&captured.input[0])).log10();
        assert!((measured+9.0).abs()<0.01,"Captured processed spectrum must contain the actual EQ cut: {measured}");
        assert_eq!(captured.input[1],captured.output[1]);
        spectrum.block().stop();
        ((*effect).process_replacing)(effect,inputs.as_ptr(),outputs.as_ptr(),512);
        assert!(spectrum.block().read(captured_sequence).is_none(),"A hidden editor must stop telemetry publication");
        mapping.block().publish(PreparedGraph::compile(&profile,48000.0).unwrap());
        // Re-enable, then expire without an explicit Off message (killed host).
        mapping.block().renew(true,now_ms());left.fill(0.25);right.fill(0.125);((*effect).process_replacing)(effect,inputs.as_ptr(),outputs.as_ptr(),512);
        mapping.block().expires_ms.store(now_ms().saturating_sub(1),Ordering::Release);
        left.fill(0.25);right.fill(0.125);((*effect).process_replacing)(effect,inputs.as_ptr(),outputs.as_ptr(),512);
        assert_eq!(left[511],0.25);assert_eq!(right[511],0.125);
        // Unsupported formats bypass, rather than muting or using stale rates.
        ((*effect).dispatcher)(effect,10,0,0,std::ptr::null_mut(),f32::NAN);
        left.fill(0.25);right.fill(0.125);((*effect).process_replacing)(effect,inputs.as_ptr(),outputs.as_ptr(),512);assert_eq!(left[0],0.25);
        ((*effect).dispatcher)(effect,1,0,0,std::ptr::null_mut(),0.0);
        drop(library);
    }
    drop(spectrum);drop(mapping);std::fs::remove_dir_all(root).unwrap();
}
