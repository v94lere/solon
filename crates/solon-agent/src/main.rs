//! Agent invité Solon : PID 1 de la machine Linux.
//!
//! Rôles, dans l'ordre du démarrage :
//! 1. monter les systèmes de fichiers virtuels, préparer et vérifier le disque de données (ext4) ;
//! 2. lancer `containerd` puis `dockerd`, les superviser, récolter les processus orphelins ;
//! 3. servir les RPC de l'hôte (vsock 5000), pousser les événements (5003), relayer l'API Docker
//!    (5001) et les ports publiés (5002). Protocole partagé : `solon_core::protocol`.
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
mod events;
#[cfg(target_os = "linux")]
mod forward;
#[cfg(target_os = "linux")]
mod net;
#[cfg(target_os = "linux")]
mod ports;
#[cfg(target_os = "linux")]
mod rpc;
#[cfg(target_os = "linux")]
mod shell;
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
        std::env::set_var(
            "PATH",
            "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        );
        std::env::set_var("HOME", "/root");
        std::env::set_var("TMPDIR", "/tmp");
    }
    system::log(&format!(
        "démarrage v{} (pid {}, init={is_init})",
        env!("CARGO_PKG_VERSION"),
        std::process::id()
    ));

    let state = Arc::new(system::State::default());
    if is_init {
        system::mount_virtual_filesystems();
        system::start_reaper(state.clone());
        system::configure_network_basics();
        match system::prepare_data_disk(&state) {
            Ok(report) => system::log(&format!(
                "disque de données : {} monté sur {} (formaté maintenant : {}, fsck code {} : {})",
                report.device,
                report.mounted_at,
                report.formatted_now,
                report.fsck_exit,
                report.fsck_summary
            )),
            Err(e) => system::log(&format!(
                "AVERTISSEMENT disque de données : {e} — repli sur tmpfs (données non persistantes)"
            )),
        }
        system::start_engine(&state);
        system::start_periodic_sync();
        system::start_idle_cache_release();
    } else {
        system::log("pas PID 1 : mode RPC seul (aucun service démarré)");
    }

    {
        let st = state.clone();
        std::thread::spawn(move || forward::serve(st));
    }
    {
        let st = state.clone();
        std::thread::spawn(move || events::serve(st));
    }
    {
        let st = state.clone();
        std::thread::spawn(move || ports::serve(st));
    }
    {
        let st = state.clone();
        std::thread::spawn(move || shell::serve(st));
    }
    let ready_ms = t0.elapsed().as_millis();
    println!(
        "SOLON-AGENT-READY agent_ms={ready_ms} uptime_s={:.2}",
        system::uptime_secs()
    );
    rpc::serve(state);
}
