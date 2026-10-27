import { useMemo } from "react";
import { EChart } from "./EChart";
import { fmt, num } from "../../lib/format";
import type { CountryTradeTreemapDto, CountryTradeTreemapEntry } from "../../types/api";

export type TreemapMode = "exports" | "imports" | "domestic";

/// SITC Rev.2 one-digit section colors — muted OEC-style palette keyed
/// by the section's leading digit (the labels arrive as
/// "3 — Mineral Fuels & Energy"; services use the "S" pseudo-section).
const SITC_COLORS: Record<string, string> = {
  "0": "#65a30d", // Food & Live Animals — olive
  "1": "#a16207", // Beverages, Tobacco & Luxury Crops — amber
  "2": "#78716c", // Crude Materials (ex Fuels) — stone
  "3": "#b45309", // Mineral Fuels & Energy — burnt orange
  "5": "#7c3aed", // Chemicals — violet
  "6": "#0891b2", // Manufactured Goods (by Material) — cyan
  "7": "#2563eb", // Machinery & Transport — blue
  "8": "#db2777", // Miscellaneous Manufactures — pink
  "9": "#dc2626", // Arms, Bullion & Unclassified — red
  S: "#64748b", // Services & Intangibles — slate
};

function sitcColor(section: string): string {
  const key = section.trim().charAt(0);
  return SITC_COLORS[key] ?? "#475569";
}

/// "hard_coal" → "Hard Coal" — serde keys are the canonical display ids.
function prettyCommodity(key: string): string {
  return key
    .split("_")
    .map((w) => (w.length > 0 ? w[0].toUpperCase() + w.slice(1) : w))
    .join(" ");
}

/// v4.23 WS4: per-country trade treemap — the SELECTED country's real
/// settled export / import / domestic legs this turn (order-book legs
/// classified by endpoint ownership + the world-market sweeps), grouped
/// by SITC Rev.2 section. Area ∝ settled value; tooltips show quantity,
/// VWAP-equivalent average price, and section share.
export function TradeTreemapChart({
  data,
  mode,
}: {
  data: CountryTradeTreemapDto;
  mode: TreemapMode;
}) {
  const option = useMemo(() => {
    const entries =
      mode === "exports"
        ? data.exports
        : mode === "imports"
          ? data.imports
          : data.domestic;
    const groups = new Map<
      string,
      {
        name: string;
        value: number;
        children: { name: string; value: number; entry: CountryTradeTreemapEntry }[];
      }
    >();
    for (const e of entries) {
      const key = e.sitc_section || "9 — Arms, Bullion & Unclassified";
      const g = groups.get(key) ?? { name: key, value: 0, children: [] };
      g.value += e.value;
      g.children.push({ name: prettyCommodity(e.commodity), value: e.value, entry: e });
      groups.set(key, g);
    }
    // Deterministic: sections by value desc, children by value desc
    // (entries already arrive value-sorted from the engine).
    const seriesData = [...groups.values()]
      .sort((a, b) => b.value - a.value)
      .map((g) => ({
        name: g.name,
        value: g.value,
        itemStyle: { color: sitcColor(g.name), borderColor: "#0f172a" },
        children: g.children
          .sort((a, b) => b.value - a.value)
          .map((c) => ({ name: c.name, value: c.value, entry: c.entry })),
      }));
    return {
      tooltip: {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        formatter: (p: any) => {
          const e = (p.data as { entry?: CountryTradeTreemapEntry } | undefined)?.entry;
          if (!e) return `${p.name}: ${fmt(Number(p.value) || 0)}`;
          return (
            `<b>${prettyCommodity(e.commodity)}</b><br/>` +
            `${e.sitc_section}<br/>` +
            `Settled: ${num(Math.round(e.quantity))} units @ ${fmt(e.avg_price)}<br/>` +
            `Value: ${fmt(e.value)} (${(e.share * 100).toFixed(1)}%)`
          );
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
  }, [data, mode]);

  return <EChart option={option} style={{ height: 420 }} />;
}
