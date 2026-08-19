import { EChart } from "./EChart";
import type { EChartsOption } from "echarts";

interface SeatDistributionChartProps {
  data: Array<[string, number]>;
  title?: string;
}

export function SeatDistributionChart({ data, title }: SeatDistributionChartProps) {
  const option: EChartsOption = {
    backgroundColor: "transparent",
    tooltip: {
      trigger: "item",
      formatter: "{b}: {c} seats ({d}%)",
    },
    legend: {
      orient: "vertical",
      left: "left",
      textStyle: { color: "#94a3b8" },
    },
    series: [
      {
        name: title || "Seats",
        type: "pie",
        radius: ["40%", "70%"],
        center: ["60%", "50%"],
        avoidLabelOverlap: false,
        itemStyle: {
          borderRadius: 4,
          borderColor: "#1e293b",
          borderWidth: 2,
        },
        label: {
          show: true,
          color: "#cbd5e1",
          formatter: "{b}\n{c} seats",
        },
        emphasis: {
          label: { show: true, fontSize: 14, fontWeight: "bold" },
        },
        data: data.map(([name, value]) => ({ name, value })),
      },
    ],
  };

  return <EChart option={option} style={{ minHeight: 300 }} />;
}
