//! Programmes eBPF de la radiographie, chargés dans le noyau de la machine par l'agent.
//!
//! Ce code ne tourne pas comme un programme ordinaire : le noyau le vérifie instruction par
//! instruction avant de l'accepter, puis l'exécute à chaque fois que l'événement observé se
//! produit. D'où les contraintes : pas d'allocation, pas de boucle non bornée, pas de panique, et
//! une pile de 512 octets. Les événements ressortent par un tampon circulaire que l'agent lit.
//!
//! Pour l'instant un seul programme, sur le lancement de processus : il sert à vérifier que la
//! chaîne noyau → agent → service → interface tient. Les sondes réseau arrivent à l'étape
//! suivante.

#![no_std]
#![no_main]

use aya_ebpf::helpers::{
    bpf_get_current_cgroup_id, bpf_get_current_comm, bpf_get_current_pid_tgid, bpf_ktime_get_ns,
};
use aya_ebpf::macros::{map, tracepoint};
use aya_ebpf::maps::RingBuf;
use aya_ebpf::programs::TracePointContext;
use solon_trace_common::{KIND_EXEC, RawEvent};

/// 256 Kio : à 40 octets par événement, de quoi absorber une rafale de quelques milliers
/// d'événements si l'agent prend du retard. Au-delà, le noyau laisse tomber les nouveaux plutôt
/// que de bloquer le processus observé — c'est le comportement voulu, une sonde ne doit jamais
/// ralentir ce qu'elle regarde.
#[map]
static EVENTS: RingBuf = RingBuf::with_byte_size(256 * 1024, 0);

#[tracepoint(category = "sched", name = "sched_process_exec")]
pub fn solon_exec(_ctx: TracePointContext) -> u32 {
    emit(KIND_EXEC);
    0
}

/// Réserve une place dans le tampon, la remplit, la publie. Si le tampon est plein, `reserve`
/// renvoie `None` et l'événement est perdu sans bruit.
fn emit(kind: u32) {
    let Some(mut slot) = EVENTS.reserve::<RawEvent>(0) else {
        return;
    };
    // SAFETY : ces trois auxiliaires sont utilisables depuis un tracepoint et ne lisent que le
    // contexte du processus courant.
    let (cgroup_id, ts_ns, pid) = unsafe {
        (
            bpf_get_current_cgroup_id(),
            bpf_ktime_get_ns(),
            (bpf_get_current_pid_tgid() >> 32) as u32,
        )
    };
    slot.write(RawEvent {
        kind,
        pid,
        cgroup_id,
        ts_ns,
        comm: bpf_get_current_comm().unwrap_or([0u8; 16]),
    });
    slot.submit(0);
}

/// Jamais appelé : `panic = "abort"` et aucun chemin de ce code ne panique. Le noyau refuserait
/// de toute façon un programme qui saute dans du code non vérifié.
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
