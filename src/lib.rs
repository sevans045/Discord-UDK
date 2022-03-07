use windows::Win32::Foundation::HINSTANCE;

mod udk_log;
mod discord;
mod dll;

use tokio;

#[no_mangle]
pub extern "C" fn DLLBindInit(in_init_data: FDLLBindInitData) {
    unsafe { RUNTIME = Some(tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap()) };
    get_runtime().spawn(set_activity());
}

#[repr(C)]
#[allow(non_snake_case)]
pub struct FDLLBindInitData {
    Version: u32,
    ReallocFunctionPtr: u64
}

#[no_mangle]
pub extern "stdcall" fn DllMain(_hinst_dll: HINSTANCE, fdw_reason: u32, _lpv_reserved: usize) -> i32 {
    dll::DllMain(_hinst_dll, fdw_reason, _lpv_reserved)
}