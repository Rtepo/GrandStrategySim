import { useEffect, useRef, useState } from "react";
import * as echarts from "echarts";
import { Pin, PinOff } from "lucide-react";

interface SparklineProps {
  /** Historical values to plot. */
  data: number[];
  /** Turn labels for the x-axis. */
  labels?: number[];
  /** Color of the line. */
  color?: string;
  /** Label for the metric (shown in tooltip). */
  label?: string;
  /** Unit suffix for values (e.g., "$", "B"). */
  unit?: string;
  /** Width in pixels. */
  width?: number;
  /** Height in pixels. */
  height?: number;
}

/**
 * Phase 54: Compact ECharts line chart for historical banking data.
 * v4.34.0 (TASK-041): sunken #f4f4f0 terminal mini-plot — sharp 1.5px
 * piecewise polyline with vertex dots, dotted zero/mean reference line,
 * no gradient glow, no spline smoothing.
 */
export function Sparkline({
  data,
  labels,
  color = "#0000aa",
  label = "Value",
  unit = "",
  width = 200,
  height = 60,
}: SparklineProps) {
  const chartRef = useRef<HTMLDivElement>(null);
  const chartInstance = useRef<echarts.ECharts | null>(null);
  const [pinned, setPinned] = useState(false);

  useEffect(() => {
    if (!chartRef.current) return;
    if (chartInstance.current) {
      chartInstance.current.dispose();
    }
    const chart = echarts.init(chartRef.current);
    chartInstance.current = chart;

    const xData = labels && labels.length === data.length
      ? labels.map(String)
      : data.map((_, i) => String(i));
    const mean = data.length > 0 ? data.reduce((a, b) => a + b, 0) / data.length : 0;

    chart.setOption({
      backgroundColor: "#f4f4f0",
      grid: { left: 4, right: 4, top: 4, bottom: 4 },
      xAxis: {
        type: "category",
        show: false,
        data: xData,
      },
      yAxis: {
        type: "value",
        show: false,
        min: (v: { min: number; max: number }) => Math.min(v.min, mean),
        max: (v: { min: number; max: number }) => Math.max(v.max, mean),
      },
      tooltip: {
        trigger: "axis",
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        formatter: (params: any) => {
          const p = params[0];
          if (!p) return "";
          const val = typeof p.value === "number" ? p.value.toFixed(2) : p.value;
          return `${label}: ${unit}${val}`;
        },
        backgroundColor: "#ffffe1",
        borderColor: "#000000",
        textStyle: { color: "#000000", fontSize: 11, fontFamily: '"Courier New", monospace' },
        extraCssText: "border-radius:0;box-shadow:none;",
      },
      series: [
        {
          type: "line",
          data,
          smooth: false,
          symbol: "circle",
          symbolSize: 3,
          lineStyle: { color, width: 1.5 },
          itemStyle: { color },
          markLine: {
            silent: true,
            symbol: "none",
            lineStyle: { color: "#808080", width: 1, type: "dashed" },
            data: [{ yAxis: mean }],
            label: { show: false },
          },
        },
      ],
    });

    const handleResize = () => chart.resize();
    window.addEventListener("resize", handleResize);

    return () => {
      window.removeEventListener("resize", handleResize);
      chart.dispose();
      chartInstance.current = null;
    };
  }, [data, labels, color, label, unit]);

  const handleDoubleClick = () => {
    setPinned((p) => !p);
  };

  return (
    <div className="relative inline-block retro-spark-frame" onDoubleClick={handleDoubleClick}>
      <div
        ref={chartRef}
        style={{ width: `${width}px`, height: `${height}px` }}
        className="cursor-pointer"
      />
      {pinned && (
        <div className="absolute top-0 right-0 flex items-center gap-1 bg-primary/10   px-1 py-0.5">
          <Pin size={10} className="text-primary" />
        </div>
      )}
      {!pinned && data.length > 0 && (
        <div className="absolute top-0 right-0 flex items-center gap-1 bg-muted/30   px-1 py-0.5 opacity-0 hover:opacity-100 transition-opacity">
          <PinOff size={10} className="text-muted-foreground" />
        </div>
      )}
    </div>
  );
}
