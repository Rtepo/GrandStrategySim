import { useEffect, useState } from "react";
import { EChart } from "./EChart";
import type { MacroHistoryPoint, TelemetryDeltas } from "../../types/api";
import type { EChartsOption } from "echarts";

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

  const option: EChartsOption = {
    backgroundColor: "transparent",
    tooltip: {
      trigger: "axis",
      valueFormatter: (v: unknown) => {
        const n = typeof v === "number" ? v : Number(v);
        if (!isFinite(n)) return String(v);
        if (Math.abs(n) >= 1e9) return (n / 1e9).toFixed(2) + "B";
        if (Math.abs(n) >= 1e6) return (n / 1e6).toFixed(2) + "M";
        if (Math.abs(n) >= 1e3) return (n / 1e3).toFixed(2) + "K";
        return n.toFixed(2);
      },
    },
    legend: {
      data: ["GDP", "CPI", "PPI"],
      textStyle: { color: "#94a3b8" },
      top: 0,
    },
    grid: { left: 60, right: 60, top: 40, bottom: 30 },
    xAxis: {
      type: "category",
      data: history.map((h) => `T${h.turn}`),
      axisLine: { lineStyle: { color: "#475569" } },
      axisLabel: { color: "#94a3b8" },
    },
    yAxis: [
      {
        type: "value",
        name: "GDP",
        position: "left",
        axisLine: { lineStyle: { color: "#475569" } },
        axisLabel: { color: "#94a3b8", formatter: (v: number) => (v / 1e9).toFixed(1) + "B" },
        splitLine: { lineStyle: { color: "#334155" } },
      },
      {
        type: "value",
        name: "Index",
        position: "right",
        axisLine: { lineStyle: { color: "#475569" } },
        axisLabel: { color: "#94a3b8" },
        splitLine: { show: false },
      },
    ],
    series: [
      {
        name: "GDP",
        type: "line",
        yAxisIndex: 0,
        data: history.map((h) => h.gdp),
        smooth: true,
        itemStyle: { color: "#3b82f6" },
        areaStyle: { opacity: 0.1 },
      },
      {
        name: "CPI",
        type: "line",
        yAxisIndex: 1,
        data: history.map((h) => h.cpi),
        smooth: true,
        itemStyle: { color: "#ef4444" },
      },
      {
        name: "PPI",
        type: "line",
        yAxisIndex: 1,
        data: history.map((h) => h.ppi),
        smooth: true,
        itemStyle: { color: "#eab308" },
      },
    ],
  };

  return <EChart option={option} style={{ minHeight: 320 }} />;
}
