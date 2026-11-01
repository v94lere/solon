// Historique des compteurs du moteur, tenu en dehors de l'écran Activité.
//
// Avant, la collecte vivait dans la vue : quitter l'écran effaçait tout, et les boutons « 10 min » et
// « 1 h » ne pouvaient rien montrer. Elle vit maintenant ici, pour que l'historique survive à la
// navigation, à trois conditions pour que ça ne coûte rien :
//
//   - deux cadences : 2 s quand on regarde l'écran Activité, 5 s le reste du temps ;
//   - rien du tout quand la fenêtre est masquée (Solon vit dans la barre des tâches : personne ne
//     regarde, rien ne tourne) ;
//   - seulement les compteurs **globaux** du moteur. Le détail par conteneur reste dans la vue : le
//     flux `stats` de Docker coûte du processeur par conteneur, et une heure de courbes pour vingt
//     conteneurs est chère à collecter et rarement regardée.
//
// Une heure à 5 s fait 720 points pour trois séries, soit quelques dizaines de kilo-octets.
// Au passage, le bloc « moteur » de la barre latérale relevait les mêmes compteurs pour son compte
// toutes les 3 s : il lit maintenant ce magasin, ce qui supprime une boucle de sondage en double.
import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { engine, type MachineMetrics } from "./api";
import type { Pt } from "./components/TimeChart";

/** Cadence quand l'écran Activité est ouvert. */
const PERIOD_WATCHING_MS = 2_000;
/** Cadence le reste du temps : la jauge du moteur en bas de la barre latérale la lit aussi, donc
 *  assez vive pour ne pas paraître figée, et deux fois moins d'appels que l'ancien écran Activité. */
const PERIOD_IDLE_MS = 5_000;
/** Historique conservé : la plus grande fenêtre affichable, plus un peu de marge. */
export const RETENTION_MS = 3_600_000 + 20_000;

export interface EngineNow {
  cpuPct: number;
  memUsed: number;
  memTotal: number;
  diskUsed: number;
  diskTotal: number;
  rxRate: number;
  txRate: number;
  load1: number;
  cpus: number;
}

export interface History {
  cpu: Pt[];
  mem: Pt[];
  net: Pt[];
}

export const emptyHistory = (): History => ({ cpu: [], mem: [], net: [] });

/** Ajoute un point et oublie ce qui dépasse la rétention. */
export function push(list: Pt[], p: Pt): Pt[] {
  const cutoff = p.t - RETENTION_MS;
  let i = 0;
  while (i < list.length && list[i].t < cutoff) i++;
  const next = i > 0 ? list.slice(i) : list.slice();
  next.push(p);
  return next;
}

interface MetricsValue {
  now: EngineNow | null;
  history: History;
  unavailable: string | null;
  /** À appeler par l'écran Activité tant qu'il est affiché : passe la collecte en cadence rapide. */
  watch: () => () => void;
}

const MetricsContext = createContext<MetricsValue>({
  now: null,
  history: emptyHistory(),
  unavailable: null,
  watch: () => () => {},
});

export function MetricsProvider({ children }: { children: ReactNode }) {
  const [now, setNow] = useState<EngineNow | null>(null);
  const [history, setHistory] = useState<History>(emptyHistory);
  const [unavailable, setUnavailable] = useState<string | null>(null);
  // Nombre d'écrans qui regardent en ce moment : au-delà de zéro, cadence rapide.
  const [watchers, setWatchers] = useState(0);
  const [visible, setVisible] = useState(() => !document.hidden);
  const prev = useRef<{ m: MachineMetrics; at: number } | null>(null);

  // Fenêtre masquée (réduite dans la barre des tâches, ou cachée) : on ne mesure plus rien.
  useEffect(() => {
    const onChange = () => setVisible(!document.hidden);
    document.addEventListener("visibilitychange", onChange);
    return () => document.removeEventListener("visibilitychange", onChange);
  }, []);

  const watch = useCallback(() => {
    setWatchers((n) => n + 1);
    return () => setWatchers((n) => Math.max(0, n - 1));
  }, []);

  useEffect(() => {
    if (!visible) {
      // Le prochain relevé ne doit pas être comparé à un relevé d'il y a dix minutes.
      prev.current = null;
      return;
    }
    const period = watchers > 0 ? PERIOD_WATCHING_MS : PERIOD_IDLE_MS;
    let cancelled = false;
    let timer: number | undefined;
    const tick = async () => {
      try {
        const m = await engine.metrics();
        if (cancelled) return;
        setUnavailable(null);
        const at = performance.now();
        const before = prev.current;
        prev.current = { m, at };
        const memUsed = (m.mem_total_kb - m.mem_available_kb) * 1024;
        let cpuPct = 0;
        let rxRate = 0;
        let txRate = 0;
        if (before) {
          const dt = Math.max((at - before.at) / 1000, 0.1);
          const dTotal = m.cpu_total_ticks - before.m.cpu_total_ticks;
          const dBusy = m.cpu_busy_ticks - before.m.cpu_busy_ticks;
          cpuPct = dTotal > 0 ? Math.min(100, Math.max(0, (100 * dBusy) / dTotal)) : 0;
          rxRate = Math.max(0, m.net_rx_bytes - before.m.net_rx_bytes) / dt;
          txRate = Math.max(0, m.net_tx_bytes - before.m.net_tx_bytes) / dt;
          const ts = Date.now();
          // Le processeur du moteur est exprimé comme celui des conteneurs : 100 % = un cœur entier.
          setHistory((h) => ({
            cpu: push(h.cpu, { t: ts, v: cpuPct * Math.max(1, m.cpus) }),
            mem: push(h.mem, { t: ts, v: memUsed }),
            net: push(h.net, { t: ts, v: rxRate + txRate }),
          }));
        }
        setNow({
          cpuPct,
          memUsed,
          memTotal: m.mem_total_kb * 1024,
          diskUsed: m.disk_used_bytes,
          diskTotal: m.disk_total_bytes,
          rxRate,
          txRate,
          load1: m.load1,
          cpus: m.cpus,
        });
      } catch (e) {
        // Moteur arrêté : on le dit, on vide le relevé courant, et on garde l'historique passé.
        if (!cancelled) {
          setUnavailable(String(e));
          setNow(null);
          prev.current = null;
        }
      }
      if (!cancelled) timer = window.setTimeout(() => void tick(), period);
    };
    void tick();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [visible, watchers]);

  return (
    <MetricsContext.Provider value={{ now, history, unavailable, watch }}>
      {children}
    </MetricsContext.Provider>
  );
}

export function useMetrics() {
  return useContext(MetricsContext);
}

export interface EngineLoad {
  cpuPct: number;
  memPct: number;
  memUsed: number;
  memTotal: number;
}

/** Charge du moteur pour le bloc de la barre latérale. Lit le même magasin que l'écran Activité :
 *  avant, ce bloc relevait les compteurs pour son compte toutes les 3 s, en double. */
export function useEngineLoad(enabled: boolean): EngineLoad | null {
  const { now } = useMetrics();
  if (!enabled || !now) return null;
  return {
    cpuPct: now.cpuPct,
    memPct: now.memTotal ? (100 * now.memUsed) / now.memTotal : 0,
    memUsed: now.memUsed,
    memTotal: now.memTotal,
  };
}

/** À utiliser dans un écran qui affiche les courbes : cadence rapide tant qu'il est monté. */
export function useWatchMetrics() {
  const { watch } = useMetrics();
  useEffect(() => watch(), [watch]);
}
