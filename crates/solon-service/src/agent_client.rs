//! Client Tokio du protocole agent : requêtes corrélées sur le port de contrôle, flux d'événements.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use solon_core::protocol::{AgentEvent, Command, PORT_CONTROL, PORT_EVENTS, Request, Response};
use solon_core::{ErrorCode, Result, SolonError};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use windows::core::GUID;

async fn connect(vm: &GUID, port: u32, timeout: Duration) -> Result<TcpStream> {
    let vm = *vm;
    let std_stream =
        tokio::task::spawn_blocking(move || solon_hvsock::connect_with_retry(&vm, port, timeout))
            .await
            .map_err(|e| SolonError::internal(e.to_string()))?
            .map_err(|e| {
                SolonError::new(
                    ErrorCode::AgentUnreachable,
                    format!("hvsock port {port} : {e}"),
                )
            })?;
    std_stream.set_nonblocking(true)?;
    Ok(TcpStream::from_std(std_stream)?)
}

/// Connexion de contrôle : une requête à la fois (sérialisées par un verrou), réponses corrélées.
pub struct AgentClient {
    io: Mutex<(
        BufReader<tokio::net::tcp::OwnedReadHalf>,
        tokio::net::tcp::OwnedWriteHalf,
    )>,
    next_id: AtomicU64,
}

impl AgentClient {
    pub async fn connect(vm: &GUID, timeout: Duration) -> Result<Arc<Self>> {
        let stream = connect(vm, PORT_CONTROL, timeout).await?;
        let (r, w) = stream.into_split();
        Ok(Arc::new(Self {
            io: Mutex::new((BufReader::new(r), w)),
            next_id: AtomicU64::new(1),
        }))
    }

    pub async fn call(&self, command: Command, timeout: Duration) -> Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut line = serde_json::to_string(&Request { id, command })?;
        line.push('\n');
        let mut io = self.io.lock().await;
        io.1.write_all(line.as_bytes())
            .await
            .map_err(|e| SolonError::new(ErrorCode::AgentUnreachable, e.to_string()))?;
        let mut reply = String::new();
        let n = tokio::time::timeout(timeout, io.0.read_line(&mut reply))
            .await
            .map_err(|_| {
                SolonError::new(
                    ErrorCode::AgentUnreachable,
                    "délai dépassé en attendant l'agent",
                )
            })?
            .map_err(|e| SolonError::new(ErrorCode::AgentUnreachable, e.to_string()))?;
        if n == 0 {
            return Err(SolonError::new(
                ErrorCode::AgentUnreachable,
                "connexion fermée par l'agent",
            ));
        }
        let response: Response = serde_json::from_str(reply.trim())?;
        if response.id != id {
            return Err(SolonError::internal(format!(
                "réponse {} pour la requête {id}",
                response.id
            )));
        }
        if response.ok {
            Ok(response.result.unwrap_or(serde_json::Value::Null))
        } else {
            Err(SolonError::new(
                ErrorCode::AgentUnreachable,
                response.error.unwrap_or_else(|| "erreur agent".into()),
            ))
        }
    }

    pub async fn call_typed<T: serde::de::DeserializeOwned>(
        &self,
        command: Command,
        timeout: Duration,
    ) -> Result<T> {
        let v = self.call(command, timeout).await?;
        Ok(serde_json::from_value(v)?)
    }

    pub async fn ping(&self) -> Result<()> {
        self.call(Command::Ping, Duration::from_secs(5))
            .await
            .map(|_| ())
    }
}

/// Flux d'événements : l'hôte se connecte, l'agent pousse des lignes JSON.
pub struct AgentEvents {
    reader: BufReader<TcpStream>,
}

impl AgentEvents {
    pub async fn connect(vm: &GUID, timeout: Duration) -> Result<Self> {
        let stream = connect(vm, PORT_EVENTS, timeout).await?;
        Ok(Self {
            reader: BufReader::new(stream),
        })
    }

    /// Prochain événement ; `None` si l'agent a fermé la connexion.
    pub async fn next(&mut self) -> Result<Option<AgentEvent>> {
        let mut line = String::new();
        loop {
            line.clear();
            let n = self
                .reader
                .read_line(&mut line)
                .await
                .map_err(|e| SolonError::new(ErrorCode::AgentUnreachable, e.to_string()))?;
            if n == 0 {
                return Ok(None);
            }
            match serde_json::from_str::<AgentEvent>(line.trim()) {
                Ok(e) => return Ok(Some(e)),
                Err(e) => tracing::warn!("événement agent illisible : {e} — {}", line.trim()),
            }
        }
    }
}
