# <img src="apps/desktop/src-tauri/icons/128x128.png" alt="Solon" width="60"> Solon

*Version française : [README.fr.md](README.fr.md).*

[![CI](https://github.com/v94lere/solon/actions/workflows/ci.yml/badge.svg)](https://github.com/v94lere/solon/actions/workflows/ci.yml) [![Release](https://img.shields.io/github/v/release/v94lere/solon?label=release&color=f860c0)](https://github.com/v94lere/solon/releases/latest) [![Downloads](https://img.shields.io/github/downloads/v94lere/solon/total?color=e01818)](https://github.com/v94lere/solon/releases) [![License](https://img.shields.io/badge/license-Apache%202.0-blue)](LICENSE) [![Website](https://img.shields.io/badge/website-v94lere.github.io%2Fsolon-1a0d14)](https://v94lere.github.io/solon/)

**Docker on Windows, quietly.** Solon is a standalone Docker engine for Windows: one installer, no software
prerequisite to install yourself, and a complete Docker engine that boots in a few seconds inside a tiny,
invisible Linux machine managed entirely by Solon. The real `docker` and `docker compose`, no WSL, Windows
Home included.

Solon has **no window**. Since 0.2.0 it is three things: a Windows service that runs the engine, an icon in the
taskbar corner (state, one menu entry per project, notifications), and a `solon` command next to `docker`
for everything else, including `solon why`, which reads the Radiography and says in plain words why a call
failed. The desktop application with its screens was removed in 0.2.0; what replaced each screen is listed
in [What changed in 0.2.0](#what-changed-in-020).

Solon **is not** a front-end for an existing Docker Desktop. It replaces it: Docker engine, Compose, local
https addresses, fast shared folders, and a few things nothing else does on Windows.

> Project status (31 October 2026): **0.2.0**, nineteen releases in two months, written by one person.
> It runs my own projects every day, and it has been tested on exactly two machines: a Windows 11 Pro PC and a
> fresh Windows 11 Home VM. The installer is **not code-signed yet** (SmartScreen warning; SHA-256 published
> with every release). Expect rough edges on hardware I do not have — and please
> [tell me where it breaks](https://github.com/v94lere/solon/issues/1). That issue is the most useful place to
> start; ideas and pull requests are just as welcome.

## What Solon does

- Starts a Docker engine (dockerd, containerd, runc, Compose) ready **~3 s** after you ask for it.
- Exposes the Docker API on `\\.\pipe\solon`; the bundled `docker` and `docker compose` commands are already
  on your `PATH` and point at Solon, and any other Docker CLI works with a `docker context`.
- Relays published ports to `localhost` with no configuration.
- Shares your Windows folders on demand: `docker run -v C:\...`, `--mount`, Compose projects, exactly like
  Docker Desktop, through **solonfs**, Solon's own file sharing, measured 3.6 to 94× faster than the 9P
  sharing used by WSL2 and Docker Desktop, depending on the operation (`docs/measurements.md`).
- **Import from Docker Desktop**: `solon import` lists its volumes and images with their sizes and copies
  the ones you name into Solon's engine. Nothing is changed or removed on the Docker Desktop side, and
  anything already in Solon under the same name is left alone.
- **Addresses that always work**: every running container is reachable at `https://<name>.solon.local`
  (and `https://<service>.<project>.solon.local` for Compose), **whether it publishes a port or not**, with a
  certificate your browser trusts (a local certificate authority created on first use). Container IPs
  (`10.90.x.y`) are reachable directly from Windows too.
- **Wake on demand**: a container reached through Solon that receives no traffic for ten minutes is paused
  (zero CPU, memory kept); the next request wakes it in a fraction of a second before being served. Databases
  and background workers that only talk internally are never touched.
- **Radiography**: a journal of every exchange inside your projects — TCP connections with their outcome,
  full HTTP exchanges for anything coming in through a `*.solon.local` address, names on outside addresses.
  `solon trace` prints it, `solon why` reads it and says in plain words why a call failed. Sensitive values
  are masked before they are stored; bodies are off by default and enabled per project.
- **A `solon` command** next to `docker`: engine state and addresses, start and stop with the steps shown,
  settings, project backup and restore, find the projects on this PC, import from Docker Desktop, one
  environment per Git branch, a root shell inside the engine, update. It talks to the service through its
  local pipe: no administrator prompt, and no outgoing request except `solon update`, which you run yourself.
- **An icon in the taskbar corner**: engine state, number of running containers, one sub-menu per Compose
  project (open its address, start, restart, stop), a terminal with `solon` and `docker` ready, Windows
  notifications when something happens.
- **Restart what was running**: when the engine stops (Windows restart, update, `solon stop`), Solon
  remembers the projects and containers that were running and starts them again once the engine is back.
- Idle memory (engine + service): **430 to 520 MB** measured for 2 GB allocated; unused memory is returned
  to Windows automatically.
- **No telemetry, no outgoing network request** other than what your containers and your `docker pull` ask
  for, and `solon update`, which asks github.com for the latest release only when you run it.

## Benchmark: performance, footprint and features

### Performance: Docker Desktop vs Solon

Same machine (Windows 11 Pro, 24 logical cores, NVMe), same Compose stack (a Python web application and its PostgreSQL 16
database, `bench/compose.yaml`), each engine with its default settings: Docker Desktop 4.66.1 on WSL2 with every core
and 6.6 GB visible to containers; Solon 0.1.0 with 22 processors and 2 GB. Docker Desktop was measured on
3 September 2026, then uninstalled from the test machine; Solon was re-measured on 10 September 2026 with the
same script. Details and raw numbers: `docs/measurements.md`.

| | Docker Desktop | Solon | |
|---|---|---|---|
| Engine ready after you ask for it | 6.1 s | **2.6 to 3.4 s** (1.1 s when the machine is still up) | 2× faster |
| Memory at rest, no container | 1,998 MB | **430 to 520 MB** | 4× lighter |
| Memory with the web app + PostgreSQL running, at rest | 5,146 MB | **1,196 MB** | 4× lighter |
| `compose down` then `up -d`, images present | 7.1 s | **5.2 s** | |
| The web app answers after `up` | 1.5 s | 1.0 to 2.5 s | same |
| Login page of the web app, average of 5 loads | 27 to 46 ms | 48 to 88 ms | same |
| Initialise the app's database with sample data (CPU, mostly single-threaded) | 13.6 s | 12.9 to 13.4 s | same |
| Synchronous writes on a Docker volume (`pg_test_fsync`, fdatasync / fsync) | 154 / 79 ops/s | **240 to 290 / 132 to 137 ops/s** | 1.7× faster |
| Windows folder mounted in a container, 5,000 files (listing / attributes / reads / writes) | 9P | **solonfs**: 6.6× / 94× / 3.6 to 9× / 4× faster | |
| `docker run --rm busybox true`, warm | not measured | 0.55 s | |

What this says, honestly: Solon wins clearly on what costs you every day (start-up time, memory, file sharing,
disk writes); on pure CPU work the two engines are equivalent; web latency is identical. Both keep the guest's
disk cache in memory: Solon is capped by its allocation (2 GB by default), Docker Desktop grew to 5 GB.

Reproduce it: `powershell -ExecutionPolicy Bypass -File bench\bench.ps1` on the engine your `docker` command
points at (`-Docker "C:\Program Files\Solon\bin\docker.exe"` for Solon). The script only touches a Compose
project named `solon-bench` on port 18069 and removes it when done.

### Footprint

Solon measured on 13 September 2026 (0.1.10, three containers running); Docker Desktop measured on the same machine
on 3 September; the other two from their documentation (marked *published*).

| | Solon | Docker Desktop | Podman Desktop | Rancher Desktop |
|---|---|---|---|---|
| Installer | **102 MB** | ≈ 600 MB *(published)* | ≈ 100 MB + WSL machine *(published)* | ≈ 500 MB *(published)* |
| Installed on disk | **406 MB** (Linux image 310, `docker` CLI 73, Solon 22) | 6 GB required *(published)* | 500 MB + 2 to 10 GB of images *(published)* | not published |
| Memory at rest, no container | **430 to 520 MB** | 1,998 MB | WSL 2 machine, up to 50 % of RAM by default *(published)* | half of CPU and RAM by default *(published)* |
| Memory with a web app + PostgreSQL running | **1,196 MB** | 5,146 MB | not measured | not measured |
| Needs WSL 2 | no (own Hyper-V machine) | yes (or legacy Hyper-V backend) | yes | yes |

### Features compared

Solon as of 0.2.0; the others as documented in September 2026. "Partial" means it exists with a caveat.

| | Solon | Docker Desktop | Podman Desktop | Rancher Desktop |
|---|---|---|---|---|
| Official `docker` CLI and Compose, no alias | yes | yes | partial: compatible socket, some differences | yes (moby) |
| Works without WSL 2 | yes | partial: legacy Hyper-V backend | no | no |
| Windows Home | yes (Virtual Machine Platform) | yes (WSL 2) | yes | yes |
| `https://<service>.<project>.solon.local` for every container, trusted local certificate | yes | no | no | no |
| Graphical interface (windows, screens) | no, by choice: a taskbar icon and the `solon` command | yes | yes | yes |
| Compose projects listed with their addresses, started and stopped from the taskbar icon | yes | partial: Compose list | partial: Compose list | no |
| One environment per Git branch, volumes copied | yes (`solon clone`) | no | no | no |
| Project backup and restore, volumes included | yes (`solon backup`, `solon restore`) | partial: third-party extension | no | no |
| Command line for the engine itself: state, start and stop, settings, backup, why a call failed, update | yes (`solon`) | partial: `docker desktop start/stop/status` *(documented)* | partial: `podman machine` *(documented)* | partial: `rdctl` *(documented)* |
| Says why a call between containers failed, in plain words | yes (`solon why`) | no | no | no |
| Idle containers paused, woken on demand | yes | partial: Resource Saver pauses the whole machine | no | no |
| Windows folders shared with solonfs (3.6 to 94× faster than 9P, depending on the operation) | yes | no: 9P through WSL 2 | no: 9P | no: 9P |
| Find Compose projects on the disk (7 s on 928 GB) | yes (`solon scan`) | no | no | no |
| Projects restarted after a Windows restart or an update | yes | partial: Docker `restart` policy | partial | partial |
| VS Code Dev Containers | yes, verified | yes | partial: docker path setting | yes |
| Kubernetes built in | no | yes | partial: Kind / Minikube extensions | yes (K3s) |
| Windows containers | no | yes | no | no |
| Extensions marketplace | no | yes | yes | no |
| Free for companies | yes (Apache 2.0) | paid above 250 employees or $10M revenue | yes (Apache 2.0) | yes (Apache 2.0) |
| Signed installer | not yet | yes | yes | yes |

## Requirements

| Requirement | Detail |
|---|---|
| Windows | **Windows 11 or Windows 10 22H2, any edition, Home included.** Pro, Enterprise and Education use Hyper-V; Home uses the Virtual Machine Platform, which the installer enables (one restart). Verified on Windows 11 Home 25H2. On Home the legacy 9P file-sharing fallback is unavailable; the default sharing works. |
| CPU | Hardware virtualization **enabled in the BIOS/UEFI**: Intel VT-x ("Intel Virtualization Technology") or AMD-V ("SVM Mode"). |
| Memory | 8 GB recommended (4 GB minimum). Solon gives the engine 2 GB by default, adjustable with `solon settings memory=…`. |
| Disk | ~400 MB for Solon and its Linux image, plus a dynamic data disk (64 GB maximum by default, used on demand). |
| Windows features | "Hyper-V" and "Virtual Machine Platform". The installer enables them; a reboot may be required. |

Solon coexists with WSL2 and Docker Desktop: it uses neither their pipe, nor their networks, nor your default
`docker` context.

## Installation

### Installer

Download `Solon_<version>_x64-setup.exe` from the [releases](https://github.com/v94lere/solon/releases) and
check its SHA-256 against the value published with the release. The installer asks for elevation once, then:
enables the required Windows features (a reboot may be requested), installs the `SolonService` service, copies
the Linux image and the `docker` and `solon` commands, and creates the shortcut. Silent install: `Solon_<version>_x64-setup.exe /S`.
Installing 0.2.0 over a 0.1.x keeps everything: images, containers, volumes, settings, Radiography history.

The installer is **not signed yet**: SmartScreen shows "Windows protected your PC"; click "More info" then
"Run anyway". The signing chain is ready (`installer/sign.ps1`, `tauri.signed.conf.json`) and will be enabled
as soon as a certificate is available.

Uninstall: Settings → Apps → Solon. The uninstaller stops the engine, removes the service and **asks** before
deleting your images and volumes (`%ProgramData%\Solon`); by default they are kept.

### From source (developers)

Prerequisites: Rust stable (≥ 1.85), Node 22 (only for the Tauri bundler: there is no JavaScript left in
Solon), and **Solon itself** installed (the engine's Linux image is built inside a Solon container: no WSL
needed). An **administrator** Windows session is only needed for the
console mode.

```powershell
# 1. Guest agent (cross-compiled from Windows, no C toolchain)
rustup target add x86_64-unknown-linux-musl
cargo build --release -p solon-agent --target x86_64-unknown-linux-musl

# 2. Linux image (reused kernel + Alpine root filesystem + initrd), built in a Solon container (~15 s)
docker run --rm -v "${PWD}:/work" -w /work public.ecr.aws/docker/library/alpine:3.24 sh -c "apk add -q bash curl python3 e2fsprogs coreutils tar grep findutils gzip; SKIP_KERNEL=1 SOLON_IMAGE_VERSION=0.1.0-dev.N bash image/build.sh /work/target/x86_64-unknown-linux-musl/release/solon-agent"
#    (the kernel itself is compiled once with image/kernel/build-kernel.sh, ~7 min, in the same kind of container)

# 3. Service, `solon` command and taskbar icon (the installer embeds image/out/<version>, see tauri.conf.json)
cargo build --release -p solon-service -p solon-cli
cd apps\desktop && npm install && npm run tauri build
#    (the service and the CLI are copied as resources: `tauri build` does not rebuild them, hence the order)
```

Development mode without installing: start the service in console mode, either by hand
(`solon-service.exe console --start` in an administrator terminal) or with
`.\tests\e2e\start-console.ps1 -ImageDir .\image\out\<version> -Release` (one UAC prompt), then
`target\release\solon-cli.exe status` for the command and `cd apps\desktop; npm run tauri dev` for the icon.

## Using Solon

There is nothing to open. After installation the engine starts, `docker` works in any new terminal, and
Solon's icon sits in the taskbar corner. Everything else is one of the `solon` commands below.

### The icon

One click, left or right, opens a native menu:

- **Engine: running / stopped / failed…** Clicking it opens a terminal.
- **N containers running.**
- **One sub-menu per Compose project**, « ● blog (2/2) »: *Open https://web.blog.solon.local*, *Start
  project*, *Restart project*, *Stop project*. Containers started outside a project have the same sub-menu
  (Open, Start, Restart, Stop).
- **Open a terminal (solon, docker)**: a console in your home folder with `solon` and `docker` on the PATH.
- **Why is it broken? (solon why)**: the same terminal, with the answer already printed.
- **Update Solon…**: runs `solon update`.
- **Start with Windows**: on first launch Solon registers itself to start with your Windows session (your
  user's Run key) and a notification says so, because an icon that is not there when the engine fails is of
  no use. Untick to say no; that no is remembered, Solon does not register itself again. Uninstalling removes
  the entry.
- **Start engine** / **Stop engine**, **Quit Solon**.

The icon's dot is green, orange, red or grey depending on the engine state. Windows notifications, rare by
design: engine failed, Docker restarting, engine disk almost full, projects restarted after the engine came
up, an action from the menu that failed. Launching `Solon.exe` a second time only notifies « Solon is already
running ». Labels are in English, or in French when Windows is (`SOLON_LANG=fr` forces it).

### Bundled `docker` and `docker compose`

The installer puts `C:\Program Files\Solon\bin` first on the `PATH`. It contains the **official Docker CLI**
and the **Compose plugin** (Apache-2.0, versions in `bin\NOTICE-third-party.txt`) behind a small `docker.exe`
launcher that points them at the Solon engine. In a **new** terminal:

```powershell
docker version
docker compose -f examples\wordpress\compose.yaml up -d
```

The launcher respects your choices: `-H`, `--context`, `DOCKER_HOST` or `DOCKER_CONTEXT` win, so Docker
Desktop stays reachable if you keep it (`docker context use desktop-linux`). With another Docker CLI:

```powershell
docker context create solon --docker host=npipe:////./pipe/solon
docker context use solon
```

> If Docker Desktop is installed, the CLI uses the Windows credential manager and may send stale Docker Hub
> credentials ("unauthorized: incorrect username or password"). Run `docker logout` or test with an empty
> `DOCKER_CONFIG`; this is not related to Solon.

### Command line: `solon`

The same folder holds `solon`, Solon's own command. `docker` talks to the containers; `solon` talks to the
engine around them: its state, its addresses, its settings, the Radiography, backups, updates. It goes
through the service's local pipe: no administrator prompt, no outgoing request (except `solon update`, see
below). Messages are in English, or in French when Windows is (`SOLON_LANG=fr` forces it). `solon help` lists
every command; `--json` on `status`, `projects`, `domains`, `trace`, `settings`, `prereq`, `watch`, `scan`,
`import` and `update` prints machine-readable output. Exit codes: 0, 1 error, 2 usage, 3 service unreachable.

| Command | What it does |
|---|---|
| `solon` or `solon status` | Engine state, image, address, local domains, then the running containers with their project, state (« asleep » when Solon paused it) and `https://service.project.solon.local` address. |
| `solon start`, `solon stop [--force]`, `solon restart` | Start or stop the engine, each step shown. |
| `solon projects` | The Compose projects known to the engine, with their Windows folder and addresses. |
| `solon domains` | Every `*.solon.local` address answering right now. |
| `solon why [name]` | What is wrong, in plain words, from the Radiography. |
| `solon trace [name] [-n N \| --all] [-f]` | The journal of exchanges, 50 last lines by default; `-f` follows live. |
| `solon clear [project]` | Forget the observed history, of one project or all. |
| `solon backup [folder] [file.zip] [--force]` | One zip with the Compose files, the `.env` and every volume of the project, running or not. |
| `solon restore <file.zip> [folder]` | Recreate the volumes and files; then `docker compose up -d`. |
| `solon clone <from> <to>` | Copy the volumes of a project under another name: one environment per Git branch (`blog` → `blog-feature-login`), nothing overwritten. |
| `solon scan` | Find the Docker projects on this PC: the service reads the file table of the internal drives (a few seconds, nothing leaves the PC) and lists every folder with a `compose.yaml`, a `Dockerfile` or a `devcontainer.json`. |
| `solon import [--all \| --volumes a,b --images x:tag]` | Copy volumes and images from a running Docker Desktop without touching it; with no option, list what it contains. |
| `solon shell` | A root shell inside the engine (`docker`, `ps`, `df`, `dmesg`…), through a privileged busybox container and `nsenter`. |
| `solon exec <command…>` | One command as root inside the engine; one quoted word is a full shell line. |
| `solon settings [key=value…]` | Show or change the settings: `memory` (MB), `cpus`, `disk` (GiB), `autostart`, `resume`, `sleep`, `idle` (min), `never` (list or `none`), `bodies` (list or `none`), `legacy-9p`. Memory, processors, disk and file sharing apply at the next engine start. |
| `solon update [--check]` | Latest version published on GitHub; without `--check`, downloads the installer, checks its SHA-256 against the release's `SHA256SUMS.txt` and launches it (Windows asks for permission). This is the only outgoing request Solon makes, and only when you ask. From the Microsoft Store it answers « updates come from the Store ». |
| `solon prereq` | The Windows prerequisites, line by line. |
| `solon logs [-n N] [-f]`, `solon diag [file.zip]` | The service log; the zip to attach to a bug report (logs, state, settings, prerequisites, `docker info`; no credentials). |
| `solon watch [--json]` | Engine events as they happen. |
| `solon version` | Versions of the command, the service and the image. |

`solon` alone is `solon status`. Outputs below are shortened:

```text
> solon
Solon 0.2.0 · engine ready since 09:12:04 (2 h 31 min)
  image 0.1.0-dev.22 · address 172.30.0.2
  local domains *.solon.local : http and https

2 containers running, 1 asleep (woken on demand)
CONTAINER              PROJECT    STATUS      ADDRESS
wordpress-db-1         wordpress  Up 2 hours  https://db.wordpress.solon.local
wordpress-wordpress-1  wordpress  asleep      https://wordpress.wordpress.solon.local

> solon restart
Restarting the engine…
  checking prerequisites…
  cleaning up…
  verifying the image…
  preparing the data disk…
  creating the network…
  creating the engine…
  booting…
  waiting for the engine…
  configuring the network…
  waiting for Docker…
Engine ready.

> solon projects
PROJECT    STATUS      FOLDER                     ADDRESSES
wordpress  running(2)  C:\Users\me\wordpress      db.wordpress.solon.local, wordpress.wordpress.solon.local

> solon why wordpress
Since 01:27:40, … What stands out:

- 10.90.5.4 tried 4 times to reach db (wordpress) on port 9999 and never got an answer (last at 01:28:51,
  from the program "wget"). Nothing was listening there. Is the container up, and does it listen on that
  port? `docker logs` on it usually says why.

Detail: solon trace wordpress

> solon trace wordpress -n 3
01:28:25  wordpress (wordpress) -> db (wordpress):3306  closed after 48 ms  [apache2]
01:28:25  GET https://wordpress.wordpress.solon.local/wp-login.php  -> 302 in 170 ms  (wordpress (wordpress))
01:28:51  10.90.5.4 -> db (wordpress):9999  FAILED, no answer  [wget]

> solon backup C:\Users\me\wordpress
Backing up wordpress (C:\Users\me\wordpress)…
Written: wordpress-20261031-1422.zip (2 volumes, …)

> solon settings memory=4096 idle=20
Saved.
Memory, processors, disk and file sharing apply at the next engine start: solon restart
```

`solon why` reads the Radiography history and groups what went wrong: refused calls by
caller, destination and port, with a hint (container asleep, nothing listening, address outside the engine),
server errors (5xx) per host, 401/403 refusals, the slowest request when it is above one second. When nothing
stands out it says so, with the counts. `solon trace` is the journal (`-f` follows live, `--all` prints
everything), `solon clear [project]` forgets it.

`solon backup` writes one zip with the Compose file, the `.env` and the volumes; `solon restore` puts it back
into the folder of your choice, on the same PC or another one. `solon settings` alone shows every setting
with its meaning.

### What changed in 0.2.0

The desktop application (window, screens) was removed on 31 October 2026. The engine, the service, your data
(images, containers, volumes, settings, history) and the `docker` command did not move. What each screen did,
and what does it now:

| Gone | Instead |
|---|---|
| Containers, Images, Volumes, Networks screens | `docker ps`, `docker images`, `docker volume`, `docker network` |
| Projects page: cards, compose and `.env` editor with port check, filtered logs, Files panel, project templates, find projects on this PC, import from Docker Desktop, one environment per branch, backup and restore | `solon projects`, `solon scan`, `solon import`, `solon clone`, `solon backup` / `solon restore`, your editor, `docker compose logs`, `docker cp` |
| Terminal inside the machine | `solon shell` |
| Activity screen (CPU, memory, disk, network charts) | `docker stats`; no graphical equivalent |
| Radiography screen (map, journal, playback bar, bodies per project) | `solon trace`, `solon why`, `solon settings bodies=…`; the map and the visual playback have no equivalent for now: a VS Code extension is the next step |
| Settings screen | `solon settings` |
| Command palette, `Ctrl+Alt+S`, welcome tour, theme and accent colour, update banner | gone |

### VS Code Dev Containers

The **Dev Containers** extension works with Solon as is: it uses the `docker` on your PATH, which is Solon's.
Open a folder that has `.devcontainer/devcontainer.json`, run "Dev Containers: Reopen in Container", and VS Code
builds or pulls the image, starts the container with the folder mounted at `/workspaces/<name>`, installs its
server inside and connects. Forwarded ports work, `postCreateCommand` runs, and the container appears in
`solon status` (and its folder in `solon scan`). Nothing to configure; if you had set
`dev.containers.dockerPath` for Docker Desktop, remove it. Tested with `@devcontainers/cli` 0.89 and the
`mcr.microsoft.com/devcontainers/base:alpine` image.

The **Containers** view (extension `ms-azuretools.vscode-containers`) refreshes live through `docker events`.
The extension arms that listener once, at startup, and gives up after three immediate failures. Solon's
Docker pipe exists as soon as the Solon service starts and `docker` waits while the engine boots, so a VS Code
window restored at logon keeps live refresh. If Solon is updated while VS Code is open, the pipe vanishes for a
few seconds and that window falls back to a refresh every minute and the Refresh button: run
*Developer: Reload Window* to get live refresh back.

## Shared Windows folders: how it works

Windows folders mounted into containers (`-v C:\...`, Compose projects) go through **solonfs**, Solon's file
system: a server on the Windows side, a client on the Linux side, and a protocol that fetches a whole folder in
one question instead of one per file. Measured on 5,000 files against Windows 9P sharing (the one used by WSL2
and Docker Desktop): 3.6 to 94× faster depending on the operation (listing 6.6×, attributes 94×, reads 3.6 to
9×, writes 4×). Details and method: `docs/measurements.md`.

- Files appear as owned by `root` with `0777` / `0666`; `chmod` and `chown` are accepted and ignored
  (Windows has no POSIX permissions), as with Docker Desktop.
- A change made on the Windows side is visible in the container within 1.5 s.
- For dependencies and databases, always prefer **Docker volumes**: they live on Solon's disk at native speed
  (20 to 50 ms for the same 5,000 files).
- Fallback: `solon settings legacy-9p=on` restores the old mechanism at the next engine start. The old share also stays mounted under `/mnt/host9p/<letter>` inside the machine.

## Troubleshooting

Step-by-step guide for the most common problems (SmartScreen, reboot after enabling Hyper-V, port already in
use, container that exits at once, network and `solon.local` addresses, disk space):
**<https://v94lere.github.io/solon/troubleshooting/>**. To report a problem, attach the zip written by
`solon diag` (logs, state, settings, `docker info`; no credentials).

Error messages carry a **stable code**, printed by `solon status` and in the taskbar notification; logs are in
`%ProgramData%\Solon\logs` (service, `solon logs`).

| Code | Cause | What to do |
|---|---|---|
| `VIRTUALIZATION_DISABLED_IN_FIRMWARE` | VT-x / AMD-V disabled | Enable virtualization in the BIOS/UEFI (Advanced, CPU or Security tab), reboot. |
| `WINDOWS_FEATURE_MISSING` | Hyper-V or Virtual Machine Platform disabled | Reinstall Solon (the installer enables them) or, in an administrator PowerShell: `Enable-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V,VirtualMachinePlatform -All`, then reboot. |
| `WINDOWS_FEATURE_BLOCKED_BY_POLICY` | Company policy (WSUS, GPO) refuses the feature | Ask your administrator to enable `Microsoft-Hyper-V` and `VirtualMachinePlatform`. |
| `HYPERVISOR_NOT_RUNNING` | Windows hypervisor not started | Uninstall old VirtualBox/VMware (< 6.1 / < 15.5), check `bcdedit /enum` (`hypervisorlaunchtype Auto`), do not run Solon in a VM without nested virtualization. |
| `HOST_COMPUTE_SERVICE_UNAVAILABLE` | `vmcompute` or `hns` service stopped or missing | Reboot Windows; otherwise reinstall Solon. |
| `BLOCKED_BY_SECURITY_SOFTWARE` | Antivirus / EDR blocks the disks or the service | Add `%ProgramData%\Solon` and the installation folder to the exclusions. |
| `INSUFFICIENT_PRIVILEGES` | The service does not run with the expected rights | Reinstall Solon (the service must run as LocalSystem). |
| `IMAGE_CORRUPTED` | Engine files missing or SHA-256 mismatch | Reinstall Solon. |
| `DATA_DISK_ERROR` | `data.vhdx` cannot be created or opened | Check disk space and antivirus exclusions; as a last resort rename `%ProgramData%\Solon\data.vhdx` (loses Docker data). |
| `VM_BOOT_TIMEOUT`, `AGENT_UNREACHABLE`, `ENGINE_UNREACHABLE` | The machine does not answer | Restart the engine; read `solon-service.log`; report with the log. |
| `UNSUPPORTED_WINDOWS_EDITION` | Windows edition not supported | Solon runs on Windows 10 22H2 and Windows 11, all editions including Home; older builds are not supported. Update Windows. |
| `VM_CONFIGURATION_REJECTED`, `HCS_ERROR` | Windows refused the engine configuration, or the virtualization layer returned an error | Reboot once; if it persists, report with `solon-service.log` (it carries the Windows error code). |
| `IO` | A file under `%ProgramData%\Solon` could not be read or written | Check disk space and antivirus exclusions on `%ProgramData%\Solon`. |
| `INTERNAL` | A bug in Solon | Report with `solon-service.log`; the engine usually recovers after a restart. |
| No network from containers | Corporate VPN or IP range conflict | Solon picks a free range among `172.30.0.0/24`… and sets the MTU to 1400; some VPNs (AnyConnect, GlobalProtect) still block virtual adapters: disable the VPN to test, then report. |
| "The Solon service is not running" | Service stopped | `sc start SolonService` as administrator, or reinstall. |

Power loss or hard shutdown: at the next start Solon checks and repairs the data disk (`fsck`), then restarts
the engine. Unsynced writes of the last two seconds may be lost, as on any Linux machine.

## Known limitations (0.2)

- **No graphical interface**: no window, no charts, no map. A deliberate choice made for 0.2.0: Solon is the
  engine, the icon and the command; the visual part of the Radiography will come back as a VS Code extension.
- **UDP** published ports are not relayed to `localhost` (TCP only).
- **One engine per machine**, no multiple profiles.
- **Not signed** (SmartScreen warning). **No automatic update, not even a check**: `solon update --check`
  asks github.com whether a newer version exists, `solon update` (or « Update Solon… » in the icon's menu)
  downloads the installer, checks its SHA-256 against the release's `SHA256SUMS.txt`, then Windows asks you
  to allow the install; the service restarts what was running. Nothing installs without you.
- A sleeping container does not run its internal scheduled tasks until something calls it;
  `solon settings never=<name>` excludes it from sleep if that matters.
- `curl.exe` on Windows rejects the local HTTPS certificates unless you pass `--ssl-no-revoke` (same as mkcert);
  browsers and .NET accept them.

## Where it is going, and where you can help

Solon is young and the list of what is left to do is longer than the list of what is done. These are the
next things I want to build, each with an issue you can comment on or take:

- **A VS Code extension** for the Radiography: the map of who calls whom and the playback bar, which left
  with the window in 0.2.0, belong next to the code.
- **Update images with one command** ([#3](https://github.com/v94lere/solon/issues/3)): say when a newer image exists, pull and recreate.
- **Relay UDP published ports** ([#5](https://github.com/v94lere/solon/issues/5)), the last gap against `docker run -p`.
- **Hot reload from Windows** ([#7](https://github.com/v94lere/solon/issues/7)): tell the container when a Windows file changes, so Vite, nodemon and Odoo's reloader notice.
- **Windows 10**, tested rather than assumed.
- **A signed installer**: the certificate sponsor asked for community traction first, so this one depends
  on people finding the project useful.

Good places to start are the issues labelled [good first issue](https://github.com/v94lere/solon/labels/good%20first%20issue)
and [help wanted](https://github.com/v94lere/solon/labels/help%20wanted). `CONTRIBUTING.md` builds the whole
thing in one command, and documentation, translations and bug reproductions count as much as code.

## Documentation

- `ARCHITECTURE.md`: technical choices (HCS virtualization, Linux image, HvSocket, solonfs, network),
  measured results block by block, risk register. In French.
- `docs/guide-fonctionnel.md`: the functional guide: the icon, the commands, what the engine does by itself. In French.
- `docs/radiographie.md`: the Radiography, phase by phase. In French.
- `docs/measurements.md`: every measurement and established fact. In French.
- `bench/`: the comparison script and its Compose stack (see the benchmark above).
- `site/`: the presentation website (Astro, static), published to GitHub Pages by the `Site` workflow.
- `tests/e2e/`: end-to-end scenarios.
- `CONTRIBUTING.md`: how to contribute; `SECURITY.md`: how to report a vulnerability;
  `CODE_OF_CONDUCT.md`.

## Licence

Apache License 2.0, see `LICENSE` and `NOTICE`. Solon bundles third-party software (Docker CLI, Docker
Compose, a Linux kernel, Alpine Linux packages, Rust dependencies) under their own licences: see
`THIRD-PARTY.md`.
