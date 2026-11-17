import { useEffect, useState } from "react";
import { RetroLineChart } from "../../lib/retroChart";
import type { MacroHistoryPoint, TelemetryDeltas } from "../../types/api";

interface GdpChartProps {
  country: string;
  gdp: number;
  cpi: number;
  ppi: number;
  deltas: TelemetryDeltas;
  turn: number;
  /** Engine-side telemetry ring buffer for this country (authoritative). */
  backendHistory?: MacroHistoryPoint[];
}

interface HistoryPoint {
  turn: number;
  gdp: number;
  cpi: number;
  ppi: number;
}

const MAX_POINTS = 50;

/// History is persisted per-country (`gdp_history:<country>`) so switching the
/// selected country never carries another country's points into this series.
function storageKeyFor(country: string): string {
  return `gdp_history:${country}`;
}

function readHistory(country: string): HistoryPoint[] {
  try {
    const raw = localStorage.getItem(storageKeyFor(country));
    return raw ? (JSON.parse(raw) as HistoryPoint[]) : [];
  } catch {
    return [];
  }
}

export function GdpChart({ country, gdp, cpi, ppi, deltas, turn, backendHistory }: GdpChartProps) {
  const [history, setHistory] = useState<HistoryPoint[]>(() => readHistory(country));

  // Country switched or backend series refreshed: prefer the engine's own
  // telemetry history (authoritative, survives restarts); fall back to the
  // per-country localStorage series when the backend buffer is empty (e.g.
  // saves predating telemetry_history). Never keep the previous country's
  // points.
  useEffect(() => {
    const backend: HistoryPoint[] = (backendHistory ?? []).map((p) => ({
      turn: p.turn,
      gdp: p.gdp,
      cpi: p.cpi,
      ppi: p.ppi,
    }));
    setHistory(backend.length > 0 ? backend : readHistory(country));
  }, [country, backendHistory]);

  // Append the latest snapshot to the current country's series only. The
  // storage key is derived from this render's `country`, and the functional
  // update re-checks the latest state, so a late/duplicated run (e.g. rapid
  // country switching or StrictMode double-effects) cannot write one country's
  // point into another country's series.
  useEffect(() => {
    setHistory((prev) => {
      if (prev.some((h) => h.turn === turn)) return prev;
      const next = [...prev, { turn, gdp, cpi, ppi }];
      if (next.length > MAX_POINTS) next.shift();
      try {
        localStorage.setItem(storageKeyFor(country), JSON.stringify(next));
      } catch {
        // localStorage unavailable/full — keep the in-memory series only.
      }
      return next;
    });
  }, [country, turn, gdp, cpi, ppi]);

  const fmtB = (v: number) => (v / 1e9).toFixed(1) + "B";
  return (
    <RetroLineChart
      categories={history.map((h) => `T${h.turn}`)}
      height={320}
      toolbar
      checkboxes
      series={[
        { name: "GDP", data: history.map((h) => h.gdp), color: "#0000aa", yAxisIndex: 0 },
        { name: "CPI", data: history.map((h) => h.cpi), color: "#aa0000", yAxisIndex: 1 },
        { name: "PPI", data: history.map((h) => h.ppi), color: "#b86b00", yAxisIndex: 1 },
      ]}
      yAxes={[
        { name: "GDP", axisLabel: { color: "#000000", fontSize: 9, fontFamily: '"Courier New", monospace', formatter: fmtB } },
        { name: "Index", axisLabel: { color: "#800080", fontSize: 9, fontFamily: '"Courier New", monospace' } },
      ]}
    />
  );
}
