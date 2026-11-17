/**
 * v4.34.0 (TASK-041/042) — shared "Institutional Win95 Terminal" ECharts
 * theme. Every line/bar/pie in the app renders on a crisp white sunken
 * canvas with black axes, dotted grey gridlines, sharp piecewise-linear
 * polylines carrying geometric vertex markers, and 16-color terminal hues.
 * Zero spline smoothing, zero translucent gradient fills, zero dark
 * backgrounds.
 */
import { useMemo, useState } from "react";
import { EChart } from "../components/charts/EChart";
import type { EChartsOption, SeriesOption } from "echarts";

/** Classic 16-color terminal series hues (draw order). */
export const RETRO_SERIES_COLORS = [
  "#0000aa", // navy
  "#aa0000", // crimson
  "#800080", // purple
  "#008000", // forest green
  "#b86b00", // dark amber
  "#008080", // teal
  "#aa5500", // burnt orange
  "#5555ff", // violet blue
  "#805520", // umber
  "#aa00aa", // magenta
  "#555555", // dark grey
  "#000000", // black
];

/** ECharts built-in marker shapes — 5px glyphs at every data vertex. */
export const RETRO_SYMBOLS = [
  "rect",
  "triangle",
  "diamond",
  "circle",
  "pin",
  "arrow",
] as const;

const MONO = '"Courier New", monospace';
const SANS = '"MS Sans Serif", Tahoma, Arial, sans-serif';

/** Black 1px axes + dotted grey gridlines shared by every terminal chart. */
export function retroXAxis(extra: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    type: "category",
    axisLine: { lineStyle: { color: "#000000", width: 1 } },
    axisLabel: { color: "#000000", fontSize: 9, fontFamily: MONO },
    axisTick: { lineStyle: { color: "#000000" } },
    splitLine: { show: true, lineStyle: { color: "#c0c0c0", type: "dashed", width: 1 } },
    ...extra,
  };
}

export function retroYAxis(extra: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    type: "value",
    axisLine: { show: true, lineStyle: { color: "#000000", width: 1 } },
    axisLabel: { color: "#000000", fontSize: 9, fontFamily: MONO },
    axisTick: { lineStyle: { color: "#000000" } },
    splitLine: { show: true, lineStyle: { color: "#c0c0c0", type: "dashed", width: 1 } },
    ...extra,
  };
}

export function retroTooltip(extra: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    trigger: "axis",
    backgroundColor: "#ffffe1",
    borderColor: "#000000",
    borderWidth: 1,
    textStyle: { color: "#000000", fontSize: 11, fontFamily: MONO },
    extraCssText: "border-radius:0;box-shadow:none;",
    ...extra,
  };
}

/** Sharp piecewise-linear series with a geometric marker per vertex. */
export function retroLine(
  name: string,
  data: (number | null | [number, number])[],
  seriesIndex: number,
  extra: Record<string, unknown> = {},
): Record<string, unknown> {
  const color = RETRO_SERIES_COLORS[seriesIndex % RETRO_SERIES_COLORS.length];
  return {
    name,
    type: "line",
    data,
    smooth: false,
    symbol: RETRO_SYMBOLS[seriesIndex % RETRO_SYMBOLS.length],
    symbolSize: 5,
    lineStyle: { color, width: 2, type: "solid" },
    itemStyle: { color },
    ...extra,
  };
}

/** Flat grey volume/fiscal sub-strip bars (bottom strip). */
export function retroVolumeBars(
  name: string,
  data: number[],
  extra: Record<string, unknown> = {},
): Record<string, unknown> {
  return {
    name,
    type: "bar",
    data,
    barMaxWidth: 10,
    itemStyle: { color: "#808080", borderColor: "#000000", borderWidth: 0.5 },
    ...extra,
  };
}

/** Solid flat pie — crisp white radial slice borders + dark outer rim. */
export function retroPieSeries(
  data: { name: string; value: number; color?: string }[],
  extra: Record<string, unknown> = {},
): SeriesOption[] {
  return [
    {
      type: "pie",
      radius: "66%",
      center: ["50%", "50%"],
      avoidLabelOverlap: true,
      itemStyle: { borderColor: "#ffffff", borderWidth: 1.5 },
      label: {
        show: true,
        formatter: "{d}%",
        color: "#000000",
        fontSize: 10,
        fontFamily: MONO,
      },
      labelLine: { lineStyle: { color: "#000000" }, length: 8, length2: 6 },
      data: data.map((d) => ({
        name: d.name,
        value: d.value,
        itemStyle: d.color ? { color: d.color } : undefined,
      })),
      ...extra,
    } as SeriesOption,
    // 1px dark outer rim — an invisible full-disk slice ring.
    {
      type: "pie",
      radius: "66%",
      center: ["50%", "50%"],
      silent: true,
      z: 1,
      itemStyle: { borderColor: "#222222", borderWidth: 1 },
      label: { show: false },
      labelLine: { show: false },
      data: [{ name: "", value: 1, itemStyle: { color: "rgba(0,0,0,0)" } }],
    } as SeriesOption,
  ];
}

/** "Name:........24 (48%)" — 90s monospace dot-leader legend. */
const DOTS = "................................"; // 32 leader dots
export function dotLeader(name: string, value: string, width = 26): string {
  const body = `${name}:`.slice(0, width);
  const pad = Math.max(3, width - body.length + 1);
  return `${body}${DOTS.slice(0, pad)}${value}`;
}

export function DotLeaderLegend({
  entries,
  emblem = true,
}: {
  entries: { name: string; value: string; color: string; note?: string }[];
  emblem?: boolean;
}) {
  const EMBLEMS = ["■", "◆", "★", "✚", "●", "▲"];
  return (
    <div className="dot-leader-legend">
      {entries.map((e, i) => (
        <div key={e.name}>
          {emblem && (
            <span className="emblem" style={{ color: e.color }}>
              {EMBLEMS[i % EMBLEMS.length]}
            </span>
          )}
          {dotLeader(e.name, e.value)}
          {e.note ? <span style={{ color: e.color }}> {e.note}</span> : null}
        </div>
      ))}
    </div>
  );
}

export interface RetroLineSeriesSpec {
  name: string;
  data: (number | null)[];
  /** Overrides the cycling terminal palette. */
  color?: string;
  yAxisIndex?: number;
  dashed?: boolean;
}

interface RetroLineChartProps {
  categories: string[];
  series: RetroLineSeriesSpec[];
  height?: number;
  /** Turn-window toolbar ([6T][12T][24T][ALL]) + zoom + checkboxes. */
  toolbar?: boolean;
  /** Show HTML checkbox legend above the plot. */
  checkboxes?: boolean;
  yFormatter?: (v: number) => string;
  yAxes?: [Record<string, unknown>, Record<string, unknown>];
  /** Optional bottom grey volume sub-strip (parallel to categories). */
  volume?: number[];
  volumeName?: string;
}

/**
 * Full institutional line chart: sunken white frame, optional [6T/12T/24T/
 * ALL] window buttons + zoom + reset, series checkboxes, dotted grid.
 */
export function RetroLineChart({
  categories,
  series,
  height = 260,
  toolbar = true,
  checkboxes = true,
  yFormatter,
  yAxes,
  volume,
  volumeName = "Volume",
}: RetroLineChartProps) {
  const [window_, setWindow_] = useState<number | "all">("all");
  const [hidden, setHidden] = useState<Set<string>>(new Set());

  const sliced = useMemo(() => {
    const n = window_ === "all" ? categories.length : Math.min(window_, categories.length);
    const off = categories.length - n;
    return {
      cats: categories.slice(off),
      off,
    };
  }, [categories, window_]);

  const visible = series.filter((s) => !hidden.has(s.name));
  const zoomStep = (dir: 1 | -1) => {
    const steps = ["all", 24, 12, 6] as const;
    const idx = window_ === "all" ? 0 : steps.indexOf(window_ as never);
    const next = Math.max(0, Math.min(steps.length - 1, idx + dir));
    setWindow_(steps[next] as number | "all");
  };

  const option: EChartsOption = useMemo(() => {
    const volGrid = volume && volume.length > 0;
    const lines = visible.map((s, i) => {
      const base = retroLine(
        s.name,
        s.data.slice(sliced.off),
        i,
        s.yAxisIndex ? { yAxisIndex: s.yAxisIndex } : {},
      );
      if (s.color) {
        (base.lineStyle as { color: string }).color = s.color;
        (base.itemStyle as { color: string }).color = s.color;
      }
      if (s.dashed) (base.lineStyle as { type: string }).type = "dashed";
      return base;
    });
    const grids: Record<string, unknown>[] = [
      { left: 62, right: yAxes ? 62 : 14, top: 14, bottom: volGrid ? 72 : 30 },
    ];
    const xAxes: Record<string, unknown>[] = [
      retroXAxis({ data: sliced.cats, gridIndex: 0 }),
    ];
    const yAxesArr: Record<string, unknown>[] = yAxes
      ? [
          retroYAxis(yAxes[0]),
          retroYAxis({ ...yAxes[1], position: "right", splitLine: { show: false } }),
        ]
      : [retroYAxis(yFormatter ? { axisLabel: { color: "#000000", fontSize: 9, fontFamily: MONO, formatter: yFormatter } } : {})];
    if (volGrid) {
      grids.push({ left: 62, right: yAxes ? 62 : 14, top: "78%", bottom: 26 });
      xAxes.push(
        retroXAxis({
          data: sliced.cats,
          gridIndex: 1,
          axisLabel: { show: false },
          axisTick: { show: false },
          splitLine: { show: false },
        }),
      );
      yAxesArr.push(
        retroYAxis({ gridIndex: 1, splitLine: { show: false }, axisLabel: { color: "#555555", fontSize: 8, fontFamily: MONO } }),
      );
      lines.push(
        retroVolumeBars(volumeName, volume!.slice(sliced.off), {
          xAxisIndex: 1,
          yAxisIndex: 1,
        }),
      );
    }
    return {
      backgroundColor: "#ffffff",
      tooltip: retroTooltip() as never,
      grid: grids,
      xAxis: xAxes,
      yAxis: yAxesArr,
      series: lines,
      textStyle: { fontFamily: SANS },
    } as EChartsOption;
  }, [visible, sliced, yAxes, yFormatter, volume, volumeName]);

  const btn = (label: string, active: boolean, onClick: () => void, title?: string) => (
    <button
      key={label}
      type="button"
      title={title}
      onClick={onClick}
      style={active ? { borderStyle: "inset" } : undefined}
      className={active ? "win95-inset-grey" : undefined}
    >
      {label}
    </button>
  );

  return (
    <div>
      {toolbar && (
        <div className="retro-chart-toolbar mb-1">
          <span className="toolbar-group">
            {btn("[ 6T ]", window_ === 6, () => setWindow_(6))}
            {btn("[ 12T ]", window_ === 12, () => setWindow_(12))}
            {btn("[ 24T ]", window_ === 24, () => setWindow_(24))}
            {btn("[ ALL ]", window_ === "all", () => setWindow_("all"))}
          </span>
          <span className="toolbar-group">
            {btn("[ Zoom In (+) ]", false, () => zoomStep(1), "Narrow the window")}
            {btn("[ Zoom Out (-) ]", false, () => zoomStep(-1), "Widen the window")}
            {btn("[ Reset View ]", false, () => setWindow_("all"))}
          </span>
        </div>
      )}
      {checkboxes && series.length > 1 && (
        <div className="retro-legend mb-1">
          {series.map((s, i) => (
            <label key={s.name}>
              <input
                type="checkbox"
                checked={!hidden.has(s.name)}
                onChange={() =>
                  setHidden((h) => {
                    const n = new Set(h);
                    if (n.has(s.name)) n.delete(s.name);
                    else n.add(s.name);
                    return n;
                  })
                }
              />
              <span style={{ color: s.color ?? RETRO_SERIES_COLORS[i % RETRO_SERIES_COLORS.length] }}>
                {["■", "▲", "◆", "●", "▌", "▶"][i % 6]}
              </span>
              {s.name}
            </label>
          ))}
        </div>
      )}
      <div className="retro-chart-frame p-1">
        <EChart option={option} style={{ height }} />
      </div>
    </div>
  );
}
