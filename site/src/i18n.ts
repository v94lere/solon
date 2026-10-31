// Textes du site dans les deux langues. Les chiffres viennent de docs/measurements.md (10 septembre 2026).
export type Lang = "en" | "fr";

export const REPO = "https://github.com/v94lere/solon";
export const RELEASES = `${REPO}/releases`;
export const LATEST = `${REPO}/releases/latest`;

const en = {
  nav: { features: "Features", compare: "Benchmark", stacks: "Stacks", releases: "Releases", help: "Help", github: "GitHub", download: "Download" },
  hero: {
    kicker: "Open source · Windows 10 & 11, any edition · Apache 2.0",
    title: "Docker on Windows, quietly.",
    lead: "Solon is a standalone Docker engine for Windows: one installer, the real docker and docker compose, no WSL, Windows Home included. No window: an icon in the taskbar corner, a solon command next to docker, and solon why, which says in plain words why a call failed. Since 0.2.0 (31 October 2026).",
    download: "Download for Windows",
    winget: "winget coming soon",
    requirements: "Windows 11 or 10 22H2, any edition (Home included), hardware virtualization enabled. Check the SHA-256 published with each release.",
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
      { title: "docker compose up, and an address", body: "cd into a project, docker compose up -d. The project appears in the taskbar icon's menu with its https://service.project.solon.local address, trusted certificate included. No port to publish, nothing to install on Windows. (Clip to be recorded for 0.2.0: it still shows the former window.)" },
      { title: "Why is it broken?", body: "solon why reads the Radiography and says in plain words what failed: who called whom, on which port, how many times, and what to check first. (Clip to be recorded for 0.2.0: it still shows the former window.)" },
      { title: "The icon", body: "One click: engine state, one sub-menu per project with Open, Start, Restart, Stop, a terminal with solon and docker ready, Update Solon. (Clip to be recorded for 0.2.0: it still shows the former window.)" },
    ],
  },
  features: {
    title: "What Solon does",
    items: [
      { title: "Addresses that always work", body: "Every container answers at https://<name>.solon.local, published port or not, with a certificate your browser trusts. Compose services get <service>.<project>.solon.local." },
      { title: "Fast Windows folders", body: "Folders mounted into containers go through solonfs, Solon's own file sharing: 3.6 to 94× faster than the 9P sharing used by WSL2 and Docker Desktop, depending on the operation (docs/measurements.md)." },
      { title: "Wake on demand", body: "A container that receives no traffic for ten minutes is paused; the next request wakes it in a fraction of a second. Databases that only talk internally are never touched." },
      { title: "The real docker, nothing else to learn", body: "The official docker CLI and Compose plugin, on your PATH, pointed at Solon. VS Code Dev Containers work as is. Solon does not duplicate docker: it runs the engine and adds what docker cannot do." },
      { title: "The solon command", body: "solon status, start, stop, projects, domains, settings, backup and restore with volumes, clone for one environment per Git branch, scan to find the projects already on this PC, import from Docker Desktop, shell inside the engine, update. No administrator prompt, no outgoing request unless you ask." },
      { title: "An icon, no window", body: "In the taskbar corner: engine state, one sub-menu per Compose project (Open its address, Start, Restart, Stop), a terminal with solon and docker ready, Windows notifications when the engine fails or a project comes back. Nothing to open." },
      { title: "Radiography: solon why", body: "A journal of every exchange inside your projects: connections with their outcome, full HTTP for what comes through a local address, names on outside hosts. solon trace prints it; solon why reads it and says in plain words why a call failed. Sensitive values masked before they are stored; everything stays on your PC." },
    ],
  },
  compare: {
    title: "Measured against Docker Desktop, compared with the others",
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
    footprint: {
      title: "Footprint",
      lead: "Solon measured on 13 September 2026 with three containers running; Docker Desktop measured on the same machine; Podman Desktop and Rancher Desktop from their documentation (published, not measured here).",
      cols: ["", "Solon", "Docker Desktop", "Podman Desktop", "Rancher Desktop"],
      rows: [
        ["Installer", "102 MB", "≈ 600 MB (published)", "≈ 100 MB + WSL machine (published)", "≈ 500 MB (published)"],
        ["Installed on disk", "406 MB", "6 GB required (published)", "500 MB + 2–10 GB of images (published)", "not published"],
        ["Memory at rest, no container", "430 to 520 MB", "1,998 MB", "WSL 2 machine, up to 50 % of RAM (published)", "half of CPU and RAM by default (published)"],
        ["Memory with a web app + PostgreSQL", "1,196 MB", "5,146 MB", "not measured", "not measured"],
        ["Needs WSL 2", "no", "yes, or legacy Hyper-V", "yes", "yes"],
      ],
    },
    features: {
      title: "Features compared",
      lead: "Solon as of 0.2.0; the others as documented in September 2026. “Partial” means the feature exists with a caveat.",
      cols: ["", "Solon", "Docker Desktop", "Podman Desktop", "Rancher Desktop"],
      rows: [
        ["Official docker CLI and Compose, no alias", "yes", "yes", "partial: compatible socket", "yes (moby)"],
        ["Works without WSL 2", "yes", "partial: legacy Hyper-V", "no", "no"],
        ["Windows Home", "yes", "yes", "yes", "yes"],
        ["https://service.project.solon.local for every container, trusted certificate", "yes", "no", "no", "no"],
        ["Graphical interface (windows, screens)", "no, by choice: taskbar icon and solon command", "yes", "yes", "yes"],
        ["Compose projects with their addresses, started and stopped from the taskbar icon", "yes", "partial: Compose list", "partial: Compose list", "no"],
        ["Says why a call between containers failed, in plain words", "yes (solon why)", "no", "no", "no"],
        ["One environment per Git branch, volumes copied", "yes (solon clone)", "no", "no", "no"],
        ["Project backup and restore, volumes included", "yes (solon backup / restore)", "partial: extension", "no", "no"],
        ["Idle containers paused, woken on demand", "yes", "partial: whole machine (Resource Saver)", "no", "no"],
        ["Windows folders shared with solonfs, 3.6 to 94× faster than 9P depending on the operation", "yes", "no: 9P", "no: 9P", "no: 9P"],
        ["Find Compose projects on the disk", "yes (solon scan)", "no", "no", "no"],
        ["VS Code Dev Containers", "yes, verified", "yes", "partial: path setting", "yes"],
        ["Kubernetes built in", "no", "yes", "partial: extensions", "yes (K3s)"],
        ["Windows containers", "no", "yes", "no", "no"],
        ["Extensions marketplace", "no", "yes", "yes", "no"],
        ["Free for companies", "yes (Apache 2.0)", "paid above 250 employees or $10M", "yes", "yes"],
        ["Signed installer", "not yet", "yes", "yes", "yes"],
      ],
    },
  },
  stacks: {
    title: "Stacks and starter kits",
    lead: "The gallery left with the window in 0.2.0. The ready-made compose files remain in the repository's examples/ folder: copy one into a folder, docker compose up -d, and the project appears in the icon's menu with its address.",
    kits: "Starter kits",
    kitsBody: "The project folder is mounted in the container; on the first Up a small script installs the dependencies, generates the official skeleton if the folder is empty, then runs the development server with live reload. Nothing to install on Windows.",
    apps: "Ready-made apps",
    tools: "Developer tools",
  },
  releases: { title: "Releases", lead: "Every release ships the installer and its SHA-256. Installing over a previous version keeps your images, containers and volumes.", assets: "Files", view: "View on GitHub" },
  limits: {
    title: "Known limitations",
    items: [
      "Windows Home works too: the installer enables the Virtual Machine Platform, which is enough for Solon.",
      "No Kubernetes, no separate Linux machines, no ARM images (x86-64 only).",
      "The installer is not signed yet; signing is the next step before a wider launch.",
      "No graphical interface since 0.2.0: no window, no charts, no map. A deliberate choice; the visual part of the Radiography will come back as a VS Code extension.",
      "No automatic update, not even a check: solon update --check asks github.com for the latest version, solon update (or “Update Solon…” in the icon's menu) downloads the installer, checks its SHA-256 and lets Windows ask you before installing.",
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
        symptom: "The icon turns red, a notification says the engine did not start, and solon status shows a code such as WINDOWS_FEATURE_MISSING, HYPERVISOR_NOT_RUNNING or VIRTUALIZATION_DISABLED_IN_FIRMWARE.",
        steps: [
          "<b>Reboot first.</b> The installer enables Hyper-V and the Virtual Machine Platform when they were off; they only work after a restart. Most first-start failures end here.",
          "If the code is <code>VIRTUALIZATION_DISABLED_IN_FIRMWARE</code>: enable hardware virtualization in the BIOS/UEFI (called Intel VT-x, AMD-V or SVM, usually under Advanced, CPU or Security), save, reboot.",
          "Windows Home is supported since 0.1.12: the installer enables the Virtual Machine Platform and asks for one restart. Solon needs Windows 10 22H2 or 11, any edition.",
          "If the code is <code>HYPERVISOR_NOT_RUNNING</code>: an old VirtualBox or VMware, or a <code>bcdedit</code> setting, turned the Windows hypervisor off. Remove or update them (VirtualBox ≥ 6.1, VMware ≥ 15.5), then in an administrator PowerShell: <code>bcdedit /set hypervisorlaunchtype auto</code> and reboot.",
          "Still stuck: run <code>solon diag</code>, and open a bug report with the zip (see below).",
        ],
        note: null,
      },
      {
        title: "A stack refuses to start: a port is already in use",
        symptom: "docker compose up fails and the output says “port is already allocated” or “address already in use”.",
        steps: [
          "Another program (another stack, IIS, a dev server, Docker Desktop) already listens on that port. Change the host port in your compose file (the left number in <code>\"8080:80\"</code>) and run <code>docker compose up -d</code> again; <code>netstat -ano | findstr :8080</code> names the culprit.",
          "You may not need a published port at all: every container answers at <code>https://&lt;name&gt;.solon.local</code>, port or not. Comment the <code>ports:</code> lines and use that address.",
          "Two copies of the same stack (two WordPress, two Odoo) always collide on the same port: give the second one another host port, or drop the ports and use the two solon.local addresses.",
        ],
        note: null,
      },
      {
        title: "A container “does nothing” or stops at once",
        symptom: "You start a container, it appears for a second in docker ps and goes to Exited; hello-world is the classic case.",
        steps: [
          "That is normal for a program that does its job and quits: hello-world prints its message and exits in a tenth of a second. <code>docker ps -a</code> shows “Exited (0)” and <code>docker logs</code> shows the message.",
          "A server that exits with a non-zero code is a real failure: read the last lines of its logs (<code>docker logs &lt;name&gt;</code>). The usual causes are a missing environment variable, a wrong volume path or a port the container itself cannot bind.",
          "A container that Solon put to sleep (“asleep” in <code>solon status</code>) is not stopped: the next request wakes it. <code>solon settings never=&lt;name&gt;</code> excludes it from sleep if it runs scheduled tasks.",
        ],
        note: null,
      },
      {
        title: "No network in the containers, or a solon.local address does not open",
        symptom: "apt or pip time out inside a container, or the browser cannot reach https://name.solon.local.",
        steps: [
          "<b>Corporate VPN.</b> Some VPN clients (AnyConnect, GlobalProtect, Zscaler) block traffic to virtual adapters. Disconnect the VPN and try again; if it works, add it to your bug report so a workaround can be found.",
          "<b>The address.</b> A container has an address only while it runs; the name is the container's (<code>web.solon.local</code>) or <code>service.project.solon.local</code> for Compose. <code>solon domains</code> lists every address currently available, and so does <code>http://anything.solon.local</code>. <code>solon why</code> says whether the call reached the container at all.",
          "<b>The certificate.</b> Browsers and .NET trust the local certificates. <code>curl.exe</code> on Windows needs <code>--ssl-no-revoke</code>, like with mkcert.",
          "<b>Links that point to http://.</b> WordPress, Nextcloud and friends generate links from the request: Solon passes <code>X-Forwarded-Proto: https</code>, which the official images honour. For an existing WordPress installed over http, update the site URL in Settings → General.",
          "<b>The hosts file.</b> Solon writes the names into <code>C:\\Windows\\System32\\drivers\\etc\\hosts</code> between <code># solon-begin</code> and <code># solon-end</code>. A security product that protects that file blocks the addresses: allow Solon or add the names by hand.",
        ],
        note: null,
      },
      {
        title: "The disk fills up",
        symptom: "Windows warns that drive C: is almost full, or Solon notifies “Engine disk almost full”.",
        steps: [
          "Solon keeps everything in one file, <code>%ProgramData%\\Solon\\data.vhdx</code>, which grows with images, containers and volumes and only shrinks after a clean-up.",
          "<code>docker system prune</code> removes the stopped containers, the images no container uses and the build cache; Solon then gives the freed space back to Windows. Volumes are never touched unless you add <code>--volumes</code>.",
          "Still large? <code>docker system df -v</code> shows what remains; <code>docker volume ls -f dangling=true</code> lists volumes no container references (data you may want to keep: check before removing).",
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
        "In a terminal: <code>solon diag</code> (or the icon's menu → Open a terminal, then the same command). It writes the zip and prints its path.",
        "Open a bug report on GitHub and drop the zip into the form, with what you did and what you expected.",
        "Mention your Windows version (<code>winver</code>), whether a VPN, another antivirus, Docker Desktop or WSL are present.",
      ],
      privacy: "The zip contains Solon's logs, its state and settings, the prerequisites report, docker info and the versions. It does not contain your containers' data, passwords or Docker Hub credentials; you can open it and check before sending.",
      button: "Open a bug report",
    },
    codes: {
      title: "Error codes",
      lead: "Every error shown by Solon carries a stable code, printed by solon status and in the taskbar notification. Logs are in %ProgramData%\\Solon\\logs (solon logs).",
      cols: ["Code", "Cause", "What to do"],
      power: "Power loss or hard shutdown: at the next start Solon checks and repairs the data disk (<code>fsck</code>), then restarts the engine. Unsynced writes of the last two seconds may be lost, as on any Linux machine.",
      rows: [
        ["VIRTUALIZATION_DISABLED_IN_FIRMWARE", "VT-x / AMD-V disabled", "Enable virtualization in the BIOS/UEFI (Advanced, CPU or Security tab), reboot."],
        ["WINDOWS_FEATURE_MISSING", "Hyper-V or Virtual Machine Platform disabled", "Reboot if you just installed. Otherwise reinstall Solon (the installer enables them) or, in an administrator PowerShell: <code>Enable-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V,VirtualMachinePlatform -All</code>, then reboot."],
        ["WINDOWS_FEATURE_BLOCKED_BY_POLICY", "Company policy (WSUS, GPO) refuses the feature", "Ask your administrator to enable Microsoft-Hyper-V and VirtualMachinePlatform."],
        ["HYPERVISOR_NOT_RUNNING", "Windows hypervisor not started", "Uninstall old VirtualBox/VMware (< 6.1 / < 15.5), check <code>bcdedit /enum</code> (hypervisorlaunchtype Auto), do not run Solon in a VM without nested virtualization."],
        ["HOST_COMPUTE_SERVICE_UNAVAILABLE", "vmcompute or hns service stopped or missing", "Reboot Windows; otherwise reinstall Solon."],
        ["BLOCKED_BY_SECURITY_SOFTWARE", "Antivirus / EDR blocks the disks or the service", "Add %ProgramData%\\Solon and the installation folder to the exclusions."],
        ["INSUFFICIENT_PRIVILEGES", "The service does not run with the expected rights", "Reinstall Solon (the service must run as LocalSystem)."],
        ["IMAGE_CORRUPTED", "Engine files missing or SHA-256 mismatch", "Reinstall Solon."],
        ["DATA_DISK_ERROR", "data.vhdx cannot be created or opened", "Check disk space and antivirus exclusions; as a last resort rename %ProgramData%\\Solon\\data.vhdx (loses Docker data)."],
        ["VM_BOOT_TIMEOUT, AGENT_UNREACHABLE, ENGINE_UNREACHABLE", "The machine does not answer", "Restart the engine; read solon-service.log; report with the diagnostic zip."],
        ["UNSUPPORTED_WINDOWS_EDITION", "Windows edition not supported", "Solon runs on Windows 10 22H2 and Windows 11, all editions including Home; older builds are not supported. Update Windows."],
        ["VM_CONFIGURATION_REJECTED, HCS_ERROR", "Windows refused the engine configuration, or the virtualization layer returned an error", "Reboot once; if it persists, report with solon-service.log (it carries the Windows error code)."],
        ["IO", "A file under %ProgramData%\\Solon could not be read or written", "Check disk space and antivirus exclusions on %ProgramData%\\Solon."],
        ["INTERNAL", "A bug in Solon", "Report with solon-service.log; the engine usually recovers after a restart."],
        ["No network from containers", "Corporate VPN or IP range conflict", "Solon picks a free range and sets the MTU to 1400; some VPNs still block virtual adapters: disable the VPN to test, then report."],
        ["“The Solon service is not running”", "Service stopped", "<code>sc start SolonService</code> as administrator, or reinstall."],
      ],
    },
  },
  privacy: {
    kicker: "Privacy",
    title: "Solon collects nothing",
    lead: "No account, no telemetry, no analytics, no crash reporting. This page says exactly what leaves your PC, which is almost nothing, and where the rest is kept.",
    updated: "Last updated 31 October 2026.",
    sections: [
      {
        title: "What Solon sends",
        body: [
          "The Solon background service makes no outgoing network request of any kind. Not one.",
          "The solon command makes exactly one, and only when you run it: solon update (or “Update Solon…” in the taskbar icon's menu) asks github.com for the latest published version. That request carries no identifier, nothing about you and nothing about your machine — it is the same request your browser makes by opening the releases page. Solon never checks on its own.",
          "solon update then downloads that release's installer from github.com and checks its SHA-256 against the checksums published beside it. Nothing is sent in the other direction.",
        ],
      },
      {
        title: "What Solon keeps, and where",
        body: [
          "Everything stays on your PC, under C:\\ProgramData\\Solon: the engine's data disk holding your images, containers and volumes; your settings; the local certificate authority used for the https://….solon.local addresses; and log files.",
          "The log files record what the engine does, including the paths of the Windows folders you share with containers. They never contain credentials, environment variable values, or anything from inside your containers.",
          "Uninstalling Solon leaves that folder in place unless you ask for it to be removed, and even then it is moved aside rather than deleted.",
        ],
      },
      {
        title: "The diagnostic export",
        body: [
          "solon diag writes a zip file on your disk: logs, engine state, your settings, the system prerequisites and docker info. It is created only when you run the command, it is never sent anywhere automatically, and you decide whether to attach it to a bug report.",
        ],
      },
      {
        title: "Your containers",
        body: [
          "Containers reach the internet as they would under any container engine: pulling images from the registries you name, and whatever the images themselves do. That traffic is governed by what you run, not by Solon, and Solon neither inspects nor records it.",
        ],
      },
      {
        title: "No account, no third parties",
        body: [
          "Solon has no sign-in, no licence server and no paid tier. No advertising identifier, no analytics library, no third-party service embedded in the service, the icon or the command.",
        ],
      },
    ],
    questions: "Questions about any of this? Open an issue and ask.",
  },
  footer: { made: "Made in Luxembourg. No telemetry; the only outgoing request is solon update, which you run yourself.", licence: "Apache 2.0", source: "Source code", issues: "Report a problem", security: "Security", privacy: "Privacy" },
};

const fr: typeof en = {
  nav: { features: "Fonctions", compare: "Comparatif", stacks: "Piles", releases: "Versions", help: "Aide", github: "GitHub", download: "Télécharger" },
  hero: {
    kicker: "Open source · Windows 10 et 11, toutes éditions · Apache 2.0",
    title: "Docker sur Windows, en silence.",
    lead: "Solon est un moteur Docker autonome pour Windows : un installateur, le vrai docker et le vrai docker compose, pas de WSL, Windows Famille compris. Pas de fenêtre : une icône dans le coin de la barre des tâches, une commande solon à côté de docker, et solon why, qui dit en français courant pourquoi un appel a échoué. Depuis la 0.2.0 (31 octobre 2026).",
    download: "Télécharger pour Windows",
    winget: "winget bientôt",
    requirements: "Windows 11 ou 10 22H2, toutes éditions (Famille comprise), virtualisation matérielle activée. Vérifiez le SHA-256 publié avec chaque version.",
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
      { title: "docker compose up, et une adresse", body: "cd dans un projet, docker compose up -d. Le projet apparaît dans le menu de l'icône avec son adresse https://service.projet.solon.local, certificat reconnu compris. Aucun port à publier, rien à installer sur Windows. (Clip à enregistrer pour la 0.2.0 : il montre encore l'ancienne fenêtre.)" },
      { title: "Pourquoi ça casse ?", body: "solon why lit la Radiographie et dit en français courant ce qui a échoué : qui a appelé qui, sur quel port, combien de fois, et quoi vérifier d'abord. (Clip à enregistrer pour la 0.2.0 : il montre encore l'ancienne fenêtre.)" },
      { title: "L'icône", body: "Un clic : état du moteur, un sous-menu par projet avec Ouvrir, Démarrer, Redémarrer, Arrêter, un terminal avec solon et docker prêts, Mettre à jour Solon. (Clip à enregistrer pour la 0.2.0 : il montre encore l'ancienne fenêtre.)" },
    ],
  },
  features: {
    title: "Ce que Solon fait",
    items: [
      { title: "Des adresses qui marchent toujours", body: "Chaque conteneur répond sur https://<nom>.solon.local, port publié ou non, avec un certificat reconnu par votre navigateur. Les services Compose ont <service>.<projet>.solon.local." },
      { title: "Dossiers Windows rapides", body: "Les dossiers montés dans les conteneurs passent par solonfs, le partage de fichiers de Solon : 3,6 à 94× plus rapide que le 9P utilisé par WSL2 et Docker Desktop, selon l'opération (docs/measurements.md)." },
      { title: "Réveil à la demande", body: "Un conteneur sans trafic pendant dix minutes est mis en pause ; la requête suivante le réveille en une fraction de seconde. Les bases qui ne parlent qu'en interne ne sont jamais touchées." },
      { title: "Le vrai docker, rien d'autre à apprendre", body: "Le CLI docker officiel et le plugin Compose, dans votre PATH, dirigés vers Solon. VS Code Dev Containers fonctionne tel quel. Solon ne double pas docker : il fait tourner le moteur et ajoute ce que docker ne sait pas faire." },
      { title: "La commande solon", body: "solon status, start, stop, projects, domains, settings, backup et restore avec les volumes, clone pour un environnement par branche Git, scan pour retrouver les projets déjà sur ce PC, import depuis Docker Desktop, shell dans le moteur, update. Aucune demande d'administrateur, aucune requête sortante sans votre geste." },
      { title: "Une icône, pas de fenêtre", body: "Dans le coin de la barre des tâches : état du moteur, un sous-menu par projet Compose (Ouvrir son adresse, Démarrer, Redémarrer, Arrêter), un terminal avec solon et docker prêts, des notifications Windows quand le moteur tombe ou qu'un projet repart. Rien à ouvrir." },
      { title: "Radiographie : solon why", body: "Un journal de chaque échange dans vos projets : connexions avec leur issue, HTTP complet pour ce qui entre par une adresse locale, des noms sur les hôtes extérieurs. solon trace l'imprime ; solon why le lit et dit en français courant pourquoi un appel a échoué. Valeurs sensibles masquées avant d'être gardées ; tout reste sur votre PC." },
    ],
  },
  compare: {
    title: "Mesuré face à Docker Desktop, comparé aux autres",
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
    footprint: {
      title: "Empreinte",
      lead: "Solon mesuré le 13 septembre 2026 avec trois conteneurs en marche ; Docker Desktop mesuré sur la même machine ; Podman Desktop et Rancher Desktop d'après leur documentation (publié, pas mesuré ici).",
      cols: ["", "Solon", "Docker Desktop", "Podman Desktop", "Rancher Desktop"],
      rows: [
        ["Installateur", "102 Mo", "≈ 600 Mo (publié)", "≈ 100 Mo + machine WSL (publié)", "≈ 500 Mo (publié)"],
        ["Installé sur le disque", "406 Mo", "6 Go requis (publié)", "500 Mo + 2 à 10 Go d'images (publié)", "non publié"],
        ["Mémoire au repos, aucun conteneur", "430 à 520 Mo", "1 998 Mo", "machine WSL 2, jusqu'à 50 % de la RAM (publié)", "moitié des cœurs et de la RAM par défaut (publié)"],
        ["Mémoire avec une application web + PostgreSQL", "1 196 Mo", "5 146 Mo", "non mesuré", "non mesuré"],
        ["Exige WSL 2", "non", "oui, ou ancien moteur Hyper-V", "oui", "oui"],
      ],
    },
    features: {
      title: "Fonctionnalités comparées",
      lead: "Solon en 0.2.0 ; les autres d'après leur documentation en septembre 2026. « Partiel » : la fonction existe avec une réserve.",
      cols: ["", "Solon", "Docker Desktop", "Podman Desktop", "Rancher Desktop"],
      rows: [
        ["CLI docker et Compose officiels, sans alias", "oui", "oui", "partiel : socket compatible", "oui (moby)"],
        ["Fonctionne sans WSL 2", "oui", "partiel : ancien moteur Hyper-V", "non", "non"],
        ["Windows Famille", "oui", "oui", "oui", "oui"],
        ["https://service.projet.solon.local pour chaque conteneur, certificat reconnu", "oui", "non", "non", "non"],
        ["Interface graphique (fenêtre, écrans)", "non, par choix : icône de la barre des tâches et commande solon", "oui", "oui", "oui"],
        ["Projets Compose avec leurs adresses, démarrés et arrêtés depuis l'icône", "oui", "partiel : liste Compose", "partiel : liste Compose", "non"],
        ["Dit pourquoi un appel entre conteneurs a échoué, en français courant", "oui (solon why)", "non", "non", "non"],
        ["Un environnement par branche Git, volumes copiés", "oui (solon clone)", "non", "non", "non"],
        ["Sauvegarde et restauration d'un projet, volumes compris", "oui (solon backup / restore)", "partiel : extension", "non", "non"],
        ["Conteneurs inactifs en veille, réveil à la demande", "oui", "partiel : toute la machine (Resource Saver)", "non", "non"],
        ["Dossiers Windows partagés par solonfs, 3,6 à 94× plus rapide que 9P selon l'opération", "oui", "non : 9P", "non : 9P", "non : 9P"],
        ["Recherche des projets Compose sur le disque", "oui (solon scan)", "non", "non", "non"],
        ["VS Code Dev Containers", "oui, vérifié", "oui", "partiel : réglage du chemin", "oui"],
        ["Kubernetes intégré", "non", "oui", "partiel : extensions", "oui (K3s)"],
        ["Conteneurs Windows", "non", "oui", "non", "non"],
        ["Place de marché d'extensions", "non", "oui", "oui", "non"],
        ["Gratuit en entreprise", "oui (Apache 2.0)", "payant au-delà de 250 salariés ou 10 M$", "oui", "oui"],
        ["Installateur signé", "pas encore", "oui", "oui", "oui"],
      ],
    },
  },
  stacks: {
    title: "Piles et kits de démarrage",
    lead: "La galerie est partie avec la fenêtre en 0.2.0. Les fichiers Compose prêts restent dans le dossier examples/ du dépôt : copiez-en un dans un dossier, docker compose up -d, et le projet apparaît dans le menu de l'icône avec son adresse.",
    kits: "Kits de démarrage",
    kitsBody: "Le dossier du projet est monté dans le conteneur ; au premier Up, un petit script installe les dépendances, génère le squelette officiel si le dossier est vide, puis lance le serveur de développement avec rechargement à chaud. Rien à installer sur Windows.",
    apps: "Applications prêtes",
    tools: "Outils du développeur",
  },
  releases: { title: "Versions", lead: "Chaque version fournit l'installateur et son SHA-256. Installer par-dessus une version précédente conserve images, conteneurs et volumes.", assets: "Fichiers", view: "Voir sur GitHub" },
  limits: {
    title: "Limites connues",
    items: [
      "Windows Famille fonctionne aussi : l'installateur active la Plateforme de machine virtuelle, qui suffit à Solon.",
      "Pas de Kubernetes, pas de machines Linux séparées, pas d'images ARM (x86-64 seulement).",
      "L'installateur n'est pas encore signé ; c'est la prochaine étape avant un lancement plus large.",
      "Pas d'interface graphique depuis la 0.2.0 : ni fenêtre, ni courbes, ni carte. Un choix assumé ; la partie visuelle de la Radiographie reviendra sous la forme d'une extension VS Code.",
      "Pas de mise à jour automatique, pas même de vérification : solon update --check demande à github.com la dernière version, solon update (ou « Mettre à jour Solon… » dans le menu de l'icône) télécharge l'installateur, vérifie son SHA-256 et laisse Windows vous demander l'autorisation avant d'installer.",
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
        symptom: "L'icône passe au rouge, une notification dit que le moteur n'a pas démarré, et solon status montre un code comme WINDOWS_FEATURE_MISSING, HYPERVISOR_NOT_RUNNING ou VIRTUALIZATION_DISABLED_IN_FIRMWARE.",
        steps: [
          "<b>Redémarrez d'abord.</b> L'installateur active Hyper-V et la Plateforme de machine virtuelle s'ils étaient éteints ; ils ne fonctionnent qu'après un redémarrage. La plupart des échecs du premier démarrage s'arrêtent là.",
          "Code <code>VIRTUALIZATION_DISABLED_IN_FIRMWARE</code> : activez la virtualisation matérielle dans le BIOS/UEFI (Intel VT-x, AMD-V ou SVM, en général sous Advanced, CPU ou Security), enregistrez, redémarrez.",
          "Windows Famille est pris en charge depuis 0.1.12 : l'installateur active la Plateforme de machine virtuelle et demande un redémarrage. Il faut Windows 10 22H2 ou 11, toutes éditions.",
          "Code <code>HYPERVISOR_NOT_RUNNING</code> : un vieux VirtualBox ou VMware, ou un réglage <code>bcdedit</code>, a éteint l'hyperviseur Windows. Retirez-les ou mettez-les à jour (VirtualBox ≥ 6.1, VMware ≥ 15.5), puis en PowerShell administrateur : <code>bcdedit /set hypervisorlaunchtype auto</code> et redémarrez.",
          "Toujours bloqué : lancez <code>solon diag</code>, et ouvrez un rapport avec le zip (voir plus bas).",
        ],
        note: null,
      },
      {
        title: "Une pile refuse de démarrer : un port est déjà pris",
        symptom: "docker compose up échoue et la sortie dit « port is already allocated » ou « address already in use ».",
        steps: [
          "Un autre programme (une autre pile, IIS, un serveur de développement, Docker Desktop) écoute déjà sur ce port. Changez le port hôte dans votre fichier Compose (le nombre de gauche dans <code>\"8080:80\"</code>) et relancez <code>docker compose up -d</code> ; <code>netstat -ano | findstr :8080</code> nomme le coupable.",
          "Vous n'avez peut-être pas besoin de publier un port : chaque conteneur répond sur <code>https://&lt;nom&gt;.solon.local</code>, port ou pas. Commentez les lignes <code>ports:</code> et utilisez cette adresse.",
          "Deux exemplaires de la même pile (deux WordPress, deux Odoo) se heurtent toujours sur le même port : donnez un autre port hôte au second, ou retirez les ports et utilisez les deux adresses solon.local.",
        ],
        note: null,
      },
      {
        title: "Un conteneur « ne fait rien » ou s'arrête aussitôt",
        symptom: "Vous démarrez un conteneur, il apparaît une seconde dans docker ps puis passe en Exited ; hello-world est le cas classique.",
        steps: [
          "C'est normal pour un programme qui fait son travail et se termine : hello-world affiche son message et sort en un dixième de seconde. <code>docker ps -a</code> montre « Exited (0) » et <code>docker logs</code> le message.",
          "Un serveur qui sort avec un code non nul est un vrai échec : lisez les dernières lignes de ses journaux (<code>docker logs &lt;nom&gt;</code>). Les causes habituelles : une variable d'environnement manquante, un chemin de volume faux, un port que le conteneur lui-même ne peut pas ouvrir.",
          "Un conteneur que Solon a endormi (« endormi » dans <code>solon status</code>) n'est pas arrêté : la requête suivante le réveille. <code>solon settings never=&lt;nom&gt;</code> l'exclut de la mise en veille s'il exécute des tâches planifiées.",
        ],
        note: null,
      },
      {
        title: "Pas de réseau dans les conteneurs, ou une adresse solon.local ne s'ouvre pas",
        symptom: "apt ou pip expirent dans un conteneur, ou le navigateur n'atteint pas https://nom.solon.local.",
        steps: [
          "<b>VPN d'entreprise.</b> Certains clients VPN (AnyConnect, GlobalProtect, Zscaler) bloquent le trafic des cartes virtuelles. Déconnectez le VPN et réessayez ; si ça marche, indiquez-le dans votre rapport pour qu'un contournement soit trouvé.",
          "<b>L'adresse.</b> Un conteneur n'a une adresse que pendant qu'il tourne ; le nom est celui du conteneur (<code>web.solon.local</code>) ou <code>service.projet.solon.local</code> pour Compose. <code>solon domains</code> liste toutes les adresses disponibles, et <code>http://nimportequoi.solon.local</code> aussi. <code>solon why</code> dit si l'appel a seulement atteint le conteneur.",
          "<b>Le certificat.</b> Les navigateurs et .NET reconnaissent les certificats locaux. <code>curl.exe</code> sur Windows a besoin de <code>--ssl-no-revoke</code>, comme avec mkcert.",
          "<b>Des liens en http://.</b> WordPress, Nextcloud et compagnie construisent leurs liens à partir de la requête : Solon transmet <code>X-Forwarded-Proto: https</code>, que les images officielles respectent. Pour un WordPress déjà installé en http, changez l'adresse du site dans Réglages → Général.",
          "<b>Le fichier hosts.</b> Solon écrit les noms dans <code>C:\\Windows\\System32\\drivers\\etc\\hosts</code> entre <code># solon-begin</code> et <code># solon-end</code>. Un produit de sécurité qui protège ce fichier bloque les adresses : autorisez Solon ou ajoutez les noms à la main.",
        ],
        note: null,
      },
      {
        title: "Le disque se remplit",
        symptom: "Windows prévient que le lecteur C: est presque plein, ou Solon notifie « Disque du moteur presque plein ».",
        steps: [
          "Solon garde tout dans un seul fichier, <code>%ProgramData%\\Solon\\data.vhdx</code>, qui grossit avec les images, conteneurs et volumes et ne se réduit qu'après un nettoyage.",
          "<code>docker system prune</code> supprime les conteneurs arrêtés, les images qu'aucun conteneur n'utilise et le cache de construction ; Solon rend ensuite l'espace libéré à Windows. Les volumes ne sont jamais touchés sans <code>--volumes</code>.",
          "Toujours gros ? <code>docker system df -v</code> montre ce qui reste ; <code>docker volume ls -f dangling=true</code> liste les volumes qu'aucun conteneur ne référence (des données que vous voulez peut-être garder : vérifiez avant de supprimer).",
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
        "Dans un terminal : <code>solon diag</code> (ou menu de l'icône → Ouvrir un terminal, puis la même commande). Il écrit le zip et affiche son chemin.",
        "Ouvrez un rapport de bug sur GitHub et déposez le zip dans le formulaire, avec ce que vous avez fait et ce que vous attendiez.",
        "Indiquez votre version de Windows (<code>winver</code>), et si un VPN, un autre antivirus, Docker Desktop ou WSL sont présents.",
      ],
      privacy: "Le zip contient les journaux de Solon, son état et ses réglages, le rapport des prérequis, docker info et les versions. Il ne contient ni les données de vos conteneurs, ni mots de passe, ni identifiants Docker Hub ; vous pouvez l'ouvrir et vérifier avant d'envoyer.",
      button: "Ouvrir un rapport de bug",
    },
    codes: {
      title: "Codes d'erreur",
      lead: "Chaque erreur affichée par Solon porte un code stable, imprimé par solon status et dans la notification de la barre des tâches. Les journaux sont dans %ProgramData%\\Solon\\logs (solon logs).",
      cols: ["Code", "Cause", "Que faire"],
      power: "Coupure de courant ou arrêt brutal : au démarrage suivant, Solon vérifie et répare le disque de données (<code>fsck</code>), puis redémarre le moteur. Les écritures non synchronisées des deux dernières secondes peuvent être perdues, comme sur toute machine Linux.",
      rows: [
        ["VIRTUALIZATION_DISABLED_IN_FIRMWARE", "VT-x / AMD-V désactivé", "Activer la virtualisation dans le BIOS/UEFI (onglet Advanced, CPU ou Security), redémarrer."],
        ["WINDOWS_FEATURE_MISSING", "Hyper-V ou Plateforme de machine virtuelle désactivés", "Redémarrer si vous venez d'installer. Sinon réinstaller Solon (l'installeur les active) ou, en PowerShell administrateur : <code>Enable-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V,VirtualMachinePlatform -All</code>, puis redémarrer."],
        ["WINDOWS_FEATURE_BLOCKED_BY_POLICY", "Stratégie d'entreprise (WSUS, GPO) refuse l'activation", "Demander à l'administrateur d'activer Microsoft-Hyper-V et VirtualMachinePlatform."],
        ["HYPERVISOR_NOT_RUNNING", "Hyperviseur Windows non démarré", "Désinstaller les anciens VirtualBox/VMware (< 6.1 / < 15.5), vérifier <code>bcdedit /enum</code> (hypervisorlaunchtype Auto), ne pas exécuter Solon dans une VM sans virtualisation imbriquée."],
        ["HOST_COMPUTE_SERVICE_UNAVAILABLE", "Service vmcompute ou hns arrêté ou absent", "Redémarrer Windows ; sinon réinstaller Solon."],
        ["BLOCKED_BY_SECURITY_SOFTWARE", "Antivirus / EDR bloque les disques ou le service", "Ajouter %ProgramData%\\Solon et le dossier d'installation aux exclusions."],
        ["INSUFFICIENT_PRIVILEGES", "Le service ne tourne pas avec les droits attendus", "Réinstaller Solon (le service doit tourner en LocalSystem)."],
        ["IMAGE_CORRUPTED", "Fichiers du moteur absents ou empreinte SHA-256 invalide", "Réinstaller Solon."],
        ["DATA_DISK_ERROR", "data.vhdx impossible à créer ou ouvrir", "Vérifier l'espace disque et les exclusions antivirus ; en dernier recours renommer %ProgramData%\\Solon\\data.vhdx (perte des données Docker)."],
        ["VM_BOOT_TIMEOUT, AGENT_UNREACHABLE, ENGINE_UNREACHABLE", "La machine ne répond pas", "Redémarrer le moteur ; consulter solon-service.log ; signaler avec le zip de diagnostic."],
        ["UNSUPPORTED_WINDOWS_EDITION", "Édition de Windows non prise en charge", "Solon fonctionne sur Windows 10 22H2 et Windows 11, toutes éditions, Famille comprise ; les versions plus anciennes ne sont pas prises en charge. Mettre Windows à jour."],
        ["VM_CONFIGURATION_REJECTED, HCS_ERROR", "Windows a refusé la configuration du moteur, ou la couche de virtualisation a renvoyé une erreur", "Redémarrer une fois ; si cela persiste, signaler avec solon-service.log (il contient le code d'erreur Windows)."],
        ["IO", "Un fichier sous %ProgramData%\\Solon n'a pas pu être lu ou écrit", "Vérifier l'espace disque et les exclusions antivirus sur %ProgramData%\\Solon."],
        ["INTERNAL", "Un bogue de Solon", "Signaler avec solon-service.log ; le moteur repart en général après un redémarrage."],
        ["Pas d'accès réseau depuis les conteneurs", "VPN d'entreprise ou conflit de plage IP", "Solon choisit une plage libre et fixe le MTU à 1400 ; certains VPN bloquent tout de même les cartes virtuelles : désactiver le VPN pour tester, puis signaler."],
        ["« Le service Solon n'est pas en cours d'exécution »", "Service arrêté", "<code>sc start SolonService</code> en administrateur, ou réinstaller."],
      ],
    },
  },
  privacy: {
    kicker: "Confidentialité",
    title: "Solon ne collecte rien",
    lead: "Aucun compte, aucune télémétrie, aucune statistique d'usage, aucun rapport d'incident. Cette page dit exactement ce qui sort de votre PC, c'est-à-dire presque rien, et où le reste est conservé.",
    updated: "Mise à jour le 31 octobre 2026.",
    sections: [
      {
        title: "Ce que Solon envoie",
        body: [
          "Le service Solon en arrière-plan ne fait aucune requête réseau sortante. Aucune.",
          "La commande solon en fait exactement une, et seulement quand vous la lancez : solon update (ou « Mettre à jour Solon… » dans le menu de l'icône) demande à github.com la dernière version publiée. Cette requête ne porte aucun identifiant, rien sur vous et rien sur votre machine — c'est la même que celle de votre navigateur ouvrant la page des versions. Solon ne vérifie jamais de lui-même.",
          "solon update télécharge alors l'installateur de cette version depuis github.com et vérifie son empreinte SHA-256 avec les empreintes publiées à côté. Rien ne part dans l'autre sens.",
        ],
      },
      {
        title: "Ce que Solon conserve, et où",
        body: [
          "Tout reste sur votre PC, sous C:\\ProgramData\\Solon : le disque de données du moteur avec vos images, conteneurs et volumes ; vos réglages ; l'autorité de certification locale qui sert aux adresses https://….solon.local ; et les fichiers journaux.",
          "Les journaux enregistrent ce que fait le moteur, y compris les chemins des dossiers Windows que vous partagez avec vos conteneurs. Ils ne contiennent jamais d'identifiants, ni les valeurs de vos variables d'environnement, ni quoi que ce soit venant de l'intérieur de vos conteneurs.",
          "La désinstallation laisse ce dossier en place, sauf demande explicite, et même alors il est déplacé plutôt que supprimé.",
        ],
      },
      {
        title: "L'export de diagnostic",
        body: [
          "solon diag écrit un fichier zip sur votre disque : journaux, état du moteur, vos réglages, les prérequis système et docker info. Il n'est créé que si vous lancez la commande, n'est jamais envoyé automatiquement, et c'est vous qui décidez de le joindre ou non à un rapport de bogue.",
        ],
      },
      {
        title: "Vos conteneurs",
        body: [
          "Les conteneurs accèdent à Internet comme sous n'importe quel moteur de conteneurs : téléchargement des images depuis les registres que vous indiquez, et ce que les images font d'elles-mêmes. Ce trafic dépend de ce que vous lancez, pas de Solon, qui ne l'inspecte ni ne l'enregistre.",
        ],
      },
      {
        title: "Aucun compte, aucun tiers",
        body: [
          "Solon n'a ni connexion, ni serveur de licences, ni offre payante. Aucun identifiant publicitaire, aucune bibliothèque de statistiques, aucun service tiers embarqué dans le service, l'icône ou la commande.",
        ],
      },
    ],
    questions: "Une question là-dessus ? Ouvrez une issue et demandez.",
  },
  footer: { made: "Fait au Luxembourg. Aucune télémétrie ; la seule requête sortante est solon update, que vous lancez vous-même.", licence: "Apache 2.0", source: "Code source", issues: "Signaler un problème", security: "Sécurité", privacy: "Confidentialité" },
};

export const dict: Record<Lang, typeof en> = { en, fr };

/** Page « Dépannage » : `troubleshooting/` en anglais, `depannage/` en français. */
export const HELP_PATH: Record<Lang, string> = { en: "troubleshooting/", fr: "depannage/" };

/** Page « Confidentialité » : `privacy/` en anglais, `confidentialite/` en français. Exigée par le
 *  Microsoft Store, qui demande une adresse publique même quand l'application ne collecte rien. */
export const PRIVACY_PATH: Record<Lang, string> = { en: "privacy/", fr: "confidentialite/" };

/** Préfixe des liens internes pour une langue (l'anglais est à la racine). */
export function langPath(lang: Lang, path = ""): string {
  const clean = path.replace(/^\/+/, "");
  return lang === "en" ? `/${clean}` : `/fr/${clean}`;
}
