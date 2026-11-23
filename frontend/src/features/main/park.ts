import type { ParkPosition } from "../../protocol";

export const NO_PARK = "No park position yet: press Set Park Here, or enter one in Settings ▸ Maslow.";

/** The park position as stored: mm from the bed's centre, in machine coordinates. */
export const parkText = (park: ParkPosition) => `Parks at X ${park.x}, Y ${park.y} mm (machine coordinates).`;

export const parkNote = (park: ParkPosition | null) => (park === null ? NO_PARK : parkText(park));
