import { useState } from "react";
import type { FormEvent } from "react";
import { Link, useNavigate } from "react-router";
import { useFetch } from "../../app/context";
import type { BoardDetails } from "../../protocol";
import { useResource, useSubmit } from "../files/hooks";
import { FormError, Loaded, SubPage } from "../files/ui";
import { getBoardDetails, trimBoard } from "./api";
import type { Trims } from "./api";
import { TextField } from "./fields";
import { formatNumber, parseNumber } from "./geometry";

const EDGES: { name: keyof Trims; label: string; area: string }[] = [
  { name: "trimTop", label: "Trim top", area: "top" },
  { name: "trimLeft", label: "Trim left", area: "left" },
  { name: "trimRight", label: "Trim right", area: "right" },
  { name: "trimBottom", label: "Trim bottom", area: "bottom" },
];

const NO_TRIM: Trims = { trimTop: "0", trimBottom: "0", trimLeft: "0", trimRight: "0" };

const edgeProblems = (trims: Trims): Partial<Record<keyof Trims, string>> =>
  Object.fromEntries(
    EDGES.flatMap(({ name }) => {
      const value = parseNumber(trims[name]);
      return value === null ? [[name, "Enter a number."]] : value < 0 ? [[name, "Cannot be negative."]] : [];
    }),
  );

/** Trims that leave nothing of the board, which would corrupt its cut record. */
const sizeProblem = (trims: Trims, board: BoardDetails) => {
  const amount = (name: keyof Trims) => parseNumber(trims[name]) ?? 0;
  return amount("trimLeft") + amount("trimRight") >= board.width
    ? "Left and right trims together must be less than the board's width."
    : amount("trimTop") + amount("trimBottom") >= board.height
      ? "Top and bottom trims together must be less than the board's height."
      : null;
};

const TrimForm = ({ board }: { board: BoardDetails }) => {
  const fetchImpl = useFetch();
  const navigate = useNavigate();
  const [trims, setTrims] = useState(NO_TRIM);
  const [checked, setChecked] = useState(false);
  const { busy, error, setError, run } = useSubmit();
  const problems = checked ? edgeProblems(trims) : {};

  const submit = (event: FormEvent) => {
    event.preventDefault();
    setChecked(true);
    if (Object.keys(edgeProblems(trims)).length > 0) return setError("Fix the highlighted fields.");
    const tooMuch = sizeProblem(trims, board);
    if (tooMuch) return setError(tooMuch);
    return run(async () => {
      await trimBoard(fetchImpl, trims, board.units);
      navigate("/boards");
    }, "Could not trim the board.");
  };

  return (
    <form onSubmit={submit} noValidate>
      <p className="muted">
        The board is {formatNumber(board.width)} × {formatNumber(board.height)} {board.units}. Trimming shrinks it and
        keeps the cut record of what remains.
      </p>
      <div className="trim-grid">
        {EDGES.map(({ name, label, area }) => (
          <TextField
            key={name}
            className={`trim-${area}`}
            label={label}
            value={trims[name]}
            onChange={(value) => setTrims((current) => ({ ...current, [name]: value }))}
            units={board.units}
            error={problems[name]}
          />
        ))}
      </div>
      <FormError message={error} />
      <div className="actions">
        <button type="submit" className="primary" disabled={busy}>
          {busy ? "Trimming…" : "Trim board"}
        </button>
        <Link className="button" to="/boards">
          Cancel
        </Link>
      </div>
    </form>
  );
};

export const TrimBoard = () => {
  const { resource } = useResource(getBoardDetails);
  return (
    <SubPage title="Trim board" back="/boards">
      <Loaded resource={resource} what="the current board">
        {(board) => <TrimForm board={board} />}
      </Loaded>
    </SubPage>
  );
};
