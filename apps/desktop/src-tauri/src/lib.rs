use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    sync::Arc,
    thread,
    time::{Duration, Instant, SystemTime},
};

use interprocess::local_socket::{ListenerOptions, traits::ListenerExt as _};
use lastfm_client::{LastFmClient, LastFmCredentials};
use scrobble_core::OutboxStatus;
use scrobble_daemon::{
    AppState, LASTFM_API_KEY, LASTFM_PENDING_TOKEN, LASTFM_SESSION_KEY, LASTFM_SHARED_SECRET,
    LASTFM_USERNAME, RuntimeStatus, YTMUSIC_ACCOUNT_ID, YTMUSIC_AUTH_USER, YTMUSIC_COOKIE,
    YTMUSIC_DELEGATED_SESSION_ID, scheduler,
};
use scrobble_ipc::{IpcRequest, IpcResponse, MAX_MESSAGE_BYTES, local_socket_name};
use scrobble_keyring::OsKeyringVault;
use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, State, WindowEvent,
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
#[cfg(target_os = "macos")]
use tauri_plugin_autostart::MacosLauncher;
use ytmusic_client::BrowserCredentials;

mod updates;

const DEFAULT_SYNC_INTERVAL: Duration = Duration::from_secs(600);
const KEYRING_SERVICE: &str = "com.scrobblebridge.desktop";
const NATIVE_HOST_NAME: &str = "com.scrobblebridge.host";
const DEVELOPMENT_EXTENSION_ID: &str = "nocefljecnigpgfgalgjefcigeidoglj";
const MAX_LABEL_CHARS: usize = 128;
const MAX_CREDENTIAL_CHARS: usize = 512;
const WAKE_CATCH_UP_THRESHOLD: Duration = Duration::from_secs(90);

#[derive(Debug)]
struct DesktopContext {
    runtime: Arc<AppState>,
    diagnostics_dir: std::path::PathBuf,
}

#[derive(Debug, Serialize)]
struct Diagnostics<'a> {
    app_version: &'a str,
    os: &'a str,
    architecture: &'a str,
    generated_at: chrono::DateTime<chrono::Utc>,
    status: RuntimeStatus,
}

#[tauri::command]
async fn status(context: State<'_, DesktopContext>) -> Result<RuntimeStatus, String> {
    context
        .runtime
        .snapshot_status()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn sync_now(
    context: State<'_, DesktopContext>,
) -> Result<scrobble_engine::SyncReport, String> {
    context
        .runtime
        .run_sync()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn activity(
    context: State<'_, DesktopContext>,
    limit: usize,
    offset: usize,
    search: Option<String>,
    status: Option<OutboxStatus>,
) -> Result<scrobble_storage::ActivityPage, String> {
    context
        .runtime
        .activity_page(limit, offset, search.as_deref(), status)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn refresh_ytmusic_identity(
    context: State<'_, DesktopContext>,
) -> Result<ytmusic_client::AccountInfo, String> {
    context
        .runtime
        .refresh_ytmusic_identity()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn pause_sync(context: State<'_, DesktopContext>) -> Result<(), String> {
    set_paused(&context.runtime, true).await
}

#[tauri::command]
async fn resume_sync(context: State<'_, DesktopContext>) -> Result<(), String> {
    set_paused(&context.runtime, false).await?;
    context.runtime.trigger.notify_one();
    Ok(())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes owned command arguments.
fn save_ytmusic_credentials(
    context: State<'_, DesktopContext>,
    account_id: String,
    auth_user: u8,
    delegated_session_id: Option<String>,
    cookie_header: String,
) -> Result<(), String> {
    save_ytmusic_snapshot(
        &context.runtime,
        &account_id,
        auth_user,
        delegated_session_id.as_deref(),
        &cookie_header,
    )
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes owned command arguments.
fn save_lastfm_application(
    context: State<'_, DesktopContext>,
    api_key: String,
    shared_secret: String,
) -> Result<(), String> {
    if api_key.trim().is_empty()
        || shared_secret.trim().is_empty()
        || api_key.len() > MAX_CREDENTIAL_CHARS
        || shared_secret.len() > MAX_CREDENTIAL_CHARS
    {
        return Err("Last.fm API key and shared secret are required".to_owned());
    }
    context
        .runtime
        .vault
        .set(LASTFM_API_KEY, api_key.trim())
        .map_err(|error| error.to_string())?;
    context
        .runtime
        .vault
        .set(LASTFM_SHARED_SECRET, shared_secret.trim())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn start_lastfm_authorization(context: State<'_, DesktopContext>) -> Result<String, String> {
    let client = lastfm_client(&context.runtime)?;
    let authorization = client
        .request_authorization()
        .await
        .map_err(|error| error.to_string())?;
    context
        .runtime
        .vault
        .set(LASTFM_PENDING_TOKEN, &authorization.token)
        .map_err(|error| error.to_string())?;
    Ok(authorization.url.to_string())
}

#[tauri::command]
async fn finish_lastfm_authorization(context: State<'_, DesktopContext>) -> Result<(), String> {
    let token = context
        .runtime
        .vault
        .get(LASTFM_PENDING_TOKEN)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Start Last.fm authorization first".to_owned())?;
    let session = lastfm_client(&context.runtime)?
        .exchange_token(&token)
        .await
        .map_err(|error| error.to_string())?;
    context
        .runtime
        .vault
        .set(LASTFM_USERNAME, &session.username)
        .map_err(|error| error.to_string())?;
    context
        .runtime
        .vault
        .set(LASTFM_SESSION_KEY, session.expose_key())
        .map_err(|error| error.to_string())?;
    context
        .runtime
        .vault
        .delete(LASTFM_PENDING_TOKEN)
        .map_err(|error| error.to_string())?;
    context
        .runtime
        .storage
        .expedite_retryable_failures("lastfm_auth", chrono::Utc::now())
        .map_err(|error| error.to_string())?;
    context.runtime.trigger.notify_one();
    Ok(())
}

#[tauri::command]
async fn export_diagnostics(context: State<'_, DesktopContext>) -> Result<String, String> {
    let diagnostics = Diagnostics {
        app_version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        generated_at: chrono::Utc::now(),
        status: context
            .runtime
            .snapshot_status()
            .await
            .map_err(|error| error.to_string())?,
    };
    fs::create_dir_all(&context.diagnostics_dir).map_err(|error| error.to_string())?;
    let path = context.diagnostics_dir.join(format!(
        "scrobble-bridge-diagnostics-{}.json",
        diagnostics.generated_at.format("%Y%m%d-%H%M%S")
    ));
    fs::write(
        &path,
        serde_json::to_vec_pretty(&diagnostics).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(path.display().to_string())
}

/// Starts the desktop event loop.
///
/// # Panics
///
/// Panics only when Tauri cannot initialize or its platform event loop fails.
pub fn run() {
    let autostart = tauri_plugin_autostart::Builder::new();
    #[cfg(target_os = "macos")]
    let autostart = autostart.macos_launcher(MacosLauncher::LaunchAgent);

    let app = tauri::Builder::default()
        .plugin(autostart.build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let diagnostics_dir = app.path().app_log_dir()?;
            fs::create_dir_all(&data_dir)?;
            app.manage(updates::UpdateContext::new(&data_dir));
            let vault = Arc::new(OsKeyringVault::new(KEYRING_SERVICE));
            let runtime = Arc::new(AppState::new(
                &data_dir.join("state.sqlite3"),
                vault,
                [0; 32],
                DEFAULT_SYNC_INTERVAL,
            )?);
            bootstrap_bundled_lastfm_application(&runtime)?;
            app.manage(DesktopContext {
                runtime: Arc::clone(&runtime),
                diagnostics_dir,
            });
            tauri::async_runtime::spawn(scheduler(Arc::clone(&runtime)));
            tauri::async_runtime::spawn(wake_monitor(Arc::clone(&runtime), app.handle().clone()));
            updates::start_update_monitor(app.handle().clone());
            start_ipc_listener(Arc::clone(&runtime));
            if let Err(error) = register_native_messaging_host() {
                tracing::warn!(error = %error, "could not register Chrome native messaging host");
            }
            build_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            } else if let WindowEvent::Focused(true) = event {
                updates::check_after_resume(window.app_handle().clone());
            }
        })
        .invoke_handler(tauri::generate_handler![
            status,
            activity,
            sync_now,
            pause_sync,
            resume_sync,
            save_ytmusic_credentials,
            refresh_ytmusic_identity,
            save_lastfm_application,
            start_lastfm_authorization,
            finish_lastfm_authorization,
            export_diagnostics,
            updates::software_update_status,
            updates::check_for_software_update,
            updates::download_software_update,
            updates::install_software_update,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Scrobble Bridge");

    app.run(|app_handle, event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            show_main_window(app_handle);
            updates::check_after_resume(app_handle.clone());
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (app_handle, event);
    });
}

fn build_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "打开 / Open", true, None::<&str>)?;
    let sync = MenuItem::with_id(app, "sync", "立即同步 / Sync Now", true, None::<&str>)?;
    let pause = MenuItem::with_id(
        app,
        "pause",
        "暂停或继续 / Pause or Resume",
        true,
        None::<&str>,
    )?;
    let diagnostics = MenuItem::with_id(
        app,
        "diagnostics",
        "诊断信息 / Diagnostics",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "退出 / Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &sync, &pause, &diagnostics, &quit])?;
    // Tauri's icon-less macOS tray item is rendered as an anonymous gray
    // placeholder dot. Give the single tray item an explicit identity and a
    // short title so it is recognizable and cannot look like a stale process.
    TrayIconBuilder::with_id("main")
        .icon(Image::new_owned(vec![0, 0, 0, 0], 1, 1))
        .title("SB")
        .tooltip("Scrobble Bridge")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| handle_tray_menu(app, event.id.as_ref()))
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn handle_tray_menu(app: &AppHandle, id: &str) {
    match id {
        "open" => show_main_window(app),
        "sync" => {
            let runtime = Arc::clone(&app.state::<DesktopContext>().runtime);
            tauri::async_runtime::spawn(async move {
                let _ = runtime.run_sync().await;
            });
        }
        "pause" => {
            let runtime = Arc::clone(&app.state::<DesktopContext>().runtime);
            tauri::async_runtime::spawn(async move {
                let paused = !runtime.status.read().await.paused;
                if let Err(error) = set_paused(&runtime, paused).await {
                    tracing::error!(error = %error, "could not persist pause state");
                    return;
                }
                if !paused {
                    runtime.trigger.notify_one();
                }
            });
        }
        "diagnostics" => {
            show_main_window(app);
            let _ = app.emit("open-diagnostics", ());
        }
        "quit" => app.exit(0),
        _ => {}
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

async fn set_paused(runtime: &AppState, paused: bool) -> Result<(), String> {
    runtime
        .set_paused(paused)
        .await
        .map_err(|error| error.to_string())
}

fn save_ytmusic_snapshot(
    runtime: &AppState,
    account_id: &str,
    auth_user: u8,
    delegated_session_id: Option<&str>,
    cookie_header: &str,
) -> Result<(), String> {
    BrowserCredentials::new(cookie_header, auth_user)
        .with_delegated_session_id(delegated_session_id.map(ToOwned::to_owned))
        .validate()
        .map_err(|error| error.to_string())?;
    if account_id.trim().is_empty() || account_id.chars().count() > MAX_LABEL_CHARS {
        return Err("Account label must contain 1 to 128 characters".to_owned());
    }
    runtime
        .vault
        .set(YTMUSIC_COOKIE, cookie_header)
        .map_err(|error| error.to_string())?;
    runtime
        .vault
        .set(YTMUSIC_AUTH_USER, &auth_user.to_string())
        .map_err(|error| error.to_string())?;
    runtime
        .vault
        .set(YTMUSIC_ACCOUNT_ID, account_id.trim())
        .map_err(|error| error.to_string())?;
    if let Some(value) = delegated_session_id {
        runtime
            .vault
            .set(YTMUSIC_DELEGATED_SESSION_ID, value)
            .map_err(|error| error.to_string())?;
    } else {
        runtime
            .vault
            .delete(YTMUSIC_DELEGATED_SESSION_ID)
            .map_err(|error| error.to_string())?;
    }
    runtime.trigger.notify_one();
    Ok(())
}

fn lastfm_client(runtime: &AppState) -> Result<LastFmClient, String> {
    let required = |name: &str, label: &str| {
        runtime
            .vault
            .get(name)
            .map_err(|error| error.to_string())?
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("{label} is not configured"))
    };
    LastFmClient::new(LastFmCredentials::new(
        required(LASTFM_API_KEY, "Last.fm API key")?,
        required(LASTFM_SHARED_SECRET, "Last.fm shared secret")?,
    ))
    .map_err(|error| error.to_string())
}

fn start_ipc_listener(runtime: Arc<AppState>) {
    thread::spawn(move || {
        let Ok(name) = local_socket_name() else {
            return;
        };
        let Ok(listener) = ListenerOptions::new()
            .name(name)
            .try_overwrite(true)
            .create_sync()
        else {
            return;
        };
        for connection in listener.incoming().flatten() {
            handle_ipc_connection(connection, &runtime);
        }
    });
}

fn handle_ipc_connection(connection: interprocess::local_socket::Stream, runtime: &Arc<AppState>) {
    let mut connection = BufReader::new(connection);
    let mut payload = String::new();
    let response = match connection
        .by_ref()
        .take(u64::try_from(MAX_MESSAGE_BYTES).unwrap_or(u64::MAX) + 1)
        .read_line(&mut payload)
    {
        Ok(length) if length <= MAX_MESSAGE_BYTES => process_ipc_request(&payload, runtime),
        _ => IpcResponse::failure("IPC request is invalid"),
    };
    if response.ok {
        let runtime = Arc::clone(runtime);
        tauri::async_runtime::spawn(async move {
            if let Err(error) = runtime.refresh_ytmusic_identity().await {
                tracing::warn!(error = %error, "could not refresh YouTube Music account identity");
            }
        });
    }
    if let Ok(encoded) = serde_json::to_vec(&response) {
        let _ = connection.get_mut().write_all(&encoded);
        let _ = connection.get_mut().write_all(b"\n");
        let _ = connection.get_mut().flush();
    }
}

fn process_ipc_request(payload: &str, runtime: &AppState) -> IpcResponse {
    let request = serde_json::from_str::<IpcRequest>(payload);
    let result = match request {
        Ok(request) if request.validate().is_ok() => match request {
            IpcRequest::CredentialSnapshot { payload, .. } => save_ytmusic_snapshot(
                runtime,
                &payload.account_id,
                payload.auth_user,
                payload.delegated_session_id.as_deref(),
                &payload.cookie_header,
            ),
        },
        _ => Err("IPC request is invalid".to_owned()),
    };
    match result {
        Ok(()) => IpcResponse::success(),
        Err(error) => IpcResponse::failure(error),
    }
}

async fn wake_monitor(runtime: Arc<AppState>, app: AppHandle) {
    let mut previous = Instant::now();
    let mut previous_wall_clock = SystemTime::now();
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let now = Instant::now();
        let wall_clock = SystemTime::now();
        if wake_requires_catch_up(
            now.duration_since(previous),
            wall_clock.duration_since(previous_wall_clock).ok(),
        ) {
            runtime.trigger.notify_one();
            updates::check_after_resume(app.clone());
        }
        previous = now;
        previous_wall_clock = wall_clock;
    }
}

fn wake_requires_catch_up(
    monotonic_elapsed: Duration,
    wall_clock_elapsed: Option<Duration>,
) -> bool {
    monotonic_elapsed > WAKE_CATCH_UP_THRESHOLD
        || wall_clock_elapsed.is_some_and(|elapsed| elapsed > WAKE_CATCH_UP_THRESHOLD)
}

fn bootstrap_bundled_lastfm_application(
    runtime: &AppState,
) -> Result<(), Box<dyn std::error::Error>> {
    let api_key = option_env!("SCROBBLE_LASTFM_API_KEY")
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let shared_secret = option_env!("SCROBBLE_LASTFM_SHARED_SECRET")
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match (api_key, shared_secret) {
        (Some(api_key), Some(shared_secret)) => {
            runtime.bootstrap_lastfm_application(api_key, shared_secret)?;
            Ok(())
        }
        (None, None) => Ok(()),
        _ => Err("bundled Last.fm application credentials must be configured as a pair".into()),
    }
}

#[derive(Debug, Serialize)]
struct NativeHostManifest {
    name: &'static str,
    description: &'static str,
    path: String,
    #[serde(rename = "type")]
    kind: &'static str,
    allowed_origins: Vec<String>,
}

fn register_native_messaging_host() -> Result<(), Box<dyn std::error::Error>> {
    let host_path = std::env::current_exe()?
        .parent()
        .ok_or("desktop executable has no parent directory")?
        .join(if cfg!(windows) {
            "scrobble-native-host.exe"
        } else {
            "scrobble-native-host"
        });
    if !host_path.is_file() {
        return Err(format!("native host is missing at {}", host_path.display()).into());
    }
    let manifest = NativeHostManifest {
        name: NATIVE_HOST_NAME,
        description: "Scrobble Bridge credential transport",
        path: host_path.display().to_string(),
        kind: "stdio",
        allowed_origins: native_messaging_origins(option_env!("SCROBBLE_PRODUCTION_EXTENSION_ID"))?,
    };
    let manifest_path = native_host_manifest_path()?;
    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
    set_private_file_permissions(&manifest_path)?;
    #[cfg(windows)]
    register_windows_native_host(&manifest_path)?;
    Ok(())
}

fn native_messaging_origins(production_id: Option<&str>) -> Result<Vec<String>, String> {
    let mut origins = vec![format!("chrome-extension://{DEVELOPMENT_EXTENSION_ID}/")];
    if let Some(production_id) = production_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if !valid_extension_id(production_id) {
            return Err("production Chrome extension ID is invalid".to_owned());
        }
        if production_id != DEVELOPMENT_EXTENSION_ID {
            origins.push(format!("chrome-extension://{production_id}/"));
        }
    }
    Ok(origins)
}

fn valid_extension_id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| matches!(byte, b'a'..=b'p'))
}

fn native_host_manifest_path() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    {
        let home = directories::BaseDirs::new()
            .ok_or("could not determine the home directory")?
            .home_dir()
            .to_path_buf();
        Ok(home
            .join("Library/Application Support/Google/Chrome/NativeMessagingHosts")
            .join(format!("{NATIVE_HOST_NAME}.json")))
    }
    #[cfg(windows)]
    {
        let config = directories::BaseDirs::new()
            .ok_or("could not determine the local configuration directory")?
            .config_local_dir()
            .to_path_buf();
        Ok(config
            .join("Scrobble Bridge/NativeMessagingHosts")
            .join(format!("{NATIVE_HOST_NAME}.json")))
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    Err("desktop native messaging is supported only on macOS and Windows".into())
}

#[cfg(windows)]
fn register_windows_native_host(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    use winreg::{RegKey, enums::HKEY_CURRENT_USER};
    let root = RegKey::predef(HKEY_CURRENT_USER);
    let key_path = format!(r"Software\Google\Chrome\NativeMessagingHosts\{NATIVE_HOST_NAME}");
    let (key, _) = root.create_subkey(key_path)?;
    key.set_value("", &path.display().to_string())?;
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_file_permissions(path: &std::path::Path) -> std::io::Result<()> {
    // Windows protects this per-user manifest through its parent directory ACL.
    // Verify that the file is readable before registering it with Chrome.
    let _ = fs::metadata(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use scrobble_storage::MemoryVault;

    #[test]
    fn native_ipc_request_saves_valid_snapshot_without_echoing_it() {
        let storage_dir = tempfile::tempdir().unwrap();
        let runtime = AppState::new(
            &storage_dir.path().join("state.sqlite3"),
            Arc::new(MemoryVault::default()),
            [0; 32],
            DEFAULT_SYNC_INTERVAL,
        )
        .unwrap();
        let request = serde_json::json!({
            "version": 1,
            "type": "credential_snapshot",
            "payload": {
                "account_id": "123456789012345678901",
                "auth_user": 1,
                "delegated_session_id": null,
                "cookie_header": "__Secure-3PAPISID=secret-value"
            }
        });
        let response = process_ipc_request(&request.to_string(), &runtime);
        assert!(response.ok);
        assert_eq!(
            runtime.vault.get(YTMUSIC_AUTH_USER).unwrap().as_deref(),
            Some("1")
        );
        assert_eq!(
            runtime.vault.get(YTMUSIC_ACCOUNT_ID).unwrap().as_deref(),
            Some("123456789012345678901")
        );
        assert!(
            !serde_json::to_string(&response)
                .unwrap()
                .contains("secret-value")
        );
    }

    #[test]
    fn extension_id_validation_is_strict() {
        assert!(valid_extension_id(DEVELOPMENT_EXTENSION_ID));
        assert!(!valid_extension_id("too-short"));
        assert!(!valid_extension_id("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"));
    }

    #[test]
    fn official_extension_origin_is_exact_and_never_wildcarded() {
        let production = "abcdefghijklmnopabcdefghijklmnop";
        assert_eq!(
            native_messaging_origins(Some(production)).unwrap(),
            vec![
                format!("chrome-extension://{DEVELOPMENT_EXTENSION_ID}/"),
                format!("chrome-extension://{production}/"),
            ]
        );
        assert_eq!(
            native_messaging_origins(Some(DEVELOPMENT_EXTENSION_ID))
                .unwrap()
                .len(),
            1
        );
        assert!(native_messaging_origins(Some("*")).is_err());
    }

    #[test]
    fn bundled_lastfm_configuration_matches_the_build_environment() {
        let storage_dir = tempfile::tempdir().unwrap();
        let runtime = AppState::new(
            &storage_dir.path().join("state.sqlite3"),
            Arc::new(MemoryVault::default()),
            [0; 32],
            DEFAULT_SYNC_INTERVAL,
        )
        .unwrap();

        bootstrap_bundled_lastfm_application(&runtime).unwrap();
        let bundled =
            option_env!("SCROBBLE_LASTFM_API_KEY").is_some_and(|value| !value.trim().is_empty());
        assert_eq!(runtime.is_lastfm_application_configured(), bundled);
    }

    #[test]
    fn normal_scheduler_delays_do_not_trigger_sleep_recovery() {
        assert!(!wake_requires_catch_up(
            Duration::from_secs(30),
            Some(Duration::from_secs(30))
        ));
        assert!(!wake_requires_catch_up(
            Duration::from_secs(90),
            Some(Duration::from_secs(90))
        ));
    }

    #[test]
    fn system_sleep_or_long_suspension_triggers_one_catch_up() {
        assert!(wake_requires_catch_up(
            Duration::from_secs(91),
            Some(Duration::from_secs(91))
        ));
        assert!(wake_requires_catch_up(
            Duration::from_secs(8 * 60 * 60),
            Some(Duration::from_secs(8 * 60 * 60))
        ));
    }

    #[test]
    fn macos_sleep_is_detected_when_its_monotonic_clock_stops() {
        assert!(wake_requires_catch_up(
            Duration::from_secs(30),
            Some(Duration::from_secs(8 * 60 * 60))
        ));
    }

    #[test]
    fn backward_wall_clock_adjustments_do_not_trigger_sleep_recovery() {
        assert!(!wake_requires_catch_up(Duration::from_secs(30), None));
        assert!(wake_requires_catch_up(Duration::from_secs(91), None));
    }
}
