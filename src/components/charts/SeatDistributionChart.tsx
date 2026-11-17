import { useMemo } from "react";
import { EChart } from "./EChart";
import type { EChartsOption } from "echarts";
import { retroPieSeries, retroTooltip, DotLeaderLegend, RETRO_SERIES_COLORS } from "../../lib/retroChart";

interface SeatDistributionChartProps {
  data: Array<[string, number]>;
  title?: string;
}

/// v4.34.0 (TASK-042): solid flat pie — no hollow donut ring, white
/// radial slice borders, dark outer rim, dot-leader legend.
export function SeatDistributionChart({ data, title }: SeatDistributionChartProps) {
  const { option, entries } = useMemo(() => {
    const total = data.reduce((s, [, v]) => s + v, 0);
    const entries = data.map(([name, value], i) => ({
      name,
      value,
      color: RETRO_SERIES_COLORS[i % RETRO_SERIES_COLORS.length],
      share: total > 0 ? (value / total) * 100 : 0,
    }));
    const option: EChartsOption = {
      backgroundColor: "transparent",
      tooltip: retroTooltip({
        trigger: "item",
        formatter: "{b}: {c} seats ({d}%)",
      }) as never,
      series: retroPieSeries(
        entries.map((e) => ({ name: e.name, value: e.value, color: e.color })),
        {
          center: ["50%", "50%"],
          label: { show: true, color: "#000000", fontSize: 10, formatter: "{d}%" },
        },
      ),
    };
    return { option, entries };
  }, [data, title]);

  return (
    <div className="pinstripe-inset p-2">
      <div className="flex items-center gap-3">
        <div className="flex-1 min-w-0">
          <EChart option={option} style={{ minHeight: 300 }} />
        </div>
        <DotLeaderLegend
          entries={entries.map((e) => ({
            name: e.name,
            value: `${e.value} (${e.share.toFixed(1)}%)`,
            color: e.color,
          }))}
        />
      </div>
    </div>
  );
}
