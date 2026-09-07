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

## Bloc 3 — application de bureau (2 septembre 2026)

Application Tauri v2 (React 19, TypeScript, Tailwind 4, i18n anglais/français) testée contre le service en
mode console (build release) : captures dans la conversation de développement, pas de chiffres de
performance spécifiques à ce bloc.

### Validé à l'écran

- Barre d'état du moteur alimentée par l'abonnement au service (aucun polling) ; boutons démarrer / redémarrer / arrêter.
- Vue conteneurs : liste temps réel (invalidée par les événements Docker), filtre, groupe « projet Compose » via le
  label `com.docker.compose.project`, statut, ports publiés, CPU et mémoire en direct (flux de statistiques agrégé
  sur un seul canal), actions Démarrer / Arrêter / Redémarrer / Journaux / Supprimer.
- Suppression avec boîte de confirmation accessible (`<dialog>`, Échap annule, option « volumes anonymes »).
- Détail : journaux en flux avec suivi et horodatage, terminal `xterm.js` sur un exec hijacké bidirectionnel avec
  redimensionnement du TTY, inspection JSON copiable.
- Écran de première installation : étapes de provisionnement en direct, erreurs traduites par code stable avec
  détails techniques et accès au dossier des journaux ; message dédié quand le service est absent.
- Thème sombre suivant Windows, interface en anglais par défaut, français disponible dans les réglages.

### Faits établis par le bloc 3

- **`ERROR_PIPE_BUSY` (231)** : un named pipe serveur n'a qu'une instance libre à la fois entre deux connexions ;
  une application qui ouvre plusieurs connexions en rafale doit réessayer (5 s, pas de 30 ms). Corrigé dans le
  client de l'application et dans le CLI du service.
- Les Channels Tauri v2 transportent sans difficulté les flux de journaux, de statistiques et le terminal
  (octets en base64) ; chaque flux a un identifiant et une fermeture explicite au démontage de la vue.
- `i18next` v26 attend les clés de pluriel `count_one` / `count_other` (plus `count_plural`).

## Bloc 4 — images, volumes, réseaux, Compose, barre des tâches (2 septembre 2026)

### Validé

- **Compose** : dossier Windows partagé à la demande (`EnsureShare` → partage 9P du lecteur `C:` ajouté à chaud,
  monté sur `/mnt/host/c` par l'agent, chemin traduit), puis `docker compose up -d` exécuté **dans la machine**
  via l'agent : deux services démarrés, port `8090` publié relayé vers `http://localhost:8090` (réponse reçue),
  bind mount `./data` créé côté Windows par le conteneur (traverse le partage 9P). `docker compose ps` et
  regroupement par projet dans la vue conteneurs.
- Vues **Images** (lancer un conteneur avec ports/variables/commande, inspecter, supprimer avec confirmation),
  **Volumes** (créer, inspecter, supprimer avec confirmation) et **Réseaux** (créer, inspecter, supprimer ; réseaux
  intégrés protégés) alimentées par bollard et rafraîchies par les événements Docker.
- **Barre des tâches** : icône avec état du moteur, nombre de conteneurs en marche (rafraîchi toutes les 5 s),
  démarrer / arrêter, ouvrir, quitter ; fermer la fenêtre la cache.

### Faits établis par le bloc 4

- **Le périphérique `Plan9` doit exister dès la création de la machine** (`Devices.Plan9.Shares = []`), sinon
  `HcsModifyComputeSystem` pour ajouter un partage échoue avec `ERROR_NOT_FOUND` (`0x80070490`). Le spike du
  bloc 0b ne l'avait pas révélé car la machine y démarrait déjà avec un partage.
- Compose dans la machine n'a besoin d'aucun binaire côté Windows : le plugin `docker-cli-compose` d'Alpine suffit ;
  la sortie est capturée en fin de commande (pas de flux) — limitation acceptée pour le MVP.

## Bloc 5 — durcissement (2 septembre 2026)

### Installeur NSIS et vrai service Windows (`tests/e2e/install-test.ps1`, release, image 0.1.0-dev.3)

| Étape | Résultat |
|---|---|
| Installation silencieuse `/S` (une fenêtre UAC) | 40 s, code 0 ; `C:\Program Files\Solon\{solon.exe, solon-service.exe, image\, installer\}` ; 80 Mo d'installeur (324 Mo décompressés) |
| `setup.ps1` (élevé) | Hyper-V et Plateforme de machine virtuelle détectés « déjà activés » ; `SolonService` installé (Automatique) et démarré |
| Premier démarrage du moteur par le **service Windows réel** (LocalSystem, session 0, disque de données créé et formaté) | prêt en **3 974 ms** (canal de contrôle) ; démarrages suivants : 1 144 ms (rattachement) |
| `docker version` depuis un utilisateur non élevé (ACL `IU`) | OK (29.5.3) |
| Port publié 8080 → conteneur, retrait à la suppression | OK (2,4 s jusqu'à la première réponse HTTP, image absente au départ) |
| Arrêt propre | 673 ms |
| `docker pull public.ecr.aws/…/busybox` | **échec `toomanyrequests: Rate exceeded`** : quota anonyme du registre ECR public, pas Solon (le `docker run` suivant a bien tiré l'image) |

### Mémoire au repos (2 Go alloués, moteur sans conteneur, machine hôte au repos)

| Instant | `vmmem` (working set) |
|---|---|
| +1 min après démarrage | 454 Mo |
| +2 à +5 min | **426 Mo** (plancher stable) ; `solon-service` 16 Mo |

Dans l'invité au même moment : `used` 105 Mo, `buff/cache` 97 Mo, `available` 1 737 Mo. `dmesg` confirme que le
mécanisme est actif : `hv_balloon: Dynamic Memory protocol version 2.0`, `Free page reporting enabled`,
`Cold memory discard hint enabled with order 9` (blocs de 2 Mo). **Les hints HCS n'ont pas abaissé le plancher**
(426 Mo était déjà le minimum observé au bloc 2) : ~220 Mo restent tenus par l'hôte (pages fragmentées non
signalables par blocs de 2 Mo, tables de pages, tampons de vmwp). Pistes si l'on veut descendre : allouer moins
par défaut (1 Go), `drop_caches` plus agressif suivi d'un compactage (`/proc/sys/vm/compact_memory`) pour former
des blocs de 2 Mo signalables. Non fait au MVP : 426–516 Mo reste le budget annoncé.

### Faits établis par le bloc 5

- **Le vrai service Windows fonctionne comme le mode console** : même provisionnement, mêmes pipes, moteur prêt en
  moins de 4 s au premier démarrage (création + formatage du disque compris).
- **PowerShell 5.1 exige un BOM UTF-8** pour tout script contenant des accents ; sans lui, `setup.ps1` échouait
  en silence dans l'installeur (aucun journal, service absent). Règle notée dans `CONTRIBUTING.md`.
- Les variables PowerShell sont insensibles à la casse : `$svc` a écrasé `$Svc` de `common.ps1`.
- **Aucun enregistrement HvSocket dans le registre n'est nécessaire** : toutes les connexions sont ouvertes par
  l'hôte, et les tests passent sans (la première version de `setup.ps1` en créait 204 en 26 s ; retirés).
- Le ballon Hyper-V (`CONFIG_HYPERV_BALLOON=y`) et le signalement de pages libres sont actifs avec les hints
  `EnableColdDiscardHint`/`EnableHotHint`/`EnableColdHint` ; ils n'apportent rien de mesurable sur le plancher.
- Les tests unitaires couvrent désormais le **catalogue d'erreurs** : chaque variante de `ErrorCode` doit avoir un
  message dans les deux langues, sinon `cargo test -p solon` échoue.
- Le registre ECR public limite les tirages anonymes (`toomanyrequests`) : les tests de bout en bout doivent
  tolérer cet échec ou utiliser une image déjà présente.

## Bloc 6 — livraison (3 septembre 2026)

### Installeur final `Solon_0.1.0_x64-setup.exe` (80 Mo), mise à jour par-dessus l'installation du bloc 5

| Étape | Résultat |
|---|---|
| Installation silencieuse `/S` par-dessus une version installée (service en marche) | **16 s**, code 0 ; hook pré-installation arrête le service, `setup.ps1` le réinstalle et le redémarre ; données de `%ProgramData%\Solon` conservées (image busybox déjà présente au test suivant) |
| Moteur prêt (service Windows réel, rattachement du disque existant) | 3 435 ms |
| Scénario `e2e.ps1` complet (prérequis, `docker` non élevé, pull, port 8080, retrait du port, arrêt propre) | **9/9 OK** ; pull 960 ms ; première réponse HTTP 2,3 s ; arrêt propre 634 ms |
| Mémoire au repos 60 s après activité | 494 Mo (vmmem) + 16 Mo (service) = 510 Mo |

### Faits établis par le bloc 6

- **Une mise à jour doit arrêter le service avant la copie des fichiers** : le bundler NSIS de Tauri ferme
  l'application mais ignore le service, qui tient `solon-service.exe` ouvert. Hook `NSIS_HOOK_PREINSTALL` ajouté.
- Le menu de la barre des tâches lit ses libellés dans les mêmes fichiers `locales/*.json` que le frontend
  (`include_str!`) : une seule source de traduction, couverte par le test de parité des clés.
- `tauri icon` accepte un SVG et régénère toutes les tailles (Windows, macOS, mobiles) ; les jeux Android/iOS
  ont été retirés du dépôt (hors périmètre).

## Comparatif Docker Desktop 4.66.1 (WSL2) — Solon 0.1.0, même machine, 3 septembre 2026

Machine : Windows 11 Pro 26200, 16 cœurs. Charge : `examples/odoo18` (Odoo 18 Community + PostgreSQL 16), même fichier
Compose, même dossier partagé (`config/`, `addons/`), volumes nommés pour les données. Docker Desktop utilise ses
réglages par défaut (WSL2, tous les cœurs, 6,6 Go visibles par les conteneurs) ; Solon ses réglages par défaut
(4 processeurs, 2 Go).

| Mesure | Docker Desktop | Solon | Lecture |
|---|---|---|---|
| Moteur prêt après l'ordre de démarrage | 6,1 s (application déjà installée, WSL chaud) | 1,1 s (rattachement) / 2,6–3,4 s (démarrage complet) | Solon 2 à 5× plus rapide |
| Mémoire au repos, aucun conteneur | **1 998 Mo** (vmmemWSL 1 335 + processus Docker Desktop 663) | **426–516 Mo** | Solon ≈ 4× plus léger |
| Mémoire avec Odoo + PostgreSQL, 150 s de repos | **5 146 Mo** (vmmemWSL 4 771) | **1 892 Mo** (dont invité : 353 Mo utilisés, 400 Mo de cache ; conteneurs 151 + 114 Mo) | Solon ≈ 2,7× plus léger ; les deux gardent trop de cache |
| `docker pull odoo:18.0` (2 Go) depuis ECR public | **échec** (`cloudfront.net … EOF`, 3 essais) ; images transférées depuis Solon par `docker save/load` | 38,8 s | Réseau sortant de Docker Desktop défaillant sur cette machine, pas mesuré |
| `compose down` + `compose up -d`, images présentes | 7,1 s | 10,3 s | Docker Desktop plus rapide (plus de cœurs, `down` compris côté Solon) |
| Odoo répond après `up` | 1,5 s | 2,9–3,1 s | |
| Création d'une base avec données de démonstration | 13,6 s | 16,4–17,7 s | Docker Desktop +20 % : 16 cœurs contre 4 pour Solon (réglable) |
| Page de connexion, moyenne de 5 chargements | 27–46 ms | 34 ms | Équivalent |

Conclusions honnêtes : Solon gagne nettement sur ce qui coûte tous les jours (démarrage, mémoire) ; à charge égale
Docker Desktop reste 20 à 30 % plus rapide sur les tâches CPU parce qu'il dispose par défaut de tous les cœurs (Solon :
4, modifiable dans Réglages) ; la latence des requêtes web est identique. Les deux moteurs gardent le cache disque de
l'invité en mémoire ; Solon plafonne à l'allocation (2 Go), Docker Desktop monte à 5 Go.

Incidents pendant la mesure : rate limit anonyme du registre ECR (`toomanyrequests`) sur les deux moteurs ; Odoo lancé
en double sur le même dossier `config/` réécrit `admin_passwd` haché à tour de rôle (sans conséquence, même mot de passe).

## Lot du 3 septembre 2026 (soir) — projets, terminal machine, CLI intégré, recherche, cœurs

| Mesure | Résultat |
|---|---|
| Processeurs par défaut sur la machine de test (24 cœurs logiques) | 22 (`nproc` dans l'invité) ; le réglage reste modifiable |
| Terminal dans la machine | ouvert en < 1 s, `sh -l` root, redimensionnement suivi ; `Ctrl+\`` |
| CLI intégré (`C:\Program Files\Solonin` en tête du PATH) | `docker version` : client 29.7.2 / serveur 29.5.3 ; `docker compose version` : **c'était le plugin de Docker Desktop** qui répondait ; sans lui (test machine vierge), `docker compose` était introuvable : `DOCKER_CLI_PLUGIN_EXTRA_DIRS` n'est pas lue par le CLI 29, seule la clé `cliPluginsExtraDirs` de `config.json` l'est. Corrigé dans le lanceur. |
| Installeur | 105 Mo avec le CLI et Compose (+25 Mo) |
| **Défaut corrigé** : partages non remontés après redémarrage du moteur | reproduit sur Odoo (`/etc/odoo/odoo.conf` absent, `No section: 'options'`) ; après correctif : `solon.shares=c:9100` dans la ligne de commande du noyau, `/mnt/host/c` monté avant dockerd, Odoo redémarre avec sa configuration, connexion HTTP 200 |
| Redémarrage du moteur avec un partage à remonter | 5,2 s (contre 1,1 s en rattachement) |

Faits établis : une machine créée à chaud n'a que les partages ajoutés pendant sa vie ; tout partage doit être
**persisté côté service** (`state.json`) et redéclaré à la création suivante, sinon les montages `-v` des conteneurs
pointent sur des dossiers vides. Le port vsock d'un partage est `9100 + index d'ajout`, identique à chaud et au boot.

### Complément du 3 septembre 2026 (soir) — après passage à 22 processeurs

| Mesure | Docker Desktop | Solon (22 cœurs) | Solon (4 cœurs, plus haut) |
|---|---|---|---|
| Création d'une base Odoo avec démo | 13,6 s | **17,9 s** | 16,4–17,7 s |
| `compose down` + `up -d` | 7,1 s | 8,3 s | 10,3 s |
| Odoo répond après `up` | 1,5 s | 1,4 s | 3 s |
| Page de connexion (moyenne de 5) | 27–46 ms | 64 ms | 34 ms |
| Mémoire avec la pile, après repos | 5 146 Mo | **1 196 Mo** | 1 892 Mo |
| `pg_test_fsync` fdatasync (écritures synchrones, volume Docker) | 154 ops/s (6,5 ms) | **290 ops/s (3,4 ms)** | — |
| `pg_test_fsync` fsync | 79 ops/s | **137 ops/s** | — |

Lecture : le nombre de cœurs **n'était pas** la cause de l'écart sur la création de base (tâche essentiellement
mono-thread : Python d'Odoo + une connexion PostgreSQL). Le disque n'est pas en cause non plus : les écritures
synchrones sont ~2× plus rapides sur Solon. Suspects restants, à tester isolément : le `sync()` global toutes les
2 s de l'agent (peut bloquer les écrivains pendant un import massif), et la vitesse par cœur perçue dans la machine
(ordonnancement HCS avec 22 vCPU pour 24 cœurs logiques). Le redémarrage de pile et la mise en route d'Odoo sont
désormais au niveau de Docker Desktop ; la mémoire est passée sous 1,2 Go avec la pile.

## Lot « trois défauts » (5–6 septembre 2026)

| Vérification | Résultat |
|---|---|
| `docker run -v C:\…:/data` depuis le CLI Windows (via `bin\docker.exe`) | lecture et écriture du dossier Windows OK, lecteur partagé à la volée |
| `--mount type=bind,source=C:/…,readonly` | OK (chemins avec `/` acceptés) |
| `docker compose up -d` avec le CLI **Windows** et des binds relatifs (`./config`) | OK ; `inspect` montre `/mnt/host/c/Users/…/config` |
| `docker exec` (connexion hijackée) à travers le mandataire | OK |
| Sortie Compose en direct dans l'écran projet | lignes affichées au fil de l'eau, code de sortie en fin |
| Mémoire hôte avec Odoo + PostgreSQL, 150 s de repos | **612 Mo** (contre 1 196–1 892 Mo avant) ; invité : 357 Mo utilisés, 207 Mo de cache |

Faits établis : le compactage mémoire (`/proc/sys/vm/compact_memory`) après `drop_caches` est ce qui permet au ballon
Hyper-V de rendre la mémoire ; sans lui, les pages libres restent fragmentées et l'hôte garde 1,2 à 1,9 Go. Un
installeur construit pendant l'écriture de `rootfs.vhd` embarque un fichier de bonne taille mais d'empreinte fausse :
le contrôle SHA-256 du service l'a détecté (`IMAGE_CORRUPTED`) ; attendre la fin complète de `image/build.sh`.

## Lot « caractère » (6 septembre 2026)

| Vérification | Résultat |
|---|---|
| Bloc `hosts` écrit par le service | `odoo.odoo18.solon.local`, `odoo18-odoo-1.solon.local`, `web.solon.local` → 127.0.0.1 |
| `Resolve-DnsName odoo.odoo18.solon.local` | 127.0.0.1 (le fichier hosts l'emporte sur mDNS pour `.local`) |
| `http://odoo.odoo18.solon.local/web/login?db=demo` | HTTP 200 (page Odoo complète, 5 087 octets) |
| `http://web.solon.local/` | HTTP 200 |
| Nom inconnu | non résolu (pas d'entrée hosts) ; en cas de `Host` inconnu sur 127.0.0.1:80, page 404 listant les domaines |
| Port 80 sur cette machine | libre : mandataire actif (`local_domains=true` dans l'état) || Notification Windows | toast « Solon : conteneur arrêté — crashtest s'est terminé avec le code 3 » 3 s après `exit 3`, icône Solon |
| Export de diagnostic | archive de 14 fichiers (18 Ko) : LISEZMOI, état, prérequis, réglages, partages, version, docker info, state.json, journaux récents, bloc hosts |

## Lot « dossiers rapides » — solonfs contre 9P (6 septembre 2026)

Arbre de test : 5 000 fichiers de 0,2 à 4 Ko dans 250 × 2 dossiers (forme d'un `node_modules`), sur le disque C:,
**copie fraîche** pour chaque mesure (jamais ouverte auparavant). Mesures prises depuis la machine Linux du moteur.
Disque natif ext4 de la machine, pour l'échelle : `find` 19 ms, `stat` de tout 50 ms, `cat` de tout 19 ms.

| Opération | 9P Windows (`/mnt/host/c`) | **solonfs** (`/mnt/solonfs/c`) | Gain |
|---|---|---|---|
| `find` (5 000 fichiers, 500 dossiers) | 5 279 ms | **800 ms** | 6,6× |
| `stat` de chaque fichier | 11 240 ms | **119 ms** | 94× |
| lecture de chaque fichier, 1er passage | 23 570 ms | **6 579 ms** | 3,6× |
| lecture de chaque fichier, 2e passage | 21 500 ms | **2 390 ms** | 9× |
| `grep -r` sur l'arbre | 13 730 ms | **2 300 ms** | 6× |
| écriture de 500 fichiers | 3 699 ms | **849 ms** | 4,4× |
| relecture des 500 | 3 940 ms | **200 ms** | 20× |
| renommage de 100 | 1 740 ms | **200 ms** | 8,7× |
| `rm -rf` | 1 689 ms | **279 ms** | 6× |

Comment : un aller-retour hôte↔invité coûte ~0,5 ms quel que soit le protocole ; 9P en dépense un par `stat`.
solonfs renvoie les attributs de **tout un dossier** en une réponse et les garde 1,5 s en cache côté invité ; les
lectures demandent 128 Ko d'un coup ; et, mesure clé, la **première ouverture d'un fichier côté Windows coûte
~3,5 ms** (contre 0,2 ensuite), d'où un préchauffage des petits fichiers d'un dossier dès qu'il est listé.
Reste : la première lecture est encore à 1,3 ms/fichier (le préchauffage ne rattrape pas tout), et le client FUSE
est mono-thread.

Défauts trouvés et corrigés pendant le lot : (1) `rm -rf` sautait des entrées parce que la liste d'un dossier
était relue pendant la suppression → listage figé par descripteur ouvert ; (2) un démarrage a monté le disque de
données comme racine (`/dev/sda` n'est pas garanti) → disques choisis par étiquette ext4 (`solon-root`, `solon-data`)
dans l'initrd et l'agent.

### Bascule des conteneurs sur solonfs (6 septembre 2026)

| Vérification | Résultat |
|---|---|
| Montages après démarrage | `solonfs on /mnt/host/c` (FUSE), `hcs-plan9 on /mnt/host9p/c` (secours) |
| Odoo (compose, `./config` et `./addons` montés) | démarre, « Using configuration file at /etc/odoo/odoo.conf », page de connexion HTTP 200 via `odoo.odoo18.solon.local` |
| `docker run -v %TEMP%\…:/data` : lecture d'un fichier Windows, écriture d'un fichier et d'un sous-dossier | vus côté Windows immédiatement |
| Fichier modifié côté Windows puis lu par un nouveau conteneur 2 s après | nouveau contenu (cache 1,5 s) |
| Démarrage du moteur avec les deux montages | 2,1 s |

## Test « machine vierge » (6 septembre 2026) — phase 1

Machine de développement remise dans l'état d'un PC sans Docker (`tests/e2e/fresh-machine-phase1.ps1`, élevé) :
Docker Desktop 4.66.1 désinstallé (12 s), distributions WSL `docker-desktop` et `Ubuntu` retirées (Ubuntu exportée
avant en 134 s, 10,25 Go), WSL 2.7.12 désinstallé (8 s), Solon désinstallé avec ses données (6 s, bloc `hosts` propre),
composants `Microsoft-Hyper-V-All`, `VirtualMachinePlatform` et `Microsoft-Windows-Subsystem-Linux` désactivés
(redémarrage requis). Phase 2 (installation de Solon seul, activation des composants par l'installeur, redémarrages,
vérifications `fresh-machine-phase2.ps1`) : résultats à la suite.

### Test « machine vierge » — phase 2 (6 septembre 2026)

Après redémarrage, installeur lancé depuis le Bureau comme un utilisateur (SmartScreen accepté à la main).
`setup.log` : `Microsoft-Hyper-V` activé en 20 s, `VirtualMachinePlatform` en 8 s, PATH ajouté, service installé,
**redémarrage requis** signalé à l'installeur ; après ce second redémarrage, `SolonService` était démarré tout seul.

| Vérification (`fresh-machine-phase2.ps1`, sans élévation) | Résultat |
|---|---|
| Docker Desktop absent, WSL absent (paquet MSI) | OK (`wsl.exe` de façade répond « non installé ») |
| Service installé et en marche après le redémarrage | OK |
| Prérequis tous verts (composants activés par l'installeur) | OK |
| Premier démarrage du moteur (création + formatage du disque de données) | **6,7 s** ; redémarrage suivant 1,0 s |
| `docker version` avec le CLI livré par Solon (PATH machine) | 29.7.2 / 29.5.3 |
| `docker run hello-world` (réseau sortant, registre ECR public) | OK en 539 ms à la seconde exécution ; **la première a échoué** ~1 min après le démarrage de Windows (réseau ou registre pas encore prêt), voir ci-dessous |
| Montage d'un dossier Windows via solonfs, aller-retour | OK |
| Port publié relayé sur localhost | OK |
| Domaine local `fresh-web.solon.local` | OK |
| Mémoire du moteur | 630–672 Mo |

Conclusion : **Solon fonctionne seul**, sans Docker Desktop ni WSL, y compris le chemin « composants désactivés →
activation → redémarrage » de l'installeur jamais testé jusque-là. Point à surveiller : le premier `docker pull`
juste après un redémarrage de Windows peut échouer (réseau pas encore stable ou quota du registre) ; à
reproduire avant d'ajouter un nouvel essai automatique côté moteur.

### Après le test machine vierge (6 septembre 2026) : corrections révélées par un vrai usage

| Défaut | Cause | Correction | Vérification |
|---|---|---|---|
| `docker compose` introuvable | le plugin de Docker Desktop masquait l'absence du nôtre ; le CLI 29 ignore `DOCKER_CLI_PLUGIN_EXTRA_DIRS` | le lanceur inscrit `bin\cli-plugins` dans `cliPluginsExtraDirs` de `config.json` | `docker compose version` → v5.1.4 dans un shell neuf |
| Warpgate `setup` : « failed to tighten file permissions (EPERM) » | `DefaultPermissions` sur le montage FUSE : le noyau refusait `chmod` à un utilisateur non root sur des fichiers présentés comme root | option retirée, le système de fichiers arbitre (chmod/chown acceptés et ignorés) | `chmod 600` + `chown` par uid 1000 sur un dossier Windows : OK |
| Image du moteur non reconstructible sans WSL | pipeline lié à Ubuntu WSL | **construction dans un conteneur Solon** (Alpine 3.24), dépôt monté par solonfs ; scripts corrigés (SIGPIPE `curl | head`), taille de bloc ext4 fixée à 4 Kio | image dev.12 construite en **14 s** (contre 3 à 4 min sous WSL), agent présent, empreinte conforme |

## Lot « des adresses qui marchent, toujours » (6 septembre 2026)

Objectif : joindre tout conteneur en marche depuis Windows **sans publier de port**, par son adresse et par `https://nom.solon.local`, avec un certificat accepté par le navigateur. Image du moteur **0.1.0-dev.14** (agent : règles de pare-feu pour l'hôte), service : route, autorité locale, mandataire HTTPS.

### Ce qui bloquait, et la solution retenue

| Constat (mesuré dans la machine) | Conséquence | Correction |
|---|---|---|
| Windows n'a aucune route vers `10.90.0.0/16` | `curl http://10.90.0.2/` : délai dépassé | `route add 10.90.0.0 mask 255.255.0.0 172.30.0.2` posé par le service au démarrage, retiré à l'arrêt |
| `iptables -t raw -S PREROUTING` : `-d 10.90.0.2/32 ! -i docker0 -j DROP` par conteneur (Docker 28+, « protection contre l'accès direct ») | paquets de l'hôte détruits avant conntrack (0 entrée `dst=10.90.0.2`) | `ACCEPT -i eth0 -s 172.30.0.1` inséré **en tête** de `raw PREROUTING` par l'agent ; Docker **ajoute** ses règles en fin (`-A`), vérifié après redémarrage d'un conteneur et création d'un autre |
| `DOCKER` : `! -i docker0 -o docker0 -j DROP` | trafic hors ponts refusé dans `FORWARD` | `ACCEPT -s 172.30.0.1` en tête de `DOCKER-USER` (chaîne préservée par dockerd) |
| première version de l'agent : `/sbin/iptables` introuvable (Alpine : `/usr/sbin/iptables`) | règle jamais posée, sans trace | chemin cherché parmi `/usr/sbin`, `/sbin` ; échec journalisé |
| alternative écartée : `default-network-opts` → `gateway_mode_ipv4=nat-unprotected` dans `daemon.json` | testé : supprime les `DROP` des **nouveaux** réseaux utilisateur mais pas ceux de `docker0` (le pont par défaut n'en tient pas compte) | non retenu, les deux `ACCEPT` suffisent |

### Vérifications (PC de test, image dev.14 en place, conteneurs `web` (busybox httpd, **aucun port publié**), `odoo18` (compose))

| Test | Résultat |
|---|---|
| `route print` | `10.90.0.0/16 → 172.30.0.2` métrique 5 |
| `curl http://10.90.0.2/` (3 essais) | `hello from web container` à chaque fois |
| `curl http://10.90.1.3:8069/web/login` | HTTP 303 (Odoo, redirection vers le gestionnaire de bases) |
| `Test-NetConnection 10.90.1.2 -Port 5432` | `True` (PostgreSQL joignable directement, port non publié) |
| `ping 10.90.0.2`, `ping 10.90.1.3` | réponses |
| bloc `hosts` | `web`, `odoo.odoo18`, `db.odoo18`, `odoo18-odoo-1`, `odoo18-db-1` `.solon.local` → 127.0.0.1 |
| `http://web.solon.local/` | `hello from web container` (conteneur sans port publié) |
| `https://web.solon.local/` (.NET / Schannel) | 200, certificat `CN=web.solon.local` émis par `O=Solon, CN=Solon Local CA`, expire le 1er janvier 2036 |
| `https://odoo.odoo18.solon.local/web/login` (.NET) | 200 |
| Edge (mode headless) sur `https://web.solon.local/` | page rendue, aucun avertissement de certificat |
| `curl.exe https://…` | code 35 `CRYPT_E_NO_REVOCATION_CHECK` sans `--ssl-no-revoke` ; OK avec (limite connue de curl/Schannel, identique à mkcert) |
| `certutil -store Root "Solon Local CA"` | présent, `NotAfter 01/01/2036` |
| conteneur → hôte (`wget http://172.30.0.1/`, `ping 172.30.0.1`) | bloqué par le pare-feu Windows sur `vEthernet (Solon)` : inchangé, non nécessaire |
| démarrage du moteur | route, mandataires 80 et 443 et autorité prêts en **0,16 s** après le réseau (journal : 14:49:16.618 → 16.774) |

### Défauts trouvés en installant, corrigés dans ce lot

| Défaut | Cause | Correction |
|---|---|---|
| Après mise à jour silencieuse : `IMAGE_CORRUPTED` (manifeste dev.14, image dev.13) | l'hyperviseur garde `vmlinuz`, `initrd.img`, `rootfs.vhd` ouverts quelques secondes après l'arrêt de la machine ; l'installeur remplaçait le manifeste mais pas l'image verrouillée | `installer/hooks.nsh` : après l'arrêt du service, attente (60 s au plus) que chaque fichier de `image\` s'ouvre en exclusif |
| `solon-ca.key` lisible par tous les utilisateurs | `%ProgramData%` hérite d'un droit de lecture pour « Utilisateurs » | dossier `ca\` : héritage retiré, SYSTEM et administrateurs seuls (`icacls`), fichiers existants remis en héritage (`/reset`) ; une première version avec `/T` laissait les fichiers **sans aucun droit** (même SYSTEM refusé) |
| Installeur reconstruit sans le service | `npm run tauri build` ne recompile pas `solon-service` (ressource copiée depuis `target/release`) | rappel : `cargo build --release -p solon-service` **avant** `tauri build` (déjà dans le README) |

### Ce qui reste fragile

- Le port **443** est aussi réservé sur `127.0.0.1` : un IIS ou un autre serveur local sur 443 désactive le HTTPS (le HTTP sur 80 reste), avec un avertissement dans le journal.
- Les règles de l'agent visent la passerelle `172.30.0.1` : si la plage HNS change (collision détectée au démarrage), la règle suit puisque l'adresse vient de `ConfigureNetwork`.
- La route Windows est non persistante (recréée à chaque démarrage du moteur) : si le service est tué sans passer par l'arrêt, une route orpheline reste jusqu'au redémarrage ou au prochain démarrage du moteur (elle est d'abord supprimée puis recréée).
- La clé de l'autorité est dans `%ProgramData%\Solon\ca\solon-ca.key` (droits SYSTEM/administrateurs) : un administrateur local peut signer des certificats pour n'importe quel nom **sur cette machine seulement** (l'autorité n'est installée nulle part ailleurs), comme avec mkcert.

## Lot « menu » (6 septembre 2026, soir) — deux catégories, repli, Projets fusionnés, Activité, Terminal

Validé par Valère avant réalisation : (1) bouton pour replier le menu en icônes seules ; (2) fusion de l'onglet Projets
dans Conteneurs ; (3) menu en deux catégories, **Docker** (Containers, Volumes, Images, Networks) et **General**
(Activity, Terminal, Settings) ; (4) anglais par défaut partout. Image du moteur **0.1.0-dev.15** (agent : commande
`Metrics`).

| Élément | Réalisation | Vérification (captures `.local/build/menu-*.png`, `u*.png`, `v*.png`) |
|---|---|---|
| Menu replié | `Ctrl+B` ou bouton en haut du menu ; 56 px, icônes seules, libellés et raccourcis en infobulle, état du moteur réduit au point ; mémorisé (`localStorage` `solon.sidebar`) | replié / déplié dans les deux sens, y compris depuis le terminal |
| Projets dans Conteneurs | plus d'onglet Projets ; en-tête d'un groupe Compose → écran du projet (services, journaux mêlés, Up / Down / Rebuild, Explorateur, VS Code) avec retour « ← Containers » | clic sur « COMPOSE PROJECT · ODOO18 → » : écran du projet `C:\Users\neveu\Desktop\odoo18\compose.yaml`, 2/2 running, journaux d'Odoo en direct |
| Deux catégories | ordre du menu = ordre des raccourcis `Ctrl+1` … `Ctrl+7` ; palette `Ctrl+K` alignée | palette : Containers Ctrl+1 … Settings Ctrl+7, Terminal Ctrl+` |
| Activity | agent : `Command::Metrics` (`/proc/stat`, `/proc/meminfo`, `/proc/loadavg`, `statvfs(/var/lib/solon)`, `/proc/net/dev` eth0) ; service `ServiceCommand::Metrics` ; app : relevé toutes les 2 s, pourcentages et débits par différence, courbes glissantes de 60 points ; conteneurs : flux `stats` existant, débits réseau par différence, tri par colonne | 22 processeurs, 457 MiB sur 1,9 GiB (2,0 GiB réservés), stockage 6,1 GiB sur 62 GiB (10 %), réseau eth0 ; 3 conteneurs avec CPU, mémoire / limite, débits, courbe |
| Terminal en section | `MachineTerminalPanel` monté à la première visite puis conservé masqué (la session survit aux changements de section) ; « New session » ; raccourcis de l'application prioritaires sur le shell (`attachCustomKeyEventHandler`) ; focus rendu quand la section est masquée | `echo terminal-ok` puis `Ctrl+B` (menu replié, pas de `^B` dans le shell), `Ctrl+1` puis `Ctrl+K` (palette ouverte) |
| Anglais par défaut | déjà le cas dans l'interface ; passés en anglais : pages 404 / 502 du mandataire `*.solon.local`, messages de l'installeur (`hooks.nsh`), journal `setup.log`, console et description du service | `setup.log` : « service started: Running » |

### Défauts trouvés en chemin

| Défaut | Cause | Correction |
|---|---|---|
| Après une mise à jour, **service arrêté, rien dans `setup.log`** | en traduisant `setup.ps1`, `"feature $feature: enabled"` : PowerShell lit `$feature:` comme un lecteur → **erreur d'analyse de tout le script**, silencieuse | `${feature}:` ; règle et vérification `Parser::ParseFile` ajoutées à CONTRIBUTING |
| `Ctrl+B` sans effet après usage du terminal | xterm garde le focus quand sa section est masquée et « consomme » Ctrl+B / Ctrl+K | raccourcis exclus du terminal ; `term.blur()` quand la section est masquée |
| Clic sur l'en-tête de groupe manqué dans les tests | le bouton n'a que la largeur du texte | sans changement (test corrigé) |

## Lot « OrbStack » A (7 septembre 2026) — shell de débogage et fiche de conteneur

Demande : « fait le 1, 2, 3, 10 » de la liste des fonctionnalités d'OrbStack pertinentes ; ce lot couvre le **2**
(shell de débogage dans n'importe quel conteneur) et le **10** (fiche complète et copie de fichiers). Image du moteur
**0.1.0-dev.17** (script `solon-debug` dans `/usr/local/bin`), en-tête du terminal machine étendu (`ShellHeader.command`).

| Élément | Réalisation | Vérification (captures `.local/build/w*.png`) |
|---|---|---|
| Shell de débogage | `solon-debug <conteneur>` : boîte à outils `solon-debug` (Alpine + bash, curl, wget, dig, ps, strace, tcpdump, jq, vim, nano, less, ss, lsof, htop ; 75 Mo, construite au premier usage) lancée avec `--pid`, `--network`, `--volumes-from` du conteneur cible et `SYS_PTRACE` ; démarre dans `/proc/1/root` (système de fichiers de la cible) avec l'invite `debug(nom):chemin #`. Onglet « Debug shell » de la fiche : terminal machine avec une commande au lieu du shell (`ShellHeader.command`, `sh -lc`). | conteneur `web` (busybox, sans bash) : `ps aux` montre `httpd` (PID 1 de la cible), `ls www` → `index.html`, `curl -s http://localhost/` → `hello from web container`, `ss -tlnp` → `httpd pid=1` sur :80 |
| Fiche (Overview) | onglet par défaut : général (ID, image, dates, commande, dossier, utilisateur, nom d'hôte, politique de redémarrage, santé, code de sortie), réseau par réseau (IP, passerelle, MAC, alias), ports avec lien, montages, environnement (copiable), étiquettes ; rafraîchie toutes les 5 s | capture `w3.png` |
| Copie de fichiers | `container_copy_from` (API `GET /archive` → tar → `tar.exe` de Windows extrait dans le dossier choisi) et `container_copy_to` (`tar.exe` crée l'archive → `PUT /archive`) ; boîtes de dialogue natives, message de résultat avec « Ouvrir le dossier » | `/www` → `.local\build\copytest\www\index.html` (25 octets, contenu correct) ; `README.md` → `/tmp/README.md` dans le conteneur (16 132 octets) |

### Défauts trouvés en chemin

| Défaut | Cause | Correction |
|---|---|---|
| `failed to join IPC namespace … non-shareable IPC` | `--ipc=container:X` exige que la cible ait été créée avec `--ipc=shareable`, ce qui n'est jamais le cas par défaut | partage de l'IPC retiré (processus, réseau et volumes suffisent) |
| invite `bash-5.3#` au lieu de `debug(web)` | bash **efface `PS1`** dans un shell non interactif, même exporté | invite écrite dans un fichier rc, `bash --rcfile … -i` |
| `ps` seul n'affichait pas la cible | comportement normal de `ps` (terminal courant) | aide-mémoire du shell : `ps aux` |

### Lot « OrbStack » B (7 septembre 2026) — onglet Fichiers (point 3, première étape validée : onglet d'abord, lecteur réseau ensuite)

| Élément | Réalisation | Vérification (captures `.local/build/f*.png`) |
|---|---|---|
| Parcours | `files_list` : commande `stat` de busybox exécutée dans la machine (`ServiceCommand::Exec`) sur le système de fichiers fusionné du conteneur (`GraphDriver.Data.MergedDir`, conteneur en marche) ou sur `/var/lib/docker/volumes/<nom>/_data` ; aucun shell requis dans l'image ; chemins normalisés (`..` refusé), noms cités pour le shell | racine du conteneur Odoo (dossiers, droits, dates), navigation dans `boot`, fil d'Ariane ; volume `odoo18_odoo-db` : données PostgreSQL, droits 700 |
| Créer, supprimer | `mkdir -p` / `rm -rf` dans la machine, racine protégée, confirmation avant suppression | dossier `aaa-solon-test` créé (755, visible dans la machine), puis supprimé après confirmation |
| Copier vers Windows | API `archive` : conteneur directement ; volume via un **conteneur auxiliaire jamais démarré** (image vide `solon-empty` importée une fois, 0 octet) qui monte le volume sur `/v`, supprimé après usage | dossier `base` du volume PostgreSQL → 896 fichiers, 23 Mo, aucun auxiliaire restant |
| Envoyer | boutons fichiers / dossier et **glisser-déposer** depuis l'Explorateur (`onDragDropEvent` de la fenêtre), même mécanique en sens inverse | envoi testé dans le lot A (README.md → /tmp) |
| Intégration | onglet **Files** de la fiche de conteneur ; section Volumes : bouton « Files » ou clic sur le nom → explorateur du volume avec retour | captures `f2`, `f3`, `f5`–`f8` |

Reste à faire de la demande : **1** (machines Linux complètes, proposition « conteneurs système » en attente de
validation) et la seconde étape du **3** (lecteur réseau `\solon\` dans l'Explorateur, si l'usage le justifie).

