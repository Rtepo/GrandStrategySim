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
 *
 * Features:
 * - Renders a compact sparkline from historical values.
 * - Tooltip on hover shows the value at each point.
 * - Double-click to pin/unpin the chart (keeps it visible).
 * - Pin icon indicates pinned state.
 */
export function Sparkline({
  data,
  labels,
  color = "#3b82f6",
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

    chart.setOption({
      grid: { left: 4, right: 4, top: 4, bottom: 4 },
      xAxis: {
        type: "category",
        show: false,
        data: xData,
      },
      yAxis: {
        type: "value",
        show: false,
      },
      tooltip: {
        trigger: "axis",
        formatter: (params: any) => {
          const p = params[0];
          if (!p) return "";
          const val = typeof p.value === "number" ? p.value.toFixed(2) : p.value;
          return `${label}: ${unit}${val}`;
        },
        backgroundColor: "rgba(0,0,0,0.8)",
        borderColor: color,
        textStyle: { color: "#fff", fontSize: 11 },
      },
      series: [
        {
          type: "line",
          data,
          smooth: true,
          symbol: "none",
          lineStyle: { color, width: 1.5 },
          areaStyle: {
            color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
              { offset: 0, color: color + "40" },
              { offset: 1, color: color + "05" },
            ]),
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
    <div className="relative inline-block" onDoubleClick={handleDoubleClick}>
      <div
        ref={chartRef}
        style={{ width: `${width}px`, height: `${height}px` }}
        className="cursor-pointer"
      />
      {pinned && (
        <div className="absolute top-0 right-0 flex items-center gap-1 bg-primary/10 rounded-bl px-1 py-0.5">
          <Pin size={10} className="text-primary" />
        </div>
      )}
      {!pinned && data.length > 0 && (
        <div className="absolute top-0 right-0 flex items-center gap-1 bg-muted/30 rounded-bl px-1 py-0.5 opacity-0 hover:opacity-100 transition-opacity">
          <PinOff size={10} className="text-muted-foreground" />
        </div>
      )}
    </div>
  );
}
