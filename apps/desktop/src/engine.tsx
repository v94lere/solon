// État du moteur partagé dans toute l'interface : abonnement unique au service, sans polling.
import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { containers, dockerEvents, engine, type EngineSnapshot, type ServiceEvent } from "./api";

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
          setSnapshot(snap as EngineSnapshot);
          setServiceAvailable(true);
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

export function useEngine() {
  return useContext(EngineContext);
}
