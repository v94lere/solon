//! Enveloppes sûres autour de `computecore.dll`.
//!
//! Modèle HCS : chaque appel asynchrone reçoit une *opération* (`HCS_OPERATION`) dont on attend
//! le résultat ; le document de résultat (JSON) est alloué par HCS et libéré par `LocalFree`.
//! Les événements de vie (sortie, crash) arrivent par un callback enregistré sur le compute
//! system, **avant** son démarrage, sinon un arrêt précoce serait manqué.

use std::ffi::c_void;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use monodon_core::{ErrorCode, MonodonError, Result};
use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::System::HostComputeSystem::*;
use windows::core::{HSTRING, PCWSTR, PWSTR};

use crate::hresult;
use crate::schema::ComputeSystemSummary;

/// Accès complet demandé à `HcsOpenComputeSystem` (GENERIC_ALL).
const GENERIC_ALL: u32 = 0x1000_0000;

fn timeout_ms(d: Duration) -> u32 {
    d.as_millis().min(u32::MAX as u128) as u32
}

/// Récupère puis libère un document renvoyé par HCS.
fn take_document(p: PWSTR) -> Option<String> {
    if p.is_null() {
        return None;
    }
    // SAFETY : HCS garantit une chaîne UTF-16 terminée par zéro allouée via LocalAlloc.
    let text = unsafe { p.to_string() }.ok();
    unsafe {
        LocalFree(Some(HLOCAL(p.0 as *mut c_void)));
    }
    text
}

/// Opération HCS, libérée automatiquement.
pub struct Operation(HCS_OPERATION);

impl Operation {
    pub fn new() -> Result<Self> {
        let op = unsafe { HcsCreateOperation(None, None) };
        if op.0.is_null() {
            return Err(MonodonError::new(
                ErrorCode::HcsError,
                "HcsCreateOperation a renvoyé un handle nul",
            ));
        }
        Ok(Self(op))
    }

    pub fn raw(&self) -> HCS_OPERATION {
        self.0
    }

    /// Attend la fin de l'opération. En cas d'échec, le document de résultat HCS est intégré
    /// au message d'erreur.
    pub fn wait(&self, context: &str, timeout: Duration) -> Result<Option<String>> {
        let mut doc = PWSTR::null();
        let outcome =
            unsafe { HcsWaitForOperationResult(self.0, timeout_ms(timeout), Some(&mut doc)) };
        let text = take_document(doc);
        match outcome {
            Ok(()) => Ok(text),
            Err(e) => Err(hresult::from_windows(context, &e, text.as_deref())),
        }
    }
}

impl Drop for Operation {
    fn drop(&mut self) {
        unsafe { HcsCloseOperation(self.0) };
    }
}

/// Type d'événement reçu du compute system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HcsEventKind {
    SystemExited,
    SystemCrashInitiated,
    SystemCrashReport,
    GuestConnectionClosed,
    ServiceDisconnect,
    Other(i32),
}

impl From<HCS_EVENT_TYPE> for HcsEventKind {
    fn from(t: HCS_EVENT_TYPE) -> Self {
        // Valeurs de `HCS_EVENT_TYPE` (computecore.h) : Exited=1, CrashInitiated=2, CrashReport=3,
        // GuestConnectionClosed=5, ServiceDisconnect=0x0200_0000.
        match t.0 {
            v if v == HcsEventSystemExited.0 => Self::SystemExited,
            v if v == HcsEventSystemCrashInitiated.0 => Self::SystemCrashInitiated,
            v if v == HcsEventSystemCrashReport.0 => Self::SystemCrashReport,
            v if v == HcsEventServiceDisconnect.0 => Self::ServiceDisconnect,
            5 => Self::GuestConnectionClosed,
            other => Self::Other(other),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HcsEvent {
    pub kind: HcsEventKind,
    /// Document JSON associé (par ex. état de sortie), s'il y en a un.
    pub data: Option<String>,
    pub at: Instant,
}

#[derive(Default)]
struct EventSink {
    events: Mutex<Vec<HcsEvent>>,
    changed: Condvar,
}

impl EventSink {
    fn push(&self, event: HcsEvent) {
        self.events.lock().unwrap().push(event);
        self.changed.notify_all();
    }

    fn wait_for(&self, kind: HcsEventKind, timeout: Duration) -> Option<HcsEvent> {
        let deadline = Instant::now() + timeout;
        let mut guard = self.events.lock().unwrap();
        loop {
            if let Some(e) = guard.iter().find(|e| e.kind == kind) {
                return Some(e.clone());
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            let (g, _) = self.changed.wait_timeout(guard, deadline - now).unwrap();
            guard = g;
        }
    }
}

unsafe extern "system" fn on_event(event: *const HCS_EVENT, context: *const c_void) {
    if event.is_null() || context.is_null() {
        return;
    }
    // SAFETY : `context` est le pointeur brut d'un `Arc<EventSink>` maintenu vivant par le
    // `ComputeSystem` propriétaire ; on n'en prend pas la propriété ici.
    let sink: &EventSink = unsafe { &*(context as *const EventSink) };
    let ev = unsafe { &*event };
    let data = if ev.EventData.is_null() {
        None
    } else {
        unsafe { ev.EventData.to_string() }.ok()
    };
    tracing::debug!(kind = ?HcsEventKind::from(ev.Type), data = ?data, "événement HCS");
    sink.push(HcsEvent {
        kind: ev.Type.into(),
        data,
        at: Instant::now(),
    });
}

/// Handle sur un compute system, fermé automatiquement (la machine, elle, continue de tourner
/// si `ShouldTerminateOnLastHandleClosed` vaut `false`).
pub struct ComputeSystem {
    handle: HCS_SYSTEM,
    sink: Arc<EventSink>,
}

// SAFETY : les handles HCS sont utilisables depuis n'importe quel thread.
unsafe impl Send for ComputeSystem {}
unsafe impl Sync for ComputeSystem {}

impl ComputeSystem {
    /// Crée un compute system à partir de son document JSON. La machine n'est pas démarrée.
    pub fn create(id: &str, document_json: &str) -> Result<Self> {
        let op = Operation::new()?;
        let handle = unsafe {
            HcsCreateComputeSystem(
                &HSTRING::from(id),
                &HSTRING::from(document_json),
                op.raw(),
                None,
            )
        }
        .map_err(|e| hresult::from_windows("HcsCreateComputeSystem", &e, None))?;
        let system = Self::attach(handle)?;
        op.wait("HcsCreateComputeSystem", Duration::from_secs(60))?;
        Ok(system)
    }

    /// Rouvre un compute system existant par son identifiant.
    pub fn open(id: &str) -> Result<Self> {
        let handle = unsafe { HcsOpenComputeSystem(&HSTRING::from(id), GENERIC_ALL) }
            .map_err(|e| hresult::from_windows("HcsOpenComputeSystem", &e, None))?;
        Self::attach(handle)
    }

    fn attach(handle: HCS_SYSTEM) -> Result<Self> {
        let sink = Arc::new(EventSink::default());
        let ctx = Arc::as_ptr(&sink) as *const c_void;
        unsafe {
            HcsSetComputeSystemCallback(handle, HcsEventOptionNone, Some(ctx), Some(on_event))
        }
        .map_err(|e| hresult::from_windows("HcsSetComputeSystemCallback", &e, None))?;
        Ok(Self { handle, sink })
    }

    pub fn start(&self, timeout: Duration) -> Result<()> {
        let op = Operation::new()?;
        unsafe { HcsStartComputeSystem(self.handle, op.raw(), PCWSTR::null()) }
            .map_err(|e| hresult::from_windows("HcsStartComputeSystem", &e, None))?;
        op.wait("HcsStartComputeSystem", timeout).map(|_| ())
    }

    pub fn shutdown(&self, timeout: Duration) -> Result<()> {
        let op = Operation::new()?;
        unsafe { HcsShutDownComputeSystem(self.handle, op.raw(), PCWSTR::null()) }
            .map_err(|e| hresult::from_windows("HcsShutDownComputeSystem", &e, None))?;
        op.wait("HcsShutDownComputeSystem", timeout).map(|_| ())
    }

    pub fn terminate(&self, timeout: Duration) -> Result<()> {
        let op = Operation::new()?;
        unsafe { HcsTerminateComputeSystem(self.handle, op.raw(), PCWSTR::null()) }
            .map_err(|e| hresult::from_windows("HcsTerminateComputeSystem", &e, None))?;
        op.wait("HcsTerminateComputeSystem", timeout).map(|_| ())
    }

    /// Propriétés courantes, JSON brut.
    pub fn properties(&self, timeout: Duration) -> Result<String> {
        let op = Operation::new()?;
        unsafe { HcsGetComputeSystemProperties(self.handle, op.raw(), PCWSTR::null()) }
            .map_err(|e| hresult::from_windows("HcsGetComputeSystemProperties", &e, None))?;
        Ok(op
            .wait("HcsGetComputeSystemProperties", timeout)?
            .unwrap_or_default())
    }

    /// Envoie une requête de modification (ajout/retrait de périphérique à chaud).
    pub fn modify(&self, request_json: &str, timeout: Duration) -> Result<Option<String>> {
        let op = Operation::new()?;
        unsafe {
            HcsModifyComputeSystem(self.handle, op.raw(), &HSTRING::from(request_json), None)
        }
        .map_err(|e| hresult::from_windows("HcsModifyComputeSystem", &e, None))?;
        op.wait("HcsModifyComputeSystem", timeout)
    }

    pub fn wait_for(&self, kind: HcsEventKind, timeout: Duration) -> Option<HcsEvent> {
        self.sink.wait_for(kind, timeout)
    }

    /// Tous les événements reçus jusqu'ici.
    pub fn events(&self) -> Vec<HcsEvent> {
        self.sink.events.lock().unwrap().clone()
    }

    /// Énumère les compute systems correspondant à une requête JSON (ex. `{"Owners":["Monodon"]}`).
    pub fn enumerate(query_json: &str, timeout: Duration) -> Result<Vec<ComputeSystemSummary>> {
        let op = Operation::new()?;
        unsafe { HcsEnumerateComputeSystems(&HSTRING::from(query_json), op.raw()) }
            .map_err(|e| hresult::from_windows("HcsEnumerateComputeSystems", &e, None))?;
        let doc = op
            .wait("HcsEnumerateComputeSystems", timeout)?
            .unwrap_or_else(|| "[]".to_owned());
        let list = serde_json::from_str(&doc).map_err(|e| {
            MonodonError::internal(format!("énumération HCS illisible : {e} — {doc}"))
        })?;
        Ok(list)
    }
}

impl Drop for ComputeSystem {
    fn drop(&mut self) {
        // Retire le callback avant de libérer le contexte, puis ferme le handle.
        unsafe {
            let _ = HcsSetComputeSystemCallback(self.handle, HcsEventOptionNone, None, None);
            HcsCloseComputeSystem(self.handle);
        }
    }
}
