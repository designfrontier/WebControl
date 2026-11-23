export type ZMove = "zToZero" | "zToSafeHeight";

export const Z_MOVES: Record<ZMove, { label: string; title: string }> = {
  zToZero: { label: "Go to Z0", title: "Lower the bit to Z zero, where Set Z Zero Here or Touch Plate put it" },
  zToSafeHeight: { label: "Go to Safe Height", title: "Raise the bit to the Z-axis safe height" },
};

export const Z_MOVE_BUSY = "Not while a job runs or Touch Plate probes";

export const isZMove = (button: string): button is ZMove => button in Z_MOVES;

export const zMoveTitle = (move: ZMove, enabled: boolean) => (enabled ? Z_MOVES[move].title : Z_MOVE_BUSY);
