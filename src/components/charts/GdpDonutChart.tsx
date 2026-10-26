import { useMemo } from "react";
import { EChart } from "./EChart";
import { fmt } from "../../lib/format";

interface GdpDonutChartProps {
  consumption: number;
  imputedConsumption: number;
  investment: number;
  governmentSpending: number;
  netExports: number;
  height?: number;
}

/// v4.22 WS4: GDP expenditure donut — C + imputed-C + I + G + NX legs.
/// Net exports can be negative; the slice shows |NX| with a signed label.
export function GdpDonutChart({
  consumption,
  imputedConsumption,
  investment,
  governmentSpending,
  netExports,
  height = 320,
}: GdpDonutChartProps) {
  const option = useMemo(() => {
    const legs = [
      { name: "Consumption", value: consumption + imputedConsumption, color: "#3b82f6" },
      { name: "Investment", value: investment, color: "#22c55e" },
      { name: "Gov. Spending", value: governmentSpending, color: "#f59e0b" },
      {
        name: `Net Exports (${netExports >= 0 ? "+" : ""}${fmt(netExports)})`,
        value: Math.abs(netExports),
        color: "#a855f7",
      },
    ].filter((l) => l.value > 0);
    return {
      tooltip: {
        trigger: "item" as const,
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        formatter: (p: any) =>
          `${p.name}: ${fmt(Number(p.value) || 0)} (${Number(p.percent || 0).toFixed(1)}%)`,
      },
      legend: {
        bottom: 0,
        textStyle: { color: "#cbd5e1", fontSize: 11 },
        itemWidth: 12,
        itemHeight: 8,
      },
      series: [
        {
          type: "pie" as const,
          radius: ["42%", "68%"],
          center: ["50%", "46%"],
          label: { show: true, formatter: "{d}%", color: "#e2e8f0", fontSize: 11 },
          labelLine: { length: 8, length2: 6 },
          data: legs.map((l) => ({
            name: l.name,
            value: l.value,
            itemStyle: { color: l.color },
          })),
        },
      ],
    };
  }, [consumption, imputedConsumption, investment, governmentSpending, netExports]);

  return <EChart option={option} style={{ height }} />;
}
