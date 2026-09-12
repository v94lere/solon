// État du moteur partagé dans toute l'interface : abonnement unique au service, sans polling.
import { createContext, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { containers, dockerEvents, engine, type EngineSnapshot, type ServiceEvent } from "./api";
import { notify } from "./notifications";
import i18n from "./i18n";

export interface EngineContextValue {
  snapshot: EngineSnapshot | null;
  serviceAvailable: boolean;
  serviceDetail: string | null;
  /** Le moteur répond : les vues Docker peuvent charger leurs données. */
  ready: boolean;
}

const EngineContext = createContext<EngineContextValue>({ snapshot: null, serviceAvailable: true, serviceDetail: null, ready: false });

export function EngineProvider({ children }: { children: ReactNode }) {
  const [snapshot, setSnapshot] = useState<EngineSnapshot | null>(null);
  const [serviceAvailable, setServiceAvailable] = useState(true);
  const [serviceDetail, setServiceDetail] = useState<string | null>(null);
  const queryClient = useQueryClient();
  // État courant du moteur, lisible depuis les rappels du flux Docker (sans re-souscrire).
  const stateRef = useRef<string | null>(null);

  useEffect(() => {
    let alive = true;
    engine
      .status()
      .then((s) => {
        if (alive) {
          setSnapshot(s);
          setServiceAvailable(true);
        }
      })
      .catch((e: unknown) => {
        if (alive) {
          setServiceAvailable(false);
          setServiceDetail(String(e));
        }
      });
    const channel = engine.subscribe((ev: ServiceEvent) => {
      if (!alive) return;
      switch (ev.event) {
        case "state": {
          const { event: _e, ...snap } = ev;
          const next = snap as EngineSnapshot;
          stateRef.current = next.state;
          setSnapshot((prev) => {
            if (prev && prev.state !== next.state) {
              if (next.state === "failed") void notify(i18n.t("notify.engine_failed_title"), next.error?.message ?? i18n.t("notify.engine_failed_body"));
              else if (next.state === "degraded") void notify(i18n.t("notify.engine_degraded_title"), i18n.t("notify.engine_degraded_body"));
            }
            return next;
          });
          setServiceAvailable(true);
          break;
        }
        case "disk_pressure":
          void notify(i18n.t("notify.disk_title"), i18n.t("notify.disk_body", { pct: ev.used_pct, free: Math.round(ev.free_mb / 1024) }));
          break;
        case "resumed": {
          // Le moteur a relancé ce qui tournait : une seule notification, avec les noms.
          const names = ev.projects.length > 0 ? ev.projects.join(", ") : i18n.t("notify.resumed_containers", { count: ev.containers });
          void notify(i18n.t("notify.resumed_title"), i18n.t("notify.resumed_body", { names }));
          void queryClient.invalidateQueries({ queryKey: ["containers"] });
          break;
        }
        case "service_unavailable":
          setServiceAvailable(false);
          setServiceDetail(ev.detail);
          break;
        case "container":
          void queryClient.invalidateQueries({ queryKey: ["containers"] });
          break;
        case "ports":
          setSnapshot((s) => (s ? { ...s, published_ports: ev.bindings } : s));
          break;
        default:
          break;
      }
    });
    return () => {
      alive = false;
      channel.onmessage = () => {};
    };
  }, [queryClient]);

  const ready = serviceAvailable && (snapshot?.state === "ready" || snapshot?.state === "degraded");

  // Événements Docker : invalidation ciblée des listes.
  useEffect(() => {
    if (!ready) return;
    let streamId: number | null = null;
    let cancelled = false;
    dockerEvents
      .subscribe((ev) => {
        const key = ev.type === "container" ? "containers" : ev.type === "image" ? "images" : ev.type === "volume" ? "volumes" : ev.type === "network" ? "networks" : null;
        if (key) void queryClient.invalidateQueries({ queryKey: [key] });
        if (ev.type === "container" && ev.action === "destroy") void queryClient.invalidateQueries({ queryKey: ["container", ev.id] });
        // Un conteneur qui meurt avec un code non nul, sans action de l'utilisateur dans l'application, et
        // pas pendant un arrêt du moteur (tous les conteneurs s'arrêtent alors : ce serait du bruit).
        const engineUp = stateRef.current === "ready" || stateRef.current === "degraded";
        if (engineUp && ev.type === "container" && ev.action === "die" && ev.exit_code && ev.exit_code !== "0" && !recentlyActed(ev.id)) {
          void notify(i18n.t("notify.container_died_title"), i18n.t("notify.container_died_body", { name: ev.name || ev.id.slice(0, 12), code: ev.exit_code }), `die:${ev.id}`);
        }
      })
      .then((id) => {
        if (cancelled) void containers.streamClose(id);
        else streamId = id;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      if (streamId !== null) void containers.streamClose(streamId);
    };
  }, [ready, queryClient]);

  const value = useMemo(() => ({ snapshot, serviceAvailable, serviceDetail, ready }), [snapshot, serviceAvailable, serviceDetail, ready]);
  return <EngineContext.Provider value={value}>{children}</EngineContext.Provider>;
}

// Actions déclenchées par l'utilisateur dans l'application (arrêt, suppression…) : pas de notification
// pour la mort d'un conteneur qui en découle, pendant 15 s.
const userActions = new Map<string, number>();
export function markUserAction(containerId: string) {
  userActions.set(containerId, Date.now());
}
function recentlyActed(containerId: string) {
  const t = userActions.get(containerId);
  return t !== undefined && Date.now() - t < 15_000;
}

export function useEngine() {
  return useContext(EngineContext);
}
