use windows::Win32::Foundation::HINSTANCE;

mod udk_log;
mod discord;
mod dll;
mod error;

#[no_mangle]
pub extern "C" fn DLLBindInit(_in_init_data: FDLLBindInitData) {
    tracing_subscriber::fmt()
    .compact()
    .with_max_level(tracing::Level::TRACE)
    .init();
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