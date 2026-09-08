# Monodon

*Version française : [README.fr.md](README.fr.md).* Monodon was called **Solon** until 8 September 2026; existing installations are migrated automatically (data folder, certificate authority, hosts entries).

**Monodon is a standalone container manager for Windows**, in the spirit of OrbStack: one installer, no
software prerequisite to install yourself, and a complete Docker engine that boots in a few seconds inside a
tiny, invisible Linux machine managed entirely by Monodon.

Monodon **is not** a front-end for an existing Docker Desktop. It replaces it: Docker engine, Compose, images,
volumes, networks, terminal, logs, a system-tray icon, and a few things nothing else does on Windows.

> Project status (8 September 2026): **0.1.0, beta**. Everything below works on the development machine
> (Windows 11 Pro) and on a fresh machine with Docker Desktop and WSL removed. The installer is **not signed
> yet** (SmartScreen warning). Feedback and bug reports are very welcome.

![Containers list](docs/screenshots/containers.png)

## What Monodon does

- Starts a Docker engine (dockerd, containerd, runc, Compose) ready **~2.5 s** after you ask for it.
- Exposes the Docker API on `\\.\pipe\monodon`; the bundled `docker` and `docker compose` commands are already
  on your `PATH` and point at Monodon, and any other Docker CLI works with a `docker context`.
- Relays published ports to `localhost` with no configuration.
- Shares your Windows folders on demand: `docker run -v C:\...`, `--mount`, Compose projects, exactly like
  Docker Desktop, through **monodonfs**, Monodon's own file sharing, measured 4 to 94 times faster than the 9P
  sharing used by WSL2 and Docker Desktop.
- **Addresses that always work**: every running container is reachable at `https://<name>.monodon.local`
  (and `https://<service>.<project>.monodon.local` for Compose), **whether it publishes a port or not**, with a
  certificate your browser trusts (a local certificate authority created on first use). Container IPs
  (`10.90.x.y`) are reachable directly from Windows too.
- **Wake on demand**: a container reached through Monodon that receives no traffic for ten minutes is paused
  (zero CPU, memory kept); the next request wakes it in a fraction of a second before being served. Databases
  and background workers that only talk internally are never touched.
- **Files** tab for containers and volumes: browse, copy to Windows, send files or folders (buttons or
  drag-and-drop from Explorer), create folders, delete. Works even when the image has no shell.
- **Debug shell** into any container, including "distroless" images with no shell: a toolbox (bash, curl, dig,
  ps, strace, tcpdump, jq, vim…) that shares the container's processes, network and volumes.
- **Activity**: live CPU, memory, storage and network of the engine and of each container.
- Compose projects: services, merged logs, Up / Down / Rebuild with live output, open in Explorer or VS Code.
- Idle memory (engine + service): **430 to 520 MB** measured for 2 GB allocated; unused memory is returned
  to Windows automatically.
- **No telemetry, no outgoing network request** other than what your containers and your `docker pull` ask for.

## Requirements

| Requirement | Detail |
|---|---|
| Windows | **Windows 11 (or Windows 10 22H2) Pro, Enterprise or Education**. Windows Home is not supported in this version: it lacks a Hyper-V component used for file sharing (see `ARCHITECTURE.md` §13, risk R2). |
| CPU | Hardware virtualization **enabled in the BIOS/UEFI**: Intel VT-x ("Intel Virtualization Technology") or AMD-V ("SVM Mode"). |
| Memory | 8 GB recommended (4 GB minimum). Monodon gives the engine 2 GB by default, adjustable in Settings. |
| Disk | ~400 MB for Monodon and its Linux image, plus a dynamic data disk (64 GB maximum by default, used on demand). |
| Windows features | "Hyper-V" and "Virtual Machine Platform". The installer enables them; a reboot may be required. |

Monodon coexists with WSL2 and Docker Desktop: it uses neither their pipe, nor their networks, nor your default
`docker` context.

## Installation

### Installer

Download `Monodon_<version>_x64-setup.exe` from the [releases](https://github.com/v94lere/monodon/releases) and
check its SHA-256 against the value published with the release. The installer asks for elevation once, then:
enables the required Windows features (a reboot may be requested), installs the `MonodonService` service, copies
the Linux image and creates the shortcut. Silent install: `Monodon_<version>_x64-setup.exe /S`.

The installer is **not signed yet**: SmartScreen shows "Windows protected your PC"; click "More info" then
"Run anyway". The signing chain is ready (`installer/sign.ps1`, `tauri.signed.conf.json`) and will be enabled
as soon as a certificate is available.

Uninstall: Settings → Apps → Monodon. The uninstaller stops the engine, removes the service and **asks** before
deleting your images and volumes (`%ProgramData%\Monodon`); by default they are kept.

### From source (developers)

Prerequisites: Rust stable (≥ 1.85), Node 22, and **Monodon itself** installed (the engine's Linux image is
built inside a Monodon container: no WSL needed). An **administrator** Windows session is only needed for the
console mode.

```powershell
# 1. Guest agent (cross-compiled from Windows, no C toolchain)
rustup target add x86_64-unknown-linux-musl
cargo build --release -p monodon-agent --target x86_64-unknown-linux-musl

# 2. Linux image (reused kernel + Alpine root filesystem + initrd), built in a Monodon container (~15 s)
docker run --rm -v "${PWD}:/work" -w /work public.ecr.aws/docker/library/alpine:3.24 sh -c "apk add -q bash curl python3 e2fsprogs coreutils tar grep findutils gzip; SKIP_KERNEL=1 MONODON_IMAGE_VERSION=0.1.0-dev.N bash image/build.sh /work/target/x86_64-unknown-linux-musl/release/monodon-agent"
#    (the kernel itself is compiled once with image/kernel/build-kernel.sh, ~7 min, in the same kind of container)

# 3. Service and application (the installer embeds image/out/<version>, see tauri.conf.json)
cargo build --release -p monodon-service
cd apps\desktop && npm install && npm run tauri build
```

Development mode without installing: `monodon-service.exe console` in an administrator terminal, then
`cd apps\desktop; npm run tauri dev`.

## Using Monodon

The left menu has two groups: **Docker** (Containers, Volumes, Images, Networks) and **General** (Activity,
Terminal, Settings). The button at the top (or `Ctrl+B`) collapses it to icons. The block at the bottom shows
the engine state, its uptime and two mini gauges (CPU, RAM). The tray icon carries a green, orange, red or grey
dot depending on the engine state.

- **Containers**: live list with CPU and memory, filter, Compose groups, icon actions, streamed logs,
  terminal, inspection. A **published port is a link**, and so is the local domain. Compose projects live
  here: "Open a project…" picks a folder containing `compose.yaml`, and clicking a group header opens the
  project screen (services, merged logs, Up / Down / Rebuild with live output, Explorer, VS Code).
- **Container page**: Overview (image, command, dates, restart policy, networks, ports, mounts, environment,
  labels, file copy), Logs (search, wrap, colours), Files, Terminal, Debug shell, Inspect. Start, stop, restart,
  remove and "Keep awake" in the header.
- **Images, Volumes, Networks**: list, create, inspect, remove (always with confirmation). Volumes have a
  Files browser.
- **Activity**: the engine's CPU, memory, storage and network with one-minute curves, then each running
  container with CPU, memory, network rates and a CPU curve.
- **Terminal** (`Ctrl+\``): a root shell inside the Linux engine itself, for `docker`, `ps`, `df`, `dmesg`…
- **Search `Ctrl+K`**: containers, images, volumes, networks, projects, engine actions, sections.
  `Ctrl+1` to `Ctrl+7` switch sections.
- **Settings**: language (English by default, French), appearance (light, dark, follow Windows), accent
  colour (Monodon blue or the Windows accent), engine memory and processors (all cores minus two by default),
  storage limit, start at sign-in, sleep of idle containers, legacy file sharing fallback, diagnostic export.
- **Windows notifications**: container exited with an error (outside actions you made in Monodon), engine
  failed or restarting, engine disk 90 % full.
- **Diagnostic** (Settings → Export a diagnostic…): a zip with logs, state, settings, prerequisites and
  `docker info` to attach to a bug report. No credentials are included.
- **Tray**: one click opens the menu: engine state, running containers, **each container with Start /
  Restart / Stop**, open Monodon, start or stop the engine, quit. Closing the window keeps Monodon in the tray.

### Bundled `docker` and `docker compose`

The installer puts `C:\Program Files\Monodon\bin` first on the `PATH`. It contains the **official Docker CLI**
and the **Compose plugin** (Apache-2.0, versions in `bin\NOTICE-third-party.txt`) behind a small `docker.exe`
launcher that points them at the Monodon engine. In a **new** terminal:

```powershell
docker version
docker compose -f examples\odoo18\compose.yaml up -d
```

The launcher respects your choices: `-H`, `--context`, `DOCKER_HOST` or `DOCKER_CONTEXT` win, so Docker
Desktop stays reachable if you keep it (`docker context use desktop-linux`). With another Docker CLI:

```powershell
docker context create monodon --docker host=npipe:////./pipe/monodon
docker context use monodon
```

> If Docker Desktop is installed, the CLI uses the Windows credential manager and may send stale Docker Hub
> credentials ("unauthorized: incorrect username or password"). Run `docker logout` or test with an empty
> `DOCKER_CONFIG`; this is not related to Monodon.

## Shared Windows folders: how it works

Windows folders mounted into containers (`-v C:\...`, Compose projects) go through **monodonfs**, Monodon's file
system: a server on the Windows side, a client on the Linux side, and a protocol that fetches a whole folder in
one question instead of one per file. Measured on 5,000 files against Windows 9P sharing (the one used by WSL2
and Docker Desktop): listing 6.6× faster, attributes 94× faster, reads 3.6 to 9× faster, writes 4× faster.
Details and method: `docs/measurements.md`.

- Files appear as owned by `root` with `0777` / `0666`; `chmod` and `chown` are accepted and ignored
  (Windows has no POSIX permissions), as with Docker Desktop.
- A change made on the Windows side is visible in the container within 1.5 s.
- For dependencies and databases, always prefer **Docker volumes**: they live on Monodon's disk at native speed
  (20 to 50 ms for the same 5,000 files).
- Fallback: Settings → "Use the legacy Windows file sharing (9P)" restores the old mechanism at the next
  engine start. The old share also stays mounted under `/mnt/host9p/<letter>` inside the machine.

## Troubleshooting

Error messages carry a **stable code**; logs are in `%ProgramData%\Monodon\logs` (service) and can be copied
from the error screen.

| Code | Cause | What to do |
|---|---|---|
| `VIRTUALIZATION_DISABLED_IN_FIRMWARE` | VT-x / AMD-V disabled | Enable virtualization in the BIOS/UEFI (Advanced, CPU or Security tab), reboot. |
| `UNSUPPORTED_WINDOWS_EDITION` | Windows Home | Move to Windows Pro/Enterprise/Education. |
| `WINDOWS_FEATURE_MISSING` | Hyper-V or Virtual Machine Platform disabled | Reinstall Monodon (the installer enables them) or, in an administrator PowerShell: `Enable-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V,VirtualMachinePlatform -All`, then reboot. |
| `WINDOWS_FEATURE_BLOCKED_BY_POLICY` | Company policy (WSUS, GPO) refuses the feature | Ask your administrator to enable `Microsoft-Hyper-V` and `VirtualMachinePlatform`. |
| `HYPERVISOR_NOT_RUNNING` | Windows hypervisor not started | Uninstall old VirtualBox/VMware (< 6.1 / < 15.5), check `bcdedit /enum` (`hypervisorlaunchtype Auto`), do not run Monodon in a VM without nested virtualization. |
| `HOST_COMPUTE_SERVICE_UNAVAILABLE` | `vmcompute` or `hns` service stopped or missing | Reboot Windows; otherwise reinstall Monodon. |
| `BLOCKED_BY_SECURITY_SOFTWARE` | Antivirus / EDR blocks the disks or the service | Add `%ProgramData%\Monodon` and the installation folder to the exclusions. |
| `INSUFFICIENT_PRIVILEGES` | The service does not run with the expected rights | Reinstall Monodon (the service must run as LocalSystem). |
| `IMAGE_CORRUPTED` | Engine files missing or SHA-256 mismatch | Reinstall Monodon. |
| `DATA_DISK_ERROR` | `data.vhdx` cannot be created or opened | Check disk space and antivirus exclusions; as a last resort rename `%ProgramData%\Monodon\data.vhdx` (loses Docker data). |
| `VM_BOOT_TIMEOUT`, `AGENT_UNREACHABLE`, `ENGINE_UNREACHABLE` | The machine does not answer | Restart the engine; read `monodon-service.log`; report with the log. |
| No network from containers | Corporate VPN or IP range conflict | Monodon picks a free range among `172.30.0.0/24`… and sets the MTU to 1400; some VPNs (AnyConnect, GlobalProtect) still block virtual adapters: disable the VPN to test, then report. |
| "The Monodon service is not running" | Service stopped | `sc start MonodonService` as administrator, or reinstall. |

Power loss or hard shutdown: at the next start Monodon checks and repairs the data disk (`fsck`), then restarts
the engine. Unsynced writes of the last two seconds may be lost, as on any Linux machine.

## Known limitations (0.1)

- **Windows Home** is not supported (missing Hyper-V component).
- **UDP** published ports are not relayed to `localhost` (TCP only).
- **One engine per machine**, no multiple profiles.
- **Not signed** (SmartScreen warning); **no automatic update**: install the new version over the old one.
- A sleeping container does not run its internal scheduled tasks until something calls it; use "Keep awake"
  on its page if that matters.
- `curl.exe` on Windows rejects the local HTTPS certificates unless you pass `--ssl-no-revoke` (same as mkcert);
  browsers and .NET accept them.

## Documentation

- `ARCHITECTURE.md`: technical choices (HCS virtualization, Linux image, HvSocket, monodonfs, network),
  measured results block by block, risk register. In French.
- `docs/measurements.md`: every measurement and established fact. In French.
- `tests/e2e/`: end-to-end scenarios.
- `CONTRIBUTING.md`: how to contribute; `SECURITY.md`: how to report a vulnerability;
  `CODE_OF_CONDUCT.md`.

## Licence

Apache License 2.0, see `LICENSE` and `NOTICE`. Monodon bundles third-party software (Docker CLI, Docker
Compose, a Linux kernel, Alpine Linux packages, Rust and npm dependencies, the Urbanist font) under their own
licences: see `THIRD-PARTY.md`.
