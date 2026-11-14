import { useState } from "react";
import { Link } from "react-router";
import { useClient, useServerMessage } from "../../app/context";
import type { BoardDetails } from "../../protocol";
import { useResource } from "../files/hooks";
import { Loaded } from "../files/ui";
import { getBoardDetails } from "./api";
import { formatNumber } from "./geometry";

const Details = ({ board }: { board: BoardDetails }) => {
  const unit = board.units === "mm" ? "mm" : "in";
  const measure = (value: number) => `${formatNumber(value)} ${unit}`;
  const rows: [string, string][] = [
    ["Board ID", board.boardID],
    ["Material", board.material],
    ["Width", measure(board.width)],
    ["Height", measure(board.height)],
    ["Thickness", measure(board.thickness)],
    ["Center", `X ${measure(board.centerX)}, Y ${measure(board.centerY)}`],
    ["File", board.fileName || "Not saved"],
  ];
  return (
    <dl className="details">
      {rows.map(([term, value]) => (
        <div key={term}>
          <dt>{term}</dt>
          <dd>{value}</dd>
        </div>
      ))}
    </dl>
  );
};

const ClearCutData = () => {
  const client = useClient();
  const [confirming, setConfirming] = useState(false);
  if (!confirming)
    return (
      <button type="button" className="danger" onClick={() => setConfirming(true)}>
        Clear cut data…
      </button>
    );
  return (
    <div className="confirm" role="group" aria-labelledby="clear-cut-question">
      <p id="clear-cut-question">Forget which parts of this board have been cut?</p>
      <div className="actions">
        <button
          type="button"
          className="danger"
          onClick={() => {
            client.sendAction("boardClearBoard");
            setConfirming(false);
          }}
        >
          Clear cut data
        </button>
        <button type="button" onClick={() => setConfirming(false)}>
          Keep it
        </button>
      </div>
    </div>
  );
};

export const BoardsHome = () => {
  const client = useClient();
  const { resource, reload } = useResource(getBoardDetails);
  useServerMessage("boardDataUpdate", reload);

  return (
    <>
      <section className="panel" aria-labelledby="boards-current">
        <h2 id="boards-current">Current board</h2>
        <Loaded resource={resource} what="the current board">
          {(board) => <Details board={board} />}
        </Loaded>
        <div className="actions">
          <Link className="button" to="/boards/open">
            Open…
          </Link>
          <Link className="button" to="/boards/save">
            Save as…
          </Link>
          <Link className="button" to="/boards/edit">
            Create or edit…
          </Link>
          <Link className="button" to="/boards/trim">
            Trim…
          </Link>
        </div>
      </section>
      <section className="panel" aria-labelledby="boards-cut">
        <h2 id="boards-cut">Cut areas</h2>
        <p className="muted">
          WebControl records which parts of the board have been cut, so you can see the free space left.
        </p>
        <div className="actions">
          <button type="button" onClick={() => client.sendAction("boardProcessGCode")}>
            Mark loaded gcode as cut
          </button>
          <ClearCutData />
        </div>
      </section>
    </>
  );
};
