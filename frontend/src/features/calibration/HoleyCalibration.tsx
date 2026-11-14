import type { Fetch } from "../../api/http";
import { calculateHoley, getHoley } from "./api";
import { MeasurementCalibration } from "./MeasurementCalibration";
import type { MeasurementConfig } from "./MeasurementCalibration";

const fields = Array.from({ length: 12 }, (_, index) => ({
  key: `M${index + 1}`,
  label: `M${index + 1}`,
  initial: "0",
}));

const config: MeasurementConfig = {
  title: "Holey calibration",
  intro: (
    <p>
      For holey firmware. Cuts a pattern of six holes, then fits the motor spacing, motor height and chain tolerances to
      the twelve distances M1 to M12 in the diagram. Enter every distance in millimetres.
    </p>
  ),
  cutAction: "cutHoleyCalibrationPattern",
  acceptAction: "acceptHoleyCalibrationResults",
  image: {
    src: "/static/images/holeyCalibration/holeyCalibration.png",
    alt: "Six holes in two rows of three. M1 to M4 run horizontally, M5 to M7 vertically, M8 to M11 diagonally, and M12 from the top edge of the sheet to the top middle hole.",
  },
  fields,
  parameters: [
    { key: "motorYoffset", label: "Height of motors" },
    { key: "distanceBetweenMotors", label: "Distance between motors" },
    { key: "leftChainTolerance", label: "Left chain tolerance" },
    { key: "rightChainTolerance", label: "Right chain tolerance" },
  ],
  load: getHoley,
  calculate: (fetchImpl: Fetch, values) =>
    calculateHoley(
      fetchImpl,
      fields.map(({ key }) => values[key]),
    ),
};

export const HoleyCalibration = () => <MeasurementCalibration config={config} />;
