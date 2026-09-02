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

## Bloc 1 — image Linux complète et moteur Docker (2 septembre 2026)

Outil : `cargo run -p solon-vm-hcs --example engine_smoke -- <vmlinuz> <initrd.img> <rootfs.vhd> <data.vhdx> --busybox <busybox>`
en Administrateur. Image construite par `image/build.sh` : noyau `6.18.40.1-solon` (16 Mo, netfilter en dur), initrd 0,6 Mo,
`rootfs.vhd` 293 Mo (Alpine v3.24, docker-engine 29.5.3, containerd 2.3.2, runc 1.4.3, compose 5.1.4, 42 paquets),
`data.vhdx` dynamique de 20 Go. Machine : 2 048 Mo, 4 processeurs.

### Chaîne de démarrage (depuis `HcsStartComputeSystem`, machine hôte au repos)

| Étape | Temps |
|---|---|
| `SOLON-INITRD-OK` (rootfs monté sous overlay, `switch_root`) | 688 ms |
| Agent PID 1 prêt (systèmes de fichiers virtuels, `e2fsck -p` du disque de données, RPC et relais à l'écoute) | 757 ms |
| **`SOLON-ENGINE-READY` : dockerd répond sur `/run/docker.sock`** | **1 158 ms** (réseau des conteneurs désactivé) ; **1 265 ms et 1 459 ms** sur deux cycles avec pont `docker0` et règles netfilter |
| `docker version` depuis le CLI Windows via `\\.\pipe\solon` | 47–125 ms |
| `docker import` d'une image de 1 Mo | 78 ms |
| `docker run --rm … echo` (création, exécution, suppression) | 491–514 ms (réseau pont) ; 620–640 ms sans réseau |
| `docker run -d` | 298–308 ms |
| Arrêt propre demandé par RPC → événement HCS `GracefulExit` | ~380 ms |

Le disque de données est reconnu, vérifié (`fsck` code 0) et remonté à chaque cycle ; l'image importée au cycle 1
est présente au cycle 2 (**persistance validée**). Sous forte charge CPU de l'hôte (compilation du noyau en
parallèle dans WSL), la même chaîne mesure 3,1–3,3 s : l'ordre de grandeur nominal est bien ~1,2 s.

Premier démarrage (disque de données vierge) : le formatage ext4 de 20 Go ajoute ~240 ms.

### Faits établis par le bloc 1

- **ACL obligatoire** : sans ACE pour le groupe « NT VIRTUAL MACHINE\Virtual Machines » (`S-1-5-83-0`) sur les
  disques, `HcsStartComputeSystem` échoue avec `E_ACCESSDENIED` (`0x80070005`) alors que la création réussit.
  `solon-vm-hcs::acl` l'accorde (lecture, ou lecture-écriture pour le disque de données, plus traversée du dossier).
- Un **VHD fixe** généré par `image/tools/mkvhd.py` est accepté comme disque SCSI par HCS. Un pied de fichier
  mal aligné donne `ERROR_FILE_CORRUPT` (`0x80070570`) au démarrage : la disposition (checksum à l'octet 64,
  UUID à 68) est maintenant vérifiée par relecture.
- **PID 1 démarre sans environnement** : sans `PATH`, dockerd ne trouve ni `runc` ni `iptables` (« executable file
  not found in $PATH »). L'agent fixe `PATH`, `HOME`, `TMPDIR` avant tout.
- **Aucun module** n'est embarqué : toutes les options netfilter que Docker utilise doivent être `=y`. La
  configuration WSL les laisse en modules ; la première image échouait sur « Extension addrtype revision 0 not
  supported ». Le script du noyau convertit désormais les familles réseau (`NF_*`, `NFT_*`, `NETFILTER_*`,
  `IP_NF_*`, `IP6_NF_*`, `BRIDGE_*`, `VETH`, `MACVLAN`, …) en dur et vérifie les options clés.
- **Named pipe sans demi-fermeture** : `docker run --rm` restait bloqué tant que le relais utilisait
  `copy_bidirectional` ; le client ne voit la fin d'un flux hijacké que si le serveur déconnecte le pipe. Le relais
  copie les deux sens séparément et ferme tout dès qu'un sens se termine.
- containerd 2.x : `disable = true` sous `[plugins."io.containerd.cri.v1.runtime"]` est ignoré (mode strict) ;
  il faut `disabled_plugins = [...]` au niveau racine.
- L'invité voit 1,95 Go de RAM pour 2 048 Mo alloués, avec ~1,8 Go disponibles une fois dockerd démarré :
  **le moteur au repos occupe ~150 Mo côté invité**. La mesure côté hôte (processus `vmmem`, ballon, hints
  mémoire) est l'objet du bloc 2.

## Bloc 2 — service Windows, réseau, ports publiés, robustesse (2 septembre 2026)

Outil : `solon-service console --start` (Administrateur, build **debug**) puis le scénario non élevé
`e2e.ps1` : état via `\.\pipe\solon-control`, CLI `docker` via `\.\pipe\solon`, `docker pull`,
port publié relayé vers `localhost`, mesure mémoire, arrêt propre. Image `0.1.0-dev.2`, machine 2 048 Mo / 4 processeurs.

| Mesure | Valeur |
|---|---|
| **Provisionnement complet, prérequis → dockerd prêt, build release** (premier démarrage de la session : 2 606 ms ; redémarrage à chaud : 2 500 ms) | **2,5–2,6 s** |
| Le même en build debug (dont ~10 s de SHA-256 non optimisé sur 300 Mo) | 11,7–12,8 s |
| dont vérification SHA-256 de l'image en release (300 Mo) | ~1 s |
| Création du réseau HNS ICS + endpoint | ~0,7 s |
| `docker version` depuis un utilisateur **non élevé** | 60–80 ms |
| `docker pull public.ecr.aws/docker/library/alpine:3.20` (réseau sortant NAT + DNS, image ~3,5 Mo) | 0,9 s |
| `docker run -d -p 8080:80` → première réponse HTTP sur `http://localhost:8080` depuis Windows | 2,3 s (dont démarrage du serveur dans le conteneur et détection de la publication) |
| Fermeture du relais après suppression du conteneur | < 1 s |
| **RAM au repos après 60 s** (processus `vmmem` + `solon-service`), 4 exécutions | **426–516 Mo** (408–500 + 16–18) pour 2 048 Mo alloués ; la variation suit le cache de pages de l'invité (images tirées juste avant) |
| Arrêt propre (agent → dockerd → `poweroff`, réseau HNS supprimé) | 0,6 s |

### Faits établis par le bloc 2

- **`connect()` HvSocket lancé avant que l'invité n'écoute reste bloqué 30 s** (délai système) au lieu
  d'être refusé. Sans `HVSOCKET_CONNECT_TIMEOUT` (option de socket niveau `HV_PROTOCOL_RAW`), la boucle de
  réessai ne réessaie jamais ; borné à 1 s par tentative.
- **Réseau HNS de type ICS** (JSON WSL : `Type=ICS`, `Flags=9`, `IsolateSwitch=true`, sous-réseau statique)
  créé par `HcnCreateNetwork` fonctionne sur Windows 11 26200 : carte `vEthernet (Solon)` 172.30.0.1/24,
  invité 172.30.0.2 configuré statiquement par l'agent, NAT et résolution DNS opérationnels (DNS de l'hôte
  relayés). L'endpoint reçoit un GUID distinct de la machine.
- **Le CLI `docker events` ne vide pas sa sortie tant qu'il tourne** quand elle est redirigée : l'agent
  lit désormais `GET /events` directement sur le socket Unix (HTTP/1.1 par morceaux). Les ports publiés sont
  recalculés par `docker ps` + `docker inspect` à chaque événement de conteneur.
- **Relais des ports publiés sans réseau virtuel** : écouteur TCP sur l'hôte → HvSocket → agent →
  conteneur (`10.90.0.x`). Fonctionne pour `0.0.0.0` et `127.0.0.1` ; UDP hors périmètre.
- **Le CLI `docker` de Windows avec Docker Desktop installé** utilise par défaut le gestionnaire
  d'identifiants Windows (`docker-credential-wincred`) et envoie des identifiants Docker Hub périmés
  (« unauthorized: incorrect username or password ») ; ce n'est pas lié à Solon (les tirages depuis un autre
  registre réussissent). À documenter pour les utilisateurs ; l'application (bollard) n'envoie aucun identifiant.
- **La cible RAM < 500 Mo est atteinte sans marge** (426–516 Mo mesurés). Leviers identifiés pour le bloc de durcissement : `drop_caches` côté invité après inactivité, hints mémoire HCS (`EnableColdDiscardHint`), allocation initiale plus basse que 2 Go quand la machine hôte a peu de RAM.

### Robustesse (scripts `tests/e2e/`)

| Test | Résultat | Détail |
|---|---|---|
| `crash-force-stop.ps1` : terminaison brutale de la machine (`stop --force`, sans arrêt invité) pendant qu'un conteneur écrit en boucle dans un volume | **OK** | `e2fsck -p` code 0 (journal rejoué) ; compteur écrit avec `sync` : 507 avant, 514 après (rien de perdu) ; journal écrit sans `sync` : 513 lignes conservées sur ~514 (fenêtre de perte bornée à ~2 s par le `sync()` périodique de l'agent) ; conteneur marqué `Exited (255)` par dockerd au redémarrage ; moteur prêt 12,8 s après (build debug) |
| `crash-service-kill.ps1` : service tué (`taskkill /F`) pendant que la machine tourne, relance du service, rattachement | **OK** | même identifiant de machine avant/après, conteneur `solon-survivor` toujours `Up`, journal « rattachement à une machine encore en marche » ; le rattachement passe désormais avant la vérification d'image (chemin rapide) |

Sans le `sync()` périodique, la première exécution avait perdu **toutes** les écritures des 4 dernières
secondes (compteur revenu à 0, conteneur en état « Created » car l'état de dockerd lui-même n'avait pas
atteint le disque) : c'est le comportement ext4 `data=ordered` standard (validation toutes les 5 s). Le
`sync()` toutes les 2 s dans l'agent ramène la fenêtre à ~2 s pour un coût négligeable au repos.
