# Solon — guide fonctionnel

*Version 0.2.0, 31 octobre 2026. Ce guide décrit ce que fait Solon du point de vue de la personne qui
l'utilise : l'icône de la barre des tâches, la commande `solon`, et ce que le moteur fait tout seul. Les
choix techniques sont dans `ARCHITECTURE.md`, les mesures dans `docs/measurements.md`. Jusqu'à la 0.1.18, ce
guide décrivait une application à fenêtre, écran par écran ; cette application a été retirée en 0.2.0 et le
§ 12 dit par quoi chaque écran est remplacé.*

## 1. Ce qu'est Solon, en une phrase

**Docker sur Windows, en silence.** Solon fait tourner vos projets Docker sur Windows, sans Docker Desktop
ni WSL, Windows Famille compris : un installateur, un moteur Docker complet dans une petite machine Linux
que Solon crée et gère lui-même, le vrai `docker` et le vrai `docker compose` dans votre terminal. Le moteur
démarre en quelques secondes, s'endort et rend la mémoire quand on ne s'en sert pas ; chaque projet reçoit
une adresse `https://….solon.local` qui marche toujours ; ce qui tournait avant un redémarrage retourne ;
les fichiers Windows montés dans les conteneurs sont rapides ; et quand ça casse, `solon why` dit pourquoi.

La promesse qui guide chaque choix : **tes projets tournent, tu ne t'occupes de rien.** Il n'y a pas de
fenêtre à ouvrir : une icône dans le coin de la barre des tâches pour voir l'état et relancer un projet en
deux clics, et une commande `solon` à côté de `docker` pour tout le reste.

## 2. Vocabulaire

| Mot | Ce que c'est |
|---|---|
| **Moteur** | Le Docker de Solon : une machine Linux légère lancée par le service Windows `SolonService`. Il démarre en quelques secondes, s'arrête proprement, redémarre seul ce qui tournait. |
| **Projet** | Un dossier Windows contenant un `compose.yaml` (ou `docker-compose.yml`). C'est l'unité de travail de Solon : une entrée dans le menu de l'icône, une ligne dans `solon projects`. |
| **Service** | Une ligne du fichier Compose (`web`, `db`…) ; en marche, c'est un conteneur. |
| **Adresse locale** | `https://<service>.<projet>.solon.local` pour un service Compose, `https://<nom>.solon.local` pour un conteneur isolé. Fournie par Solon, avec un certificat reconnu par le navigateur, port publié ou non. |
| **Endormi** | Un conteneur joint par Solon (adresse locale ou port publié) sans trafic depuis dix minutes est mis en pause ; la première requête le réveille en une fraction de seconde. Il compte comme en marche. |
| **Radiographie** | L'historique de ce que les services se disent : connexions entre conteneurs avec leur issue, échanges HTTP entrés par une adresse locale. Lu par `solon trace` et `solon why`. |

## 3. Installation et premier démarrage

1. **Installateur** `Solon_x.y.z_x64-setup.exe` (GitHub Releases, bientôt winget). Windows 10 22H2 ou 11, toutes
   éditions, Famille comprise ; virtualisation activée dans le BIOS. L'installateur active Hyper-V et la
   Plateforme de machine virtuelle si besoin et demande alors un redémarrage (sur Famille, la Plateforme de
   machine virtuelle seule suffit ; le repli 9P du partage de fichiers n'y existe pas). Il n'est pas encore
   signé : sur l'écran SmartScreen, « Informations complémentaires » puis « Exécuter quand même ».
2. **Service Windows** `SolonService` installé et démarré ; il gère le moteur. Le moteur démarre au premier
   lancement de Solon, puis à l'ouverture de session tant que « Démarrer avec Windows » reste coché.
3. **Rien à ouvrir.** L'icône de Solon apparaît dans le coin de la barre des tâches ; `docker` et `solon`
   fonctionnent dans tout nouveau terminal. Pour commencer : `cd` dans un dossier qui a un `compose.yaml`,
   `docker compose up -d`, et le projet apparaît dans le menu de l'icône avec son adresse. Pas de projet sous
   la main ? `solon scan` retrouve ceux qui sont déjà sur le PC, `solon import` ramène volumes et images
   depuis Docker Desktop.
4. **Mise à jour** : `solon update` (ou « Mettre à jour Solon… » dans le menu). Installer la nouvelle version
   par-dessus conserve images, conteneurs, volumes, réglages et historique ; la 0.2.0 par-dessus une 0.1.x
   aussi. **Désinstallation** : la question « supprimer aussi les données ? » **déplace** le dossier de
   données vers `%ProgramData%\Solon.removed-<date>` (rien n'est effacé) ; répondre Non le laisse en place
   pour une installation future.

## 4. L'icône de la barre des tâches

Un programme sans fenêtre, toujours `Solon.exe`. Son point est vert (moteur en marche), orange (démarrage,
arrêt, redémarrage), rouge (échec) ou gris (arrêté). Un clic, gauche ou droit, ouvre un menu natif :

- **Moteur : en marche / arrêté / en échec…** : cliquer ouvre un terminal, pratique quand c'est rouge.
- **N conteneurs en marche.**
- **Un sous-menu par projet Compose**, « ● blog (2/2) » (point coloré, services en marche sur services
  déclarés) : **Ouvrir https://web.blog.solon.local** (navigateur), **Démarrer le projet**, **Redémarrer le
  projet**, **Arrêter le projet**. Les conteneurs lancés hors projet (`docker run`) ont chacun leur sous-menu :
  Ouvrir, Démarrer, Redémarrer, Arrêter.
- **Ouvrir un terminal (solon, docker)** : une console dans votre dossier personnel, `solon` et `docker` dans
  le PATH.
- **Pourquoi ça casse ? (solon why)** : le même terminal, avec la réponse déjà affichée (§ 9).
- **Mettre à jour Solon…** : lance `solon update` dans un terminal (§ 6).
- **Démarrer avec Windows** : au premier lancement, Solon s'inscrit au démarrage de votre session (clé Run
  de votre compte) et une notification le signale : une icône absente quand le moteur tombe ne sert à rien.
  Décocher la case, c'est dire non ; ce non est retenu, Solon ne se réinscrit pas tout seul. La
  désinstallation retire l'entrée.
- **Démarrer le moteur** / **Arrêter le moteur** (celui qui a un sens selon l'état), **Quitter Solon** (l'icône
  disparaît ; le service et le moteur, eux, continuent).

Le menu est reconstruit à chaque changement d'état du moteur ou de la liste des conteneurs. Lancer
`Solon.exe` une seconde fois (raccourci, menu Démarrer, fin d'installation) ne fait que notifier « Solon
tourne déjà ». Libellés en anglais, ou en français si Windows l'est (`SOLON_LANG=fr` force).

**Notifications Windows**, rares par choix : moteur en échec (avec le code d'erreur), Docker qui redémarre,
disque du moteur presque plein, « c'est reparti » avec les projets relancés après le démarrage du moteur, et
une action du menu qui a échoué.

## 5. La commande `solon`

À côté de `docker`, l'installateur place `solon` dans `C:\Program Files\Solon\bin` : `docker` parle aux
conteneurs, `solon` parle au moteur qui les entoure. Elle passe par le pipe local du service, sans demande
d'administrateur et sans requête sortante (sauf `solon update`, § 6). Les messages sont en anglais, ou en
français quand Windows l'est (`SOLON_LANG=fr` force). `solon help` résume tout ; `--json` sur `status`,
`projects`, `domains`, `trace`, `settings`, `prereq`, `watch`, `scan`, `import` et `update` donne une sortie
lisible par un programme. Codes de sortie : 0, 1 erreur, 2 usage, 3 service injoignable.

### 5.1 Le moteur

| Commande | Ce qu'elle fait |
|---|---|
| `solon` ou `solon status` | État du moteur, image, adresse, domaines locaux, puis les conteneurs en marche avec leur projet, leur état (« endormi » si Solon l'a mis en veille) et leur adresse `https://service.projet.solon.local`. En cas d'échec, le code d'erreur et son message. |
| `solon start`, `solon stop [--force]`, `solon restart` | Démarrer ou arrêter le moteur ; chaque étape s'affiche (vérification des prérequis, nettoyage, vérification de l'image, préparation du disque, création du réseau, création du moteur, amorçage, attente du moteur, configuration du réseau, attente de Docker), puis « Moteur prêt. ». |
| `solon settings [clé=valeur…]` | Les réglages (§ 7) : seul, chaque réglage avec sa signification ; avec `clé=valeur`, les change. |
| `solon prereq` | Les prérequis Windows, ligne par ligne. |
| `solon watch [--json]` | Les événements du moteur au fil de l'eau. |
| `solon logs [-n N] [-f]` | Le journal du service. |
| `solon diag [fichier.zip]` | L'export de diagnostic : journaux, état, réglages, prérequis, `docker info`, versions ; aucun identifiant, aucune donnée de conteneur. À joindre à un rapport de bug. |
| `solon version` | Les versions de la commande, du service et de l'image. |
| `solon update [--check]` | La mise à jour (§ 6). |

### 5.2 Les projets

| Commande | Ce qu'elle fait |
|---|---|
| `solon projects` | Les projets Compose connus du moteur, avec leur état, leur dossier Windows et leurs adresses. |
| `solon domains` | Toutes les adresses `*.solon.local` qui répondent en ce moment (`service.projet` et nom de conteneur). |
| `solon scan` | Retrouver les projets Docker déjà sur ce PC : le service lit la table des fichiers des disques internes (quelques secondes, rien ne quitte le PC) et liste chaque dossier qui contient un `compose.yaml`, un `Dockerfile` ou un `devcontainer.json`, hors dossiers système, de dépendances et de cache, avec son type et s'il est sous Git. Ensuite : `cd <dossier> && docker compose up -d`. |
| `solon import [--all \| --volumes a,b --images x:tag]` | Copier depuis Docker Desktop, s'il est installé et démarré, sans rien y modifier ni supprimer. Sans option, liste ses volumes et ses images avec leur taille et ce qui existe déjà dans Solon ; avec les options, copie. Ce qui porte déjà le même nom dans Solon n'est jamais écrasé. Les conteneurs ne sont pas importés : ils se recréent depuis le fichier Compose. |
| `solon backup [dossier] [fichier.zip] [--force]` | **Un seul fichier zip** : les fichiers Compose, le `.env` et les données de chaque volume du projet (bases de données comprises), prises directement dans le moteur, projet en marche ou non. Nom par défaut `<projet>-AAAAMMJJ-HHMM.zip` ; `--force` écrase un zip existant. |
| `solon restore <fichier.zip> [dossier]` | Recrée les volumes et les fichiers dans le dossier choisi ; il reste `docker compose up -d`. Déplacer un projet vers un autre PC est une copie de fichier. Si le dossier a un autre nom, les volumes sont renommés en conséquence. |
| `solon clone <source> <cible>` | Copie les volumes d'un projet sous un autre nom, sans écraser ce qui existe. C'est l'environnement par branche Git : `solon clone blog blog-feature-login`, puis `docker compose -p blog-feature-login up -d` dans la branche ; chaque branche a ses conteneurs, ses volumes et ses adresses (`web.blog-feature-login.solon.local`). |

### 5.3 Regarder dedans

| Commande | Ce qu'elle fait |
|---|---|
| `solon why [nom]` | Ce qui ne va pas, en français courant, à partir de la Radiographie (§ 9). |
| `solon trace [nom] [-n N \| --all] [-f]` | Le journal des échanges, 50 dernières lignes par défaut ; `nom` filtre sur un projet, un service, un conteneur, un hôte ou une adresse ; `-f` suit le direct. |
| `solon clear [projet]` | Oublier l'historique observé, d'un projet ou de tout. |
| `solon shell` | Un shell root **dans le moteur Linux lui-même** (`docker`, `ps`, `df`, `dmesg`…), pour les curieux et le dépannage. Passe par `docker run -it` d'un conteneur busybox privilégié qui entre dans les espaces de noms du moteur ; `exit` pour sortir. |
| `solon exec <commande…>` | Une commande en root dans le moteur, sans session interactive ; un seul mot entre guillemets vaut une ligne shell. |

Pour tout ce qui touche aux conteneurs, images, volumes et réseaux, la commande est `docker` : `docker ps`,
`docker logs`, `docker compose logs -f`, `docker stats`, `docker cp`, `docker exec -it <nom> sh`, `docker
system prune`. Solon ne les double pas.

## 6. Mise à jour

Solon ne vérifie jamais seul s'il existe une nouvelle version : aucune requête ne part sans un geste de votre
part. `solon update --check` demande à GitHub la dernière version publiée et affiche ses notes ;
`solon update` (ou « Mettre à jour Solon… » dans le menu de l'icône) télécharge l'installateur, vérifie son
SHA-256 contre le `SHA256SUMS.txt` publié avec la version (un fichier qui ne correspond pas est effacé, jamais
lancé), puis Windows demande l'autorisation d'installer. Le moteur et les conteneurs s'arrêtent une minute,
Solon est remplacé, le service relance ce qui tournait. C'est la seule requête sortante de Solon. Un Solon
installé depuis le Microsoft Store répond « les mises à jour viennent du Store ».

## 7. Réglages

`solon settings` seul affiche chaque réglage, sa valeur et sa signification ; `solon settings clé=valeur …`
en change un ou plusieurs.

| Clé | Réglage |
|---|---|
| `memory` (Mo), `cpus` | Mémoire et processeurs du moteur (par défaut 2 Go et tous les cœurs moins deux). |
| `disk` (Gio) | Limite du disque de données (64 Go par défaut, occupé à la demande). |
| `autostart` | Démarrer le moteur à l'ouverture de session. |
| `resume` | Relancer ce qui tournait quand le moteur revient (§ 8). |
| `sleep`, `idle` (min), `never` (liste ou `none`) | Mise en veille des conteneurs inactifs, délai, conteneurs à ne jamais endormir. |
| `bodies` (liste ou `none`) | Projets dont la Radiographie garde les corps de requêtes et de réponses (§ 9). |
| `legacy-9p` | Repli sur l'ancien partage de fichiers Windows (9P) au prochain démarrage du moteur. |

Mémoire, processeurs, disque et partage de fichiers s'appliquent au prochain démarrage du moteur
(`solon restart`) ; les autres, tout de suite.

## 8. Ce qui se passe tout seul

- **Relance** : quand le moteur s'arrête (redémarrage de Windows, mise à jour de Solon, `solon stop`), Solon
  note les projets et conteneurs en marche et les relance une fois le moteur revenu, dans l'ordre des
  dépendances. Docker seul ne le fait que pour les conteneurs marqués `restart: always`. La notification
  « c'est reparti » liste ce qui a été relancé.
- **Adresses locales** : chaque conteneur en marche est joignable sur `https://<nom>.solon.local` (et
  `https://<service>.<projet>.solon.local` pour Compose), qu'il publie un port ou non. Solon lit le port que
  l'image expose (80, 8080, 3000, 8069…), tient à jour un bloc dans le fichier `hosts` de Windows, et route
  les requêtes depuis `127.0.0.1:80` et `127.0.0.1:443` d'après le nom demandé. Le HTTPS repose sur une
  autorité de certification locale « Solon Local CA », créée à la première utilisation et installée dans le
  magasin racine de la machine : le navigateur n'affiche aucun avertissement. Les en-têtes `X-Forwarded-*`
  sont transmis (WordPress et compagnie génèrent des liens `https://`). Les adresses des conteneurs
  (`10.90.x.y`) sont aussi joignables directement depuis Windows. Si un autre logiciel occupe le port 80 ou
  443, la partie correspondante est désactivée et les ports publiés restent utilisables. `curl.exe` de
  Windows refuse les certificats sans point de révocation : ajouter `--ssl-no-revoke` (comme avec mkcert).
- **Mise en veille** : un conteneur joint via Solon (adresse locale ou port publié) qui ne reçoit plus rien
  pendant dix minutes (réglable) est mis en pause : zéro processeur, mémoire conservée. La première requête
  suivante le réveille en une fraction de seconde, avant d'être servie. Les bases de données et les tâches
  de fond qui ne parlent qu'en interne ne sont jamais concernées ; `solon settings never=<nom>` exclut un
  conteneur ; `solon status` montre « endormi ».
- **Mémoire rendue** : la mémoire que le moteur n'utilise plus est rendue à Windows automatiquement.
- **Notifications Windows** : voir § 4.

## 9. Radiographie

Un débogueur de ce que vos services se disent, pas un outil de supervision : on ne cherche pas à savoir si
tout va bien, on cherche pourquoi un appel a échoué. Le service garde l'historique ; deux commandes le lisent.

**`solon trace [nom]`** est le journal, un échange par ligne. Une connexion TCP montre qui appelle qui, sur
quel port, avec son issue (connecté, refusé, fermé) et le programme à l'origine de l'appel. Un échange HTTP
— tout ce qui entre par une adresse `*.solon.local` — montre la méthode, le chemin, le code, la durée et la
taille. `-f` suit le direct, `--all` imprime tout, `--json` donne les faits bruts.

**`solon why [nom]`** dit en français courant ce qui ne va pas : les appels refusés regroupés par appelant,
destination et port, avec le nombre, la dernière heure, le programme à l'origine et un conseil (conteneur
endormi, rien n'écoute, adresse hors du moteur) ; les erreurs de serveur (5xx) par hôte ; les refus 401/403 ;
la requête la plus lente quand elle dépasse une seconde. Quand rien ne ressort, il le dit avec les comptes.
Depuis l'icône, « Pourquoi ça casse ? » ouvre un terminal avec cette réponse.

**Les corps** de requêtes et de réponses ne sont pas gardés par défaut. `solon settings bodies=blog` les
active pour un projet : Solon garde alors les 4 premiers Ko des corps, seulement pour du texte (JSON, HTML,
formulaires, XML), jamais pour une image ou une réponse compressée. `bodies=none` les coupe partout.

**Ce qui est masqué, avant d'être gardé** : les en-têtes d'authentification et de session (le nom reste, la
valeur devient `•••`), les paramètres d'URL, champs de formulaire et clés JSON dont le nom évoque un mot de
passe, un jeton ou une clé, les adresses de courriel et les numéros de téléphone. Ce qui n'a pas été capturé
ne peut pas fuir.

**Les adresses extérieures portent un nom** (`api.stripe.com` plutôt qu'un numéro) grâce à un relais DNS dans
le moteur, qui transmet les questions aux serveurs de votre PC et retient les réponses.

Tout reste sur votre PC. Rien n'est envoyé nulle part, rien n'est écrit en dehors de `%ProgramData%\Solon`.
`solon clear [projet]` oublie l'historique. Les appels de conteneur à conteneur montrent la connexion mais pas
encore son contenu. La carte de qui appelle qui et la barre de lecture (pause, pas à pas, relecture), qui
étaient dans la fenêtre, n'ont plus d'équivalent en 0.2.0 : elles reviendront dans une extension VS Code.

## 10. Le `docker` de Windows

L'installateur ajoute `docker` et `docker compose` au PATH (le CLI Docker officiel et le plugin Compose,
derrière un petit lanceur qui les dirige vers le moteur Solon ; `-H`, `--context`, `DOCKER_HOST` et
`DOCKER_CONTEXT` l'emportent). Les chemins Windows (`C:\…`) dans `-v`, `--mount` et les fichiers Compose
sont traduits et le lecteur partagé à la volée. Les dossiers Windows montés passent par le partage de
fichiers de Solon, bien plus rapide que le 9P de WSL ; pour les dépendances et les bases de données,
préférez tout de même des volumes. Quand le moteur est arrêté, `docker` le dit et explique comment le
démarrer.

## 11. VS Code Dev Containers

Un dossier avec `.devcontainer/devcontainer.json` s'ouvre dans VS Code par « Rouvrir dans un conteneur » :
l'extension se sert du `docker` de Solon sans rien régler. Le dossier est monté dans le conteneur dans les
deux sens, les ports déclarés sont transmis, le `postCreateCommand` s'exécute, et le conteneur apparaît dans
`solon status` comme les autres, avec son adresse locale. `solon scan` repère aussi ces dossiers.

La vue **Conteneurs** de VS Code (extension Containers) suit l'état des conteneurs en direct. Le pipe Docker
de Solon existe dès le lancement du service, et une commande `docker` lancée pendant que le moteur démarre
l'attend au lieu d'échouer : une fenêtre VS Code ouverte à l'ouverture de session garde le direct. Si Solon
est mis à jour pendant que VS Code est ouvert, cette fenêtre retombe sur un rafraîchissement par minute ;
*Developer: Reload Window* lui rend le direct.

## 12. Ce qui a disparu en 0.2.0, et par quoi c'est remplacé

L'application de bureau a été retirée le 31 octobre 2026. Les données (images, conteneurs, volumes, réglages,
historique de la Radiographie) ne bougent pas.

| Disparu | À la place |
|---|---|
| Écrans Conteneurs, Images, Volumes, Réseaux (listes, actions, journaux, inspection, onglet Fichiers, shell de débogage, lancement d'une image) | `docker ps`, `docker logs`, `docker inspect`, `docker cp`, `docker exec`, `docker run`, `docker images`, `docker volume`, `docker network` |
| Page Projets : cartes, adresse principale, éditeur de `compose.yaml` et de `.env` avec vérification des ports, journaux filtrés, panneau Fichiers, galerie de piles et modèles de projets, recherche sur le PC, import depuis Docker Desktop, environnement par branche Git avec détection du changement de branche, sauvegarde et restauration | `solon projects`, `solon scan`, `solon import`, `solon clone`, `solon backup` / `solon restore`, votre éditeur, `docker compose logs`, `docker cp` ; la galerie de piles n'a pas d'équivalent (le dossier `examples/` du dépôt garde des fichiers Compose prêts) |
| Terminal dans la machine | `solon shell` |
| Écran Activité (courbes processeur, mémoire, disque, réseau par conteneur) | `docker stats` ; pas d'équivalent graphique |
| Écran Radiographie (carte, journal dépliable, barre de lecture, case « Capturer les corps ») | `solon trace`, `solon why`, `solon settings bodies=…` ; la carte et la relecture visuelle reviendront dans une extension VS Code |
| Écran Réglages (dont « Récupérer l'espace » et l'export de diagnostic) | `solon settings`, `docker system prune`, `solon diag` |
| Bannière et page Mises à jour | `solon update`, « Mettre à jour Solon… » dans le menu |
| Palette de commandes Ctrl+K, raccourci Ctrl+Alt+S, raccourcis de sections, visite de bienvenue, thème clair/sombre, couleur d'accent, mémorisation de la fenêtre | disparus, sans remplacement |
| Notification « conteneur terminé avec une erreur » | disparue ; `docker ps -a` et `docker logs` |

## 13. Limites connues

- Pas d'interface graphique : ni fenêtre, ni courbes, ni carte. C'est un choix de la 0.2.0, pas un manque
  provisoire ; seule la partie visuelle de la Radiographie est prévue de revenir, dans VS Code.
- Ports UDP publiés non relayés vers `localhost` ; un seul moteur par PC ; sur Windows Famille, pas de repli 9P
  pour le partage de fichiers (le partage par défaut fonctionne).
- Installateur non signé (SmartScreen) ; signature via la SignPath Foundation demandée.
- Pas de Kubernetes, pas de machines Linux séparées, pas d'images ARM.
- Un conteneur endormi n'exécute pas ses tâches planifiées internes jusqu'à ce qu'on l'appelle
  (`solon settings never=<nom>` si cela compte).
- `curl.exe` de Windows demande `--ssl-no-revoke` pour les certificats locaux ; les navigateurs et .NET les
  acceptent.
- Le partage d'un projet à un collègue sur le réseau et la mise à jour des images ne sont pas encore là.

## 14. Où chercher de l'aide

- Page Dépannage du site : <https://v94lere.github.io/solon/fr/depannage/> (SmartScreen, moteur qui ne démarre
  pas, port pris, conteneur qui se termine aussitôt, réseau et adresses, disque plein, Docker Desktop ou WSL
  présents, codes d'erreur).
- Rapport de bug : `solon diag`, puis le formulaire GitHub avec le zip.
