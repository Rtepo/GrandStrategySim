import { useMemo } from "react";
import { EChart } from "./EChart";
import { fmt } from "../../lib/format";
import { retroPieSeries, retroTooltip, DotLeaderLegend } from "../../lib/retroChart";

interface GdpDonutChartProps {
  consumption: number;
  imputedConsumption: number;
  investment: number;
  governmentSpending: number;
  netExports: number;
  height?: number;
}

/// v4.22 WS4: GDP expenditure pie — C + imputed-C + I + G + NX legs.
/// v4.34.0 (TASK-042): solid flat pie (no hollow donut), white radial
/// slice borders, dark outer rim, pinstripe inset + dot-leader legend.
/// Net exports can be negative; the slice shows |NX| with a signed label.
export function GdpDonutChart({
  consumption,
  imputedConsumption,
  investment,
  governmentSpending,
  netExports,
  height = 320,
}: GdpDonutChartProps) {
  const { option, legs, total } = useMemo(() => {
    const legs = [
      { name: "Consumption", value: consumption + imputedConsumption, color: "#0000aa" },
      { name: "Investment", value: investment, color: "#008000" },
      { name: "Gov. Spending", value: governmentSpending, color: "#b86b00" },
      {
        name: `Net Exports (${netExports >= 0 ? "+" : ""}${fmt(netExports)})`,
        value: Math.abs(netExports),
        color: "#800080",
      },
    ].filter((l) => l.value > 0);
    const total = legs.reduce((s, l) => s + l.value, 0);
    const option = {
      backgroundColor: "transparent",
      tooltip: retroTooltip({
        trigger: "item" as const,
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        formatter: (p: any) =>
          `${p.name}: ${fmt(Number(p.value) || 0)} (${Number(p.percent || 0).toFixed(1)}%)`,
      }),
      series: retroPieSeries(legs.map((l) => ({ name: l.name, value: l.value, color: l.color })), {
        center: ["50%", "46%"],
      }),
    };
    return { option, legs, total };
  }, [consumption, imputedConsumption, investment, governmentSpending, netExports]);

  return (
    <div className="pinstripe-inset p-2">
      <div className="flex items-center gap-3">
        <div className="flex-1 min-w-0">
          <EChart option={option} style={{ height }} />
        </div>
        <DotLeaderLegend
          entries={legs.map((l) => ({
            name: l.name,
            value: `${total > 0 ? ((l.value / total) * 100).toFixed(1) : "0.0"}%`,
            color: l.color,
          }))}
        />
      </div>
    </div>
  );
}
