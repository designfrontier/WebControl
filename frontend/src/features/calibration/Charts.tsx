import { bounds, project } from "./chartData";
import type { Bounds, ChartPoint } from "./chartData";

const WIDTH = 320;
const HEIGHT = 200;
const PAD = 28;
const PLOT_W = WIDTH - PAD * 2;
const PLOT_H = HEIGHT - PAD * 2;
const LINE_COLOURS = 8;

const label = (value: number) => Number(value.toPrecision(3)).toString();

const Axes = ({ x, y }: { x: Bounds; y: Bounds }) => (
  <g className="chart-axes">
    <rect x={PAD} y={PAD} width={PLOT_W} height={PLOT_H} />
    <text x={PAD - 4} y={PAD + 4} textAnchor="end">
      {label(y.max)}
    </text>
    <text x={PAD - 4} y={PAD + PLOT_H} textAnchor="end">
      {label(y.min)}
    </text>
    <text x={PAD} y={HEIGHT - 8}>
      {label(x.min)}
    </text>
    <text x={PAD + PLOT_W} y={HEIGHT - 8} textAnchor="end">
      {label(x.max)}
    </text>
  </g>
);

/** One polyline per series, with the series index as the x value, like the old Plotly line charts. */
export const LineChart = ({ title, series }: { title: string; series: number[][] }) => {
  const length = Math.max(1, ...series.map((values) => values.length));
  const x = { min: 0, max: Math.max(1, length - 1) };
  const y = bounds(series.flat());
  return (
    <figure className="chart">
      <svg viewBox={`0 0 ${WIDTH} ${HEIGHT}`} role="img" aria-label={`${title}: ${series.length} lines`}>
        <Axes x={x} y={y} />
        {series.map((values, index) => (
          <polyline
            key={index}
            className={`chart-line chart-line-${index % LINE_COLOURS}`}
            points={values
              .map((value, column) => `${PAD + project(column, x, PLOT_W)},${PAD + project(value, y, PLOT_H, true)}`)
              .join(" ")}
          />
        ))}
      </svg>
      <figcaption>{title}</figcaption>
    </figure>
  );
};

/** Ideal grid points against where the camera found them. */
export const ScatterChart = ({
  title,
  ideal,
  measured,
}: {
  title: string;
  ideal: ChartPoint[];
  measured: ChartPoint[];
}) => {
  const all = [...ideal, ...measured];
  const x = bounds(all.map((point) => point.x));
  const y = bounds(all.map((point) => point.y));
  const dots = (points: ChartPoint[], className: string) =>
    points.map((point, index) => (
      <circle
        key={`${className}-${index}`}
        className={className}
        cx={PAD + project(point.x, x, PLOT_W)}
        cy={PAD + project(point.y, y, PLOT_H, true)}
        r={1.5}
      />
    ));
  return (
    <figure className="chart">
      <svg viewBox={`0 0 ${WIDTH} ${HEIGHT}`} role="img" aria-label={`${title}: ${ideal.length} grid points`}>
        <Axes x={x} y={y} />
        {dots(ideal, "chart-dot-ideal")}
        {dots(measured, "chart-dot-measured")}
      </svg>
      <figcaption>
        {title} <span className="chart-key chart-dot-ideal">● ideal</span>{" "}
        <span className="chart-key chart-dot-measured">● measured</span>
      </figcaption>
    </figure>
  );
};
