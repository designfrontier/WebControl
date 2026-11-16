const LONE_COMMA = /^[^.,]*,[^.,]*$/;
const THOUSANDS = /^[-+]?[1-9]\d{0,2},\d{3}$/;

/** Comma-locale keyboards type "2,5": a lone comma becomes the point. Mixed or repeated separators, and a
 *  "1,500" that reads as a thousands group, stay as typed and fail to parse. */
export const decimalPoint = (text: string) =>
  LONE_COMMA.test(text) && !THOUSANDS.test(text.trim()) ? text.replace(",", ".") : text;
