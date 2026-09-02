//! Agent invité Solon : PID 1 de la machine Linux.
//!
//! Rôles, dans l'ordre du démarrage :
//! 1. monter les systèmes de fichiers virtuels, préparer et vérifier le disque de données (ext4) ;
//! 2. lancer `containerd` puis `dockerd`, les superviser, récolter les processus orphelins ;
//! 3. servir les RPC de l'hôte sur vsock 5000 (santé, montages 9P, commandes, arrêt propre) ;
//! 4. relayer l'API Docker (vsock 5001 → `/run/docker.sock`).
//!
//! Hors Linux, le binaire n'a pas de sens : il s'arrête avec un message.

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("solon-agent ne s'exécute que sous Linux (cible x86_64-unknown-linux-musl).");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
mod bench;
#[cfg(target_os = "linux")]
mod forward;
#[cfg(target_os = "linux")]
mod rpc;
#[cfg(target_os = "linux")]
mod system;
#[cfg(target_os = "linux")]
mod vsock;

#[cfg(target_os = "linux")]
fn main() {
    use std::sync::Arc;
    use std::time::Instant;

    let t0 = Instant::now();
    let is_init = std::process::id() == 1;
    // PID 1 démarre sans environnement : dockerd et containerd cherchent runc, iptables, nft dans PATH.
    // SAFETY : aucun autre thread n'existe encore.
    unsafe {
        std::env::set_var("PATH", "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin");
        std::env::set_var("HOME", "/root");
        std::env::set_var("TMPDIR", "/tmp");
    }
    system::log(&format!("démarrage (pid {}, init={is_init})", std::process::id()));

    let state = Arc::new(system::State::default());
    if is_init {
        system::mount_virtual_filesystems();
        system::start_reaper(state.clone());
        system::configure_network_basics();
        match system::prepare_data_disk(&state) {
            Ok(report) => system::log(&format!("disque de données : {report}")),
            Err(e) => system::log(&format!("AVERTISSEMENT disque de données : {e} — repli sur tmpfs (données non persistantes)")),
        }
        system::start_engine(&state);
    } else {
        system::log("pas PID 1 : mode RPC seul (aucun service démarré)");
    }

    // Relais de l'API Docker et RPC de contrôle.
    {
        let st = state.clone();
        std::thread::spawn(move || forward::serve(st));
    }
    let ready_ms = t0.elapsed().as_millis();
    let uptime = system::uptime_secs();
    println!("SOLON-AGENT-READY agent_ms={ready_ms} uptime_s={uptime:.2}");
    rpc::serve(state);
}
