// Couche d'accès au backend Tauri : commandes typées et canaux de flux.
import { Channel, invoke } from "@tauri-apps/api/core";

export type EngineState = "stopped" | "starting" | "ready" | "degraded" | "stopping" | "failed";
export type ProvisionStep =
  | "checking_prerequisites"
  | "verifying_image"
  | "preparing_data_disk"
  | "cleaning_orphans"
  | "creating_network"
  | "creating_machine"
  | "booting"
  | "waiting_agent"
  | "configuring_network"
  | "waiting_engine"
  | "ready";

export interface SolonError {
  code: string;
  message: string;
  hresult?: number;
}

export interface PortBinding {
  container_name?: string;
  compose_project?: string | null;
  compose_service?: string | null;
  container_id: string;
  protocol: string;
  host_ip: string;
  host_port: number;
  container_ip: string;
  container_port: number;
}

export interface EngineSnapshot {
  state: EngineState | null;
  step: ProvisionStep | null;
  error: SolonError | null;
  vm_id: string | null;
  image_version: string | null;
  docker_pipe: string | null;
  last_boot_ms: number | null;
  ready_since_unix_ms: number | null;
  guest_address: string | null;
  published_ports: PortBinding[];
  recovered_from_crash: boolean;
  local_domains?: boolean;
  local_domains_tls?: boolean;
  sleeping?: string[];
  reattached?: boolean;
}

export type ServiceEvent =
  | ({ event: "state" } & EngineSnapshot)
  | { event: "container"; action: string; id: string; name: string }
  | { event: "ports"; bindings: PortBinding[] }
  | { event: "log"; level: string; message: string }
  | { event: "disk_pressure"; used_pct: number; free_mb: number }
  | { event: "resumed"; projects: string[]; containers: number }
  | { event: "service_unavailable"; detail: string };

export interface PrereqItem {
  id: string;
  ok: boolean;
  blocking: boolean;
  detail: string;
  code: string | null;
}
export interface PrereqReport {
  ok: boolean;
  items: PrereqItem[];
}

export interface Settings {
  memory_mb: number;
  processors: number;
  data_disk_gib: number;
  autostart: boolean;
  legacy_file_sharing?: boolean;
  sleep_enabled?: boolean;
  sleep_idle_minutes?: number;
  sleep_never?: string[];
  /** Relancer au démarrage du moteur les projets et conteneurs qui tournaient à son arrêt. */
  resume_running?: boolean;
  /** Projets Compose (noms) démarrés à chaque démarrage du moteur. */
  autostart_projects?: string[];
}

// ---- moteur / service ----
export interface ExecResult {
  code: number | null;
  stdout: string;
  stderr: string;
  ms: number;
  timed_out?: boolean;
}

export interface MachineMetrics {
  uptime_s: number;
  cpu_busy_ticks: number;
  cpu_total_ticks: number;
  cpus: number;
  load1: number;
  mem_total_kb: number;
  mem_available_kb: number;
  disk_total_bytes: number;
  disk_used_bytes: number;
  net_rx_bytes: number;
  net_tx_bytes: number;
  containers_running: number;
}

export const engine = {
  status: () => invoke<EngineSnapshot>("engine_status"),
  metrics: () => invoke<MachineMetrics>("engine_metrics"),
  /** Commande shell dans la machine (sortie capturée). */
  exec: (command: string, timeoutS = 120) => invoke<ExecResult>("service_exec", { command, timeoutS }),
  start: () => invoke<void>("engine_start"),
  stop: (force = false) => invoke<void>("engine_stop", { force }),
  restart: () => invoke<void>("engine_restart"),
  prereq: () => invoke<PrereqReport>("prereq_report"),
  settingsGet: () => invoke<Settings>("settings_get"),
  settingsSet: (settings: Settings) => invoke<void>("settings_set", { settings }),
  logsDir: () => invoke<string>("paths_logs_dir"),
  subscribe: (onEvent: (e: ServiceEvent) => void) => {
    const channel = new Channel<ServiceEvent>();
    channel.onmessage = onEvent;
    void invoke("engine_subscribe", { channel });
    return channel;
  },
};

// ---- conteneurs ----
export interface ContainerPort {
  IP?: string;
  PrivatePort: number;
  PublicPort?: number;
  Type: string;
}
export interface ContainerSummary {
  Id: string;
  Names: string[];
  Image: string;
  ImageID: string;
  Command: string;
  Created: number;
  Ports: ContainerPort[];
  Labels: Record<string, string>;
  State: string;
  Status: string;
  Mounts?: { Type?: string; Name?: string; Source?: string; Destination?: string }[];
  NetworkSettings?: { Networks?: Record<string, unknown> };
}

export interface StatSample {
  id: string;
  cpu_percent: number;
  mem_usage: number;
  mem_limit: number;
  rx_bytes: number;
  tx_bytes: number;
}

export interface LogChunk {
  stream: "stdout" | "stderr" | "end" | "error";
  text: string;
}

export interface ExecOutput {
  kind: "data" | "end" | "error";
  /** base64 */
  data?: string;
  message?: string;
}

export const containers = {
  list: (all: boolean) => invoke<ContainerSummary[]>("containers_list", { all }),
  inspect: (id: string) => invoke<unknown>("container_inspect", { id }),
  copyFrom: (id: string, path: string, destDir: string) => invoke<string>("container_copy_from", { id, path, destDir }),
  copyTo: (id: string, source: string, dest: string) => invoke<void>("container_copy_to", { id, source, dest }),
  start: (id: string) => invoke<void>("container_start", { id }),
  rename: (id: string, name: string) => invoke<void>("container_rename", { id, name }),
  stop: (id: string) => invoke<void>("container_stop", { id }),
  restart: (id: string) => invoke<void>("container_restart", { id }),
  kill: (id: string) => invoke<void>("container_kill", { id }),
  remove: (id: string, force: boolean, volumes: boolean) => invoke<void>("container_remove", { id, force, volumes }),
  logsOpen: (id: string, tail: number, timestamps: boolean, onChunk: (c: LogChunk) => void) => {
    const channel = new Channel<LogChunk>();
    channel.onmessage = onChunk;
    return invoke<number>("logs_open", { id, tail, timestamps, channel }).then(trackStream);
  },
  statsOpen: (onSample: (s: StatSample) => void) => {
    const channel = new Channel<StatSample>();
    channel.onmessage = onSample;
    return invoke<number>("stats_open", { channel }).then(trackStream);
  },
  streamClose: (streamId: number) => {
    untrackStream(streamId);
    return invoke<void>("stream_close", { streamId });
  },
  execOpen: (id: string, cmd: string[], cols: number, rows: number, onOutput: (o: ExecOutput) => void) => {
    const channel = new Channel<ExecOutput>();
    channel.onmessage = onOutput;
    return invoke<number>("exec_open", { id, cmd, cols, rows, channel }).then(trackExec);
  },
  execInput: (execId: number, data: string) => invoke<void>("exec_input", { execId, data }),
  execResize: (execId: number, cols: number, rows: number) => invoke<void>("exec_resize", { execId, cols, rows }),
  execClose: (execId: number) => {
    untrackExec(execId);
    return invoke<void>("exec_close", { execId });
  },
};

export interface DockerEventInfo {
  action: string;
  type: string;
  id: string;
  name: string;
  exit_code?: string;
}
export const dockerEvents = {
  subscribe: (onEvent: (e: DockerEventInfo) => void) => {
    const channel = new Channel<DockerEventInfo>();
    channel.onmessage = onEvent;
    return invoke<number>("docker_events_open", { channel }).then(trackStream);
  },
};

// ---- images, volumes, réseaux ----
export interface ImageSummary {
  Id: string;
  RepoTags: string[] | null;
  Created: number;
  Size: number;
  Containers: number;
}
export interface RunSpec {
  image: string;
  name: string | null;
  cmd: string[] | null;
  env: string[];
  ports: { host: number; container: number; proto: string }[];
}
export interface ReclaimReport {
  images_removed: number;
  build_cache_removed: number;
  space_reclaimed: number;
}
export const images = {
  list: () => invoke<ImageSummary[]>("images_list"),
  /** Supprime les images inutilisées et le cache de construction ; conteneurs et volumes intacts. */
  reclaim: () => invoke<ReclaimReport>("docker_reclaim"),
  inspect: (id: string) => invoke<unknown>("image_inspect", { id }),
  remove: (id: string, force: boolean) => invoke<void>("image_remove", { id, force }),
  run: (spec: RunSpec) => invoke<string>("image_run", { spec }),
};

export interface Volume {
  Name: string;
  Driver: string;
  Mountpoint: string;
  CreatedAt?: string;
  Labels?: Record<string, string> | null;
}
export const volumes = {
  list: () => invoke<{ Volumes: Volume[] | null }>("volumes_list"),
  create: (name: string) => invoke<Volume>("volume_create", { name }),
  remove: (name: string, force: boolean) => invoke<void>("volume_remove", { name, force }),
  inspect: (name: string) => invoke<unknown>("volume_inspect", { name }),
};

export interface Network {
  Id: string;
  Name: string;
  Driver?: string;
  Scope?: string;
  IPAM?: { Config?: { Subnet?: string; Gateway?: string }[] | null } | null;
}
export const networks = {
  list: () => invoke<Network[]>("networks_list"),
  create: (name: string, driver?: string) => invoke<string>("network_create", { name, driver: driver ?? null }),
  remove: (id: string) => invoke<void>("network_remove", { id }),
  inspect: (id: string) => invoke<unknown>("network_inspect", { id }),
};

// ---- Diagnostic ----
export const diagnostic = {
  /** Écrit l'archive zip ; renvoie le nombre de fichiers inclus. */
  export: (dest: string) => invoke<number>("diagnostic_export", { dest }),
};

// ---- Terminal dans la machine ----
// ---- Fichiers (conteneur en marche ou volume) ----
export type FilesTarget = { kind: "container"; id: string } | { kind: "volume"; name: string };
export interface FileEntry {
  name: string;
  kind: "dir" | "file" | "link" | "other";
  size: number;
  mtime: number;
  mode: string;
}
export const files = {
  list: (target: FilesTarget, path: string) => invoke<FileEntry[]>("files_list", { target, path }),
  mkdir: (target: FilesTarget, path: string) => invoke<void>("files_mkdir", { target, path }),
  remove: (target: FilesTarget, path: string) => invoke<void>("files_delete", { target, path }),
  download: (target: FilesTarget, path: string, destDir: string) => invoke<string>("files_download", { target, path, destDir }),
  upload: (target: FilesTarget, destPath: string, source: string) => invoke<void>("files_upload", { target, destPath, source }),
};

// ---- Piles prêtes et détection de projet ----
export interface Probe {
  dir: string;
  name: string;
  has_compose: boolean;
  has_dockerfile: boolean;
  node_scripts: string[];
  node_framework: string | null;
  python_requirements: boolean;
  python_pyproject: boolean;
  python_entries: string[];
  php_composer: boolean;
  php_files: number;
  go_mod: boolean;
  cargo: boolean;
  java_maven: boolean;
  java_gradle: boolean;
  dotnet_projects: string[];
  ruby_gemfile: boolean;
  index_html: boolean;
  odoo_addons: string[];
}
export const stacks = {
  probe: (dir: string) => invoke<Probe>("stack_probe", { dir }),
  /** Écrit les fichiers dans `dir` (créé au besoin) sans écraser ; renvoie le dossier du projet. */
  scaffold: (dir: string, files: { path: string; content: string }[]) => invoke<string>("project_scaffold", { dir, files }),
};

export const machineShell = {
  /** `command` : lancée dans le pseudo-terminal à la place du shell interactif (`sh -lc`). */
  open: (cols: number, rows: number, onOutput: (o: ExecOutput) => void, command?: string) => {
    const channel = new Channel<ExecOutput>();
    channel.onmessage = onOutput;
    return invoke<number>("machine_shell_open", { cols, rows, command: command ?? null, channel });
  },
  input: (id: number, data: string) => invoke<void>("machine_shell_input", { id, data }),
  resize: (id: number, cols: number, rows: number) => invoke<void>("machine_shell_resize", { id, cols, rows }),
  close: (id: number) => invoke<void>("machine_shell_close", { id }),
};

export const system = {
  openInVsCode: (dir: string) => invoke<void>("open_in_vscode", { dir }),
};

// ---- Sauvegarde et restauration d'un projet (zip : fichiers Compose + volumes) ----
export interface BackupReport {
  path: string;
  volumes: number;
  bytes: number;
}
export interface BackupInfo {
  project: string;
  compose_file: string;
  created_unix_ms: number;
  solon_version: string;
  volumes: string[];
}
export interface RestoreReport {
  dir: string;
  project: string;
  volumes: number;
}
export const backup = {
  create: (dir: string, project: string, dest: string) => invoke<BackupReport>("project_backup", { dir, project, dest }),
  info: (zipPath: string) => invoke<BackupInfo>("project_backup_info", { zipPath }),
  restore: (zipPath: string, targetDir: string) => invoke<RestoreReport>("project_restore", { zipPath, targetDir }),
};

// ---- Le PC Windows : ports déjà pris, place disque ----
export interface PortProbe {
  port: number;
  in_use: boolean;
  suggestion: number | null;
}
export interface HostDiskInfo {
  data_disk_path: string;
  data_disk_bytes: number;
  drive: string;
  drive_total_bytes: number;
  drive_free_bytes: number;
}
export const host = {
  portsProbe: (ports: number[]) => invoke<PortProbe[]>("ports_probe", { ports }),
  diskInfo: () => invoke<HostDiskInfo>("host_disk_info"),
};

// ---- Compose ----
export interface ComposeProject {
  dir: string;
  file: string;
  name: string;
}
export interface ComposeResult {
  code: number | null;
  output: string;
  ms: number;
  guest_dir: string;
}
export interface ComposeChunk {
  kind: "stdout" | "stderr" | "exit" | "error";
  text: string;
}
export const compose = {
  detect: (dir: string) => invoke<ComposeProject | null>("compose_detect", { dir }),
  read: (dir: string) => invoke<string>("compose_read", { dir }),
  write: (dir: string, content: string) => invoke<void>("compose_write", { dir, content }),
  /** Fichier `.env` du projet (chaîne vide s'il n'existe pas). */
  envRead: (dir: string) => invoke<string>("env_read", { dir }),
  envWrite: (dir: string, content: string) => invoke<void>("env_write", { dir, content }),
  run: (dir: string, args: string[], timeoutS?: number) => invoke<ComposeResult>("compose_run", { dir, args, timeoutS: timeoutS ?? null }),
  /** Sortie en flux ; la promesse se résout avec le code de sortie. */
  stream: (dir: string, args: string[], onChunk: (c: ComposeChunk) => void) => {
    const channel = new Channel<ComposeChunk>();
    channel.onmessage = onChunk;
    return invoke<number>("compose_stream", { dir, args, channel });
  },
};

// Flux ouverts côté Rust : fermés si la page se recharge (sinon les tâches continueraient à
// envoyer vers des callbacks disparus).
const openStreams = new Set<number>();
const openExecs = new Set<number>();
export function trackStream(id: number) {
  openStreams.add(id);
  return id;
}
export function untrackStream(id: number) {
  openStreams.delete(id);
}
export function trackExec(id: number) {
  openExecs.add(id);
  return id;
}
export function untrackExec(id: number) {
  openExecs.delete(id);
}
window.addEventListener("beforeunload", () => {
  for (const id of openStreams) void invoke("stream_close", { streamId: id });
  for (const id of openExecs) void invoke("exec_close", { execId: id });
});

export function bytesToBase64(bytes: Uint8Array): string {
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin);
}

export function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v < 10 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}
