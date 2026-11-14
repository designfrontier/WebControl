import { calculateTriangular, getTriangular } from "./api";
import { MeasurementCalibration } from "./MeasurementCalibration";
import type { MeasurementConfig } from "./MeasurementCalibration";

const config: MeasurementConfig = {
  title: "Triangular calibration",
  intro: (
    <p>
      Cuts four vertical marks and one corner mark across the work area, then fits the motor height, rotation radius and
      chain sag to your measurements. Enter every distance in millimetres.
    </p>
  ),
  cutAction: "cutTriangularCalibrationPattern",
  acceptAction: "acceptTriangularCalibrationResults",
  acceptNote: "Accepting saves the values and moves the sled to the center of the work area.",
  image: {
    src: "/static/images/triangularCalibration/triangularCalibration.png",
    alt: "Test pattern: measure from cut 1 to cut 2 near the top and from cut 3 to cut 4 lower down, as the arrows show; cut 5 is the corner mark at top right, measured from the top edge of the work area.",
  },
  fields: [
    { key: "cut12", label: "Distance between cuts 1 and 2", initial: "1930" },
    { key: "cut34", label: "Distance between cuts 3 and 4", initial: "1930" },
    { key: "cut5", label: "Top of work area to cut 5", initial: "300" },
    { key: "bitDiameter", label: "Router bit diameter", initial: "6.35" },
  ],
  parameters: [
    { key: "motorYoffset", label: "Height of motors" },
    { key: "rotationRadius", label: "Rotation radius" },
    { key: "chainSagCorrection", label: "Chain sag correction" },
  ],
  load: getTriangular,
  calculate: calculateTriangular,
};

export const TriangularCalibration = () => <MeasurementCalibration config={config} />;
