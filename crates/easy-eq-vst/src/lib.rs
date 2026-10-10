//! Minimal VST2-compatible ABI for Equalizer APO, without a plug-in editor.
//! ABI fields/opcodes are translated from APO's GPL VeSTige aeffectx.h
//! (Copyright 2006 Javier Serrano Polo). No proprietary SDK is incorporated.
#![cfg(windows)]
use easy_eq_bridge::{windows::{Mapping, SpectrumMapping, now_ms}, spectrum::SpectrumCapture};
use easy_eq_core::{LeaseSnapshot, LeasedProcessor, PreparedGraph};
use std::{ffi::c_void, path::PathBuf, sync::atomic::Ordering};

pub type Dispatcher = unsafe extern "C" fn(*mut AEffect,i32,i32,isize,*mut c_void,f32)->isize;
pub type Process = unsafe extern "C" fn(*mut AEffect,*const *const f32,*const *mut f32,i32);
pub type HostCallback = unsafe extern "C" fn(*mut AEffect,i32,i32,isize,*mut c_void,f32)->isize;
#[repr(C)]
pub struct AEffect {
    pub magic:i32, pub dispatcher:Dispatcher, pub process:Process,
    pub set_parameter:unsafe extern "C" fn(*mut AEffect,i32,f32),
    pub get_parameter:unsafe extern "C" fn(*mut AEffect,i32)->f32,
    pub programs:i32,pub parameters:i32,pub inputs:i32,pub outputs:i32,pub flags:i32,
    pub reserved1:*mut c_void,pub reserved2:*mut c_void,pub initial_delay:i32,
    pub real_quality:i32,pub offline_quality:i32,pub io_ratio:f32,
    pub object:*mut c_void,pub user:*mut c_void,pub unique_id:i32,pub version:i32,
    pub process_replacing:Process,pub process_double_replacing:*mut c_void,pub future:[u8;56],
}
struct Instance { mapping:Option<Mapping>, spectrum:Option<SpectrumMapping>, capture:SpectrumCapture, processor:LeasedProcessor, rate:f64, sequence:u64, valid_rate:bool, feedback:bool, pid:u64 }
#[link(name="kernel32")]
extern "system" {
    fn GetModuleHandleExW(flags:u32,address:*const u16,module:*mut *mut c_void)->i32;
    fn GetModuleFileNameW(module:*mut c_void,path:*mut u16,length:u32)->u32;
    fn GetCurrentProcessId()->u32;
}
fn module_path(module:*mut c_void)->Option<PathBuf> {
    let mut path=[0u16;32768];
    let length=unsafe { GetModuleFileNameW(module,path.as_mut_ptr(),path.len() as u32) } as usize;
    if length==0 || length>=path.len() { return None; }
    Some(PathBuf::from(String::from_utf16_lossy(&path[..length])))
}
fn control_path()->Option<PathBuf> {
    let mut module=std::ptr::null_mut();
    // FROM_ADDRESS | UNCHANGED_REFCOUNT, so DLL lifetime remains host-owned.
    if unsafe { GetModuleHandleExW(6,VSTPluginMain as *const () as *const u16,&mut module) }==0 { return None; }
    let path=module_path(module)?;
    let guid=path.parent()?.file_name()?.to_str()?;
    easy_eq_bridge::windows::endpoint_guid(guid)?;
    Some(easy_eq_bridge::windows::state_directory().join(guid).join("control.bin"))
}
unsafe fn instance<'a>(effect:*mut AEffect)->Option<&'a mut Instance> {
    if effect.is_null() { return None; }
    ((*effect).object as *mut Instance).as_mut()
}
unsafe fn write_string(pointer:*mut c_void,bytes:&[u8])->isize {
    if pointer.is_null() { return 0; }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(),pointer.cast::<u8>(),bytes.len());
    *pointer.cast::<u8>().add(bytes.len())=0; 1
}
unsafe extern "C" fn dispatch(effect:*mut AEffect,opcode:i32,_index:i32,_value:isize,pointer:*mut c_void,opt:f32)->isize {
    if effect.is_null() { return 0; }
    match opcode {
        0 => {
            let Some(state)=instance(effect) else { return 0; };
            state.mapping=control_path().and_then(|path| Mapping::open(&path).ok());
            state.spectrum=control_path().and_then(|path| SpectrumMapping::open(&path.with_file_name("spectrum.bin")).ok());
            state.feedback=module_path(std::ptr::null_mut()).and_then(|p|p.file_name().map(|n| n.to_string_lossy().eq_ignore_ascii_case("audiodg.exe"))).unwrap_or(false);
            1
        },
        1 => { let state=(*effect).object as *mut Instance; if !state.is_null() { drop(Box::from_raw(state)); } drop(Box::from_raw(effect)); 1 },
        10 => {
            let Some(state)=instance(effect) else { return 0; };
            let rate=opt as f64; state.valid_rate=rate.is_finite() && (8000.0..=768000.0).contains(&rate);
            state.capture.reset();
            if state.valid_rate {
                state.rate=rate; state.sequence=0;
                state.processor=LeasedProcessor::new(PreparedGraph::identity(rate).unwrap()).unwrap();
            }
            if let Some(mapping)=&state.mapping { mapping.block().device_rate.store(rate.to_bits(),Ordering::Release); }
            1
        },
        11|12|71|72 => 1,
        35 => 1, // effect category
        45|48 => write_string(pointer,b"Easy EQ EQ"),
        47 => write_string(pointer,b"Easy EQ"),
        49 => 1,
        58 => 2400,
        _ => 0,
    }
}
unsafe extern "C" fn set_parameter(_effect:*mut AEffect,_index:i32,_value:f32) {}
unsafe extern "C" fn get_parameter(_effect:*mut AEffect,_index:i32)->f32 { 0.0 }
unsafe extern "C" fn process(effect:*mut AEffect,inputs:*const *const f32,outputs:*const *mut f32,frames:i32) {
    if frames<=0 || inputs.is_null() || outputs.is_null() { return; }
    let left=*inputs; let right=*inputs.add(1); let out_left=*outputs; let out_right=*outputs.add(1);
    if left.is_null() || right.is_null() || out_left.is_null() || out_right.is_null() { return; }
    let Some(state)=instance(effect) else { return; };
    let now=now_ms();
    let mut live=false;
    if state.valid_rate {
        if let Some(mapping)=&state.mapping {
            let block=mapping.block();
            if let Some((sequence,graph))=block.read_graph(state.sequence) {
                if graph.rate==state.rate && state.processor.set_profile(graph).is_ok() { state.sequence=sequence; }
            }
            let expiry=block.expires_ms.load(Ordering::Acquire);
            live=state.sequence!=0 && block.enabled.load(Ordering::Acquire)!=0 && now<expiry && expiry.saturating_sub(now)<=2000;
            // The lease stays independent of renderer lifetime. Expiry/Off
            // transitions to identity through the same bounded 5 ms fade.
            let _=state.processor.begin_block(LeaseSnapshot { enabled:live, expires_at_nanos:expiry.saturating_mul(1_000_000) },now.saturating_mul(1_000_000));
            if state.feedback {
                block.applied_sequence.store(state.sequence,Ordering::Release);
                block.processing_enabled.store(live as u64,Ordering::Release);
                block.callback_pid.store(state.pid,Ordering::Release);
                block.callback_ms.store(now,Ordering::Release);
            }
        } else { let _=state.processor.begin_block(LeaseSnapshot::default(),now.saturating_mul(1_000_000)); }
    }
    let spectrum = state.spectrum.as_ref().filter(|m| state.valid_rate && m.block().requested(now));
    if spectrum.is_none() { state.capture.reset(); }
    for i in 0..frames as usize {
        let frame=[*left.add(i) as f64,*right.add(i) as f64];
        let result=if state.valid_rate { state.processor.process_frame(frame) } else { frame };
        *out_left.add(i)=result[0] as f32; *out_right.add(i)=result[1] as f32;
        if let Some(mapping) = spectrum {
            state.capture.push(mapping.block(), [frame[0] as f32, frame[1] as f32], [result[0] as f32, result[1] as f32], state.rate, now, state.pid);
        }
    }
    let _=live;
}
/// The host owns each returned instance and releases it with effClose. VST
/// guarantees dispatcher/sample-rate lifecycle does not race this audio callback.
#[no_mangle]
pub unsafe extern "C" fn VSTPluginMain(host:Option<HostCallback>)->*mut AEffect {
    let Some(host)=host else { return std::ptr::null_mut(); };
    if host(std::ptr::null_mut(),1,0,0,std::ptr::null_mut(),0.0)<2400 { return std::ptr::null_mut(); }
    let state=Box::new(Instance { mapping:None,spectrum:None,capture:SpectrumCapture::default(),processor:LeasedProcessor::new(PreparedGraph::identity(48000.0).unwrap()).unwrap(),
        rate:48000.0,sequence:0,valid_rate:true,feedback:false,pid:GetCurrentProcessId() as u64 });
    Box::into_raw(Box::new(AEffect { magic:i32::from_be_bytes(*b"VstP"),dispatcher:dispatch,process,
        set_parameter,get_parameter,programs:1,parameters:0,inputs:2,outputs:2,flags:16,
        reserved1:std::ptr::null_mut(),reserved2:std::ptr::null_mut(),initial_delay:0,
        real_quality:0,offline_quality:0,io_ratio:1.0,object:Box::into_raw(state).cast(),user:std::ptr::null_mut(),
        unique_id:i32::from_be_bytes(*b"P2EQ"),version:1,process_replacing:process,
        process_double_replacing:std::ptr::null_mut(),future:[0;56] }))
}
