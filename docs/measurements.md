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

### Faits établis par le spike 0a

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

## Bloc 0b — canal HvSocket, partage 9P, micro-bancs (2 septembre 2026)

Outil : `cargo run -p solon-vm-hcs --example channel_smoke -- <noyau> <initrd> <dossier> --bench-opts ...`
en Administrateur. L'initrd embarque l'agent spike (`solon-agent`, musl statique, 567 Ko, compilé
croisé depuis Windows avec `rust-lld`) qui écoute sur `AF_VSOCK` port 5000. Machine : 1 024 Mo, 2 processeurs,
noyau WSL2 local. Trois exécutions complètes, chiffres de la dernière (les deux autres sont dans la même fourchette).

### Canal de contrôle HvSocket

| Mesure | Valeur |
|---|---|
| Agent à l'écoute (marqueur console) après `HcsStartComputeSystem` | 718–753 ms |
| Connexion `AF_HYPERV` depuis l'hôte (agent déjà à l'écoute) | 1 ms |
| Aller-retour `PING`/`PONG` (200 itérations, texte + JSON) | médiane 436–486 µs, p99 596–863 µs |
| Débit invité → hôte (128 MiB, blocs de 1 MiB) | 2 300–2 750 MiB/s |
| Débit hôte → invité (128 MiB) | 3 550–4 950 MiB/s |
| `tokio::net::TcpStream::from_std` sur le socket Hyper-V | fonctionne (PING async 338 µs) |

Conclusion : le canal est largement dimensionné pour l'API Docker, les journaux, l'exec et le relais de ports.
La latence sub-milliseconde autorise un protocole RPC simple sans regroupement de requêtes. Tokio accepte ces
sockets sans adaptation : le service pourra être entièrement asynchrone.

### Partage 9P (Plan9 de HCS), dossier NTFS sur SSD

| Mesure | 9P options par défaut (`msize=65536`) | `msize=1048576` | `msize=262144` | `cache=loose` | tmpfs (référence RAM) |
|---|---|---|---|---|---|
| Écriture séquentielle 128 MiB + fsync | 277–334 MiB/s | 240 MiB/s | 214 MiB/s | 164 MiB/s | 2 650–4 340 MiB/s |
| Lecture séquentielle 128 MiB (caches vidés) | 368–452 MiB/s | 392 MiB/s | 386 MiB/s | 285 MiB/s | ~10 500 MiB/s |
| Création + écriture de 1 000 fichiers de 4 Kio | 1 533–1 875 ms (≈1,7 ms/fichier) | 3 518 ms | 3 327 ms | 2 422 ms | 2–3 ms |
| `stat` de 1 000 fichiers (caches vidés) | 1 117–1 745 ms (≈1,2–1,7 ms/stat) | 1 269 ms | 1 109 ms | **438 ms** | 0 ms |
| Lecture de 1 000 fichiers de 4 Kio | 1 335–1 656 ms | 1 621 ms | 1 701 ms | **616 ms** | 1 ms |
| `readdir` de 1 000 entrées | 1–3 ms | 1 ms | 3 ms | 4 ms | 0 ms |
| Suppression de 1 000 fichiers | 793–1 053 ms | 964 ms | 1 173 ms | 527 ms | 1 ms |

Lecture honnête :

- **Débit séquentiel correct** (plusieurs centaines de Mio/s) : compilation d'un projet, lecture d'archives, journaux : acceptable.
- **Métadonnées lentes : ~1 à 2 ms par opération** (`stat`, `open`, `create`, `unlink`). C'est la classe de performance
  de `/mnt/c` sous WSL2. Un `git status` sur 50 000 fichiers ou un `npm install` sur un dossier Windows monté sera
  lent (dizaines de secondes à minutes) ; ce point est documenté dans le plan comme le risque R1 et il est confirmé.
- Augmenter `msize` **n'apporte rien** (le serveur 9P de Windows plafonne visiblement autour de 64 Kio) ; on garde 64 Kio.
- `cache=loose` divise par ~2,5 le coût des métadonnées et des petites lectures, mais au prix de la cohérence
  (modifications faites côté Windows vues avec retard, voire jamais tant que le cache tient) et d'un débit séquentiel
  moindre. Inacceptable par défaut pour un flux d'édition Windows → conteneur ; envisageable comme option « lecture
  seule » pour des dépendances figées.
- Les fichiers écrits par l'invité apparaissent immédiatement dans l'Explorateur et inversement ; `chmod` est conservé
  grâce au drapeau `LinuxMetadata` (attributs étendus NTFS).

### Faits établis par le spike 0b

- `SOCKADDR_HV.VmId` attend le `RuntimeId` du compute system (ici égal à l'`Id` fourni, mais on lit toujours `RuntimeId`).
- Toutes les connexions sont initiées par l'hôte ; aucun enregistrement dans
  `HKLM\...\GuestCommunicationServices` n'a été nécessaire pour ce sens.
- **Un partage Plan9 n'accepte qu'une seule session 9P** : une seconde connexion au même port échoue au montage avec
  `EFAULT` (« Bad address »), exactement l'erreur observée par Cowork sur Windows Home. Un dossier à exposer deux fois
  se déclare deux fois (deux ports). Conséquence : un montage par lecteur, jamais deux.
- L'ajout et le retrait de partages **à chaud** (`HcsModifyComputeSystem`, `VirtualMachine/Devices/Plan9/Shares`)
  fonctionnent, y compris cinq partages simultanés ; le drapeau `ReadOnly` est respecté (écriture refusée côté invité).
- Le montage côté invité prend < 1 ms une fois la connexion vsock ouverte.
- L'agent invité en Rust musl statique se compile depuis Windows sans chaîne de compilation C
  (`linker = "rust-lld"`, `link-self-contained=yes`) : le pipeline d'image n'a pas besoin de Linux pour cette partie.
