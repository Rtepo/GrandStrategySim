import { EChart } from "./EChart";
import type { TelemetryDeltas } from "../../types/api";
import type { EChartsOption } from "echarts";

interface GdpChartProps {
  gdp: number;
  cpi: number;
  ppi: number;
  deltas: TelemetryDeltas;
  turn: number;
}

export function GdpChart({ gdp, cpi, ppi, deltas, turn }: GdpChartProps) {
  const history = JSON.parse(localStorage.getItem("gdp_history") || "[]") as Array<{
    turn: number;
    gdp: number;
    cpi: number;
    ppi: number;
  }>;

  const existing = history.find((h) => h.turn === turn);
  if (!existing) {
    history.push({ turn, gdp, cpi, ppi });
    if (history.length > 50) history.shift();
    localStorage.setItem("gdp_history", JSON.stringify(history));
  }

  const option: EChartsOption = {
    backgroundColor: "transparent",
    tooltip: { trigger: "axis" },
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
