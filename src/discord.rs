use std::num::NonZeroU32;
use std::time::SystemTime;
use tokio::time::timeout;
use widestring::U16CStr;
use std::time::Duration;
use crate::error::Error;

use crate::udk_log::log;
use discord_sdk;
use tokio;
use tracing;

pub const APP_ID: discord_sdk::AppId = 846947824888709160;

pub struct Client {
    pub discord: discord_sdk::Discord,
    pub user: discord_sdk::user::User,
    pub wheel: discord_sdk::wheel::Wheel,
}

pub async fn make_client(subs: discord_sdk::Subscriptions) -> Result<Client, Error> {
    let (wheel, handler) = discord_sdk::wheel::Wheel::new(Box::new(|err| {
        tracing::error!(error = ?err, "encountered an error while trying to create a discord connection");
        log(crate::udk_log::LogType::Warning, "encountered an error while trying to create a discord connection");
    }));

    let mut user = wheel.user();
    let current_exe = std::env::current_exe()?;

    let application = discord_sdk::registration::Application {
        id: APP_ID,
        name: Some("Renegade X".to_string()),
        command: discord_sdk::registration::LaunchCommand::Bin {
            path: current_exe,
            args: vec![]
        }
    };
    let new_discord_result = discord_sdk::Discord::new(discord_sdk::DiscordApp::Register(application), subs, Box::new(handler));
    if let Ok(discord) = new_discord_result {
        tracing::info!("waiting for handshake...");
        let timed_out = timeout(Duration::from_millis(1500), user.0.changed()).await;

        if let Err(error) = timed_out {
            tracing::warn!("Failed to connect, shutting down discord!");
            log(crate::udk_log::LogType::Warning, "Failed to connect, shutting down discord!");
            discord.disconnect().await;
            return Err(Error::DiscordError(format!("{:?}", error)))
        } else {
            timed_out??;
        }

        let user = match &*user.0.borrow() {
            discord_sdk::wheel::UserState::Connected(user) => Ok(user.clone()),
            discord_sdk::wheel::UserState::Disconnected(error) => Err(format!("{:?}", error))
        };

        if let Err(error) = user {
            tracing::warn!("Failed to connect, shutting down discord!");
            log(crate::udk_log::LogType::Warning, &format!("Failed to connect, shutting down discord! Error: {}", error));
            discord.disconnect().await;
            return Err(Error::DiscordError(error))
        } else if let Ok(user) = user {
            unsafe { IS_INITIALIZED = true };
            tracing::info!("connected to Discord, local user is {:#?}", user);
            log(crate::udk_log::LogType::Warning, &format!("connected to Discord, local user is {:#?}", user));

            return Ok(Client {
                discord,
                user: user.clone(),
                wheel,
            });
        } else {
            return Err(Error::DiscordError(format!("Could not connect to discord, failed to retrieve current discord user")));
        }
    } else {
        return Err(Error::DiscordError(format!("Could not create discord client")));
    }
}

pub async fn start_discord_rpc() -> Result<(), Error> {
    let client = make_client(discord_sdk::Subscriptions::ACTIVITY).await?;

    let mut activity_events = client.wheel.activity();

    tokio::task::spawn(async move {
        while let Ok(ae) = activity_events.0.recv().await {
            tracing::info!(event = ?ae, "received activity event");
            log(crate::udk_log::LogType::Warning, "received activity event");
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
pub static mut IS_INITIALIZED : bool = false;

pub fn get_runtime() -> &'static mut tokio::runtime::Runtime {
    unsafe { RUNTIME.as_mut().unwrap() }
}

pub async fn update_presence(in_server_name: String, in_level_name: String, in_player_count: u32, in_max_players: u32, team: String, in_time_elapsed:u32, in_time_remaining: u32, is_firestorm: bool, in_image_name: String) -> Result<(), Error> {
    if unsafe { !IS_INITIALIZED && CLIENT.is_none() } {
        tracing::warn!("initializing tokio and discord");
        start_discord_rpc().await?;
        tracing::warn!("Initialized discord RPC");
    } else if unsafe { !IS_INITIALIZED && CLIENT.is_some() } {
        tracing::error!("Client exists, yet we're not initialized yet!");
        return Err(Error::DiscordError("Client exists, yet we're not initialized yet!".to_string()))
    }

    if in_level_name == "FrontEndMap" {
        let mut assets = discord_sdk::activity::Assets::default();
        if !is_firestorm {
            assets = assets.large("renegadex", Some("Renegade X".to_owned()));
        } else {
            assets = assets.large("fs", Some("Firestorm".to_owned()));
        }

        let rp = discord_sdk::activity::ActivityBuilder::default()
        .details("Main Menu")
        .state("")
        .assets(assets);

        let client = get_discord_client();
        let info = client.discord.update_activity(rp).await;
        log(crate::udk_log::LogType::Warning, &format!("updated activity: {:?}", &info));
        tracing::info!("updated activity: {:?}", &info);
        return Ok(());
    }

    let mut assets = discord_sdk::activity::Assets::default();
    assets = assets.large(in_image_name, Some(in_level_name.clone()));

    if !is_firestorm {
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

    let client = get_discord_client();
    let info = client.discord.update_activity(rp).await;
    log(crate::udk_log::LogType::Warning, &format!("updated activity: {:?}", &info));
    tracing::info!("updated activity: {:?}", &info);
    Ok(())
}

#[no_mangle]
pub extern "C" fn UpdateDiscordRPC(in_server_name_ptr: *const u16, in_level_name_ptr: *const u16, in_player_count: u32, in_max_players: u32, in_team_num: u32, in_time_elapsed: u32, in_time_remaining: u32, is_firestorm: u32, in_image_name_ptr: *const u16) {
    if unsafe { !IS_INITIALIZED && RUNTIME.is_some() } {
        tracing::warn!("Exiting UpdateDiscordRPC as we're not initialized yet!");
        return;
    } else if unsafe { RUNTIME.is_none() } {
        unsafe { RUNTIME = Some(tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap()) };
    }

    let in_server_name = unsafe { U16CStr::from_ptr_str(in_server_name_ptr) }.to_string_lossy();
    let in_level_name = unsafe { U16CStr::from_ptr_str(in_level_name_ptr) }.to_string_lossy();
    let in_image_name = unsafe { U16CStr::from_ptr_str(in_image_name_ptr) }.to_string_lossy();
    log(crate::udk_log::LogType::Warning, &format!("UpdateDiscordRPC, {}, {}, {}, {}, {}, {}, {}, {}, {}", in_server_name, in_level_name, in_player_count, in_max_players, in_team_num, in_time_elapsed, in_time_remaining, is_firestorm, in_image_name));

    let team = match in_team_num {
        0 => "GDI",
        1 => "Nod",
        2 => "BH",
        _ => ""
    }.to_owned();

    get_runtime().spawn(update_presence(in_server_name, in_level_name, in_player_count, in_max_players, team, in_time_elapsed, in_time_remaining, is_firestorm != 0, in_image_name));
}