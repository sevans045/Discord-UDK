use std::io::Write;
use std::time::SystemTime;
use sha2::{Digest, Sha256};
use tokio::time::sleep;
use widestring::WideCString;
use std::time::Duration;

use windows::{
    Win32::{
        Foundation::{HANDLE, HINSTANCE, PWSTR},
        System::{
            Diagnostics::Debug::OutputDebugStringW,
            LibraryLoader::GetModuleHandleA,
            ProcessStatus::{K32GetModuleFileNameExW, K32GetModuleInformation, MODULEINFO},
            SystemServices::{
                DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH, DLL_THREAD_ATTACH, DLL_THREAD_DETACH,
            },
            Threading::GetCurrentProcess,
        },
    },
    core::{Error, HRESULT},
};

mod udk_log;
use udk_log::log;
use discord_sdk::{self, activity::ActivityBuilder};
use tokio;
use tracing;
use anyhow;

pub const APP_ID: discord_sdk::AppId = 846947824888709160;

pub struct Client {
    pub discord: discord_sdk::Discord,
    pub user: discord_sdk::user::User,
    pub wheel: discord_sdk::wheel::Wheel,
}

pub async fn make_client(subs: discord_sdk::Subscriptions) -> Client {
    tracing_subscriber::fmt()
        .compact()
        .with_max_level(tracing::Level::TRACE)
        .init();

    let (wheel, handler) = discord_sdk::wheel::Wheel::new(Box::new(|err| {
        tracing::error!(error = ?err, "encountered an error");
    }));

    let mut user = wheel.user();

    let discord = discord_sdk::Discord::new(discord_sdk::DiscordApp::PlainId(APP_ID), subs, Box::new(handler))
        .expect("unable to create discord client");

    tracing::info!("waiting for handshake...");
    user.0.changed().await.unwrap();

    let user = match &*user.0.borrow() {
        discord_sdk::wheel::UserState::Connected(user) => user.clone(),
        discord_sdk::wheel::UserState::Disconnected(err) => panic!("failed to connect to Discord: {}", err),
    };

    tracing::info!("connected to Discord, local user is {:#?}", user);

    Client {
        discord,
        user,
        wheel,
    }
}

pub async fn set_activity() -> Result<(), anyhow::Error> {
    let client = make_client(discord_sdk::Subscriptions::ACTIVITY).await;

    let mut activity_events = client.wheel.activity();

    tokio::task::spawn(async move {
        while let Ok(ae) = activity_events.0.recv().await {
            tracing::info!(event = ?ae, "received activity event");
            log(udk_log::LogType::Warning, "received activity event");
        }
    });

    unsafe { CLIENT = Some(client) };
    Ok(())
}

pub static mut CLIENT : Option<Client> = None;

pub fn get_discord_client() -> &'static mut Client {
    unsafe { CLIENT.as_mut().unwrap() }
}

pub static mut RUNTIME : Option<tokio::runtime::Runtime> = None;

pub fn get_runtime() -> &'static mut tokio::runtime::Runtime {
    unsafe { RUNTIME.as_mut().unwrap() }
}

pub async fn update_presence(rp: ActivityBuilder) {
    log(udk_log::LogType::Warning, "updated activity");
    let client = get_discord_client();
    tracing::info!("updated activity: {:?}",client.discord.update_activity(rp).await);
}

#[no_mangle]
pub extern "C" fn UpdateDiscordRPC(in_server_name: &widestring::WideCString, in_level_name: &widestring::WideCString, in_player_count: u32, in_max_players: u32, in_team_num: u32, in_time_elapsed: u32, in_time_remaining: u32, in_is_firestorm: u32, in_image_name: &widestring::WideCString) {
    log(udk_log::LogType::Warning, &format!("UpdateDiscordRPC, {}, {}, {}, {}, {}, {}",  in_player_count, in_max_players, in_team_num, in_time_elapsed, in_time_remaining, in_is_firestorm));

    let rp = discord_sdk::activity::ActivityBuilder::default()
    .details("Competitive".to_owned())
    .state("Playing Solo".to_owned())
    .assets(
        discord_sdk::activity::Assets::default()
            .large("map_ts-sanctuary".to_owned(), Some("Tiberian Sun - Sanctuary".to_owned()))
            .small("tsgdi".to_owned(), Some("GDI".to_owned())),
    )
    .start_timestamp(SystemTime::now());

    get_runtime().spawn(update_presence(rp));
}

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

#[cfg(target_arch = "x86_64")]
const UDK_KNOWN_HASH: [u8; 32] = [
    0x0D, 0xE6, 0x90, 0x31, 0xEA, 0x41, 0x01, 0xF2, 0x18, 0xB6, 0x61, 0x27, 0xFD, 0x14, 0x3A, 0x8E,
    0xC3, 0xF7, 0x48, 0x3E, 0x31, 0x9C, 0x3D, 0x8D, 0xD5, 0x1F, 0xA2, 0x8D, 0x7C, 0xBF, 0x08, 0xF5,
];

#[cfg(target_arch = "x86")]
const UDK_KNOWN_HASH: [u8; 32] = [
    0xEF, 0xAF, 0xBA, 0x91, 0xD3, 0x05, 0x2D, 0x07, 0x07, 0xDD, 0xF2, 0xF2, 0x14, 0x15, 0x00, 0xFA,
    0x6C, 0x1E, 0x8F, 0x9E, 0xF0, 0x70, 0x40, 0xB8, 0xF9, 0x96, 0x73, 0x8A, 0x00, 0xFB, 0x90, 0x07,
];



/// Cached slice of UDK.exe. This is only touched once upon init, and
/// never written again.
// FIXME: The slice is actually unsafe to access; sections of memory may be unmapped!
// We should use a raw pointer slice instead (if ergonomics permit doing so).
static mut UDK_SLICE: Option<&'static [u8]> = None;

/// Return a slice of UDK.exe
pub fn get_udk_slice() -> &'static [u8] {
    // SAFETY: This is only touched once in DllMain.
    unsafe { UDK_SLICE.unwrap() }
}

/// Wrapped version of the Win32 OutputDebugString.
fn output_debug_string(s: &str) {
    let mut wstr = widestring::U16String::from_str(s);
    unsafe {
        OutputDebugStringW(PWSTR(wstr.as_mut_ptr()));
    }
}

/// Wrapped version of the Win32 GetModuleFileName.
fn get_module_filename(process: HANDLE, module: HINSTANCE) -> windows::core::Result<String> {
    // Use a temporary buffer the size of MAX_PATH for now.
    // TODO: Dynamic allocation for longer filenames. As of now, this will truncate longer filenames.
    let mut buf = [0u16; 256];

    let len = unsafe {
        K32GetModuleFileNameExW(process, module, PWSTR(buf.as_mut_ptr()), buf.len() as u32)
    } as usize;

    if len == 0 {
        // Function failed.
        
        return Err(Error::from_win32());
    }

    Ok(String::from_utf16_lossy(&buf[..len]))
}

/// Wrapped version of the Win32 GetModuleInformation.
fn get_module_information(process: HANDLE, module: HINSTANCE) -> windows::core::Result<MODULEINFO> {
    let mut module_info = MODULEINFO {
        ..Default::default()
    };

    match unsafe {
        K32GetModuleInformation(
            process,
            module,
            &mut module_info,
            std::mem::size_of::<MODULEINFO>() as u32,
        )
        .as_bool()
    } {
        true => Ok(module_info),
        false => Err(Error::from_win32()),
    }
}

/// Create a raw slice from a MODULEINFO structure.
fn get_module_slice(info: &MODULEINFO) -> *const [u8] {
    core::ptr::slice_from_raw_parts(info.lpBaseOfDll as *const u8, info.SizeOfImage as usize)
}

/// Called upon DLL attach. This function verifies the UDK and initializes
/// hooks if the UDK matches our known hash.
fn dll_attach() -> anyhow::Result<()> {
    let process = unsafe { GetCurrentProcess() };
    let module = unsafe { GetModuleHandleA(None) };

    let exe_slice = get_module_slice(
        &get_module_information(process, module).expect("Failed to get module information for UDK"),
    );

    // Now that we're attached, let's hash the UDK executable.
    // If the hash does not match what we think it should be, do not attach detours.
    let exe_filename = get_module_filename(process, module)?;

    let mut exe = std::fs::File::open(exe_filename)?;
    let hash = {
        let mut sha = Sha256::new();
        std::io::copy(&mut exe, &mut sha)?;
        sha.finalize()
    };

    // Ensure the hash matches a known hash.
    if hash[..] != UDK_KNOWN_HASH {
        output_debug_string(&format!("Hash: {:02X?}\n", hash));
        output_debug_string(&format!("Expected: {:02X?}\n", UDK_KNOWN_HASH));
        anyhow::bail!("Unknown UDK hash");
    }

    // Cache the UDK slice.
    unsafe {
        UDK_SLICE = Some(exe_slice.as_ref().unwrap());
    }

    Ok(())
}

#[no_mangle]
pub extern "stdcall" fn DllMain(
    _hinst_dll: HINSTANCE,
    fdw_reason: u32,
    _lpv_reserved: usize,
) -> i32 {
    match fdw_reason {
        DLL_PROCESS_ATTACH => {
            if let Err(e) = dll_attach() {
                // Print a debug message for anyone who's listening.
                eprintln!("{:?}", e);
            }
        }
        DLL_PROCESS_DETACH => {}

        DLL_THREAD_ATTACH => {}
        DLL_THREAD_DETACH => {}

        _ => return 0,
    }

    return 1;
}