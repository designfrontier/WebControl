import type { OpticalCell } from "../../protocol";
import { COLUMNS, ROWS, cellName, heatLevel, magnitude, sameCell, summarise } from "./chartData";
import type { GridCell } from "./chartData";

const CELL = 20;
const mm = (value: number) => value.toFixed(2);

const describe = (cell: GridCell) =>
  cell.error
    ? `${cellName(cell)}: X ${mm(cell.error.x)} mm, Y ${mm(cell.error.y)} mm`
    : `${cellName(cell)}: not measured`;

/** The target as a grid of squares, darker where the sled landed further from where it was sent. */
export const ErrorHeatMap = ({
  cells,
  current,
  failed,
}: {
  cells: readonly GridCell[];
  current: OpticalCell | null;
  failed: boolean;
}) => {
  const { measured, largest, rms } = summarise(cells);
  const summary = `${measured} of ${COLUMNS * ROWS} squares measured, largest error ${mm(largest)} mm, RMS ${mm(rms)} mm`;
  return (
    <figure className="heat-map">
      <svg viewBox={`0 0 ${COLUMNS * CELL} ${ROWS * CELL}`} role="img" aria-label={`Error map: ${summary}`}>
        {cells.map((cell) => {
          const here = sameCell(current, cell);
          const level = cell.error ? `heat-${heatLevel(magnitude(cell.error), largest)}` : "heat-empty";
          return (
            <rect
              key={`${cell.i},${cell.j}`}
              className={`heat ${level}${here ? (failed ? " heat-failed" : " heat-current") : ""}`}
              x={(cell.i + 15) * CELL}
              y={(7 - cell.j) * CELL}
              width={CELL}
              height={CELL}
            >
              <title>{describe(cell)}</title>
            </rect>
          );
        })}
      </svg>
      <figcaption>
        {summary}. Darker squares have larger errors; the outlined square is the one being measured.
      </figcaption>
    </figure>
  );
};
