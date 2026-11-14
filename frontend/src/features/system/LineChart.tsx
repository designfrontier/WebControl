import { HEIGHT, PAD, WIDTH, bounds, pathFor } from "./chartMath";
import type { Series } from "./pidData";

const tick = (value: number) => (Math.abs(value) >= 1000 ? value.toFixed(0) : String(Number(value.toPrecision(4))));

interface Props {
  title: string;
  series: Series[];
}

export const LineChart = ({ title, series }: Props) => {
  const b = bounds(series);
  const samples = `${b.length} sample${b.length === 1 ? "" : "s"}`;
  return (
    <figure className="line-chart">
      <figcaption>{title}</figcaption>
      <svg
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        role="img"
        aria-label={`${title}: ${series.map((s) => s.name).join(", ")}; ${samples} from ${tick(b.min)} to ${tick(b.max)}`}
      >
        <rect
          className="chart-frame"
          x={PAD.left}
          y={PAD.top}
          width={WIDTH - PAD.left - PAD.right}
          height={HEIGHT - PAD.top - PAD.bottom}
        />
        <text className="chart-tick" x={PAD.left - 6} y={PAD.top + 10} textAnchor="end">
          {tick(b.max)}
        </text>
        <text className="chart-tick" x={PAD.left - 6} y={HEIGHT - PAD.bottom} textAnchor="end">
          {tick(b.min)}
        </text>
        <text className="chart-tick" x={WIDTH - PAD.right} y={HEIGHT - 8} textAnchor="end">
          {samples}
        </text>
        {series.map((s, i) => (
          <path key={s.name} className={`chart-line series-${i}`} d={pathFor(s.values, b)} />
        ))}
      </svg>
      <ul className="chart-legend">
        {series.map((s, i) => (
          <li key={s.name} className={`series-${i}`}>
            {s.name}
          </li>
        ))}
      </ul>
    </figure>
  );
};
