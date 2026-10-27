//! Vocabulaire commun aux programmes eBPF qui tournent dans le noyau de la machine et à l'agent
//! qui les lit. Les deux côtés sont compilés séparément — le noyau avec une chaîne figée dans un
//! conteneur, l'agent depuis Windows — donc la disposition mémoire est écrite explicitement et
//! doit rester identique des deux côtés.
//!
//! `no_std` : ce code est partagé avec la caisse `solon-trace-ebpf`, qui n'a pas de bibliothèque
//! standard.

#![no_std]

/// Taille de `RawEvent::comm`, qui reprend celle de `TASK_COMM_LEN` du noyau.
pub const COMM_LEN: usize = 16;

/// Un processus vient d'être lancé. Premier type d'événement, il sert à vérifier que la chaîne
/// noyau → agent → service → interface fonctionne de bout en bout.
pub const KIND_EXEC: u32 = 1;

/// Événement brut tel que le noyau l'écrit dans le tampon circulaire.
///
/// `repr(C)` et des entiers de taille fixe : l'agent relit ces octets tels quels, sans
/// désérialisation. Tout champ ajouté l'est en fin de structure, et `KIND_*` dit lesquels sont
/// renseignés.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawEvent {
    /// L'une des constantes `KIND_*`.
    pub kind: u32,
    /// Identifiant du processus (partie haute de `bpf_get_current_pid_tgid`).
    pub pid: u32,
    /// Identifiant du cgroup, qui permet de retrouver le conteneur côté agent.
    pub cgroup_id: u64,
    /// Horloge monotone du noyau, en nanosecondes depuis le démarrage de la machine.
    pub ts_ns: u64,
    /// Nom court du processus, complété par des zéros.
    pub comm: [u8; COMM_LEN],
}

impl RawEvent {
    /// Taille d'un événement sur le fil ; l'agent s'en sert pour découper le tampon.
    pub const SIZE: usize = core::mem::size_of::<Self>();

    /// Le nom du processus sans les zéros de remplissage.
    pub fn comm_bytes(&self) -> &[u8] {
        let end = self.comm.iter().position(|&b| b == 0).unwrap_or(COMM_LEN);
        &self.comm[..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La disposition est un contrat entre deux binaires compilés séparément : si elle change
    /// d'un côté sans l'autre, l'agent lit n'importe quoi. Ce test fige la taille.
    #[test]
    fn taille_de_l_evenement_figee() {
        assert_eq!(RawEvent::SIZE, 40);
    }

    #[test]
    fn comm_s_arrete_au_premier_zero() {
        let mut comm = [0u8; COMM_LEN];
        comm[..5].copy_from_slice(b"nginx");
        let ev = RawEvent {
            kind: KIND_EXEC,
            pid: 1,
            cgroup_id: 0,
            ts_ns: 0,
            comm,
        };
        assert_eq!(ev.comm_bytes(), b"nginx");
    }

    #[test]
    fn comm_sans_zero_final_prend_tout() {
        let ev = RawEvent {
            kind: KIND_EXEC,
            pid: 1,
            cgroup_id: 0,
            ts_ns: 0,
            comm: [b'a'; COMM_LEN],
        };
        assert_eq!(ev.comm_bytes().len(), COMM_LEN);
    }
}
