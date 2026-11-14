import { useState } from "react";

interface Props {
  on: boolean;
  /** base64 PNG from the webcam, or null before the first frame. */
  image: string | null;
}

/** Shows each webcam frame only once it has decoded, so frames swap without a blank flash. */
export const CameraOverlay = ({ on, image }: Props) => {
  const [shown, setShown] = useState<string | null>(null);
  if (!on || !image) return null;
  const next = `data:image/png;base64,${image}`;

  return (
    <div className="viewer-camera">
      {shown ? <img src={shown} alt="Webcam view of the machine" /> : null}
      {next === shown ? null : <img className="viewer-camera-next" src={next} alt="" onLoad={() => setShown(next)} />}
    </div>
  );
};
