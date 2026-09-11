// Textes du site dans les deux langues. Les chiffres viennent de docs/measurements.md (10 septembre 2026).
export type Lang = "en" | "fr";

export const REPO = "https://github.com/v94lere/solon";
export const RELEASES = `${REPO}/releases`;
export const LATEST = `${REPO}/releases/latest`;

const en = {
  nav: { features: "Features", compare: "Benchmark", stacks: "Stacks", releases: "Releases", github: "GitHub", download: "Download" },
  hero: {
    kicker: "Open source · Windows 10 & 11 Pro · Apache 2.0",
    title: "Docker on Windows, without Docker Desktop or WSL.",
    lead: "Solon is a standalone container manager: one installer, a complete Docker engine running in a tiny Linux machine that Solon creates and manages itself. No licence, no telemetry, no WSL.",
    download: "Download for Windows",
    winget: "winget coming soon",
    requirements: "Windows 11 or 10 22H2, Pro / Enterprise / Education, hardware virtualization enabled. Check the SHA-256 published with each release.",
    unsigned: "The installer is not signed yet: SmartScreen shows a warning, choose “More info” then “Run anyway”. Code signing through the SignPath Foundation (free signing for open-source projects) has been requested; releases will be signed as soon as the project is accepted.",
  },
  numbers: [
    { value: "≈ 3 s", label: "engine ready after you ask for it", note: "Docker Desktop: 6.1 s" },
    { value: "4×", label: "less memory at rest and with a stack running", note: "430–520 MB vs 1,998 MB" },
    { value: "0", label: "telemetry, outgoing requests, licence fees", note: "Apache 2.0" },
  ],
  clips: {
    title: "See it in action",
    items: [
      { title: "A stack in one click", body: "New stack… → n8n → Create and start. Twenty seconds later the browser opens on https://n8n.n8n.solon.local with a trusted certificate. No port to publish, nothing to install on Windows." },
      { title: "Look inside", body: "A container's page: logs, the Files tab to browse and exchange files with Windows, and the debug shell with ps, curl and friends, even in images that ship no shell." },
      { title: "Activity", body: "CPU, memory and network per container, hover for exact values, click a name to isolate it, 1 min / 10 min / 1 h windows." },
    ],
  },
  features: {
    title: "What Solon does",
    items: [
      { title: "Addresses that always work", body: "Every container answers at https://<name>.solon.local, published port or not, with a certificate your browser trusts. Compose services get <service>.<project>.solon.local." },
      { title: "Fast Windows folders", body: "Folders mounted into containers go through solonfs, Solon's own file sharing: 3.6 to 94× faster than the 9P sharing used by WSL2 and Docker Desktop." },
      { title: "Wake on demand", body: "A container that receives no traffic for ten minutes is paused; the next request wakes it in a fraction of a second. Databases that only talk internally are never touched." },
      { title: "Ready-made stacks", body: "WordPress, PostgreSQL, MariaDB, MongoDB, Redis, n8n, Nextcloud, Ghost, Gitea, Jupyter Lab, Mailpit… and starter kits for Django, Flask, FastAPI and Next.js that generate a project on first start." },
      { title: "Compose, all of it", body: "Open a folder, Solon detects the stack or proposes one. Services, merged logs, Up / Down / Rebuild, and the compose file editable in place with Save and Up." },
      { title: "See inside", body: "Files tab with drag-and-drop, terminal, debug shell that works even in images without a shell, live CPU, memory and network charts per container." },
    ],
  },
  compare: {
    title: "Measured against Docker Desktop",
    lead: "Same machine (Windows 11 Pro, 24 logical cores), same Compose stack (a Python web app and PostgreSQL 16), default settings on both sides. Docker Desktop 4.66.1 was measured on 3 September 2026, then uninstalled; Solon 0.1.0 was re-measured on 10 September 2026 with the script in bench/.",
    cols: ["", "Docker Desktop", "Solon"],
    rows: [
      ["Engine ready after start", "6.1 s", "2.6 to 3.4 s"],
      ["Memory at rest, no container", "1,998 MB", "430 to 520 MB"],
      ["Memory with the stack running", "5,146 MB", "1,196 MB"],
      ["compose down then up -d", "7.1 s", "5.2 s"],
      ["The web app answers after up", "1.5 s", "1.0 to 2.5 s"],
      ["Login page, average of 5", "27 to 46 ms", "48 to 88 ms"],
      ["Initialise the database with sample data (CPU)", "13.6 s", "12.9 to 13.4 s"],
      ["Synchronous writes on a volume, fdatasync / fsync", "154 / 79 ops/s", "240–290 / 132–137 ops/s"],
      ["Windows folder, 5,000 files: listing / attributes / reads / writes", "9P", "6.6× / 94× / 3.6–9× / 4× faster"],
    ],
    honest: "Honestly: Solon wins on what costs you every day (start-up, memory, files, disk writes); on pure CPU work the two are equivalent; web latency is the same. Reproduce it with bench/bench.ps1.",
  },
  stacks: {
    title: "Stacks and starter kits",
    lead: "“New stack…” opens a gallery. Pick one, choose a folder, review the generated compose.yaml, “Create and start”. Opening a folder without a Compose file makes Solon look at what it contains and propose an environment.",
    kits: "Starter kits",
    kitsBody: "The project folder is mounted in the container; on the first Up a small script installs the dependencies, generates the official skeleton if the folder is empty, then runs the development server with live reload. Nothing to install on Windows.",
    apps: "Ready-made apps",
    tools: "Developer tools",
  },
  releases: { title: "Releases", lead: "Every release ships the installer and its SHA-256. Installing over a previous version keeps your images, containers and volumes.", assets: "Files", view: "View on GitHub" },
  limits: {
    title: "Known limitations",
    items: [
      "Windows Home is not supported: it lacks a Hyper-V component Solon needs for file sharing.",
      "No Kubernetes, no separate Linux machines, no ARM images (x86-64 only).",
      "The installer is not signed yet; signing is the next step before a wider launch.",
      "Tested on the development machine and one fresh machine. Reports from other configurations are very welcome.",
    ],
  },
  footer: { made: "Made in Luxembourg. No telemetry, no outgoing request other than what your containers ask for.", licence: "Apache 2.0", source: "Source code", issues: "Report a problem", security: "Security" },
};

const fr: typeof en = {
  nav: { features: "Fonctions", compare: "Comparatif", stacks: "Piles", releases: "Versions", github: "GitHub", download: "Télécharger" },
  hero: {
    kicker: "Open source · Windows 10 et 11 Pro · Apache 2.0",
    title: "Docker sur Windows, sans Docker Desktop ni WSL.",
    lead: "Solon est un gestionnaire de conteneurs autonome : un installateur, un moteur Docker complet dans une petite machine Linux que Solon crée et gère lui-même. Pas de licence, pas de télémétrie, pas de WSL.",
    download: "Télécharger pour Windows",
    winget: "winget bientôt",
    requirements: "Windows 11 ou 10 22H2, éditions Pro, Entreprise ou Éducation, virtualisation matérielle activée. Vérifiez le SHA-256 publié avec chaque version.",
    unsigned: "L'installateur n'est pas encore signé : SmartScreen affiche un avertissement, choisissez « Informations complémentaires » puis « Exécuter quand même ». Une signature via la SignPath Foundation (signature gratuite pour les projets open source) a été demandée ; les versions seront signées dès l'acceptation du projet.",
  },
  numbers: [
    { value: "≈ 3 s", label: "moteur prêt après l'ordre de démarrage", note: "Docker Desktop : 6,1 s" },
    { value: "4×", label: "moins de mémoire, au repos comme avec une pile", note: "430–520 Mo contre 1 998 Mo" },
    { value: "0", label: "télémétrie, requête sortante, licence", note: "Apache 2.0" },
  ],
  clips: {
    title: "En action",
    items: [
      { title: "Une pile en un clic", body: "New stack… → n8n → Create and start. Vingt secondes plus tard, le navigateur s'ouvre sur https://n8n.n8n.solon.local avec un certificat reconnu. Aucun port à publier, rien à installer sur Windows." },
      { title: "Voir dedans", body: "La fiche d'un conteneur : journaux, onglet Fichiers pour parcourir et échanger des fichiers avec Windows, et le shell de débogage avec ps, curl et compagnie, même dans les images sans shell." },
      { title: "Activity", body: "CPU, mémoire et réseau par conteneur, survol pour les valeurs exactes, clic sur un nom pour l'isoler, fenêtres 1 min / 10 min / 1 h." },
    ],
  },
  features: {
    title: "Ce que Solon fait",
    items: [
      { title: "Des adresses qui marchent toujours", body: "Chaque conteneur répond sur https://<nom>.solon.local, port publié ou non, avec un certificat reconnu par votre navigateur. Les services Compose ont <service>.<projet>.solon.local." },
      { title: "Dossiers Windows rapides", body: "Les dossiers montés dans les conteneurs passent par solonfs, le partage de fichiers de Solon : 3,6 à 94× plus rapide que le 9P utilisé par WSL2 et Docker Desktop." },
      { title: "Réveil à la demande", body: "Un conteneur sans trafic pendant dix minutes est mis en pause ; la requête suivante le réveille en une fraction de seconde. Les bases qui ne parlent qu'en interne ne sont jamais touchées." },
      { title: "Piles prêtes", body: "WordPress, PostgreSQL, MariaDB, MongoDB, Redis, n8n, Nextcloud, Ghost, Gitea, Jupyter Lab, Mailpit… et des kits de démarrage Django, Flask, FastAPI et Next.js qui génèrent un projet au premier lancement." },
      { title: "Compose, en entier", body: "Ouvrez un dossier, Solon reconnaît la pile ou en propose une. Services, journaux mêlés, Up / Down / Rebuild, et le fichier Compose modifiable sur place avec Save and Up." },
      { title: "Voir dedans", body: "Onglet Fichiers avec glisser-déposer, terminal, shell de débogage même dans les images sans shell, courbes CPU, mémoire et réseau par conteneur." },
    ],
  },
  compare: {
    title: "Mesuré face à Docker Desktop",
    lead: "Même machine (Windows 11 Pro, 24 cœurs logiques), même pile Compose (une application web Python et PostgreSQL 16), réglages par défaut des deux côtés. Docker Desktop 4.66.1 mesuré le 3 septembre 2026, puis désinstallé ; Solon 0.1.0 remesuré le 10 septembre 2026 avec le script du dossier bench/.",
    cols: ["", "Docker Desktop", "Solon"],
    rows: [
      ["Moteur prêt après l'ordre de démarrage", "6,1 s", "2,6 à 3,4 s"],
      ["Mémoire au repos, aucun conteneur", "1 998 Mo", "430 à 520 Mo"],
      ["Mémoire avec la pile qui tourne", "5 146 Mo", "1 196 Mo"],
      ["compose down puis up -d", "7,1 s", "5,2 s"],
      ["L'application web répond après up", "1,5 s", "1,0 à 2,5 s"],
      ["Page de connexion, moyenne de 5", "27 à 46 ms", "48 à 88 ms"],
      ["Initialisation de la base avec données d'exemple (CPU)", "13,6 s", "12,9 à 13,4 s"],
      ["Écritures synchrones sur un volume, fdatasync / fsync", "154 / 79 ops/s", "240–290 / 132–137 ops/s"],
      ["Dossier Windows, 5 000 fichiers : listage / attributs / lectures / écritures", "9P", "6,6× / 94× / 3,6–9× / 4× plus rapide"],
    ],
    honest: "Honnêtement : Solon gagne sur ce qui coûte tous les jours (démarrage, mémoire, fichiers, écritures disque) ; sur le calcul pur les deux sont équivalents ; la latence web est identique. Reproductible avec bench/bench.ps1.",
  },
  stacks: {
    title: "Piles et kits de démarrage",
    lead: "« New stack… » ouvre une galerie. On choisit, on désigne un dossier, on relit le compose.yaml généré, « Créer et démarrer ». Ouvrir un dossier sans fichier Compose fait regarder à Solon ce qu'il contient pour proposer un environnement.",
    kits: "Kits de démarrage",
    kitsBody: "Le dossier du projet est monté dans le conteneur ; au premier Up, un petit script installe les dépendances, génère le squelette officiel si le dossier est vide, puis lance le serveur de développement avec rechargement à chaud. Rien à installer sur Windows.",
    apps: "Applications prêtes",
    tools: "Outils du développeur",
  },
  releases: { title: "Versions", lead: "Chaque version fournit l'installateur et son SHA-256. Installer par-dessus une version précédente conserve images, conteneurs et volumes.", assets: "Fichiers", view: "Voir sur GitHub" },
  limits: {
    title: "Limites connues",
    items: [
      "Windows Famille n'est pas pris en charge : il lui manque un composant Hyper-V nécessaire au partage de fichiers.",
      "Pas de Kubernetes, pas de machines Linux séparées, pas d'images ARM (x86-64 seulement).",
      "L'installateur n'est pas encore signé ; c'est la prochaine étape avant un lancement plus large.",
      "Testé sur la machine de développement et une machine vierge. Les retours d'autres configurations sont bienvenus.",
    ],
  },
  footer: { made: "Fait au Luxembourg. Aucune télémétrie, aucune requête sortante en dehors de ce que vos conteneurs demandent.", licence: "Apache 2.0", source: "Code source", issues: "Signaler un problème", security: "Sécurité" },
};

export const dict: Record<Lang, typeof en> = { en, fr };

/** Préfixe des liens internes pour une langue (l'anglais est à la racine). */
export function langPath(lang: Lang, path = ""): string {
  const clean = path.replace(/^\/+/, "");
  return lang === "en" ? `/${clean}` : `/fr/${clean}`;
}
