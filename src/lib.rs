use windows::Win32::Foundation::HINSTANCE;

mod udk_log;
mod discord;
mod dll;
mod error;

static mut INITIALIZED_BIND : bool = false;

#[no_mangle]
pub extern "C" fn DLLBindInit(_in_init_data: FDLLBindInitData) {
    if unsafe { !INITIALIZED_BIND } {
        tracing_subscriber::fmt()
            .pretty()
            .with_max_level(tracing::Level::WARN)
            .init();
        unsafe { INITIALIZED_BIND = true };
    }
}

#[repr(C)]
#[allow(non_snake_case)]
pub struct FDLLBindInitData {
    Version: u32,
    ReallocFunctionPtr: u64
}

#[no_mangle]
pub extern "stdcall" fn DllMain(_hinst_dll: HINSTANCE, fdw_reason: u32, _lpv_reserved: usize) -> i32 {
    dll::dll_main(_hinst_dll, fdw_reason, _lpv_reserved)
}