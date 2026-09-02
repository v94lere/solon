//! Test d'intégration : démarrage et arrêt d'une machine HCS réelle.
//!
//! Exécution (PowerShell **en Administrateur**, Hyper-V / Plateforme de machine virtuelle actifs) :
//!
//! ```powershell
//! $env:SOLON_IT = "1"
//! $env:SOLON_TEST_KERNEL = "C:\chemin\vmlinuz"
//! $env:SOLON_TEST_INITRD = "C:\chemin\initramfs.gz"   # doit afficher SOLON-INIT-OK puis poweroff
//! cargo test -p solon-vm-hcs --test boot -- --nocapture
//! ```
//!
//! Sans `SOLON_IT=1`, le test est ignoré (il ne peut pas tourner sur un poste sans droits).

#![cfg(windows)]

use std::time::{Duration, Instant};

use solon_core::vm::VmConfig;
use solon_vm_hcs::{HcsEventKind, HcsVm, new_vm_id};

fn integration_env() -> Option<(String, String)> {
    if std::env::var("SOLON_IT").ok().as_deref() != Some("1") {
        eprintln!("test ignoré : SOLON_IT=1 absent");
        return None;
    }
    let kernel =
        std::env::var("SOLON_TEST_KERNEL").expect("SOLON_TEST_KERNEL requis avec SOLON_IT=1");
    let initrd =
        std::env::var("SOLON_TEST_INITRD").expect("SOLON_TEST_INITRD requis avec SOLON_IT=1");
    Some((kernel, initrd))
}

fn config(kernel: &str, initrd: &str) -> VmConfig {
    VmConfig {
        id: new_vm_id(),
        name: "solon-test".into(),
        kernel: kernel.into(),
        initrd: initrd.into(),
        cmdline: "console=ttyS0,115200 8250_core.nr_uarts=1 panic=-1 pci=off rdinit=/init".into(),
        memory_mb: 512,
        processors: 1,
        disks: vec![],
        shares: vec![],
        serial_pipe: None,
        network_adapter: None,
    }
}

#[test]
fn demarre_puis_s_arrete_proprement() {
    let Some((kernel, initrd)) = integration_env() else {
        return;
    };
    let cfg = config(&kernel, &initrd);

    let vm = HcsVm::create(&cfg).expect("création");
    let t0 = Instant::now();
    vm.start().expect("démarrage");
    let start_ms = t0.elapsed().as_millis();

    // La machine doit apparaître dans l'énumération par propriétaire pendant qu'elle tourne.
    let owned = HcsVm::list_owned().expect("énumération");
    assert!(
        owned.iter().any(|s| s.id == cfg.id),
        "la machine doit être listée sous Owner=Solon : {owned:?}"
    );

    let exit = vm
        .wait_exit(Duration::from_secs(30))
        .expect("l'invité doit s'éteindre tout seul (poweroff)");
    assert_eq!(exit.kind, HcsEventKind::SystemExited);
    let data = exit.data.expect("HCS fournit un document de sortie");
    assert!(
        data.contains("\"ExitType\":\"GracefulExit\""),
        "sortie attendue propre : {data}"
    );
    eprintln!(
        "démarrage HCS {start_ms} ms, arrêt après {} ms",
        t0.elapsed().as_millis()
    );

    // Une fois arrêtée, la machine ne doit plus être listée (compute system éphémère).
    std::thread::sleep(Duration::from_millis(500));
    let owned = HcsVm::list_owned().expect("énumération");
    assert!(
        !owned.iter().any(|s| s.id == cfg.id),
        "la machine arrêtée doit disparaître : {owned:?}"
    );
}

#[test]
fn une_machine_orpheline_est_retrouvee_et_terminee() {
    let Some((kernel, initrd)) = integration_env() else {
        return;
    };
    let mut cfg = config(&kernel, &initrd);
    // On empêche l'arrêt spontané en démarrant un init qui n'existe pas : le noyau panique
    // et `panic=-1` redémarre… `StopOnReset` transforme ce redémarrage en arrêt. On veut au
    // contraire une machine qui reste allumée : on donne un très long délai avant panique.
    cfg.cmdline =
        "console=ttyS0,115200 8250_core.nr_uarts=1 panic=600 pci=off rdinit=/inexistant".into();

    let id = cfg.id.clone();
    {
        let vm = HcsVm::create(&cfg).expect("création");
        vm.start().expect("démarrage");
        // Le handle est fermé ici sans terminer la machine : elle devient « orpheline ».
    }
    std::thread::sleep(Duration::from_millis(300));

    let reopened = HcsVm::open(&id).expect("rattachement par identifiant");
    assert_eq!(reopened.id(), id);
    drop(reopened);

    let terminated = HcsVm::terminate_orphans(None).expect("terminaison des orphelines");
    assert!(
        terminated.contains(&id),
        "l'orpheline doit être terminée : {terminated:?}"
    );

    std::thread::sleep(Duration::from_millis(500));
    let owned = HcsVm::list_owned().expect("énumération");
    assert!(!owned.iter().any(|s| s.id == id));
}
