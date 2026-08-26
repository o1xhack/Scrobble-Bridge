use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

const UPDATE_CHECK_INTERVAL_SECONDS: i64 = 24 * 60 * 60;
const UPDATE_MONITOR_INTERVAL: Duration = Duration::from_secs(60 * 60);
const UPDATE_STARTUP_DELAY: Duration = Duration::from_secs(15);
const UPDATE_EVENT: &str = "software-update-changed";

#[derive(Clone)]
pub(crate) struct UpdateContext {
    runtime: Arc<Mutex<UpdateRuntime>>,
    check_record_path: PathBuf,
}

struct UpdateRuntime {
    phase: UpdatePhase,
    available: Option<Update>,
    package: Option<Vec<u8>>,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    last_attempt_at: Option<DateTime<Utc>>,
    last_success_at: Option<DateTime<Utc>>,
    error: Option<String>,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum UpdatePhase {
    Idle,
    Checking,
    Available,
    Downloading,
    Ready,
    Installing,
}

#[derive(Clone, Serialize)]
pub(crate) struct SoftwareUpdateStatus {
    current_version: &'static str,
    phase: UpdatePhase,
    available_version: Option<String>,
    notes: Option<String>,
    published_at: Option<String>,
    last_checked_at: Option<DateTime<Utc>>,
    last_successful_check_at: Option<DateTime<Utc>>,
    next_check_at: Option<DateTime<Utc>>,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    error: Option<String>,
}

#[derive(Default, Deserialize, Serialize)]
struct UpdateCheckRecord {
    last_attempt_at: Option<DateTime<Utc>>,
    last_success_at: Option<DateTime<Utc>>,
}

impl UpdateContext {
    pub(crate) fn new(data_dir: &Path) -> Self {
        let check_record_path = data_dir.join("update-check.json");
        let record = load_check_record(&check_record_path);
        Self {
            runtime: Arc::new(Mutex::new(UpdateRuntime {
                phase: UpdatePhase::Idle,
                available: None,
                package: None,
                downloaded_bytes: 0,
                total_bytes: None,
                last_attempt_at: record.last_attempt_at,
                last_success_at: record.last_success_at,
                error: None,
            })),
            check_record_path,
        }
    }
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri injects command state by value.
pub(crate) fn software_update_status(
    context: State<'_, UpdateContext>,
) -> Result<SoftwareUpdateStatus, String> {
    current_status(&context)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri injects command state by value.
pub(crate) async fn check_for_software_update(
    app: AppHandle,
    context: State<'_, UpdateContext>,
) -> Result<SoftwareUpdateStatus, String> {
    run_update_check(&app, &context, true).await
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri injects command state by value.
pub(crate) async fn download_software_update(
    app: AppHandle,
    context: State<'_, UpdateContext>,
) -> Result<SoftwareUpdateStatus, String> {
    let update = {
        let mut runtime = lock_runtime(&context.runtime)?;
        let update = runtime
            .available
            .clone()
            .ok_or_else(|| "No software update is available".to_owned())?;
        runtime.phase = UpdatePhase::Downloading;
        runtime.package = None;
        runtime.downloaded_bytes = 0;
        runtime.total_bytes = None;
        runtime.error = None;
        update
    };
    emit_current_status(&app, &context);

    let progress_runtime = Arc::clone(&context.runtime);
    let progress_app = app.clone();
    let finish_runtime = Arc::clone(&context.runtime);
    let finish_app = app.clone();
    let download = update
        .download(
            move |chunk_size, total_size| {
                if let Ok(mut runtime) = progress_runtime.lock() {
                    runtime.downloaded_bytes = runtime
                        .downloaded_bytes
                        .saturating_add(u64::try_from(chunk_size).unwrap_or(u64::MAX));
                    runtime.total_bytes = total_size;
                }
                emit_runtime_status(&progress_app, &progress_runtime);
            },
            move || {
                emit_runtime_status(&finish_app, &finish_runtime);
            },
        )
        .await;

    match download {
        Ok(package) => {
            let mut runtime = lock_runtime(&context.runtime)?;
            runtime.downloaded_bytes = u64::try_from(package.len()).unwrap_or(u64::MAX);
            runtime.total_bytes = Some(runtime.downloaded_bytes);
            runtime.package = Some(package);
            runtime.phase = UpdatePhase::Ready;
            runtime.error = None;
        }
        Err(error) => {
            let message = error.to_string();
            let mut runtime = lock_runtime(&context.runtime)?;
            runtime.phase = UpdatePhase::Available;
            runtime.package = None;
            runtime.error = Some(message.clone());
            drop(runtime);
            emit_current_status(&app, &context);
            return Err(message);
        }
    }

    let status = current_status(&context)?;
    let _ = app.emit(UPDATE_EVENT, status.clone());
    Ok(status)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri injects command state by value.
pub(crate) fn install_software_update(
    app: AppHandle,
    context: State<'_, UpdateContext>,
) -> Result<(), String> {
    let (update, package) = {
        let mut runtime = lock_runtime(&context.runtime)?;
        let update = runtime
            .available
            .clone()
            .ok_or_else(|| "No software update is available".to_owned())?;
        let package = runtime
            .package
            .clone()
            .ok_or_else(|| "Download the software update before installing it".to_owned())?;
        runtime.phase = UpdatePhase::Installing;
        runtime.error = None;
        (update, package)
    };
    emit_current_status(&app, &context);

    if let Err(error) = update.install(&package) {
        let message = error.to_string();
        let mut runtime = lock_runtime(&context.runtime)?;
        runtime.phase = UpdatePhase::Ready;
        runtime.error = Some(message.clone());
        drop(runtime);
        emit_current_status(&app, &context);
        return Err(message);
    }

    app.restart();
}

pub(crate) fn start_update_monitor(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(UPDATE_STARTUP_DELAY).await;
        loop {
            check_if_due(app.clone()).await;
            tokio::time::sleep(UPDATE_MONITOR_INTERVAL).await;
        }
    });
}

pub(crate) fn check_after_resume(app: AppHandle) {
    tauri::async_runtime::spawn(check_if_due(app));
}

async fn check_if_due(app: AppHandle) {
    let context = app.state::<UpdateContext>().inner().clone();
    if let Err(error) = run_update_check(&app, &context, false).await {
        tracing::warn!(error = %error, "automatic software update check failed");
    }
}

async fn run_update_check(
    app: &AppHandle,
    context: &UpdateContext,
    force: bool,
) -> Result<SoftwareUpdateStatus, String> {
    let now = Utc::now();
    {
        let mut runtime = lock_runtime(&context.runtime)?;
        if matches!(
            runtime.phase,
            UpdatePhase::Checking | UpdatePhase::Downloading | UpdatePhase::Installing
        ) {
            return Ok(status_from_runtime(&runtime));
        }
        if !force && !update_check_is_due(runtime.last_attempt_at, now) {
            return Ok(status_from_runtime(&runtime));
        }
        runtime.phase = UpdatePhase::Checking;
        runtime.error = None;
        runtime.last_attempt_at = Some(now);
    }
    persist_check_record(context)?;
    emit_current_status(app, context);

    let check_result = match app.updater() {
        Ok(updater) => updater.check().await.map_err(|error| error.to_string()),
        Err(error) => Err(error.to_string()),
    };
    let completed_at = Utc::now();

    let result = match check_result {
        Ok(update) => {
            let mut runtime = lock_runtime(&context.runtime)?;
            runtime.last_success_at = Some(completed_at);
            runtime.error = None;
            runtime.package = None;
            runtime.downloaded_bytes = 0;
            runtime.total_bytes = None;
            runtime.available = update;
            runtime.phase = if runtime.available.is_some() {
                UpdatePhase::Available
            } else {
                UpdatePhase::Idle
            };
            Ok(status_from_runtime(&runtime))
        }
        Err(message) => {
            let mut runtime = lock_runtime(&context.runtime)?;
            runtime.phase = if runtime.package.is_some() {
                UpdatePhase::Ready
            } else if runtime.available.is_some() {
                UpdatePhase::Available
            } else {
                UpdatePhase::Idle
            };
            runtime.error = force.then_some(message.clone());
            Err(message)
        }
    };
    persist_check_record(context)?;
    emit_current_status(app, context);
    result
}

fn current_status(context: &UpdateContext) -> Result<SoftwareUpdateStatus, String> {
    let runtime = lock_runtime(&context.runtime)?;
    Ok(status_from_runtime(&runtime))
}

fn status_from_runtime(runtime: &UpdateRuntime) -> SoftwareUpdateStatus {
    let available = runtime.available.as_ref();
    SoftwareUpdateStatus {
        current_version: env!("CARGO_PKG_VERSION"),
        phase: runtime.phase,
        available_version: available.map(|update| update.version.clone()),
        notes: available.and_then(|update| update.body.clone()),
        published_at: available.and_then(|update| update.date.map(|date| date.to_string())),
        last_checked_at: runtime.last_attempt_at,
        last_successful_check_at: runtime.last_success_at,
        next_check_at: runtime
            .last_attempt_at
            .and_then(|last| last.checked_add_signed(chrono::Duration::days(1))),
        downloaded_bytes: runtime.downloaded_bytes,
        total_bytes: runtime.total_bytes,
        error: runtime.error.clone(),
    }
}

fn emit_current_status(app: &AppHandle, context: &UpdateContext) {
    if let Ok(status) = current_status(context) {
        let _ = app.emit(UPDATE_EVENT, status);
    }
}

fn emit_runtime_status(app: &AppHandle, runtime: &Arc<Mutex<UpdateRuntime>>) {
    if let Ok(runtime) = runtime.lock() {
        let _ = app.emit(UPDATE_EVENT, status_from_runtime(&runtime));
    }
}

fn lock_runtime(
    runtime: &Arc<Mutex<UpdateRuntime>>,
) -> Result<MutexGuard<'_, UpdateRuntime>, String> {
    runtime
        .lock()
        .map_err(|_| "Software update state is unavailable".to_owned())
}

fn load_check_record(path: &Path) -> UpdateCheckRecord {
    fs::read(path)
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_default()
}

fn persist_check_record(context: &UpdateContext) -> Result<(), String> {
    let record = {
        let runtime = lock_runtime(&context.runtime)?;
        UpdateCheckRecord {
            last_attempt_at: runtime.last_attempt_at,
            last_success_at: runtime.last_success_at,
        }
    };
    let contents = serde_json::to_vec_pretty(&record).map_err(|error| error.to_string())?;
    let temporary = context.check_record_path.with_extension("json.tmp");
    fs::write(&temporary, contents).map_err(|error| error.to_string())?;
    fs::rename(&temporary, &context.check_record_path).map_err(|error| error.to_string())
}

fn update_check_is_due(last_attempt_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    let Some(last_attempt_at) = last_attempt_at else {
        return true;
    };
    let elapsed = now.timestamp().saturating_sub(last_attempt_at.timestamp());
    if elapsed >= 0 {
        elapsed >= UPDATE_CHECK_INTERVAL_SECONDS
    } else {
        elapsed.saturating_abs() > UPDATE_CHECK_INTERVAL_SECONDS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    #[test]
    fn daily_update_check_survives_restart_boundaries() {
        let last = at(1_000_000);
        assert!(!update_check_is_due(
            Some(last),
            at(1_000_000 + UPDATE_CHECK_INTERVAL_SECONDS - 1)
        ));
        assert!(update_check_is_due(
            Some(last),
            at(1_000_000 + UPDATE_CHECK_INTERVAL_SECONDS)
        ));
        assert!(update_check_is_due(None, last));
    }

    #[test]
    fn small_backward_clock_change_does_not_repeat_update_checks() {
        let last = at(1_000_000);
        assert!(!update_check_is_due(Some(last), at(1_000_000 - 60 * 60)));
        assert!(update_check_is_due(
            Some(last),
            at(1_000_000 - UPDATE_CHECK_INTERVAL_SECONDS - 1)
        ));
    }
}
