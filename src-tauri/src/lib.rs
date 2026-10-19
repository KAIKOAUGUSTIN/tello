//! Ponte entre a UI (Svelte) e o `tello-core`.
//!
//! - eventos do drone vão para a UI como `drone-event` (JSON);
//! - o vídeo vai por um `Channel` binário: [flags u8][seq u32 LE][Annex B];
//! - a caixa-preta é enviada em lotes como `drone-log`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::ipc::{Channel, InvokeBody, InvokeResponseBody, Request};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tello_core::net::{self, NetworkCheck};
use tello_core::{Action, Blackbox, Capabilities, Config, Drone, DroneEvent, Entry, Protocol, Safety, Sticks};

type CmdResult<T> = Result<T, String>;
type VideoChannel = Arc<Mutex<Option<Channel<InvokeResponseBody>>>>;

const APP_DIR: &str = "Tello Pilot";

struct AppState {
    drone: Mutex<Option<Arc<Drone>>>,
    video: VideoChannel,
    blackbox: Arc<Blackbox>,
}

fn current(state: &AppState) -> CmdResult<Arc<Drone>> {
    state.drone.lock().unwrap().clone().ok_or_else(|| "não conectado".to_string())
}

#[tauri::command]
fn network_check(ip: String) -> CmdResult<NetworkCheck> {
    let ip = ip.trim().parse().map_err(|_| format!("IP inválido: {ip}"))?;
    Ok(net::check_network(ip))
}

#[tauri::command]
async fn connect(
    app: AppHandle,
    state: State<'_, AppState>,
    protocol: Protocol,
    mut config: Config,
) -> CmdResult<Capabilities> {
    // Fotos em alta resolução (modo App) vão para Imagens/Tello Pilot.
    config.photo_dir = media_dir(&app, "photos").ok();
    let old = state.drone.lock().unwrap().take();
    if let Some(old) = old {
        let _ = tauri::async_runtime::spawn_blocking(move || old.disconnect()).await;
    }
    let video = state.video.clone();
    let emitter = app.clone();
    let sink = Arc::new(move |e: DroneEvent| match e {
        DroneEvent::Video(frame) => {
            if let Some(ch) = video.lock().unwrap().as_ref() {
                let mut buf = Vec::with_capacity(frame.data.len() + 5);
                buf.push(frame.keyframe as u8 | (frame.has_sps as u8) << 1);
                buf.extend_from_slice(&(frame.seq as u32).to_le_bytes());
                buf.extend_from_slice(&frame.data);
                let _ = ch.send(InvokeResponseBody::Raw(buf));
            }
        }
        other => {
            let _ = emitter.emit("drone-event", &other);
        }
    });
    let blackbox = state.blackbox.clone();
    let drone =
        tauri::async_runtime::spawn_blocking(move || Drone::connect_with_blackbox(config, protocol, sink, blackbox))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
    let caps = drone.caps();
    *state.drone.lock().unwrap() = Some(Arc::new(drone));
    Ok(caps)
}

#[tauri::command]
async fn disconnect(state: State<'_, AppState>) -> CmdResult<()> {
    let drone = state.drone.lock().unwrap().take();
    if let Some(d) = drone {
        tauri::async_runtime::spawn_blocking(move || d.disconnect()).await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn action(state: State<'_, AppState>, action: Action) -> CmdResult<String> {
    let drone = current(&state)?;
    tauri::async_runtime::spawn_blocking(move || drone.action(&action))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_sticks(state: State<'_, AppState>, sticks: Sticks) {
    if let Some(d) = state.drone.lock().unwrap().as_ref() {
        d.set_sticks(sticks);
    }
}

#[tauri::command]
fn set_safety(state: State<'_, AppState>, safety: Safety) {
    if let Some(d) = state.drone.lock().unwrap().as_ref() {
        d.set_safety(safety);
    }
}

#[tauri::command]
fn set_video_channel(state: State<'_, AppState>, channel: Channel<InvokeResponseBody>) {
    *state.video.lock().unwrap() = Some(channel);
}

#[tauri::command]
fn log_snapshot(state: State<'_, AppState>) -> Vec<Entry> {
    state.blackbox.snapshot()
}

#[tauri::command]
fn export_log(app: AppHandle, state: State<'_, AppState>, stamp: String) -> CmdResult<String> {
    let dir = media_dir(&app, "logs")?;
    let path = dir.join(format!("tello-log-{}.jsonl", sanitize(&stamp)));
    let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    state.blackbox.export_jsonl(std::io::BufWriter::new(file)).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

/// Salva a captura do vídeo (PNG) enviada como corpo binário da chamada.
#[tauri::command]
fn save_snapshot(app: AppHandle, request: Request<'_>) -> CmdResult<String> {
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err("esperava o PNG como corpo binário".into());
    };
    let stamp = request.headers().get("x-stamp").and_then(|v| v.to_str().ok()).unwrap_or("foto");
    let path = media_dir(&app, "photos")?.join(format!("tello-{}.png", sanitize(stamp)));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

#[tauri::command]
fn open_folder(app: AppHandle, kind: String) -> CmdResult<()> {
    let dir = media_dir(&app, &kind)?;
    open_path(&dir).map_err(|e| e.to_string())
}

/// Pousa (se estiver voando) e fecha o app.
#[tauri::command]
async fn land_and_quit(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let drone = state.drone.lock().unwrap().take();
    if let Some(d) = drone {
        let _ = tauri::async_runtime::spawn_blocking(move || {
            if d.is_flying() {
                let _ = d.action(&Action::Land);
            }
            d.disconnect();
        })
        .await;
    }
    app.exit(0);
    Ok(())
}

fn media_dir(app: &AppHandle, kind: &str) -> CmdResult<PathBuf> {
    let paths = app.path();
    let base = match kind {
        "photos" => paths.picture_dir(),
        "videos" => paths.video_dir(),
        "logs" => paths.document_dir().map(|d| d.join(APP_DIR)),
        _ => return Err(format!("pasta desconhecida: {kind}")),
    }
    .map_err(|e| e.to_string())?;
    let dir = if kind == "logs" { base.join("logs") } else { base.join(APP_DIR) };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn sanitize(s: &str) -> String {
    let s: String = s.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').take(40).collect();
    if s.is_empty() {
        "sem-nome".into()
    } else {
        s
    }
}

fn open_path(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let program = "xdg-open";
    std::process::Command::new(program).arg(path).spawn().map(|_| ())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let blackbox = Arc::new(Blackbox::new(100_000));
    let pending_log: Arc<Mutex<Vec<Entry>>> = Arc::default();
    {
        let pending = pending_log.clone();
        blackbox.set_listener(Some(Arc::new(move |e: &Entry| {
            let mut p = pending.lock().unwrap();
            if p.len() < 5_000 {
                p.push(e.clone());
            }
        })));
    }
    let state = AppState { drone: Mutex::new(None), video: Arc::default(), blackbox };

    tauri::Builder::default()
        .manage(state)
        .setup(move |app| {
            let handle = app.handle().clone();
            std::thread::Builder::new().name("log-forwarder".into()).spawn(move || loop {
                std::thread::sleep(Duration::from_millis(250));
                let batch = std::mem::take(&mut *pending_log.lock().unwrap());
                if !batch.is_empty() {
                    let _ = handle.emit("drone-log", &batch);
                }
            })?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                let flying = state.drone.lock().unwrap().as_ref().is_some_and(|d| d.is_flying());
                if flying {
                    // A UI pergunta se deve pousar antes de sair.
                    api.prevent_close();
                    let _ = window.emit("confirm-close", ());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            network_check,
            connect,
            disconnect,
            action,
            set_sticks,
            set_safety,
            set_video_channel,
            log_snapshot,
            export_log,
            save_snapshot,
            open_folder,
            land_and_quit,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o Tello Pilot");
}
