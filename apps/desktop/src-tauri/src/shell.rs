//! Terminal dans la machine : connexion au pipe `\\.\pipe\monodon-shell` (relayé par le service vers
//! le port vsock 5004 de l'agent), en-tête JSON `{cols, rows}`, puis trames vers l'agent et octets
//! bruts du TTY vers l'application (Channel Tauri, base64).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use base64::Engine;
use monodon_core::ipc::SHELL_PIPE;
use monodon_core::protocol::{SHELL_FRAME_INPUT, SHELL_FRAME_RESIZE, ShellHeader};
use tauri::ipc::Channel;
use tokio::io::{AsyncReadExt, AsyncWriteExt, WriteHalf};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
use tokio::sync::Mutex;
use tokio::task::AbortHandle;

use crate::docker::ExecOutput;

const ERROR_PIPE_BUSY: i32 = 231;

pub struct ShellSession {
    writer: Mutex<WriteHalf<NamedPipeClient>>,
    task: AbortHandle,
}

#[derive(Default)]
pub struct ShellState {
    sessions: Mutex<HashMap<u64, Arc<ShellSession>>>,
    next: AtomicU64,
}

pub type State<'a> = tauri::State<'a, Arc<ShellState>>;

async fn open_pipe() -> Result<NamedPipeClient, String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match ClientOptions::new().open(SHELL_PIPE) {
            Ok(pipe) => return Ok(pipe),
            Err(e)
                if e.raw_os_error() == Some(ERROR_PIPE_BUSY)
                    && std::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
            Err(e) => {
                return Err(format!(
                    "terminal indisponible (le moteur est-il démarré ?) : {e}"
                ));
            }
        }
    }
}

async fn write_frame(
    w: &mut WriteHalf<NamedPipeClient>,
    kind: u8,
    payload: &[u8],
) -> Result<(), String> {
    for chunk in payload.chunks(u16::MAX as usize) {
        let len = (chunk.len() as u16).to_be_bytes();
        w.write_all(&[kind, len[0], len[1]])
            .await
            .map_err(|e| e.to_string())?;
        w.write_all(chunk).await.map_err(|e| e.to_string())?;
    }
    w.flush().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn machine_shell_open(
    state: State<'_>,
    cols: u16,
    rows: u16,
    command: Option<String>,
    channel: Channel<ExecOutput>,
) -> Result<u64, String> {
    let pipe = open_pipe().await?;
    let (mut reader, mut writer) = tokio::io::split(pipe);
    let mut header = serde_json::to_string(&ShellHeader {
        cols,
        rows,
        command,
    })
    .map_err(|e| e.to_string())?;
    header.push('\n');
    writer
        .write_all(header.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    let ch = channel.clone();
    let task = tokio::spawn(async move {
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let out = ExecOutput {
                        kind: "data",
                        data: Some(base64::engine::general_purpose::STANDARD.encode(&buf[..n])),
                        message: None,
                    };
                    if ch.send(out).is_err() {
                        return;
                    }
                }
                Err(e) => {
                    let _ = ch.send(ExecOutput {
                        kind: "error",
                        data: None,
                        message: Some(e.to_string()),
                    });
                    break;
                }
            }
        }
        let _ = ch.send(ExecOutput {
            kind: "end",
            data: None,
            message: None,
        });
    });
    let id = state.next.fetch_add(1, Ordering::Relaxed) + 1;
    state.sessions.lock().await.insert(
        id,
        Arc::new(ShellSession {
            writer: Mutex::new(writer),
            task: task.abort_handle(),
        }),
    );
    Ok(id)
}

async fn session(state: &State<'_>, id: u64) -> Result<Arc<ShellSession>, String> {
    state
        .sessions
        .lock()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| "session terminal inconnue".to_owned())
}

#[tauri::command]
pub async fn machine_shell_input(state: State<'_>, id: u64, data: String) -> Result<(), String> {
    let s = session(&state, id).await?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| e.to_string())?;
    let mut w = s.writer.lock().await;
    write_frame(&mut w, SHELL_FRAME_INPUT, &bytes).await
}

#[tauri::command]
pub async fn machine_shell_resize(
    state: State<'_>,
    id: u64,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let s = session(&state, id).await?;
    let payload = serde_json::to_vec(&ShellHeader {
        cols,
        rows,
        command: None,
    })
    .map_err(|e| e.to_string())?;
    let mut w = s.writer.lock().await;
    write_frame(&mut w, SHELL_FRAME_RESIZE, &payload).await
}

#[tauri::command]
pub async fn machine_shell_close(state: State<'_>, id: u64) -> Result<(), String> {
    if let Some(s) = state.sessions.lock().await.remove(&id) {
        s.task.abort();
        let mut w = s.writer.lock().await;
        let _ = w.shutdown().await;
    }
    Ok(())
}
