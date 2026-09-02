# Mesures

Toutes les mesures sont prises sur la machine de test décrite dans `ARCHITECTURE.md` §1.1
(Windows 11 Pro 26200, i7-13700, 13,7 Go). Elles sont reproduites à chaque bloc et jamais promises à l'avance.

## Bloc 0a — démarrage d'un noyau dans une VM HCS (2 septembre 2026)

Outil : `cargo run -p solon-vm-hcs --example boot_smoke -- <noyau> <initrd>` en Administrateur.
Initramfs de spike : busybox statique + `/init` qui affiche des marqueurs puis `poweroff -f`.
Machine : 1 024 Mo, 2 processeurs, aucun disque, console série COM1 sur named pipe.
Ligne de commande noyau : `console=ttyS0,115200 8250_core.nr_uarts=1 panic=-1 pci=off rdinit=/init`.

| Étape (depuis `HcsStartComputeSystem`) | Noyau WSL2 6.18.33 (local, non redistribué) | Alpine `linux-virt` 6.18.35 |
|---|---|---|
| `HcsCreateComputeSystem` (avant démarrage) | 51 ms | ~50 ms |
| `HcsStartComputeSystem` rend la main | 29 ms | 25 ms |
| Premier octet sur la console série | 241 ms | 185 ms |
| `/init` démarre (espace utilisateur) | 723 ms | 609 ms |
| Marqueur `SOLON-INIT-OK` | 794 ms | 611 ms |
| `poweroff` invité → événement de sortie HCS | 1 797 ms (dont 1 s de `sleep` volontaire dans init) | ~1 660 ms (idem) |

Lecture : **le noyau atteint l'espace utilisateur en ~0,6–0,7 s** après l'ordre de démarrage. Le temps total
« API Docker prête » dépendra ensuite de `containerd` + `dockerd` (mesuré au bloc 1).

### Tests d'intégration (`crates/solon-vm-hcs/tests/boot.rs`, exécutés en Administrateur)

| Test | Résultat | Durée |
|---|---|---|
| `demarre_puis_s_arrete_proprement` : création, démarrage, présence dans l'énumération `Owner=Solon`, arrêt spontané détecté avec `ExitType=GracefulExit`, disparition après arrêt | OK | 1,4 s |
| `une_machine_orpheline_est_retrouvee_et_terminee` : handle fermé sans arrêt, rattachement par `HcsOpenComputeSystem`, terminaison par `terminate_orphans`, disparition | OK | ~1,4 s |

Note pratique : une élévation UAC (`Start-Process -Verb RunAs`) **n'hérite pas** des variables d'environnement
du shell appelant ; les tests sont lancés via un fichier `.cmd` qui définit `SOLON_IT`, `SOLON_TEST_KERNEL`
et `SOLON_TEST_INITRD` lui-même.

### Faits établis par le spike

- Le schéma HCS 2.2 avec `Chipset.LinuxKernelDirect` + `ComPorts` + `HvSocket` + mémoire en surallocation
  (`AllowOvercommit`, `EnableDeferredCommit`) est accepté sur Windows 11 26200.
- Le named pipe de la console est créé par le processus de la machine (`vmwp`) en **serveur** ; on s'y
  connecte en client dès `HcsStartComputeSystem` (connexion réussie en < 1 ms après le retour de l'appel).
- L'événement `HcsEventSystemExited` porte un document JSON exploitable pour distinguer un arrêt propre
  d'un crash : `{"Status":0,"ExitType":"GracefulExit","Attribution":[{"SystemExit":{"Detail":"Shutdown","Initiator":"GuestOS"}}, ...]}`.
- Le noyau WSL2 expose `/proc/config.gz` et contient **en dur** (`=y`) tout ce dont Solon a besoin :
  `HYPERV`, `HYPERV_VSOCKETS` (hv_sock), `HYPERV_STORAGE`, `HYPERV_NET`, `HYPERV_UTILS`, `HYPERV_BALLOON`,
  `PAGE_REPORTING`, `NET_9P`, `9P_FS`, `9P_FS_POSIX_ACL`, `EXT4_FS`, `OVERLAY_FS`, `SQUASHFS`, `VETH`,
  `NF_TABLES`, `VSOCKETS`, `VIRTIO_CONSOLE`. Seuls `BRIDGE` et `EROFS_FS` sont en modules (`=m`) : notre
  configuration les passera en `=y`. `virtiofs` est même déjà enregistré côté invité (inutile sans hôte).
- Le noyau Alpine `linux-virt` démarre aussi, mais `hv_sock` et les pilotes Hyper-V y sont en modules
  (aucun chargé sans initramfs adapté) et il n'expose pas sa configuration : il reste le plan B.
- Sans élévation, la création échoue immédiatement avec `HCS_E_ACCESS_DENIED` (`0x8037011B`) et le message
  Windows « Privilèges insuffisants. Seuls les administrateurs ou les utilisateurs membres du groupe
  d'utilisateurs Administrateurs Hyper-V sont autorisés à accéder aux machines virtuelles ou conteneurs. »
  Solon le traduit en code `INSUFFICIENT_PRIVILEGES`. Le service Windows (LocalSystem) est confirmé comme
  nécessaire pour que l'application n'ait jamais à s'élever.
