import { useState } from "react";
import type { FormEvent } from "react";
import { Link, useNavigate } from "react-router";
import { useFetch, useMachine } from "../../app/context";
import type { BoardDetails } from "../../protocol";
import { useResource, useSubmit } from "../files/hooks";
import { FormError, Loaded, SubPage } from "../files/ui";
import { editBoard, getBoardDetails } from "./api";
import type { BoardFields } from "./api";
import { TextField } from "./fields";
import { ALIGNMENTS, alignedCenter, formatNumber, parseNumber } from "./geometry";
import type { Alignment } from "./geometry";

type Measure = "width" | "height" | "thickness" | "centerX" | "centerY";

const MEASURES: { name: Measure; label: string; positive: boolean }[] = [
  { name: "width", label: "Width", positive: true },
  { name: "height", label: "Height", positive: true },
  { name: "thickness", label: "Thickness", positive: true },
  { name: "centerX", label: "Center X", positive: false },
  { name: "centerY", label: "Center Y", positive: false },
];

const measureProblems = (fields: BoardFields): Partial<Record<Measure, string>> =>
  Object.fromEntries(
    MEASURES.flatMap(({ name, positive }) => {
      const value = parseNumber(fields[name]);
      return value === null
        ? [[name, "Enter a number."]]
        : positive && value <= 0
          ? [[name, "Must be more than zero."]]
          : [];
    }),
  );

const initialFields = (board: BoardDetails): BoardFields => ({
  boardID: board.boardID,
  material: board.material,
  width: formatNumber(board.width),
  height: formatNumber(board.height),
  thickness: formatNumber(board.thickness),
  centerX: formatNumber(board.centerX),
  centerY: formatNumber(board.centerY),
});

const EditForm = ({ board }: { board: BoardDetails }) => {
  const fetchImpl = useFetch();
  const navigate = useNavigate();
  const livePosition = useMachine((s) => s.position);
  const router = livePosition ?? board.router;
  const [fields, setFields] = useState(() => initialFields(board));
  const [checked, setChecked] = useState(false);
  const { busy, error, setError, run } = useSubmit();
  const problems = checked ? measureProblems(fields) : {};

  const set = (name: keyof BoardFields) => (value: string) => setFields((current) => ({ ...current, [name]: value }));

  const align = (alignment: Alignment) => {
    const width = parseNumber(fields.width);
    const height = parseNumber(fields.height);
    if (width === null || height === null) return setError("Enter the width and height before aligning.");
    setError(null);
    const { centerX, centerY } = alignedCenter(router, { width, height }, alignment);
    setFields((current) => ({
      ...current,
      centerX: centerX === null ? current.centerX : formatNumber(centerX),
      centerY: centerY === null ? current.centerY : formatNumber(centerY),
    }));
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    setChecked(true);
    if (Object.keys(measureProblems(fields)).length > 0) return setError("Fix the highlighted fields.");
    return run(async () => {
      await editBoard(fetchImpl, fields, board.units);
      navigate("/boards");
    }, "Could not update the board.");
  };

  return (
    <form onSubmit={submit} noValidate>
      <div className="board-fields">
        <TextField label="Board ID" value={fields.boardID} onChange={set("boardID")} />
        <TextField label="Material" value={fields.material} onChange={set("material")} />
        {MEASURES.map(({ name, label }) => (
          <TextField
            key={name}
            label={label}
            value={fields[name]}
            onChange={set(name)}
            units={board.units}
            error={problems[name]}
          />
        ))}
      </div>
      <fieldset>
        <legend>Align board to router</legend>
        <p className="muted">
          Moves the board's center so that edge or corner sits under the router, now at X {formatNumber(router.x)}, Y{" "}
          {formatNumber(router.y)}.
        </p>
        <div className="align-grid">
          {ALIGNMENTS.map((alignment) => (
            <button key={alignment.label} type="button" onClick={() => align(alignment)}>
              {alignment.label}
            </button>
          ))}
        </div>
      </fieldset>
      <FormError message={error} />
      <div className="actions">
        <button type="submit" className="primary" disabled={busy}>
          {busy ? "Saving…" : "Save board"}
        </button>
        <Link className="button" to="/boards">
          Cancel
        </Link>
      </div>
    </form>
  );
};

export const EditBoard = () => {
  const { resource } = useResource(getBoardDetails);
  return (
    <SubPage title="Create or edit board" back="/boards">
      <Loaded resource={resource} what="the current board">
        {(board) => <EditForm board={board} />}
      </Loaded>
    </SubPage>
  );
};
