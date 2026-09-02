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
  reattached?: boolean;
}

export type ServiceEvent =
  | ({ event: "state" } & EngineSnapshot)
  | { event: "container"; action: string; id: string; name: string }
  | { event: "ports"; bindings: PortBinding[] }
  | { event: "log"; level: string; message: string }
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
}

// ---- moteur / service ----
export const engine = {
  status: () => invoke<EngineSnapshot>("engine_status"),
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
  start: (id: string) => invoke<void>("container_start", { id }),
  stop: (id: string) => invoke<void>("container_stop", { id }),
  restart: (id: string) => invoke<void>("container_restart", { id }),
  kill: (id: string) => invoke<void>("container_kill", { id }),
  remove: (id: string, force: boolean, volumes: boolean) => invoke<void>("container_remove", { id, force, volumes }),
  logsOpen: (id: string, tail: number, timestamps: boolean, onChunk: (c: LogChunk) => void) => {
    const channel = new Channel<LogChunk>();
    channel.onmessage = onChunk;
    return invoke<number>("logs_open", { id, tail, timestamps, channel });
  },
  statsOpen: (onSample: (s: StatSample) => void) => {
    const channel = new Channel<StatSample>();
    channel.onmessage = onSample;
    return invoke<number>("stats_open", { channel });
  },
  streamClose: (streamId: number) => invoke<void>("stream_close", { streamId }),
  execOpen: (id: string, cmd: string[], cols: number, rows: number, onOutput: (o: ExecOutput) => void) => {
    const channel = new Channel<ExecOutput>();
    channel.onmessage = onOutput;
    return invoke<number>("exec_open", { id, cmd, cols, rows, channel });
  },
  execInput: (execId: number, data: string) => invoke<void>("exec_input", { execId, data }),
  execResize: (execId: number, cols: number, rows: number) => invoke<void>("exec_resize", { execId, cols, rows }),
  execClose: (execId: number) => invoke<void>("exec_close", { execId }),
};

export const dockerEvents = {
  subscribe: (onEvent: (e: { action: string; type: string; id: string; name: string }) => void) => {
    const channel = new Channel<{ action: string; type: string; id: string; name: string }>();
    channel.onmessage = onEvent;
    return invoke<number>("docker_events_open", { channel });
  },
};

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
