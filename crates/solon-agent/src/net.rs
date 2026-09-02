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
