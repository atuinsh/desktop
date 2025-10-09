// @ts-ignore
import { EChart, useECharts } from "@kbox-labs/react-echarts";

import { ChartConfig } from "@/components/ui/chart";

interface PromLineChartProps {
  data: any[];
  config: ChartConfig;
}

export function PromLineChart({ data }: PromLineChartProps) {
  const [ref, _echartsInstance] = useECharts<HTMLDivElement>({
    grid: {
      left: 0,
      right: "5px",
      top: "5px",
      bottom: 0,
      containLabel: true,
    },
    tooltip: { show: true, trigger: "axis" },
    style: { height: "100%", width: "100%" },
    xAxis: {
      type: "time",
    },
    yAxis: {
      type: "value",
      boundaryGap: [0, "30%"],
    },
    series: data,

    animation: false,
    animationDurationUpdate: 0,
    animationEasing: "linear",
    animationEasingUpdate: "linear",
    animationDuration: 0,
  });

  return <div className="flex h-full w-full" ref={ref}></div>;
}
