// Mesures du moteur partagées (bloc « moteur » de la barre latérale) : relevé périodique des
// compteurs bruts, pourcentages calculés par différence entre deux relevés.
import { useEffect, useRef, useState } from "react";
import { engine, type MachineMetrics } from "./api";

export interface EngineLoad {
  cpuPct: number;
  memPct: number;
  memUsed: number;
  memTotal: number;
}

export function useEngineLoad(enabled: boolean, periodMs = 3000): EngineLoad | null {
  const [load, setLoad] = useState<EngineLoad | null>(null);
  const prev = useRef<{ m: MachineMetrics; at: number } | null>(null);
  useEffect(() => {
    if (!enabled) {
      setLoad(null);
      prev.current = null;
      return;
    }
    let cancelled = false;
    let timer: number | undefined;
    const tick = async () => {
      try {
        const m = await engine.metrics();
        if (cancelled) return;
        const at = performance.now();
        const p = prev.current;
        prev.current = { m, at };
        const memUsed = (m.mem_total_kb - m.mem_available_kb) * 1024;
        const memTotal = m.mem_total_kb * 1024;
        let cpuPct = 0;
        if (p) {
          const dTotal = m.cpu_total_ticks - p.m.cpu_total_ticks;
          const dBusy = m.cpu_busy_ticks - p.m.cpu_busy_ticks;
          cpuPct = dTotal > 0 ? Math.min(100, Math.max(0, (100 * dBusy) / dTotal)) : 0;
        }
        setLoad({ cpuPct, memPct: memTotal ? (100 * memUsed) / memTotal : 0, memUsed, memTotal });
      } catch {
        if (!cancelled) setLoad(null);
      }
      if (!cancelled) timer = window.setTimeout(() => void tick(), periodMs);
    };
    void tick();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [enabled, periodMs]);
  return load;
}
