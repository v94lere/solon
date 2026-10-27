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

/// Une connexion TCP sortante vient de s'établir.
pub const KIND_CONNECT: u32 = 2;
/// Une connexion TCP sortante a échoué : refusée, filtrée, ou abandonnée avant réponse. C'est le
/// cas le plus utile au débogage, et celui qu'aucun journal applicatif ne montre côté appelant
/// quand la bibliothèque avale l'erreur.
pub const KIND_CONNECT_FAILED: u32 = 3;
/// Une connexion établie s'est fermée ; `duration_ns` est renseigné.
pub const KIND_CLOSE: u32 = 4;

/// Familles d'adresses, telles que le noyau les rapporte.
pub const AF_INET: u16 = 2;
pub const AF_INET6: u16 = 10;

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
    /// Identifiant du processus à l'origine de l'appel (partie haute de `bpf_get_current_pid_tgid`).
    pub pid: u32,
    /// Identifiant du cgroup de ce processus, qui permet de retrouver le conteneur appelant.
    pub cgroup_id: u64,
    /// Horloge monotone du noyau, en nanosecondes depuis le démarrage de la machine.
    pub ts_ns: u64,
    /// Durée de vie de la connexion pour [`KIND_CLOSE`], sinon zéro.
    pub duration_ns: u64,
    /// Nom court du processus, complété par des zéros.
    pub comm: [u8; COMM_LEN],
    /// Adresses sur 16 octets dans les deux cas : une adresse IPv4 occupe les 4 premiers et
    /// `family` dit comment les lire.
    pub saddr: [u8; 16],
    pub daddr: [u8; 16],
    pub sport: u16,
    pub dport: u16,
    pub family: u16,
    /// Complément d'alignement ; `repr(C)` l'ajouterait de toute façon, autant l'écrire.
    pub _pad: u16,
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

/// Ce que le noyau retient d'une connexion entre son ouverture et son issue.
///
/// La transition vers `SYN_SENT` est la seule qui se produise dans le contexte du processus
/// appelant : c'est le seul moment où le cgroup et le nom du programme sont lisibles. Les
/// transitions suivantes arrivent en interruption logicielle, à la réception d'un paquet, où le
/// processus courant n'a plus rien à voir avec la connexion. D'où cette mise de côté, reprise à
/// l'issue.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SockMeta {
    pub cgroup_id: u64,
    pub opened_ns: u64,
    pub pid: u32,
    pub _pad: u32,
    pub comm: [u8; COMM_LEN],
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La disposition est un contrat entre deux binaires compilés séparément : si elle change
    /// d'un côté sans l'autre, l'agent lit n'importe quoi. Ces tests figent les tailles.
    #[test]
    fn taille_de_l_evenement_figee() {
        assert_eq!(RawEvent::SIZE, 88);
    }

    #[test]
    fn taille_des_metadonnees_figee() {
        assert_eq!(core::mem::size_of::<SockMeta>(), 40);
    }

    #[test]
    fn comm_s_arrete_au_premier_zero() {
        let mut comm = [0u8; COMM_LEN];
        comm[..5].copy_from_slice(b"nginx");
        assert_eq!(event(comm).comm_bytes(), b"nginx");
    }

    #[test]
    fn comm_sans_zero_final_prend_tout() {
        assert_eq!(event([b'a'; COMM_LEN]).comm_bytes().len(), COMM_LEN);
    }

    fn event(comm: [u8; COMM_LEN]) -> RawEvent {
        RawEvent {
            kind: KIND_CONNECT,
            pid: 1,
            cgroup_id: 0,
            ts_ns: 0,
            duration_ns: 0,
            comm,
            saddr: [0; 16],
            daddr: [0; 16],
            sport: 0,
            dport: 0,
            family: AF_INET,
            _pad: 0,
        }
    }
}
