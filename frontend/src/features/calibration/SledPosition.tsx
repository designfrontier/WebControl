import { useMachine } from "../../app/context";

const MM_PER_INCH = 25.4;

/** The 96 x 48 inch sheet, centred on the origin, with the sled's live position. */
export const SledPosition = () => {
  const position = useMachine((state) => state.position);
  const units = useMachine((state) => state.units);
  const scale = units === "MM" ? MM_PER_INCH : 1;
  const x = (position?.x ?? 0) / scale;
  const y = (position?.y ?? 0) / scale;
  const where = position ? `X ${x.toFixed(2)} in, Y ${y.toFixed(2)} in` : "position unknown";

  return (
    <figure className="sled-position">
      <svg viewBox="-50 -26 100 52" role="img" aria-label={`Sled position: ${where}`}>
        <rect className="sheet" x={-48} y={-24} width={96} height={48} />
        <path className="sheet-grid" d="M-24 -24V24M0 -24V24M24 -24V24M-48 0H48" />
        {position && (
          <g className="sled" transform={`translate(${x} ${-y})`}>
            <circle r={1} />
            <path d="M-1.5 0H1.5M0 -1.5V1.5" />
          </g>
        )}
      </svg>
      <figcaption>{where}</figcaption>
    </figure>
  );
};
