import { useState } from "react";
import { useClient } from "../../app/context";
import { ConfirmDialog } from "../../app/Dialog";
import { Z_MOVES, zMoveTitle } from "./zMoves";
import type { ZMove } from "./zMoves";

interface Props {
  move: ZMove;
  disabled: boolean;
  className?: string;
}

/** Go to Z0 plunges toward the work, so it asks first; Go to Safe Height only rises. */
export const ZMoveButton = ({ move, disabled, className }: Props) => {
  const client = useClient();
  const [confirming, setConfirming] = useState(false);
  const send = () => {
    setConfirming(false);
    client.sendAction(move);
  };

  return (
    <>
      <button
        type="button"
        className={className}
        disabled={disabled}
        title={zMoveTitle(move, !disabled)}
        onClick={() => (move === "zToZero" ? setConfirming(true) : send())}
      >
        {Z_MOVES[move].label}
      </button>
      {confirming && (
        <ConfirmDialog
          title="Go to Z0"
          confirmLabel="Go to Z0"
          danger
          onConfirm={send}
          onCancel={() => setConfirming(false)}
        >
          <p>Move the bit down to Z0? Make sure the bit is clear of clamps.</p>
        </ConfirmDialog>
      )}
    </>
  );
};
