use std::num::NonZeroU32;
use std::{io::Write, os::raw::c_short};
use std::time::SystemTime;
use sha2::{Digest, Sha256};
use tokio::time::sleep;
use widestring::{WideCString, U16Str, U16CStr};
use std::time::Duration;

use windows::{
    Win32::{
        Foundation::{HANDLE, HINSTANCE},
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
    core::HRESULT,
};


use crate::udk_log::log;
use discord_sdk::{self, activity::ActivityBuilder};
use tokio;
use tracing;
use anyhow;


pub const APP_ID: discord_sdk::AppId = 846947824888709160;

pub enum Error {

}



pub struct Client {
    pub discord: discord_sdk::Discord,
    pub user: discord_sdk::user::User,
    pub wheel: discord_sdk::wheel::Wheel,
}

pub async fn make_client(subs: discord_sdk::Subscriptions) -> Result<Client, Error> {
    tracing_subscriber::fmt()
        .compact()
        .with_max_level(tracing::Level::TRACE)
        .init();

    let (wheel, handler) = discord_sdk::wheel::Wheel::new(Box::new(|err| {
        tracing::error!(error = ?err, "encountered an error");
    }));

    let mut user = wheel.user();

    let discord = discord_sdk::Discord::new(discord_sdk::DiscordApp::PlainId(APP_ID), subs, Box::new(handler)).expect("unable to create discord client");

    tracing::info!("waiting for handshake...");
    user.0.changed().await.unwrap();

    let user = match &*user.0.borrow() {
        discord_sdk::wheel::UserState::Connected(user) => Ok(user.clone()),
        discord_sdk::wheel::UserState::Disconnected(err) => err,
    }?;

    tracing::info!("connected to Discord, local user is {:#?}", user);

    Ok(Client {
        discord,
        user,
        wheel,
    })
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
pub extern "C" fn UpdateDiscordRPC(in_server_name_ptr: *const u16, in_level_name_ptr: *const u16, in_player_count: u32, in_max_players: u32, in_team_num: u32, in_time_elapsed: u32, in_time_remaining: u32, is_firestorm: u32, in_image_name_ptr: *const u16) {
    let in_server_name = unsafe { U16CStr::from_ptr_str(in_server_name_ptr) }.to_string_lossy();
    let in_level_name = unsafe { U16CStr::from_ptr_str(in_level_name_ptr) }.to_string_lossy();
    let in_image_name = unsafe { U16CStr::from_ptr_str(in_image_name_ptr) }.to_string_lossy();
    log(udk_log::LogType::Warning, &format!("UpdateDiscordRPC, {}, {}, {}, {}, {}, {}, {}, {}, {}", in_server_name, in_level_name, in_player_count, in_max_players, in_team_num, in_time_elapsed, in_time_remaining, is_firestorm, in_image_name));


    if in_level_name == "FrontEndMap" {
        let mut assets = discord_sdk::activity::Assets::default();
        if is_firestorm == 0 {
            assets = assets.large("renegadex", Some("Renegade X".to_owned()));
        } else {
            assets = assets.large("fs", Some("Firestorm".to_owned()));
        }

        let rp = discord_sdk::activity::ActivityBuilder::default()
        .details("Main Menu")
        .state("")
        .assets(assets);

        get_runtime().spawn(update_presence(rp));
        return;
    }

    let mut assets = discord_sdk::activity::Assets::default();
    assets = assets.large(in_image_name, Some(in_level_name.clone()));

    let team = match in_team_num {
        0 => "GDI",
        1 => "Nod",
        2 => "BH",
        _ => ""
    };

    if is_firestorm == 0 {
        assets = assets.small(team.to_lowercase(), Some(team));
    } else {
        assets = assets.small(format!("ts{}", team.to_lowercase()), Some(team));
    }

    let mut rp = discord_sdk::activity::ActivityBuilder::default()
    .details(in_server_name.clone())
    .state(in_level_name)
    .assets(assets)
    .start_timestamp(SystemTime::now().checked_sub(Duration::from_secs(in_time_elapsed as u64)).unwrap());

    if in_time_remaining != 0 {
        rp = rp.end_timestamp(SystemTime::now().checked_add(Duration::from_secs(in_time_remaining as u64)).unwrap());
    }

    if in_server_name != "Skirmish" && in_player_count > 0 && in_max_players > 0 {
        rp = rp.party(in_server_name, Some(NonZeroU32::new(in_player_count).unwrap()), Some(NonZeroU32::new(in_max_players).unwrap()), discord_sdk::activity::PartyPrivacy::Private);
    }

    get_runtime().spawn(update_presence(rp));
}