// Textes du site dans les deux langues. Les chiffres viennent de docs/measurements.md (10 septembre 2026).
export type Lang = "en" | "fr";

export const REPO = "https://github.com/v94lere/solon";
export const RELEASES = `${REPO}/releases`;
export const LATEST = `${REPO}/releases/latest`;

const en = {
  nav: { features: "Features", compare: "Benchmark", stacks: "Stacks", releases: "Releases", help: "Help", github: "GitHub", download: "Download" },
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
    { value: "0", label: "telemetry, accounts, licence fees", note: "Apache 2.0" },
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
      "No automatic update: Solon tells you when a new version exists (Settings → Updates, can be turned off); you download and install it over the old one.",
      "Tested on the development machine and one fresh machine. Reports from other configurations are very welcome.",
    ],
  },
  help: {
    kicker: "Help",
    title: "Troubleshooting",
    lead: "The problems people actually hit with Solon, in the order they meet them, and what to do. Each takes a minute. If yours is not here, the diagnostic export at the bottom is the fastest way to get it fixed.",
    tocLabel: "On this page",
    back: "← Back to the home page",
    problems: [
      {
        title: "Windows blocks the installer (SmartScreen)",
        symptom: "A blue “Windows protected your PC” screen appears when you run Solon_x.y.z_x64-setup.exe.",
        steps: [
          "Click <b>More info</b>, then <b>Run anyway</b>. The screen means the installer is not code-signed yet, not that it is harmful.",
          "If you want to be sure of what you run: compare the file's SHA-256 with the one published next to the release (<code>Get-FileHash .\\Solon_x.y.z_x64-setup.exe</code> in PowerShell). The home page shows it too.",
          "Some antivirus products quarantine unsigned installers: restore the file and add an exception for it before running it again.",
        ],
        note: "Code signing through the SignPath Foundation has been requested; releases will be signed as soon as the project is accepted, and this screen will disappear.",
      },
      {
        title: "The engine does not start after installation",
        symptom: "Solon shows “Engine failed to start” with a code such as WINDOWS_FEATURE_MISSING, HYPERVISOR_NOT_RUNNING or VIRTUALIZATION_DISABLED_IN_FIRMWARE.",
        steps: [
          "<b>Reboot first.</b> The installer enables Hyper-V and the Virtual Machine Platform when they were off; they only work after a restart. Most first-start failures end here.",
          "If the code is <code>VIRTUALIZATION_DISABLED_IN_FIRMWARE</code>: enable hardware virtualization in the BIOS/UEFI (called Intel VT-x, AMD-V or SVM, usually under Advanced, CPU or Security), save, reboot.",
          "If the code is <code>UNSUPPORTED_WINDOWS_EDITION</code>: you are on Windows Home, which lacks a Hyper-V component Solon needs. Solon needs Windows 10 22H2 or 11 in Pro, Enterprise or Education.",
          "If the code is <code>HYPERVISOR_NOT_RUNNING</code>: an old VirtualBox or VMware, or a <code>bcdedit</code> setting, turned the Windows hypervisor off. Remove or update them (VirtualBox ≥ 6.1, VMware ≥ 15.5), then in an administrator PowerShell: <code>bcdedit /set hypervisorlaunchtype auto</code> and reboot.",
          "Still stuck: Settings → Diagnostic → Export a diagnostic, and open a bug report with the zip (see below).",
        ],
        note: null,
      },
      {
        title: "A stack refuses to start: a port is already in use",
        symptom: "Up fails and the output says “port is already allocated” or “address already in use”, or Solon says “Port 8080 is already in use on this PC”.",
        steps: [
          "Another program (another stack, IIS, a dev server, Docker Desktop) already listens on that port. Solon names the port and proposes the next free one: click <b>Use 8081 instead</b> in the gallery, or in the project's <b>Compose</b> tab change the host port (the left number in <code>\"8080:80\"</code>) and click <b>Save and Up</b>.",
          "You may not need a published port at all: every container answers at <code>https://&lt;name&gt;.solon.local</code>, port or not. Comment the <code>ports:</code> lines and use that address.",
          "Two copies of the same stack (two WordPress, two Odoo) always collide on the same port: give the second one another host port, or drop the ports and use the two solon.local addresses.",
        ],
        note: null,
      },
      {
        title: "A container “does nothing” or stops at once",
        symptom: "You click Run or Start, the container appears for a second and goes to Exited; hello-world is the classic case.",
        steps: [
          "That is normal for a program that does its job and quits: hello-world prints its message and exits in a tenth of a second. Solon opens its logs and shows “exited immediately with code 0”; the message is in the Logs tab.",
          "A server that exits with a non-zero code is a real failure: read the last lines of its logs (Logs tab, or <code>docker logs &lt;name&gt;</code>). The usual causes are a missing environment variable, a wrong volume path or a port the container itself cannot bind.",
          "A container that Solon put to sleep (moon icon) is not stopped: the next request wakes it. Use <b>Keep awake</b> on its page if it runs scheduled tasks.",
        ],
        note: null,
      },
      {
        title: "No network in the containers, or a solon.local address does not open",
        symptom: "apt or pip time out inside a container, or the browser cannot reach https://name.solon.local.",
        steps: [
          "<b>Corporate VPN.</b> Some VPN clients (AnyConnect, GlobalProtect, Zscaler) block traffic to virtual adapters. Disconnect the VPN and try again; if it works, add it to your bug report so a workaround can be found.",
          "<b>The address.</b> A container has an address only while it runs; the name is the container's (<code>web.solon.local</code>) or <code>service.project.solon.local</code> for Compose. Open <code>http://anything.solon.local</code>: the page lists every address currently available.",
          "<b>The certificate.</b> Browsers and .NET trust the local certificates. <code>curl.exe</code> on Windows needs <code>--ssl-no-revoke</code>, like with mkcert.",
          "<b>Links that point to http://.</b> WordPress, Nextcloud and friends generate links from the request: Solon passes <code>X-Forwarded-Proto: https</code>, which the official images honour. For an existing WordPress installed over http, update the site URL in Settings → General.",
          "<b>The hosts file.</b> Solon writes the names into <code>C:\\Windows\\System32\\drivers\\etc\\hosts</code> between <code># solon-begin</code> and <code># solon-end</code>. A security product that protects that file blocks the addresses: allow Solon or add the names by hand.",
        ],
        note: null,
      },
      {
        title: "The disk fills up",
        symptom: "Windows warns that drive C: is almost full, or Solon shows “Engine disk almost full”.",
        steps: [
          "Solon keeps everything in one file, <code>%ProgramData%\\Solon\\data.vhdx</code>, which grows with images, containers and volumes and only shrinks after a clean-up.",
          "Settings → <b>Disk</b> → <b>Reclaim space</b>: removes the images no container uses and the build cache, then gives the freed space back to Windows. Containers and volumes are never touched.",
          "Still large? Images → “Unused only” shows what remains; Volumes → “Unused only” shows volumes no container references (data you may want to keep: check before removing).",
        ],
        note: null,
      },
      {
        title: "Docker Desktop or WSL is also installed",
        symptom: "docker commands talk to the wrong engine, or Docker Hub refuses your login.",
        steps: [
          "Solon and Docker Desktop can coexist. Which engine the <code>docker</code> command talks to depends on which <code>docker.exe</code> comes first in your PATH; Solon's is in its installation folder. <code>docker context ls</code> shows the current one.",
          "Docker Hub “unauthorized” errors: the Windows docker CLI reuses credentials stored by Docker Desktop, which may be stale. <code>docker logout</code> then <code>docker login</code> fixes it.",
          "WSL is not used by Solon and does not need to be installed or removed.",
        ],
        note: null,
      },
    ],
    report: {
      title: "Report a problem",
      lead: "One zip file tells more than a screenshot. It takes ten seconds.",
      steps: [
        "In Solon: <b>Settings → Diagnostic → Export a diagnostic…</b>, save the zip.",
        "Open a bug report on GitHub and drop the zip into the form, with what you did and what you expected.",
        "Mention your Windows version (<code>winver</code>), whether a VPN, another antivirus, Docker Desktop or WSL are present.",
      ],
      privacy: "The zip contains Solon's logs, its state and settings, the prerequisites report, docker info and the versions. It does not contain your containers' data, passwords or Docker Hub credentials; you can open it and check before sending.",
      button: "Open a bug report",
    },
    codes: {
      title: "Error codes",
      lead: "Every error shown by Solon carries a stable code. Logs are in %ProgramData%\\Solon\\logs and can be copied from the error screen.",
      cols: ["Code", "Cause", "What to do"],
      power: "Power loss or hard shutdown: at the next start Solon checks and repairs the data disk (<code>fsck</code>), then restarts the engine. Unsynced writes of the last two seconds may be lost, as on any Linux machine.",
      rows: [
        ["VIRTUALIZATION_DISABLED_IN_FIRMWARE", "VT-x / AMD-V disabled", "Enable virtualization in the BIOS/UEFI (Advanced, CPU or Security tab), reboot."],
        ["UNSUPPORTED_WINDOWS_EDITION", "Windows Home", "Move to Windows Pro, Enterprise or Education."],
        ["WINDOWS_FEATURE_MISSING", "Hyper-V or Virtual Machine Platform disabled", "Reboot if you just installed. Otherwise reinstall Solon (the installer enables them) or, in an administrator PowerShell: <code>Enable-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V,VirtualMachinePlatform -All</code>, then reboot."],
        ["WINDOWS_FEATURE_BLOCKED_BY_POLICY", "Company policy (WSUS, GPO) refuses the feature", "Ask your administrator to enable Microsoft-Hyper-V and VirtualMachinePlatform."],
        ["HYPERVISOR_NOT_RUNNING", "Windows hypervisor not started", "Uninstall old VirtualBox/VMware (< 6.1 / < 15.5), check <code>bcdedit /enum</code> (hypervisorlaunchtype Auto), do not run Solon in a VM without nested virtualization."],
        ["HOST_COMPUTE_SERVICE_UNAVAILABLE", "vmcompute or hns service stopped or missing", "Reboot Windows; otherwise reinstall Solon."],
        ["BLOCKED_BY_SECURITY_SOFTWARE", "Antivirus / EDR blocks the disks or the service", "Add %ProgramData%\\Solon and the installation folder to the exclusions."],
        ["INSUFFICIENT_PRIVILEGES", "The service does not run with the expected rights", "Reinstall Solon (the service must run as LocalSystem)."],
        ["IMAGE_CORRUPTED", "Engine files missing or SHA-256 mismatch", "Reinstall Solon."],
        ["DATA_DISK_ERROR", "data.vhdx cannot be created or opened", "Check disk space and antivirus exclusions; as a last resort rename %ProgramData%\\Solon\\data.vhdx (loses Docker data)."],
        ["VM_BOOT_TIMEOUT, AGENT_UNREACHABLE, ENGINE_UNREACHABLE", "The machine does not answer", "Restart the engine; read solon-service.log; report with the diagnostic zip."],
        ["No network from containers", "Corporate VPN or IP range conflict", "Solon picks a free range and sets the MTU to 1400; some VPNs still block virtual adapters: disable the VPN to test, then report."],
        ["“The Solon service is not running”", "Service stopped", "<code>sc start SolonService</code> as administrator, or reinstall."],
      ],
    },
  },
  footer: { made: "Made in Luxembourg. No telemetry; the only outgoing request is an optional check for new versions, which you can turn off.", licence: "Apache 2.0", source: "Source code", issues: "Report a problem", security: "Security" },
};

const fr: typeof en = {
  nav: { features: "Fonctions", compare: "Comparatif", stacks: "Piles", releases: "Versions", help: "Aide", github: "GitHub", download: "Télécharger" },
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
    { value: "0", label: "télémétrie, compte, licence", note: "Apache 2.0" },
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
      "Pas de mise à jour automatique : Solon signale qu'une nouvelle version existe (Réglages → Mises à jour, désactivable) ; on la télécharge et on l'installe par-dessus.",
      "Testé sur la machine de développement et une machine vierge. Les retours d'autres configurations sont bienvenus.",
    ],
  },
  help: {
    kicker: "Aide",
    title: "Dépannage",
    lead: "Les problèmes que l'on rencontre vraiment avec Solon, dans l'ordre où on les croise, et que faire. Chacun prend une minute. Si le vôtre n'y est pas, l'export de diagnostic en bas de page est le chemin le plus court vers une correction.",
    tocLabel: "Sur cette page",
    back: "← Retour à l'accueil",
    problems: [
      {
        title: "Windows bloque l'installateur (SmartScreen)",
        symptom: "Un écran bleu « Windows a protégé votre ordinateur » apparaît au lancement de Solon_x.y.z_x64-setup.exe.",
        steps: [
          "Cliquez sur <b>Informations complémentaires</b>, puis <b>Exécuter quand même</b>. L'écran signifie que l'installateur n'est pas encore signé, pas qu'il est dangereux.",
          "Pour être sûr de ce que vous lancez : comparez le SHA-256 du fichier avec celui publié à côté de la version (<code>Get-FileHash .\\Solon_x.y.z_x64-setup.exe</code> dans PowerShell). La page d'accueil l'affiche aussi.",
          "Certains antivirus mettent en quarantaine les installateurs non signés : restaurez le fichier et ajoutez une exception avant de le relancer.",
        ],
        note: "Une signature via la SignPath Foundation a été demandée ; les versions seront signées dès l'acceptation du projet, et cet écran disparaîtra.",
      },
      {
        title: "Le moteur ne démarre pas après l'installation",
        symptom: "Solon affiche « Le moteur n'a pas démarré » avec un code comme WINDOWS_FEATURE_MISSING, HYPERVISOR_NOT_RUNNING ou VIRTUALIZATION_DISABLED_IN_FIRMWARE.",
        steps: [
          "<b>Redémarrez d'abord.</b> L'installateur active Hyper-V et la Plateforme de machine virtuelle s'ils étaient éteints ; ils ne fonctionnent qu'après un redémarrage. La plupart des échecs du premier démarrage s'arrêtent là.",
          "Code <code>VIRTUALIZATION_DISABLED_IN_FIRMWARE</code> : activez la virtualisation matérielle dans le BIOS/UEFI (Intel VT-x, AMD-V ou SVM, en général sous Advanced, CPU ou Security), enregistrez, redémarrez.",
          "Code <code>UNSUPPORTED_WINDOWS_EDITION</code> : vous êtes sur Windows Famille, à qui manque un composant Hyper-V nécessaire à Solon. Il faut Windows 10 22H2 ou 11 en Pro, Entreprise ou Éducation.",
          "Code <code>HYPERVISOR_NOT_RUNNING</code> : un vieux VirtualBox ou VMware, ou un réglage <code>bcdedit</code>, a éteint l'hyperviseur Windows. Retirez-les ou mettez-les à jour (VirtualBox ≥ 6.1, VMware ≥ 15.5), puis en PowerShell administrateur : <code>bcdedit /set hypervisorlaunchtype auto</code> et redémarrez.",
          "Toujours bloqué : Réglages → Diagnostic → Exporter un diagnostic, et ouvrez un rapport avec le zip (voir plus bas).",
        ],
        note: null,
      },
      {
        title: "Une pile refuse de démarrer : un port est déjà pris",
        symptom: "Up échoue et la sortie dit « port is already allocated » ou « address already in use », ou Solon affiche « Le port 8080 est déjà utilisé sur ce PC ».",
        steps: [
          "Un autre programme (une autre pile, IIS, un serveur de développement, Docker Desktop) écoute déjà sur ce port. Solon nomme le port et propose le suivant libre : cliquez sur <b>Utiliser 8081 au lieu de 8080</b> dans la galerie, ou dans l'onglet <b>Compose</b> du projet changez le port hôte (le nombre de gauche dans <code>\"8080:80\"</code>) puis <b>Save and Up</b>.",
          "Vous n'avez peut-être pas besoin de publier un port : chaque conteneur répond sur <code>https://&lt;nom&gt;.solon.local</code>, port ou pas. Commentez les lignes <code>ports:</code> et utilisez cette adresse.",
          "Deux exemplaires de la même pile (deux WordPress, deux Odoo) se heurtent toujours sur le même port : donnez un autre port hôte au second, ou retirez les ports et utilisez les deux adresses solon.local.",
        ],
        note: null,
      },
      {
        title: "Un conteneur « ne fait rien » ou s'arrête aussitôt",
        symptom: "Vous cliquez sur Run ou Start, le conteneur apparaît une seconde puis passe en Exited ; hello-world est le cas classique.",
        steps: [
          "C'est normal pour un programme qui fait son travail et se termine : hello-world affiche son message et sort en un dixième de seconde. Solon ouvre ses journaux et indique « terminé aussitôt avec le code 0 » ; le message est dans l'onglet Logs.",
          "Un serveur qui sort avec un code non nul est un vrai échec : lisez les dernières lignes de ses journaux (onglet Logs, ou <code>docker logs &lt;nom&gt;</code>). Les causes habituelles : une variable d'environnement manquante, un chemin de volume faux, un port que le conteneur lui-même ne peut pas ouvrir.",
          "Un conteneur que Solon a endormi (icône lune) n'est pas arrêté : la requête suivante le réveille. Utilisez <b>Keep awake</b> sur sa page s'il exécute des tâches planifiées.",
        ],
        note: null,
      },
      {
        title: "Pas de réseau dans les conteneurs, ou une adresse solon.local ne s'ouvre pas",
        symptom: "apt ou pip expirent dans un conteneur, ou le navigateur n'atteint pas https://nom.solon.local.",
        steps: [
          "<b>VPN d'entreprise.</b> Certains clients VPN (AnyConnect, GlobalProtect, Zscaler) bloquent le trafic des cartes virtuelles. Déconnectez le VPN et réessayez ; si ça marche, indiquez-le dans votre rapport pour qu'un contournement soit trouvé.",
          "<b>L'adresse.</b> Un conteneur n'a une adresse que pendant qu'il tourne ; le nom est celui du conteneur (<code>web.solon.local</code>) ou <code>service.projet.solon.local</code> pour Compose. Ouvrez <code>http://nimportequoi.solon.local</code> : la page liste toutes les adresses disponibles.",
          "<b>Le certificat.</b> Les navigateurs et .NET reconnaissent les certificats locaux. <code>curl.exe</code> sur Windows a besoin de <code>--ssl-no-revoke</code>, comme avec mkcert.",
          "<b>Des liens en http://.</b> WordPress, Nextcloud et compagnie construisent leurs liens à partir de la requête : Solon transmet <code>X-Forwarded-Proto: https</code>, que les images officielles respectent. Pour un WordPress déjà installé en http, changez l'adresse du site dans Réglages → Général.",
          "<b>Le fichier hosts.</b> Solon écrit les noms dans <code>C:\\Windows\\System32\\drivers\\etc\\hosts</code> entre <code># solon-begin</code> et <code># solon-end</code>. Un produit de sécurité qui protège ce fichier bloque les adresses : autorisez Solon ou ajoutez les noms à la main.",
        ],
        note: null,
      },
      {
        title: "Le disque se remplit",
        symptom: "Windows prévient que le lecteur C: est presque plein, ou Solon affiche « Disque du moteur presque plein ».",
        steps: [
          "Solon garde tout dans un seul fichier, <code>%ProgramData%\\Solon\\data.vhdx</code>, qui grossit avec les images, conteneurs et volumes et ne se réduit qu'après un nettoyage.",
          "Réglages → <b>Disque</b> → <b>Récupérer l'espace</b> : supprime les images qu'aucun conteneur n'utilise et le cache de construction, puis rend l'espace libéré à Windows. Les conteneurs et les volumes ne sont jamais touchés.",
          "Toujours gros ? Images → « Inutilisées seulement » montre ce qui reste ; Volumes → « Inutilisés seulement » montre les volumes qu'aucun conteneur ne référence (des données que vous voulez peut-être garder : vérifiez avant de supprimer).",
        ],
        note: null,
      },
      {
        title: "Docker Desktop ou WSL est aussi installé",
        symptom: "Les commandes docker parlent au mauvais moteur, ou Docker Hub refuse votre connexion.",
        steps: [
          "Solon et Docker Desktop cohabitent. Le moteur auquel la commande <code>docker</code> s'adresse dépend du <code>docker.exe</code> trouvé en premier dans votre PATH ; celui de Solon est dans son dossier d'installation. <code>docker context ls</code> montre le contexte courant.",
          "Erreurs « unauthorized » de Docker Hub : le CLI docker de Windows réutilise des identifiants stockés par Docker Desktop, parfois périmés. <code>docker logout</code> puis <code>docker login</code> règle le problème.",
          "WSL n'est pas utilisé par Solon et n'a besoin ni d'être installé ni d'être retiré.",
        ],
        note: null,
      },
    ],
    report: {
      title: "Signaler un problème",
      lead: "Un fichier zip en dit plus qu'une capture d'écran. Cela prend dix secondes.",
      steps: [
        "Dans Solon : <b>Réglages → Diagnostic → Exporter un diagnostic…</b>, enregistrez le zip.",
        "Ouvrez un rapport de bug sur GitHub et déposez le zip dans le formulaire, avec ce que vous avez fait et ce que vous attendiez.",
        "Indiquez votre version de Windows (<code>winver</code>), et si un VPN, un autre antivirus, Docker Desktop ou WSL sont présents.",
      ],
      privacy: "Le zip contient les journaux de Solon, son état et ses réglages, le rapport des prérequis, docker info et les versions. Il ne contient ni les données de vos conteneurs, ni mots de passe, ni identifiants Docker Hub ; vous pouvez l'ouvrir et vérifier avant d'envoyer.",
      button: "Ouvrir un rapport de bug",
    },
    codes: {
      title: "Codes d'erreur",
      lead: "Chaque erreur affichée par Solon porte un code stable. Les journaux sont dans %ProgramData%\\Solon\\logs et se copient depuis l'écran d'erreur.",
      cols: ["Code", "Cause", "Que faire"],
      power: "Coupure de courant ou arrêt brutal : au démarrage suivant, Solon vérifie et répare le disque de données (<code>fsck</code>), puis redémarre le moteur. Les écritures non synchronisées des deux dernières secondes peuvent être perdues, comme sur toute machine Linux.",
      rows: [
        ["VIRTUALIZATION_DISABLED_IN_FIRMWARE", "VT-x / AMD-V désactivé", "Activer la virtualisation dans le BIOS/UEFI (onglet Advanced, CPU ou Security), redémarrer."],
        ["UNSUPPORTED_WINDOWS_EDITION", "Windows Famille", "Passer à Windows Pro, Entreprise ou Éducation."],
        ["WINDOWS_FEATURE_MISSING", "Hyper-V ou Plateforme de machine virtuelle désactivés", "Redémarrer si vous venez d'installer. Sinon réinstaller Solon (l'installeur les active) ou, en PowerShell administrateur : <code>Enable-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V,VirtualMachinePlatform -All</code>, puis redémarrer."],
        ["WINDOWS_FEATURE_BLOCKED_BY_POLICY", "Stratégie d'entreprise (WSUS, GPO) refuse l'activation", "Demander à l'administrateur d'activer Microsoft-Hyper-V et VirtualMachinePlatform."],
        ["HYPERVISOR_NOT_RUNNING", "Hyperviseur Windows non démarré", "Désinstaller les anciens VirtualBox/VMware (< 6.1 / < 15.5), vérifier <code>bcdedit /enum</code> (hypervisorlaunchtype Auto), ne pas exécuter Solon dans une VM sans virtualisation imbriquée."],
        ["HOST_COMPUTE_SERVICE_UNAVAILABLE", "Service vmcompute ou hns arrêté ou absent", "Redémarrer Windows ; sinon réinstaller Solon."],
        ["BLOCKED_BY_SECURITY_SOFTWARE", "Antivirus / EDR bloque les disques ou le service", "Ajouter %ProgramData%\\Solon et le dossier d'installation aux exclusions."],
        ["INSUFFICIENT_PRIVILEGES", "Le service ne tourne pas avec les droits attendus", "Réinstaller Solon (le service doit tourner en LocalSystem)."],
        ["IMAGE_CORRUPTED", "Fichiers du moteur absents ou empreinte SHA-256 invalide", "Réinstaller Solon."],
        ["DATA_DISK_ERROR", "data.vhdx impossible à créer ou ouvrir", "Vérifier l'espace disque et les exclusions antivirus ; en dernier recours renommer %ProgramData%\\Solon\\data.vhdx (perte des données Docker)."],
        ["VM_BOOT_TIMEOUT, AGENT_UNREACHABLE, ENGINE_UNREACHABLE", "La machine ne répond pas", "Redémarrer le moteur ; consulter solon-service.log ; signaler avec le zip de diagnostic."],
        ["Pas d'accès réseau depuis les conteneurs", "VPN d'entreprise ou conflit de plage IP", "Solon choisit une plage libre et fixe le MTU à 1400 ; certains VPN bloquent tout de même les cartes virtuelles : désactiver le VPN pour tester, puis signaler."],
        ["« Le service Solon n'est pas en cours d'exécution »", "Service arrêté", "<code>sc start SolonService</code> en administrateur, ou réinstaller."],
      ],
    },
  },
  footer: { made: "Fait au Luxembourg. Aucune télémétrie ; la seule requête sortante est une vérification facultative des nouvelles versions, désactivable.", licence: "Apache 2.0", source: "Code source", issues: "Signaler un problème", security: "Sécurité" },
};

export const dict: Record<Lang, typeof en> = { en, fr };

/** Page « Dépannage » : `troubleshooting/` en anglais, `depannage/` en français. */
export const HELP_PATH: Record<Lang, string> = { en: "troubleshooting/", fr: "depannage/" };

/** Préfixe des liens internes pour une langue (l'anglais est à la racine). */
export function langPath(lang: Lang, path = ""): string {
  const clean = path.replace(/^\/+/, "");
  return lang === "en" ? `/${clean}` : `/fr/${clean}`;
}
