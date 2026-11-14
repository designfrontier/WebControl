/** Maps a docs link to a help route: docs links are relative to docs/, and "help" is the classic UI's home link. */
export const helpRoute = (href: string) => {
  if (href === "help") return "/system/help";
  const [path, hash] = href.split("#");
  if (!path.endsWith(".md")) return null;
  const clean = path.replace(/^(\.\.?\/)+|^\/+/g, "");
  return `/system/help/${clean}${hash ? `#${hash}` : ""}`;
};
