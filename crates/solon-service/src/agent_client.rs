//! Client Tokio du protocole agent : requêtes corrélées sur le port de contrôle, flux d'événements.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use solon_core::protocol::{
    AgentEvent, Command, PORT_CONTROL, PORT_EVENTS, PORT_TRACE, Request, Response, TraceEvent,
};
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

/// Flux poussé par l'agent : l'hôte se connecte, l'agent envoie une ligne JSON par fait.
///
/// Deux ports suivent cette forme — les événements du moteur et la radiographie — avec le même
/// code à la charge utile près, d'où le paramètre de type.
pub struct AgentStream<T> {
    reader: BufReader<TcpStream>,
    port: u32,
    _marker: std::marker::PhantomData<T>,
}

/// Événements du moteur (vsock 5003).
pub type AgentEvents = AgentStream<AgentEvent>;
/// Faits observés par les sondes eBPF (vsock 5007).
pub type AgentTrace = AgentStream<TraceEvent>;

impl<T: serde::de::DeserializeOwned> AgentStream<T>
where
    Self: StreamPort,
{
    pub async fn connect(vm: &GUID, timeout: Duration) -> Result<Self> {
        let port = Self::PORT;
        let stream = connect(vm, port, timeout).await?;
        Ok(Self {
            reader: BufReader::new(stream),
            port,
            _marker: std::marker::PhantomData,
        })
    }

    /// Prochain fait ; `None` si l'agent a fermé la connexion.
    pub async fn next(&mut self) -> Result<Option<T>> {
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
            match serde_json::from_str::<T>(line.trim()) {
                Ok(e) => return Ok(Some(e)),
                Err(e) => tracing::warn!(
                    port = self.port,
                    "ligne illisible de l'agent : {e} — {}",
                    line.trim()
                ),
            }
        }
    }
}

/// Le port vsock sur lequel chaque flux se branche. Détail d'implémentation d'`AgentStream`,
/// public seulement parce qu'il apparaît dans sa borne.
pub trait StreamPort {
    const PORT: u32;
}

impl StreamPort for AgentStream<AgentEvent> {
    const PORT: u32 = PORT_EVENTS;
}

impl StreamPort for AgentStream<TraceEvent> {
    const PORT: u32 = PORT_TRACE;
}
