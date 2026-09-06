//! Configuration réseau de l'invité : adresse statique fournie par l'hôte (comme WSL2), via `ip`.

use std::process::Command;

use solon_core::protocol::NetworkConfig;

use crate::system::log;

fn ip(args: &[&str]) -> Result<(), String> {
    let out = Command::new("/sbin/ip")
        .args(args)
        .output()
        .map_err(|e| format!("ip {} : {e}", args.join(" ")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "ip {} : {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

pub fn configure(cfg: &NetworkConfig) -> Result<(), String> {
    let dev = cfg.interface.as_str();
    // L'interface hv_netvsc peut apparaître quelques centaines de ms après le démarrage.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !std::path::Path::new(&format!("/sys/class/net/{dev}")).exists() {
        if std::time::Instant::now() > deadline {
            return Err(format!("interface {dev} absente"));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let _ = ip(&["addr", "flush", "dev", dev]);
    if let Some(mtu) = cfg.mtu {
        ip(&["link", "set", "dev", dev, "mtu", &mtu.to_string()])?;
    }
    ip(&["link", "set", "dev", dev, "up"])?;
    ip(&[
        "addr",
        "add",
        &format!("{}/{}", cfg.address, cfg.prefix_len),
        "dev",
        dev,
    ])?;
    let _ = ip(&["route", "del", "default"]);
    ip(&["route", "add", "default", "via", &cfg.gateway, "dev", dev])?;

    let mut resolv = String::new();
    for d in &cfg.dns {
        resolv.push_str(&format!("nameserver {d}\n"));
    }
    if !cfg.search_domains.is_empty() {
        resolv.push_str(&format!("search {}\n", cfg.search_domains.join(" ")));
    }
    resolv.push_str("options timeout:2 attempts:2\n");
    std::fs::write("/etc/resolv.conf", resolv).map_err(|e| format!("resolv.conf : {e}"))?;
    log(&format!(
        "réseau : {dev} {}/{} via {} dns {:?}",
        cfg.address, cfg.prefix_len, cfg.gateway, cfg.dns
    ));
    Ok(())
}

/// Passerelle de l'hôte Windows, mémorisée pour réappliquer les règles à chaque (re)démarrage de dockerd.
static HOST_GATEWAY: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Autorise le trafic de l'hôte Windows (passerelle `gateway`) vers les réseaux Docker, pour les
/// domaines locaux sans port publié. Deux règles, réappliquées par [`reapply_host_rules`] :
/// - table `raw`, en tête de `PREROUTING` : Docker 28+ y ajoute (en fin de chaîne) une règle `DROP`
///   par conteneur pour tout paquet qui n'arrive pas par le pont ; l'`ACCEPT` placé avant l'emporte ;
/// - `DOCKER-USER` (chaîne que dockerd n'efface pas) : `ACCEPT` avant la politique des ponts.
pub fn allow_host_to_containers(gateway: String) {
    let _ = HOST_GATEWAY.set(gateway);
    reapply_host_rules();
}

/// (Ré)applique les règles ci-dessus ; attend jusqu'à 120 s que dockerd ait créé `DOCKER-USER`.
pub fn reapply_host_rules() {
    let Some(gateway) = HOST_GATEWAY.get().cloned() else {
        return;
    };
    std::thread::spawn(move || {
        let iptables = ["/usr/sbin/iptables", "/sbin/iptables"]
            .into_iter()
            .find(|p| std::path::Path::new(p).exists())
            .unwrap_or("iptables");
        let run = |args: &[&str]| -> Result<(), String> {
            let o = std::process::Command::new(iptables)
                .args(args)
                .output()
                .map_err(|e| e.to_string())?;
            if o.status.success() {
                Ok(())
            } else {
                Err(String::from_utf8_lossy(&o.stderr).trim().to_owned())
            }
        };
        // Règle idempotente : `-C` teste, `-I … 1` insère en tête.
        let ensure = |check: &[&str], insert: &[&str]| -> Result<bool, String> {
            if run(check).is_ok() {
                return Ok(false);
            }
            run(insert).map(|_| true)
        };
        let raw_rule = ["-i", "eth0", "-s", gateway.as_str(), "-j", "ACCEPT"];
        match ensure(
            &[&["-t", "raw", "-C", "PREROUTING"], &raw_rule[..]].concat(),
            &[&["-t", "raw", "-I", "PREROUTING", "1"], &raw_rule[..]].concat(),
        ) {
            Ok(true) => crate::system::log(&format!(
                "pare-feu : accès direct de l'hôte {gateway} aux conteneurs autorisé (raw)"
            )),
            Ok(false) => {}
            Err(e) => crate::system::log(&format!("pare-feu (raw) : {e}")),
        }
        for _ in 0..120 {
            if run(&["-S", "DOCKER-USER"]).is_ok() {
                let user_rule = ["-s", gateway.as_str(), "-j", "ACCEPT"];
                match ensure(
                    &[&["-C", "DOCKER-USER"], &user_rule[..]].concat(),
                    &[&["-I", "DOCKER-USER", "1"], &user_rule[..]].concat(),
                ) {
                    Ok(true) => crate::system::log(&format!(
                        "pare-feu : hôte {gateway} autorisé dans DOCKER-USER"
                    )),
                    Ok(false) => {}
                    Err(e) => crate::system::log(&format!("pare-feu (DOCKER-USER) : {e}")),
                }
                return;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        crate::system::log("pare-feu : chaîne DOCKER-USER absente après 120 s, hôte non autorisé");
    });
}
