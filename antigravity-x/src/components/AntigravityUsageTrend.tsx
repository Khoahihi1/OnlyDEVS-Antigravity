import {
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent,
} from "react";
import type { AppLanguage, TimeBucketPoint } from "../types";
import { tr } from "../i18n";

export type AntigravityTrendMetric =
  | "totalTokens"
  | "inputTokens"
  | "thinkingTokens"
  | "outputTokens"
  | "cacheTokens"
  | "turnCount";

type TrendValueLabel = {
  text: string;
  x: number;
};

const CHART_WIDTH = 960;
const CHART_HEIGHT = 280;
const PLOT = { left: 56, right: 20, top: 30, bottom: 54 };

export function formatExactNumber(n: number): string {
  return new Intl.NumberFormat("en-US").format(Math.round(n || 0));
}

export function formatTokensShort(n: number, decimals: 1 | 2 = 2): string {
  const val = Number(n || 0);
  if (val >= 1_000_000_000) return `${(val / 1_000_000_000).toFixed(decimals)}B`;
  if (val >= 1_000_000) return `${(val / 1_000_000).toFixed(decimals)}M`;
  if (val >= 1_000) return `${(val / 1_000).toFixed(1)}K`;
  return String(Math.round(val));
}

function extractMetricValue(
  bucket: TimeBucketPoint,
  metric: AntigravityTrendMetric,
): number {
  switch (metric) {
    case "inputTokens":
      return bucket.exact.inputTokens;
    case "thinkingTokens":
      return bucket.exact.thinkingTokens;
    case "outputTokens":
      return bucket.exact.outputTokens;
    case "cacheTokens":
      return bucket.exact.cacheReadTokens + bucket.exact.cacheWriteTokens;
    case "turnCount":
      return bucket.turnCount;
    case "totalTokens":
    default:
      return bucket.exact.totalTokens;
  }
}

function formatMetricValue(value: number, metric: AntigravityTrendMetric): string {
  if (metric === "turnCount") return formatExactNumber(value);
  return formatTokensShort(value, 2);
}

function buildSmoothPath(points: Array<{ x: number; y: number }>): string {
  if (points.length === 0) return "";
  if (points.length === 1) {
    return `M ${points[0].x.toFixed(2)} ${points[0].y.toFixed(2)}`;
  }
  return points.slice(0, -1).reduce((path, current, index) => {
    const next = points[index + 1];
    const previous = points[Math.max(0, index - 1)];
    const afterNext = points[Math.min(points.length - 1, index + 2)];
    const c1 = {
      x: current.x + (next.x - previous.x) / 6,
      y: current.y + (next.y - previous.y) / 6,
    };
    const c2 = {
      x: next.x - (afterNext.x - current.x) / 6,
      y: next.y - (afterNext.y - current.y) / 6,
    };
    return `${path} C ${c1.x.toFixed(2)} ${c1.y.toFixed(2)}, ${c2.x.toFixed(2)} ${c2.y.toFixed(2)}, ${next.x.toFixed(2)} ${next.y.toFixed(2)}`;
  }, `M ${points[0].x.toFixed(2)} ${points[0].y.toFixed(2)}`);
}

export function AntigravityUsageTrend({
  series,
  lang,
}: {
  series: TimeBucketPoint[];
  lang: AppLanguage;
}) {
  const vi = lang === "vi";
  const chartId = useId().replace(/:/g, "");
  const [metric, setMetric] = useState<AntigravityTrendMetric>("totalTokens");
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);
  const svgRef = useRef<SVGSVGElement>(null);
  const chartWrapRef = useRef<HTMLDivElement>(null);
  const tooltipRef = useRef<HTMLDivElement>(null);
  const [tooltipLeft, setTooltipLeft] = useState(0);

  const metricTabs: Array<[AntigravityTrendMetric, string]> = useMemo(
    () => [
      ["totalTokens", tr(lang, "acc.metric.total")],
      ["inputTokens", tr(lang, "usage.cardInput")],
      ["thinkingTokens", tr(lang, "usage.cardThinking")],
      ["outputTokens", tr(lang, "usage.cardOutput")],
      ["cacheTokens", tr(lang, "usage.cardCache")],
      ["turnCount", tr(lang, "usage.turns")],
    ],
    [lang],
  );

  const points = useMemo(() => {
    if (series.length === 0) return [];
    const values = series.map((bucket) => ({
      bucket,
      value: extractMetricValue(bucket, metric),
    }));
    const min = Math.min(...values.map((item) => item.value));
    const max = Math.max(...values.map((item) => item.value));
    const span = Math.max(max - min, max * 0.08, 1);
    const xSpan = CHART_WIDTH - PLOT.left - PLOT.right;
    const ySpan = CHART_HEIGHT - PLOT.top - PLOT.bottom;
    return values.map((item, index) => ({
      ...item,
      index,
      x:
        PLOT.left +
        (values.length === 1
          ? xSpan / 2
          : (index / (values.length - 1)) * xSpan),
      y:
        PLOT.top +
        (1 - (item.value - min + span * 0.03) / (span * 1.06)) * ySpan,
    }));
  }, [metric, series]);

  const linePath = buildSmoothPath(points);
  const areaPath =
    points.length > 0
      ? `${linePath} L ${points[points.length - 1].x.toFixed(2)} ${(CHART_HEIGHT - PLOT.bottom).toFixed(2)} L ${points[0].x.toFixed(2)} ${(CHART_HEIGHT - PLOT.bottom).toFixed(2)} Z`
      : "";
  const totalValue = points.reduce((sum, p) => sum + p.value, 0);
  const axisStep = Math.max(1, Math.ceil(points.length / 7));
  const axisIndexes = new Set(
    points
      .filter((p) => p.index % axisStep === 0 || p.index === points.length - 1)
      .map((p) => p.index),
  );

  const valueLabels = useMemo<Map<number, TrendValueLabel>>(() => {
    const occupied: Array<{ start: number; end: number }> = [];
    const labels = new Map<number, TrendValueLabel>();
    const candidates = points
      .filter((p) => p.value > 0)
      .sort((a, b) => b.value - a.value || a.index - b.index);
    for (const point of candidates) {
      const text = formatMetricValue(point.value, metric);
      const width = Math.max(46, Math.min(110, text.length * 7.5 + 16));
      const x = Math.min(
        CHART_WIDTH - PLOT.right - width / 2,
        Math.max(PLOT.left + width / 2, point.x),
      );
      const start = x - width / 2 - 4;
      const end = x + width / 2 + 4;
      if (occupied.some((r) => start < r.end && end > r.start)) continue;
      occupied.push({ start, end });
      labels.set(point.index, { text, x });
    }
    return labels;
  }, [metric, points]);

  const gridYs = [0, 1, 2].map(
    (i) => PLOT.top + (i / 2) * (CHART_HEIGHT - PLOT.top - PLOT.bottom),
  );
  const hoveredPoint =
    hoveredIndex === null ? null : (points[hoveredIndex] ?? null);
  const hoveredX = hoveredPoint?.x;

  useLayoutEffect(() => {
    const svg = svgRef.current;
    const wrap = chartWrapRef.current;
    if (!svg || !wrap || hoveredX === undefined) return;
    const updatePos = () => {
      const matrix = svg.getScreenCTM();
      const bounds = wrap.getBoundingClientRect();
      if (!matrix || bounds.width === 0) return;
      const pt = svg.createSVGPoint();
      pt.x = hoveredX;
      const localX =
        ((pt.matrixTransform(matrix).x - bounds.left) * wrap.clientWidth) /
        bounds.width;
      const half = (tooltipRef.current?.offsetWidth ?? 160) / 2 + 8;
      setTooltipLeft(Math.max(half, Math.min(wrap.clientWidth - half, localX)));
    };
    updatePos();
    const obs = new ResizeObserver(updatePos);
    obs.observe(wrap);
    return () => obs.disconnect();
  }, [hoveredX]);

  const handleChartMove = (event: MouseEvent<SVGRectElement>) => {
    const svg = event.currentTarget.ownerSVGElement;
    if (!svg || points.length === 0) return;
    const matrix = svg.getScreenCTM();
    if (!matrix) return;
    const pt = svg.createSVGPoint();
    pt.x = event.clientX;
    pt.y = event.clientY;
    const relX = pt.matrixTransform(matrix.inverse()).x;
    const frac = (relX - PLOT.left) / (CHART_WIDTH - PLOT.left - PLOT.right);
    setHoveredIndex(
      Math.round(Math.max(0, Math.min(1, frac)) * (points.length - 1)),
    );
  };

  return (
    <section className="ag-usage-trend">
      <div className="ag-usage-trend__header">
        <h3>{tr(lang, "usage.trendTitle")}</h3>
        <div className="ag-usage-trend__header-right">
          <strong className="ag-usage-trend__total">
            {formatMetricValue(totalValue, metric)}
          </strong>
          <div className="ag-usage-trend__metrics" role="group">
            {metricTabs.map(([val, label]) => (
              <button
                key={val}
                type="button"
                className={`ag-usage-trend__tab${metric === val ? " is-active" : ""}`}
                onClick={() => {
                  setMetric(val);
                  setHoveredIndex(null);
                }}
              >
                {label}
              </button>
            ))}
          </div>
        </div>
      </div>

      {points.length === 0 ? (
        <div className="ag-usage-trend__empty">
          {vi
            ? "Chưa có dữ liệu token trong khoảng thời gian đã chọn"
            : "No token usage recorded in the selected time range"}
        </div>
      ) : (
        <div ref={chartWrapRef} className="ag-usage-trend__chart-wrap">
          <svg
            ref={svgRef}
            className="ag-usage-trend__svg"
            viewBox={`0 0 ${CHART_WIDTH} ${CHART_HEIGHT}`}
            role="img"
          >
            <defs>
              <linearGradient id={`${chartId}-grad`} x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor="var(--agx-accent)" stopOpacity="0.28" />
                <stop offset="100%" stopColor="var(--agx-accent)" stopOpacity="0.02" />
              </linearGradient>
            </defs>

            {gridYs.map((y) => (
              <line
                key={y}
                className="ag-usage-trend__grid"
                x1={PLOT.left}
                x2={CHART_WIDTH - PLOT.right}
                y1={y}
                y2={y}
              />
            ))}

            <line
              className="ag-usage-trend__baseline"
              x1={PLOT.left}
              x2={CHART_WIDTH - PLOT.right}
              y1={CHART_HEIGHT - PLOT.bottom}
              y2={CHART_HEIGHT - PLOT.bottom}
            />

            {areaPath ? (
              <path
                className="ag-usage-trend__area"
                d={areaPath}
                fill={`url(#${chartId}-grad)`}
              />
            ) : null}

            {linePath ? (
              <path className="ag-usage-trend__line" d={linePath} />
            ) : null}

            {hoveredPoint ? (
              <>
                <rect
                  className="ag-usage-trend__hover-slice"
                  x={hoveredPoint.x - 16}
                  y={PLOT.top}
                  width={32}
                  height={CHART_HEIGHT - PLOT.top - PLOT.bottom}
                />
                <line
                  className="ag-usage-trend__hover-line"
                  x1={hoveredPoint.x}
                  x2={hoveredPoint.x}
                  y1={PLOT.top}
                  y2={CHART_HEIGHT - PLOT.bottom}
                />
              </>
            ) : null}

            {points.map((point) => {
              const label = valueLabels.get(point.index);
              const isHovered = hoveredIndex === point.index;
              const pillW = label
                ? Math.max(44, Math.min(108, label.text.length * 7.5 + 14))
                : 0;
              return (
                <g key={point.bucket.bucketKey}>
                  <circle
                    className={`ag-usage-trend__dot${isHovered ? " is-active" : ""}`}
                    cx={point.x}
                    cy={point.y}
                    r={isHovered ? 5.5 : 3.5}
                  />
                  {label ? (
                    <g transform={`translate(${label.x}, ${Math.max(18, point.y - 14)})`}>
                      <rect
                        className="ag-usage-trend__pill-bg"
                        x={-pillW / 2}
                        y={-11}
                        width={pillW}
                        height={20}
                        rx={7}
                      />
                      <text className="ag-usage-trend__pill-text" textAnchor="middle" y={3}>
                        {label.text}
                      </text>
                    </g>
                  ) : null}
                  {axisIndexes.has(point.index) ? (
                    <text
                      className="ag-usage-trend__axis-label"
                      x={point.x}
                      y={CHART_HEIGHT - PLOT.bottom + 24}
                      textAnchor="middle"
                    >
                      {point.bucket.label}
                    </text>
                  ) : null}
                </g>
              );
            })}

            <rect
              x={PLOT.left}
              y={PLOT.top}
              width={CHART_WIDTH - PLOT.left - PLOT.right}
              height={CHART_HEIGHT - PLOT.top - PLOT.bottom}
              fill="transparent"
              style={{ cursor: "crosshair" }}
              onMouseMove={handleChartMove}
              onMouseLeave={() => setHoveredIndex(null)}
            />
          </svg>

          {hoveredPoint ? (
            <div
              ref={tooltipRef}
              className="ag-usage-trend__tooltip"
              style={{ left: `${tooltipLeft}px` }}
            >
              <div className="ag-usage-trend__tooltip-title">
                {hoveredPoint.bucket.label}
                <span>
                  {hoveredPoint.bucket.conversationCount} {vi ? "phiên" : "sessions"} ·{" "}
                  {hoveredPoint.bucket.turnCount} {vi ? "lượt" : "turns"}
                </span>
              </div>
              <div className="ag-usage-trend__tooltip-grid">
                <span>{vi ? "Tổng" : "Total"}</span>
                <strong>{formatTokensShort(hoveredPoint.bucket.exact.totalTokens, 2)}</strong>
                <span>{vi ? "Đầu vào" : "Input"}</span>
                <strong>{formatTokensShort(hoveredPoint.bucket.exact.inputTokens, 2)}</strong>
                <span>{vi ? "Suy luận" : "Thinking"}</span>
                <strong>{formatTokensShort(hoveredPoint.bucket.exact.thinkingTokens, 2)}</strong>
                <span>{vi ? "Đầu ra" : "Output"}</span>
                <strong>{formatTokensShort(hoveredPoint.bucket.exact.outputTokens, 2)}</strong>
                <span>{vi ? "Bộ đệm" : "Cache"}</span>
                <strong>
                  {formatTokensShort(
                    hoveredPoint.bucket.exact.cacheReadTokens +
                      hoveredPoint.bucket.exact.cacheWriteTokens,
                    2,
                  )}
                </strong>
              </div>
            </div>
          ) : null}
        </div>
      )}
    </section>
  );
}
