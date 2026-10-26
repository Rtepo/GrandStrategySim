import { useMemo } from "react";
import { EChart } from "./EChart";
import { fmt, num } from "../../lib/format";
import type { TradeTreemapDto } from "../../types/api";

const CATEGORY_COLORS: Record<string, string> = {
  agriculture: "#65a30d",
  industry: "#0891b2",
  services: "#7c3aed",
};

/// v4.22 WS4: Wikipedia/OEC-style trade treemap — commodities grouped by
/// sector category, area proportional to market value (cleared volume ×
/// VWAP). Tooltips show volume, price, and share.
export function TradeTreemapChart({ data }: { data: TradeTreemapDto }) {
  const option = useMemo(() => {
    const groups = new Map<string, { name: string; value: number; children: { name: string; value: number; entry: (typeof data.entries)[number] }[] }>();
    for (const e of data.entries) {
      const key = e.category || "industry";
      const g = groups.get(key) ?? { name: key[0].toUpperCase() + key.slice(1), value: 0, children: [] };
      g.value += e.value;
      g.children.push({ name: e.commodity, value: e.value, entry: e });
      groups.set(key, g);
    }
    const seriesData = [...groups.entries()].map(([key, g]) => ({
      name: g.name,
      value: g.value,
      itemStyle: { color: CATEGORY_COLORS[key] ?? "#64748b", borderColor: "#0f172a" },
      children: g.children.map((c) => ({
        name: c.name,
        value: c.value,
        entry: c.entry,
      })),
    }));
    return {
      tooltip: {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        formatter: (p: any) => {
          const e = (p.data as { entry?: (typeof data.entries)[number] } | undefined)?.entry;
          if (!e) return `${p.name}: ${fmt(Number(p.value) || 0)}`;
          return `<b>${e.commodity}</b><br/>Value: ${fmt(e.value)} (${(e.share * 100).toFixed(1)}%)` +
            `<br/>Cleared vol: ${num(Math.round(e.cleared_volume))} @ ${fmt(e.reference_price)}` +
            `<br/>Supply ${num(Math.round(e.supply_volume))} / Demand ${num(Math.round(e.demand_volume))}`;
        },
      },
      series: [
        {
          type: "treemap" as const,
          roam: false,
          nodeClick: "zoomToNode" as const,
          breadcrumb: { show: true, textStyle: { color: "#cbd5e1" } },
          label: { show: true, formatter: "{b}", fontSize: 11, color: "#f8fafc" },
          upperLabel: {
            show: true,
            height: 22,
            color: "#f8fafc",
            backgroundColor: "transparent",
          },
          itemStyle: { borderColor: "#0f172a", borderWidth: 1, gapWidth: 1 },
          levels: [
            { itemStyle: { borderWidth: 0, gapWidth: 3 } },
            { itemStyle: { gapWidth: 1 } },
          ],
          data: seriesData,
        },
      ],
    };
  }, [data]);

  return <EChart option={option} style={{ height: 420 }} />;
}
