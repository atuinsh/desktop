import { useMemo, useState, useCallback } from "react";
import UplotReact from "uplot-react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import { useStore } from "@/state/store";
import useResizeObserver from "use-resize-observer";

interface PromLineChartProps {
  data: Array<Array<number>>;
  seriesNames: string[];
}

interface TooltipData {
  show: boolean;
  x: number;
  time: string;
  values: { name: string; value: string; color: string }[];
}

// Unified color palette that works in both light and dark modes
const CHART_COLORS = [
  "#3b82f6", // blue
  "#22c55e", // green
  "#f59e0b", // amber
  "#a855f7", // purple
  "#f43f5e", // rose
  "#06b6d4", // cyan
  "#6366f1", // indigo
  "#14b8a6", // teal
];

function buildSeriesConfig(seriesNames: string[]): uPlot.Series[] {
  return [
    {}, // x-axis placeholder (required by uPlot)
    ...seriesNames.map((name, i) => ({
      label: name,
      stroke: CHART_COLORS[i % CHART_COLORS.length],
      width: 1.5,
      points: { show: false },
    })),
  ];
}

// Grafana-style timestamp: "2026-01-23 07:50:00"
function formatTime(timestamp: number): string {
  const date = new Date(timestamp * 1000);
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  const hours = String(date.getHours()).padStart(2, "0");
  const minutes = String(date.getMinutes()).padStart(2, "0");
  const seconds = String(date.getSeconds()).padStart(2, "0");
  return `${year}-${month}-${day} ${hours}:${minutes}:${seconds}`;
}

function formatValue(val: number | null | undefined): string {
  if (val == null) return "—";
  if (Math.abs(val) >= 1000000) return (val / 1000000).toFixed(2) + "M";
  if (Math.abs(val) >= 1000) return (val / 1000).toFixed(2) + "K";
  return val.toFixed(2);
}

function buildOptions(
  seriesNames: string[],
  isDark: boolean,
  width: number,
  height: number,
  onCursor: (u: uPlot) => void
): uPlot.Options {
  return {
    width,
    height,
    series: buildSeriesConfig(seriesNames),
    axes: [
      {
        stroke: isDark ? "#a1a1aa" : "#71717a",
        grid: { stroke: isDark ? "#27272a40" : "#e4e4e740" },
        ticks: { stroke: isDark ? "#27272a" : "#e4e4e7" },
      },
      {
        stroke: isDark ? "#a1a1aa" : "#71717a",
        grid: { stroke: isDark ? "#27272a40" : "#e4e4e740" },
        ticks: { stroke: isDark ? "#27272a" : "#e4e4e7" },
      },
    ],
    cursor: {
      x: true,
      y: false,
    },
    scales: {
      x: { time: true },
    },
    legend: { show: false },
    hooks: {
      setCursor: [onCursor],
    },
  };
}

export function PromLineChart({ data, seriesNames }: PromLineChartProps) {
  const isDark = useStore((state) => state.functionalColorMode === "dark");
  const { ref, width = 400, height = 200 } =
    useResizeObserver<HTMLDivElement>();
  const [tooltip, setTooltip] = useState<TooltipData>({
    show: false,
    x: 0,
    time: "",
    values: [],
  });

  const handleCursor = useCallback(
    (u: uPlot) => {
      const { left, idx } = u.cursor;
      if (idx == null || left == null || left < 0) {
        setTooltip((t) => ({ ...t, show: false }));
        return;
      }

      const timestamp = u.data[0][idx];
      const values = seriesNames.map((name, i) => ({
        name,
        value: formatValue(u.data[i + 1]?.[idx]),
        color: CHART_COLORS[i % CHART_COLORS.length],
      }));

      setTooltip({
        show: true,
        x: left,
        time: formatTime(timestamp),
        values,
      });
    },
    [seriesNames]
  );

  const options = useMemo(
    () => buildOptions(seriesNames, isDark, width, height, handleCursor),
    [seriesNames, isDark, width, height, handleCursor]
  );

  // Calculate tooltip position - fixed at top, flips at midpoint
  const tooltipWidth = 280;
  const padding = 20;

  // Flip to left side when cursor is past midpoint
  const flipX = tooltip.x > width / 2;
  const tooltipLeft = flipX
    ? Math.max(0, tooltip.x - tooltipWidth - padding)
    : tooltip.x + padding;

  return (
    <div ref={ref} className="h-full w-full relative prom-chart">
      <style>{`
        .prom-chart .u-cursor-x {
          border-right: 1px dashed ${isDark ? "#71717a" : "#a1a1aa"};
        }
      `}</style>
      {width > 0 && height > 0 && (
        <UplotReact options={options} data={data as uPlot.AlignedData} />
      )}
      {tooltip.show && (
        <div
          className={`absolute z-50 pointer-events-none rounded px-3 py-2 text-sm shadow-xl ${
            isDark
              ? "bg-zinc-900/95 border border-zinc-700"
              : "bg-white/95 border border-zinc-200"
          }`}
          style={{
            left: tooltipLeft,
            top: 8,
          }}
        >
          <div
            className={`text-xs font-mono mb-2 ${
              isDark ? "text-zinc-300" : "text-zinc-600"
            }`}
          >
            {tooltip.time}
          </div>
          {tooltip.values.map((v, i) => (
            <div key={i} className="flex items-center gap-2 py-0.5">
              <span
                className="w-4 h-0.5 rounded-sm flex-shrink-0"
                style={{ backgroundColor: v.color }}
              />
              <span
                className={`truncate flex-1 min-w-0 ${
                  isDark ? "text-zinc-300" : "text-zinc-600"
                }`}
              >
                {v.name}
              </span>
              <span
                className={`font-semibold ml-2 tabular-nums ${
                  isDark ? "text-white" : "text-zinc-900"
                }`}
              >
                {v.value}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
