# Radiographie

Une carte en temps réel de ce que les services d'un projet se disent entre eux : qui appelle qui,
ce qui est demandé, ce qui est répondu, avec une barre de lecture que l'on met en pause, ralentit
et rembobine. L'axe est le **débogage des échanges**, pas la supervision : on ne cherche pas à
savoir si tout va bien, on cherche pourquoi un appel a échoué.

Tout reste sur la machine. Rien n'est envoyé nulle part, rien n'est écrit en dehors de
`%ProgramData%\Solon`.

Ce document suit les étapes. Une section par phase, écrite au moment où elle est livrée.

## En ligne de commande

Livré le 31 octobre 2026 avec la commande `solon` (voir `ARCHITECTURE.md`, § 8.1). Trois commandes lisent
le même historique que l'écran, par le même abonnement au service ; rien n'a changé côté service.

- `solon trace [nom] [-n N | --all] [-f] [--json]` : le journal, une ligne par échange, les 50 dernières
  par défaut. `nom` filtre sur un projet, un service, un conteneur, un hôte ou une adresse. `-f` continue
  en direct.

  ```text
  01:28:25  wordpress (wordpress) -> db (wordpress):3306  fermée après 48 ms  [apache2]
  01:28:25  GET https://wordpress.wordpress.solon.local/wp-login.php  -> 302 en 170 ms  (wordpress (wordpress))
  ```

- `solon why [nom]` : ce qui ne va pas, en langage courant. Les connexions refusées sont regroupées par
  source → destination : port, avec le nombre, la dernière heure, le programme appelant et un conseil ;
  puis les erreurs 5xx par hôte, les refus 401/403, la requête la plus lente. Sinon : « Rien d'anormal
  depuis HH:MM:SS : N connexions et M requêtes HTTP… ». Un cas réel, relevé le 31 octobre 2026 :

  > 10.90.5.4 a essayé 4 fois de joindre db (wordpress) sur le port 9999 sans jamais obtenir de réponse
  > (la dernière à 01:28:51, depuis le programme « wget »). Rien n'écoutait là. Le conteneur est-il
  > démarré, et écoute-t-il bien ce port ? `docker logs` dessus dit en général pourquoi.

- `solon clear [projet]` : oublier, un projet ou tout, comme le bouton « Effacer ».

Quelques choix qui ne vont pas de soi :

- **Le conseil dépend de ce qu'on sait de la cible.** Si l'appelé est un conteneur que le service a
  endormi, la phrase dit qu'un appel direct d'un autre conteneur ne le réveille pas, et donne les deux
  sorties (`docker unpause`, `solon settings never=…`). Si c'est un conteneur éveillé, rien n'écoutait sur
  ce port. Si ce n'est pas un conteneur du tout, le problème est hors du moteur : réseau, DNS, pare-feu, ou
  service distant arrêté. Trois phrases plutôt qu'une générique, parce que c'est précisément ce tri que
  l'on fait de tête devant l'écran.
- **Les cinq groupes d'échecs les plus fréquents, pas tous.** Une boucle de reconnexion produit des
  centaines de refus identiques ; ce qui compte est le couple source → destination, pas chaque tentative.
- **Un refus 401/403 n'est signalé que s'il touche au moins un quart des requêtes de l'hôte.** Quelques-uns
  sont normaux (page de connexion, session expirée) ; la moitié, c'est un jeton qui manque.
- **La lenteur n'est citée qu'au-dessus d'une seconde, et avec la médiane.** Une requête lente isolée au
  milieu de requêtes rapides désigne un chemin ; une médiane elle-même lente désigne le service.
- **L'historique est reconnu à son silence.** L'abonnement renvoie d'abord tout ce que le service garde,
  puis le direct ; `trace` et `why` lisent jusqu'à 600 ms sans rien recevoir et s'arrêtent là. Pas de
  marqueur de fin à ajouter au protocole, donc une application d'avant continue de fonctionner.
- **`why` n'a pas de `--json`.** Son résultat est un texte pour un humain ; un programme lira `trace --json`,
  qui donne les faits bruts, et fera son propre tri.

## Les corps, effacer par projet, et les noms de domaine

Les restes de la note de mission, livrés le 28 octobre 2026. Les deux premiers tiennent au même
mot : **par projet**.

### Les corps de requêtes et de réponses

Désactivés par défaut, activables projet par projet, depuis l'écran lui-même : on choisit un
projet dans le filtre, et une case « Capturer les corps » apparaît. Le réglage est enregistré
avec les autres (`trace_bodies` dans `settings.json`) et s'applique aux connexions suivantes
sans redémarrer le moteur. L'écran Réglages rappelle la liste des projets concernés, pour qu'on
ne l'oublie pas.

Ce qui est gardé : **les 4 premiers kilooctets** de chaque corps, et seulement quand le type de
contenu est du texte — JSON, HTML, formulaire, XML, texte brut. Une image, une archive, une police,
un corps compressé en `gzip` ne sont pas capturés du tout : ils n'apprendraient rien à l'écran, et
les décompresser n'est pas le travail d'un relais. Quatre kilooctets suffisent à lire un formulaire,
une réponse d'API ou une page d'erreur ; ils ne suffisent pas à retenir un téléchargement, et c'est
voulu.

Le masquage s'étend aux corps, toujours à la capture :

- un formulaire (`application/x-www-form-urlencoded`) suit la règle des paramètres d'URL — les
  champs dont le nom contient `pass`, `token`, `secret`, `key`… perdent leur valeur ;
- un document JSON perd la valeur de toute clé au nom suspect, à quelque profondeur qu'elle soit :
  `{"password":"•••"}`. Pas d'analyse JSON complète — un balayage des chaînes suffit, et il ne
  panique pas sur un document tronqué par la limite de capture ;
- tout le reste passe par le masquage du texte libre : courriels et téléphones disparaissent.

Comment le corps est attrapé sans ralentir le relais : les deux fonctions qui recopient un corps
(`Content-Length` ou morcelé) acceptent un « tee » facultatif, une copie bornée en mémoire de ce qui
passe. Pour un corps morcelé, seules les données sont gardées, pas l'enrobage des morceaux. Le
corps de requête pose une petite difficulté d'ordre : la requête entre dans la file d'appariement
**avant** d'être relayée (pour qu'une réponse rapide ne trouve jamais la file vide), donc avant
qu'on sache ce qu'elle contenait. Elle emporte un emplacement partagé, rempli une fois le corps
relayé, que la réponse vient lire. Si la réponse arrive avant la fin de l'envoi — `100 Continue`
refusé, erreur précoce — l'emplacement est vide et l'échange s'affiche sans corps de requête :
c'est rare, et c'est juste.

Quand le conteneur qui répond n'appartient à aucun projet Compose, rien n'est capturé : il n'y a
pas d'endroit où l'avoir demandé.

### Effacer par projet

Le bouton « Effacer » suit le filtre. Un projet choisi, et il devient « Effacer boutique » : le
service oublie les échanges qui se rattachent à ce projet et garde les autres. Un échange HTTP se
rattache au projet du conteneur qui a répondu ; une connexion, à celui de l'appelant, à défaut de
l'appelé. Sans filtre, le bouton efface tout, comme avant. Une application d'avant ce changement
envoie encore la commande sans projet : le service la comprend comme « tout effacer », un test le
garantit.

### Les noms de domaine

Une adresse extérieure s'affichait en chiffres : les conteneurs interrogeaient directement les
serveurs DNS de l'hôte Windows, personne dans Solon ne voyait passer les réponses. Depuis l'image
dev.22, l'agent tient un **relais DNS** sur l'adresse de la machine. Il transmet chaque question
aux serveurs de l'hôte, rend la réponse telle quelle, et note « cette adresse, c'est ce nom ». La
radiographie s'en sert pour nommer tout ce qui est hors de la machine : `api.stripe.com` au lieu
de `54.187.x.x`.

Pourquoi un relais et pas une sonde : une sonde sur le port 53 verrait la même chose, mais
demanderait un programme noyau de plus à maintenir, pour un trafic qui ne compte pas en
performance. Le relais tient en deux cents lignes de bibliothèque standard, sans dépendance.

Ce qui a été fait pour qu'il ne soit pas un point de défaillance :

- `resolv.conf` le cite **en premier, et les serveurs de l'hôte ensuite**. Si le relais ne répond
  pas, la bibliothèque de résolution passe au suivant au bout de deux secondes, comme avant.
- S'il ne peut pas démarrer (port pris), l'agent le dit dans son journal et écrit la configuration
  d'avant, telle quelle.
- Chaque question est servie dans son propre fil : un serveur amont lent ne retient pas les autres.
- Pas de cache, pas de filtrage, pas de réécriture. Rien ne part ailleurs que vers les serveurs que
  l'hôte utilisait déjà.

Les adresses sont rattachées au **nom demandé**, pas à la cible d'un éventuel `CNAME` : c'est
`api.stripe.com` que le développeur reconnaît, pas le nom du répartiteur derrière. La table garde
au plus 4 096 adresses ; au-delà, la moitié la plus ancienne s'en va d'un coup.

Une limite à connaître : Docker copie la liste des serveurs au démarrage de chaque conteneur. Les
conteneurs lancés **avant** cette image gardent la résolution directe tant qu'ils ne sont pas
recréés ; leurs adresses extérieures restent en chiffres.

### Vérifié

Sur l'installation réelle, le 28 octobre 2026 :

- un conteneur neuf reçoit `nameserver 172.30.0.2` puis le serveur de la box ; `example.com` se
  résout, se télécharge, et la connexion sortante s'affiche « → example.com » dans le journal, un
  appel vers un port fermé reste « refusé » ;
- capture des corps activée pour `wordpress`, un `POST /wp-login.php` envoyé avec un formulaire :
  la réponse `302` est appariée, le corps de requête est là, le courriel est masqué… et le mot de
  passe ne l'était pas. WordPress appelle son champ `pwd`, qui n'était pas dans la liste des noms
  suspects. Ajouté, avec `mdp` pour les formulaires français, et un test qui reprend la requête
  exacte. C'est précisément pour ce genre de trou que l'essai sur de vrais projets ne se saute pas.

## La barre de lecture

Ce qui fait de l'écran un débogueur plutôt qu'un journal : figer, reculer, rejouer au ralenti ou
en accéléré, avancer d'un échange à l'autre. Pendant la relecture, **tout l'écran** — carte comme
journal — montre la machine telle qu'elle était à l'instant du curseur. Rien n'est perdu pendant
ce temps : les échanges continuent d'arriver, simplement dans le « futur » du curseur, et le bouton
*Direct* y ramène.

Les commandes : reculer de 10 s, échange précédent et suivant (le pas à pas), lecture ou pause,
trois vitesses (×0,25, ×1, ×4). Au clavier, la barre prend Espace et les flèches. Un clic ou un
glissé sur la chronologie déplace le curseur.

Quelques choix qui ne vont pas de soi :

- **La chronologie est un histogramme**, pas un trait par échange. Une heure d'historique en
  compte des milliers, et ce qu'on y cherche d'un coup d'œil, ce sont les rafales et les échecs.
  Chaque barre prend la couleur du pire échange qu'elle contient — une erreur au milieu de cent
  succès doit rester visible — et la hauteur suit une échelle logarithmique, pour qu'une rafale de
  cinq cents appels n'écrase pas un appel isolé.
- **Les cercles gardent leur place.** Leur position est calculée sur tout l'historique, pas sur ce
  qui est déjà apparu à l'instant du curseur. En relecture, un service qui entre en scène prend la
  place qui l'attendait au lieu de bousculer les autres — sinon la carte se réorganiserait sous les
  yeux et on perdrait ses repères au moment précis où on cherche à comprendre.
- **Ce qui vient de se passer s'allume.** Un trait dont le dernier échange date de moins de deux
  secondes, en temps de la relecture, est épaissi et pleinement opaque. Au ralenti ×0,25, un appel
  brille donc huit vraies secondes : c'est ce qui rend le ralenti utile.
- **Rattraper le présent rend la main au direct.** À ×4, on finirait sinon par rejouer un futur
  qui n'existe pas encore. À ×1, le retard reste constant ; à ×0,25, il se creuse.
- **Le journal est trié par heure réelle**, pas par ordre d'arrivée. Un échange HTTP est daté de sa
  requête mais n'est connu qu'à sa réponse, donc après des connexions plus récentes que lui. Le
  pas à pas, lui, a besoin de l'ordre réel.

Tout se passe dans l'interface : le service gardait déjà l'historique horodaté, rien n'a changé
de son côté. L'interface garde désormais jusqu'à 5 000 échanges, au lieu de 1 000, pour que la
relecture remonte assez loin.

Vérifié sur l'installation réelle : figée 22 s en arrière, puis au ralenti ×0,25, le curseur
avance de 0,8 s en 3 vraies secondes et le retard sur le direct passe de 22 à 24 s.

## Phase 2a — ce qui est demandé, ce qui est répondu (0.1.16)

Les échanges HTTP complets : méthode, chemin, code, durée, en-têtes des deux côtés, taille des
corps. Dépliables ligne par ligne dans le journal.

### Gratuit, parce que le déchiffrement existait déjà

Solon termine déjà le TLS pour `*.solon.local`, avec son autorité locale, avant de relayer en
clair vers le conteneur. Tout ce qui entre par un domaine local est donc **déjà lisible à
l'endroit exact où il passe**, en `http` comme en `https`, sans sonde, sans eBPF et sans rien
déchiffrer de plus. C'est la source la plus riche du chantier et elle ne coûte presque rien.

Sa limite est nette, et elle est à dire : elle ne couvre que ce qui entre par un domaine
`*.solon.local`. Un appel de conteneur à conteneur n'y passe pas — la phase 2b s'en chargera, par
eBPF.

### Ce qu'il a fallu changer

Le mandataire recopiait les réponses à l'aveugle, d'un `tokio::io::copy` du conteneur vers le
navigateur. Pour savoir ce qui a répondu, il faut délimiter chaque réponse, donc lire son en-tête
avant de la relayer. Les règles ne sont pas celles d'une requête, et chacune est un piège :

- une réponse à `HEAD` annonce la taille qu'aurait un `GET` **sans envoyer de corps** — la lire
  comme un corps bloquerait la connexion jusqu'au délai d'attente ;
- `204`, `304` et les `1xx` n'ont jamais de corps ;
- `101` fait basculer la connexion en tunnel (WebSocket) : plus rien à analyser ensuite ;
- sans `Content-Length` ni `chunked`, le corps court jusqu'à la fermeture.

Chacune a son test. Le corps, lui, n'est jamais gardé : il est compté, pas lu.

L'appariement requête/réponse n'a besoin d'aucun identifiant : HTTP/1.1 sert les réponses dans
l'ordre des requêtes, une simple file suffit. La requête y entre **avant** d'être relayée, pour
qu'une réponse rapide ne trouve jamais la file vide et n'ait jamais à attendre.

### Le masquage

Il a lieu **à la capture**, jamais à l'affichage : ce qui n'est pas capturé ne peut pas fuir, même
si l'historique est relu ou exporté plus tard. `crates/solon-core/src/mask.rs`.

Sont masqués : les en-têtes d'authentification et de session (`Authorization`, `Cookie`,
`Set-Cookie`, `X-Api-Key`…), les paramètres d'URL dont le nom contient `pass`, `token`, `secret`,
`key`, `auth`, `session`, `sig`, `code`, `otp` ou `credential`, les adresses de courriel, et les
numéros de téléphone. Le **nom** de l'en-tête ou du paramètre reste visible : savoir qu'un
`Authorization` était présent fait partie du débogage.

La détection des numéros est volontairement étroite — un `+` suivi de 8 à 15 chiffres, ou un `0`
suivi de 9. Masquer toute longue suite de chiffres avalerait les horodatages, les tailles et les
identifiants, dont le débogage vit. Un test le vérifie explicitement.

Aucune expression rationnelle : les motifs sont simples, et les écrire à la main évite une
dépendance de plus dans une caisse que l'agent invité embarque aussi.

### Sur la carte

Les échanges HTTP ajoutent un nœud « Navigateur », d'où part tout ce qui entre par un domaine
local. Et quand deux projets ont un service du même nom — `db`, `web`, `cache`, ce qui est la
règle plutôt que l'exception — le nom du projet est ajouté, mais seulement aux cercles qui se
marchent dessus.

### Vérifié

Sur l'installation réelle, contre WordPress et Odoo : les redirections suivies (`303` puis `200`),
l'HTTPS aussi bien que l'HTTP, les durées et les tailles, et le masquage à l'œuvre —
`/pas-cette-page?token=•••&email=•••`, `Cookie` et `Set-Cookie` remplacés.

### Ce qui n'est pas encore là

Les corps. La note de mission les veut désactivés par défaut et activables par projet : le
réglage n'existe pas encore, donc rien n'est capturé. Les appels de conteneur à conteneur en
clair, et les protocoles de bases de données, viennent ensuite.

## Phase 1 — qui appelle qui

Les connexions TCP ouvertes depuis la machine, avec leur issue : établie, refusée, fermée. Une
carte des services et un journal des appels. L'événement « lancement de processus » de la phase 0
disparaît : il avait servi à vérifier la chaîne, il n'apprenait rien sur les échanges et coûtait du
bruit.

### Une seule sonde suffit

`sock/inet_sock_set_state`, la trace que le noyau émet à chaque changement d'état d'une socket
TCP. Elle donne les deux adresses, les deux ports, l'ancien et le nouvel état. Lire ses champs par
décalage — le noyau publie leur disposition dans
`/sys/kernel/tracing/events/sock/inet_sock_set_state/format`, c'est une interface stable — évite
d'avoir à connaître les structures internes du noyau, donc évite `bpftool`, `vmlinux.h` et toute
chaîne C.

Trois transitions sont retenues :

| De | Vers | Ce que ça veut dire |
|---|---|---|
| *(peu importe)* | `SYN_SENT` | un programme vient d'appeler : on met de côté qui il est |
| `SYN_SENT` | `ESTABLISHED` | l'appel a abouti |
| `SYN_SENT` | `CLOSE` | **l'appel a échoué** : refusé, filtré, ou abandonné sans réponse |
| *(peu importe)* | `CLOSE` | la connexion se termine, avec sa durée |

La troisième ligne est celle qui justifie le travail. Un appel refusé ne laisse aucune trace dans
les journaux quand la bibliothèque appelante avale l'erreur — c'est exactement le cas que l'on
cherche, et il apparaît ici en rouge sans rien avoir à instrumenter.

### Pourquoi l'appelant est mis de côté

Seule la transition vers `SYN_SENT` se produit dans le contexte du processus appelant. Les
suivantes arrivent en interruption logicielle, à la réception d'un paquet, où le processus courant
n'a plus rien à voir avec la connexion. Le cgroup et le nom du programme ne sont donc lisibles
qu'à l'ouverture : ils sont rangés dans une table indexée par l'adresse de la socket, et repris à
l'issue. La table est `LruHashMap` et non `HashMap` : une connexion dont on ne voit jamais l'issue
laisserait sinon une entrée pour toujours.

### Mettre un nom sur les deux bouts

Deux sources, parce qu'aucune ne suffit seule. Le numéro de cgroup désigne l'appelant avec
certitude, même quand son conteneur partage la pile réseau de la machine et n'a donc pas d'adresse
à lui. La liste des conteneurs en marche avec leur adresse, que l'agent tient déjà pour les
domaines locaux, est la seule façon de nommer l'appelé. Quand les deux se contredisent — une
adresse reprise par un conteneur qui vient de remplacer un autre — c'est le cgroup qui l'emporte.

Les connexions **entrantes** ne sont pas rapportées. Entre deux conteneurs, elles sont l'autre
extrémité d'une sortante déjà vue : les compter ferait double. Ce qui vient de l'extérieur de la
machine — un navigateur — n'est pas vu du tout ici, et c'est le mandataire de Solon qui le
rapportera, avec bien plus de détail.

### Ce que ça coûte

Mesuré sur la machine de développement, 20 000 tentatives de connexion vers un port fermé de la
boucle locale — le cas le plus défavorable, puisque c'est l'opération réseau la moins chère qui
soit, et que chacune produit un fait :

| | par connexion |
|---|---|
| sans sonde | 4,8 à 6,5 µs |
| avec sonde | 9,7 µs |

Soit **environ 4 µs ajoutées par connexion**. Le rapport impressionne, l'absolu beaucoup moins :
une requête HTTP réelle coûte des centaines de microsecondes à plusieurs millisecondes, et un
projet très actif qui ouvrirait mille connexions par seconde dépenserait 4 ms par seconde, soit
0,4 % d'un cœur. Le lecteur dans la machine est resté sous 4 Mo. Le budget de la note de mission —
moins de 3 % de processeur, moins de 50 Mo — est tenu avec de la marge.

### Vérifié

Sur le noyau d'une machine Solon en marche, dans un conteneur privilégié, sans toucher à
l'installation : un appel vers l'extérieur ressort en « établie » puis « fermée » avec sa durée
(63 ms), un appel vers un port fermé ressort en « échouée », et le cgroup de l'appelant est celui
du conteneur qui a lancé la commande.

Un défaut réel attrapé au passage : l'objet BPF doit être embarqué avec `include_bytes_aligned` et
non `include_bytes`. Sa lecture se fait sans copie et exige un tampon aligné ; avec un alignement
d'un octet, le chargement passe ou échoue selon la taille de l'objet. La phase 0 fonctionnait par
chance, la phase 1 a échoué d'un coup — une panne qui serait apparue à la première sonde ajoutée,
loin de sa cause.

### Ce qui n'est pas encore là

Le contenu des échanges. Les noms de domaine : une adresse extérieure s'affiche en chiffres tant
que les requêtes DNS ne sont pas observées. Et la barre de lecture — pour l'instant, une pause et
un filtre par projet.

## Phase 0 — la plomberie

L'objectif de cette phase n'était pas de montrer des échanges, mais de prouver que la chaîne
complète tient : un programme dans le noyau de la machine → l'agent → le service Windows →
l'écran. Un seul type de fait est observé, le lancement d'un processus. C'est volontairement le
plus bête possible : il se déclenche souvent, il se vérifie à l'œil, et il n'engage aucun choix
sur la suite.

### Ce qui a décidé de la forme

**Le noyau était déjà prêt.** `6.18.40.1-solon` est compilé avec `BPF`, `BPF_SYSCALL`, `BPF_JIT`,
`BPF_EVENTS`, `CGROUP_BPF`, `KPROBES`, `UPROBES`, `PERF_EVENTS`, et surtout `DEBUG_INFO_BTF` avec
`/sys/kernel/btf/vmlinux` présent. Aucune recompilation de noyau n'a été nécessaire, et le CO-RE
est possible pour les phases suivantes. C'était le risque le plus lourd du projet ; il n'existait
pas.

**bpf-linker ne se construit pas sous Windows.** Son script de construction appelle
`std::os::unix::ffi::OsStrExt` ; la compilation échoue avant même de chercher LLVM. C'est la seule
pièce de Solon qui exige un hôte Linux. Elle est donc fabriquée **dans un conteneur Solon**, comme
l'image Linux l'est déjà : `crates/solon-trace-ebpf/build.ps1` construit l'atelier décrit par le
`Dockerfile` voisin et produit l'objet. Aucune chaîne C n'apparaît sur la machine de
développement, et la propriété « tout se compile depuis Windows » n'est entamée que pour 1,9 Ko
d'objet BPF.

**Le couple Rust / LLVM ne se choisit pas librement.** bpf-linker relit le bitcode produit par
rustc : sa version de LLVM doit être au moins aussi récente. Or apt.llvm.org ne publie pas de
`llvm-23` (vérifié sur bookworm, trixie, noble et plucky : 20, 22 et 24 seulement), et bpf-linker
0.12 ne connaît pas encore LLVM 24. La chaîne est donc figée sur `nightly-2026-07-01`, qui utilise
LLVM 22.1.8. La relever demandera d'attendre que bpf-linker gère LLVM 24.

**L'objet BPF est versionné dans le dépôt.** La compilation croisée de l'agent depuis Windows ne
peut pas le produire, et l'agent l'embarque par `include_bytes!`. Un binaire commité dans un dépôt
ouvert se vérifie : le travail `ebpf` de la CI le reconstruit à chaque fois et refuse la
modification s'il diffère de ses sources.

**Le stockage vit sur l'hôte**, dans le service, pas dans la machine. L'invité reste sans état, le
disque de données n'est pas pollué par de l'observation, et l'interface parle déjà au service.
Pour l'instant un simple anneau de 20 000 faits en mémoire ; une base sur disque n'aura de sens
qu'avec du volume à garder entre deux démarrages, c'est-à-dire avec les sondes réseau.

**Un port vsock dédié, 5007.** Le flux des faits observés est bien plus bavard que les événements
du moteur : mêlés sur le même canal, une rafale d'échanges retarderait un changement d'état, et un
abonné en retard ferait perdre les deux. Partout où le choix se pose — tampon du noyau plein,
abonné lent — ce sont les faits observés qui sont abandonnés. Une sonde ne doit jamais ralentir ce
qu'elle regarde.

### Le chemin d'un fait

1. `crates/solon-trace-ebpf` — le programme chargé dans le noyau. Attaché au tracepoint
   `sched/sched_process_exec`, il écrit un `RawEvent` de 40 octets dans un tampon circulaire de
   256 Kio.
2. `crates/solon-trace-common` — la disposition de ce `RawEvent`, partagée entre le programme du
   noyau et l'agent, qui sont compilés séparément. Un test fige sa taille : si elle change d'un
   côté sans l'autre, l'agent lit n'importe quoi.
3. `crates/solon-agent/src/trace.rs` — charge l'objet avec aya (Rust pur, pas de libbpf, donc pas
   de chaîne C dans la compilation croisée), monte `tracefs` à la demande, lit le tampon, traduit
   l'horloge du noyau en temps réel et le numéro de cgroup en identifiant de conteneur, puis sert
   le tout sur vsock 5007.
4. `crates/solon-service/src/trace.rs` — garde l'historique et le diffuse aux abonnés.
5. `apps/desktop/src/views/RadiographyView.tsx` — l'écran, avec pause et effacement.

La correspondance cgroup → conteneur passe par le numéro d'inode : `bpf_get_current_cgroup_id`
renvoie l'inode du dossier de cgroup, et dockerd place chaque conteneur dans
`/sys/fs/cgroup/docker/<id>`. L'agent parcourt l'arbre et relève les inodes, en relisant au plus
toutes les deux secondes quand un identifiant inconnu apparaît.

### Vérifié

Le code de chargement a été exécuté dans un conteneur privilégié, sur le noyau d'une machine Solon
en marche, sans toucher à l'installation : le vérificateur accepte le programme, `tracefs` se
monte, les faits remontent, et les numéros de cgroup correspondent bien aux dossiers des
conteneurs (`94` → `041feeec…`, `238` → `f635cab5…`).

### Connu d'avance pour la suite

- **Le HTTPS sortant.** Les uprobes sur `SSL_read` / `SSL_write` couvrent ce qui utilise OpenSSL —
  Python, Node, PHP, Ruby, nginx — mais **ni Go ni Java**, qui chiffrent sans. La limite sera
  documentée plutôt que contournée.
- **Le HTTPS entrant est déjà résolu**, et gratuitement : le mandataire de Solon termine déjà le
  TLS pour `*.solon.local` avec son autorité locale. Tout le trafic navigateur → conteneur est
  donc lisible en clair là où il passe, sans sonde. C'est la source la plus riche du lot et elle
  ne coûte rien.
