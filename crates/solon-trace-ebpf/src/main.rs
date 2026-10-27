//! Programmes eBPF de la radiographie, chargés dans le noyau de la machine par l'agent.
//!
//! Ce code ne tourne pas comme un programme ordinaire : le noyau le vérifie instruction par
//! instruction avant de l'accepter, puis l'exécute à chaque fois que l'événement observé se
//! produit. D'où les contraintes : pas d'allocation, pas de boucle non bornée, pas de panique, et
//! une pile de 512 octets. Les événements ressortent par un tampon circulaire que l'agent lit.
//!
//! Une seule sonde, sur les changements d'état des sockets TCP. Elle suffit à dire qui appelle
//! qui, quand, depuis quel programme, et si l'appel a abouti.

#![no_std]
#![no_main]

use aya_ebpf::helpers::{
    bpf_get_current_cgroup_id, bpf_get_current_comm, bpf_get_current_pid_tgid, bpf_ktime_get_ns,
};
use aya_ebpf::macros::{map, tracepoint};
use aya_ebpf::maps::{LruHashMap, RingBuf};
use aya_ebpf::programs::TracePointContext;
use solon_trace_common::{
    AF_INET, COMM_LEN, KIND_CLOSE, KIND_CONNECT, KIND_CONNECT_FAILED, RawEvent, SockMeta,
};

// Décalages des champs dans la trace `sock/inet_sock_set_state`, tels que le noyau les publie
// dans `/sys/kernel/tracing/events/sock/inet_sock_set_state/format`. C'est une interface stable :
// les champs ne se réordonnent pas d'une version à l'autre, on peut lire par décalage sans avoir
// besoin des structures internes du noyau — donc sans bpftool ni en-têtes C.
const OFF_SKADDR: usize = 8;
const OFF_OLDSTATE: usize = 16;
const OFF_NEWSTATE: usize = 20;
const OFF_SPORT: usize = 24;
const OFF_DPORT: usize = 26;
const OFF_FAMILY: usize = 28;
const OFF_SADDR_V6: usize = 40;
const OFF_DADDR_V6: usize = 56;

// États TCP utilisés (`include/net/tcp_states.h`).
const TCP_ESTABLISHED: i32 = 1;
const TCP_SYN_SENT: i32 = 2;
const TCP_CLOSE: i32 = 7;

/// 256 Kio : à 88 octets par événement, de quoi absorber une rafale de quelques milliers de
/// connexions si l'agent prend du retard. Au-delà, le noyau laisse tomber les nouvelles plutôt
/// que de bloquer le processus observé — c'est le comportement voulu, une sonde ne doit jamais
/// ralentir ce qu'elle regarde.
#[map]
static EVENTS: RingBuf = RingBuf::with_byte_size(256 * 1024, 0);

/// Connexions en cours d'ouverture, par adresse de socket. `Lru` et non `Hash` : une connexion
/// dont on ne voit jamais l'issue — socket libéré sans transition observée — laisserait sinon une
/// entrée pour toujours et finirait par remplir la table.
#[map]
static SOCKETS: LruHashMap<u64, SockMeta> = LruHashMap::with_max_entries(16384, 0);

#[tracepoint(category = "sock", name = "inet_sock_set_state")]
pub fn solon_sock_state(ctx: TracePointContext) -> u32 {
    let _ = handle(&ctx);
    0
}

fn handle(ctx: &TracePointContext) -> Result<(), i64> {
    // SAFETY : lectures dans le tampon de la trace, aux décalages que le noyau publie lui-même.
    let (skaddr, oldstate, newstate) = unsafe {
        (
            ctx.read_at::<u64>(OFF_SKADDR)?,
            ctx.read_at::<i32>(OFF_OLDSTATE)?,
            ctx.read_at::<i32>(OFF_NEWSTATE)?,
        )
    };

    // Ouverture : seul moment où le processus courant est bien l'appelant. On met de côté ce que
    // lui seul peut donner ; l'issue, elle, arrive en interruption logicielle.
    if newstate == TCP_SYN_SENT {
        let meta = SockMeta {
            cgroup_id: unsafe { bpf_get_current_cgroup_id() },
            opened_ns: unsafe { bpf_ktime_get_ns() },
            pid: (bpf_get_current_pid_tgid() >> 32) as u32,
            _pad: 0,
            comm: bpf_get_current_comm().unwrap_or([0u8; COMM_LEN]),
        };
        return SOCKETS.insert(&skaddr, &meta, 0);
    }

    let kind = match (oldstate, newstate) {
        (TCP_SYN_SENT, TCP_ESTABLISHED) => KIND_CONNECT,
        // Passer de « j'appelle » à « fermé » sans jamais s'établir : refus, filtrage, ou
        // abandon. Le symptôme que l'on cherche le plus souvent.
        (TCP_SYN_SENT, TCP_CLOSE) => KIND_CONNECT_FAILED,
        (_, TCP_CLOSE) => KIND_CLOSE,
        _ => return Ok(()),
    };

    // Sans mise de côté, la connexion n'a pas été ouverte depuis cette machine : c'est une
    // connexion entrante, que l'on verra de toute façon par son autre extrémité.
    let meta = match unsafe { SOCKETS.get(&skaddr) } {
        Some(m) => *m,
        None => return Ok(()),
    };
    let now = unsafe { bpf_ktime_get_ns() };
    emit(ctx, kind, &meta, now)?;
    if newstate == TCP_CLOSE {
        let _ = SOCKETS.remove(&skaddr);
    }
    Ok(())
}

/// Réserve une place dans le tampon, la remplit, la publie. Si le tampon est plein, `reserve`
/// renvoie `None` et l'événement est perdu sans bruit.
fn emit(ctx: &TracePointContext, kind: u32, meta: &SockMeta, now: u64) -> Result<(), i64> {
    // SAFETY : mêmes décalages publiés par le noyau.
    let (sport, dport, family, saddr, daddr) = unsafe {
        (
            ctx.read_at::<u16>(OFF_SPORT)?,
            ctx.read_at::<u16>(OFF_DPORT)?,
            ctx.read_at::<u16>(OFF_FAMILY)?,
            // Le noyau remplit toujours les champs sur 16 octets, en y recopiant l'adresse IPv4
            // sous forme projetée quand la famille est AF_INET : une seule lecture suffit.
            ctx.read_at::<[u8; 16]>(OFF_SADDR_V6)?,
            ctx.read_at::<[u8; 16]>(OFF_DADDR_V6)?,
        )
    };
    let Some(mut slot) = EVENTS.reserve::<RawEvent>(0) else {
        return Ok(());
    };
    slot.write(RawEvent {
        kind,
        pid: meta.pid,
        cgroup_id: meta.cgroup_id,
        ts_ns: now,
        duration_ns: if kind == KIND_CLOSE {
            now.saturating_sub(meta.opened_ns)
        } else {
            0
        },
        comm: meta.comm,
        saddr: if family == AF_INET {
            v4_of(&saddr)
        } else {
            saddr
        },
        daddr: if family == AF_INET {
            v4_of(&daddr)
        } else {
            daddr
        },
        sport,
        dport,
        family,
        _pad: 0,
    });
    slot.submit(0);
    Ok(())
}

/// Extrait l'adresse IPv4 d'une adresse projetée `::ffff:a.b.c.d` et la met en tête, pour que
/// l'agent n'ait pas à connaître cette convention.
fn v4_of(mapped: &[u8; 16]) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[..4].copy_from_slice(&mapped[12..16]);
    out
}

/// Jamais appelé : `panic = "abort"` et aucun chemin de ce code ne panique. Le noyau refuserait
/// de toute façon un programme qui saute dans du code non vérifié.
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
