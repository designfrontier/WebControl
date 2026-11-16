/** Comma-locale keyboards type "2,5": a lone comma becomes the point, while mixed or repeated separators stay and fail. */
export const decimalPoint = (text: string) => (/^[^.,]*,[^.,]*$/.test(text) ? text.replace(",", ".") : text);
