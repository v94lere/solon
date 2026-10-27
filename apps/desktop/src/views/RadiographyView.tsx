// Radiographie : ce que les sondes du noyau voient se passer dans la machine.
//
// Cet écran est la première marche d'une fonction plus large — la carte des échanges entre
// services, avec sa barre de lecture. Pour l'instant il ne montre qu'un type de fait, le
// lancement d'un processus : de quoi vérifier que la chaîne noyau → agent → service → interface
// tient debout avant d'y faire passer du trafic réseau.
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { trace, type TraceEvent } from "../api";
import { Avatar, EmptyState, IconPulse, PageHeader } from "../components/ui";

/** Faits gardés à l'écran. Le service en garde bien davantage ; inutile d'en afficher plus. */
const SHOWN = 500;

interface Fact {
  id: number;
  ts: number;
  comm: string;
  container: string | null;
  pid: number;
}

export function RadiographyView() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<{ active: boolean; detail: string } | null>(null);
  const [facts, setFacts] = useState<Fact[]>([]);
  const [filter, setFilter] = useState("");
  const [paused, setPaused] = useState(false);
  // Le flux continue d'arriver pendant une pause : on l'accumule ici plutôt que de le perdre.
  const held = useRef<Fact[]>([]);
  const pausedRef = useRef(paused);
  const nextId = useRef(0);

  useEffect(() => {
    pausedRef.current = paused;
    if (!paused && held.current.length > 0) {
      const pending = held.current;
      held.current = [];
      setFacts((prev) => [...pending.reverse(), ...prev].slice(0, SHOWN));
    }
  }, [paused]);

  useEffect(() => {
    let alive = true;
    const channel = trace.subscribe((ev: TraceEvent) => {
      if (!alive) return;
      if (ev.kind === "status") {
        setStatus({ active: ev.active, detail: ev.detail });
        return;
      }
      const fact: Fact = {
        id: nextId.current++,
        ts: ev.ts_ms,
        comm: ev.comm,
        container: ev.container_id ?? null,
        pid: ev.pid,
      };
      if (pausedRef.current) held.current.push(fact);
      else setFacts((prev) => [fact, ...prev].slice(0, SHOWN));
    });
    return () => {
      alive = false;
      channel.onmessage = () => {};
    };
  }, []);

  const rows = useMemo(() => {
    const f = filter.trim().toLowerCase();
    if (!f) return facts;
    return facts.filter((x) => x.comm.toLowerCase().includes(f) || (x.container ?? "").includes(f));
  }, [facts, filter]);

  async function clear() {
    held.current = [];
    setFacts([]);
    await trace.clear();
  }

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title={t("radiography.title")}
        count={t("radiography.count", { count: rows.length })}
        actions={
          <div className="flex gap-2">
            <button type="button" className="btn btn-sm" onClick={() => setPaused((p) => !p)}>
              {paused ? t("radiography.resume") : t("radiography.pause")}
            </button>
            <button type="button" className="btn btn-sm" onClick={() => void clear()}>{t("radiography.clear")}</button>
          </div>
        }
        search={<input type="search" className="input w-56" placeholder={t("radiography.search")} value={filter} onChange={(e) => setFilter(e.target.value)} aria-label={t("radiography.search")} />}
      />
      {status && !status.active && (
        <div className="mx-4 mb-2 rounded px-3 py-2" role="status" style={{ background: "var(--warn-soft)", color: "var(--warn)" }}>
          {t("radiography.inactive", { detail: status.detail })}
        </div>
      )}
      {paused && held.current.length > 0 && (
        <div className="mx-4 mb-2 rounded px-3 py-2 text-[13px]" style={{ background: "var(--surface-2)", color: "var(--ink-2)" }}>
          {t("radiography.held", { count: held.current.length })}
        </div>
      )}
      <div className="card list-card mx-4 mb-4 min-h-0 flex-1 overflow-auto">
        {rows.length === 0 ? (
          <EmptyState
            icon={<IconPulse />}
            title={status?.active ? t("radiography.empty") : t("radiography.waiting")}
            hint={status?.active ? t("radiography.empty_hint") : undefined}
          />
        ) : (
          <table className="w-full text-[13px]">
            <tbody>
              {rows.map((r) => (
                <tr key={r.id}>
                  <td className="w-24 px-3 py-1.5 tabular-nums" style={{ color: "var(--ink-3)" }}>{clock(r.ts)}</td>
                  <td className="w-44 px-3 py-1.5">
                    {r.container ? (
                      <span className="flex items-center gap-2">
                        <Avatar label={r.container} seed={r.container} size={20} />
                        <code>{r.container}</code>
                      </span>
                    ) : (
                      <span style={{ color: "var(--ink-3)" }}>{t("radiography.host")}</span>
                    )}
                  </td>
                  <td className="px-3 py-1.5"><code>{r.comm}</code></td>
                  <td className="w-20 px-3 py-1.5 tabular-nums" style={{ color: "var(--ink-3)" }}>{r.pid}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}

/** Heure à la seconde près : à cette échelle, la date n'apporte rien. */
function clock(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}
