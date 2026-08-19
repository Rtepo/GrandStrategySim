import { useEffect, useRef, type CSSProperties } from "react";
import * as echarts from "echarts";

interface EChartProps {
  option: echarts.EChartsOption;
  style?: CSSProperties;
}

export function EChart({ option, style }: EChartProps) {
  const ref = useRef<HTMLDivElement>(null);
  const chartRef = useRef<echarts.ECharts | null>(null);

  useEffect(() => {
    if (!ref.current) return;
    chartRef.current = echarts.init(ref.current, "dark");
    const handleResize = () => chartRef.current?.resize();
    window.addEventListener("resize", handleResize);
    return () => {
      window.removeEventListener("resize", handleResize);
      chartRef.current?.dispose();
    };
  }, []);

  useEffect(() => {
    if (chartRef.current) {
      chartRef.current.setOption(option, true);
    }
  }, [option]);

  return <div ref={ref} style={{ width: "100%", height: "100%", minHeight: 300, ...style }} />;
}
