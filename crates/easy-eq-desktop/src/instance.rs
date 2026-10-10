use std::ffi::c_void;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
#[link(name="user32")]
extern "system" { fn ShowWindow(window:*mut c_void,command:i32)->i32; fn IsWindowVisible(window:*mut c_void)->i32; }
pub fn keep_hidden(frame:&eframe::Frame) {
    if let Ok(handle)=frame.window_handle() {
        if let RawWindowHandle::Win32(window)=handle.as_raw() {
            let handle=window.hwnd.get() as *mut c_void;
            unsafe { if IsWindowVisible(handle)!=0 { ShowWindow(handle,0); } }
        }
    }
}
#[link(name="kernel32")]
extern "system" {
    fn CreateMutexW(attributes:*mut c_void,owner:i32,name:*const u16)->*mut c_void;
    fn CreateEventW(attributes:*mut c_void,manual:i32,initial:i32,name:*const u16)->*mut c_void;
    fn SetEvent(event:*mut c_void)->i32;
    fn WaitForSingleObject(handle:*mut c_void,milliseconds:u32)->u32;
    fn GetLastError()->u32;
    fn CloseHandle(handle:*mut c_void)->i32;
}
fn wide(value:&str)->Vec<u16>{value.encode_utf16().chain(Some(0)).collect()}
pub struct Instance {mutex:*mut c_void,events:Vec<*mut c_void>,pub primary:bool}
pub const COMMANDS:[&str;7]=["show","off","on","quit","close","controls","restore-controls"];
impl Instance {
    pub fn acquire()->Result<Self,String>{unsafe{
        let events:Vec<_>=COMMANDS.iter().map(|c|CreateEventW(std::ptr::null_mut(),0,0,wide(&format!("Local\\Peace2.Native.{c}")).as_ptr())).collect();
        let mutex=CreateMutexW(std::ptr::null_mut(),0,wide("Local\\Peace2.Native.Instance").as_ptr());
        let primary=GetLastError()!=183;
        if mutex.is_null() || events.iter().any(|h|h.is_null()) {return Err(std::io::Error::last_os_error().to_string());}
        Ok(Self {mutex,events,primary})
    }}
    pub fn signal(&self,index:usize){unsafe{SetEvent(self.events[index]);}}
    pub fn drain(&self)->Vec<usize>{self.events.iter().enumerate().filter_map(|(i,h)|if unsafe{WaitForSingleObject(*h,0)}==0{Some(i)}else{None}).collect()}
}
impl Drop for Instance {fn drop(&mut self){unsafe{for h in &self.events{CloseHandle(*h);}CloseHandle(self.mutex);}}}
