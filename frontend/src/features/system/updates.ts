import type { Updates } from "../../protocol";

export const DEFAULT_REPO = "designfrontier/WebControl";

const statusFlags = ({ status, latest }: Pick<Updates, "status" | "latest">) => {
  if (status === "devBuild") return ["--force"];
  return status === "ahead" && latest ? ["--version", latest.tag] : [];
};

/** The `webcontrol-update` line to run on the Pi for this install; on its own it never installs an older version. */
export const updateCommand = (updates: Pick<Updates, "repo" | "status" | "latest">) =>
  [
    "webcontrol-update",
    ...(updates.repo === DEFAULT_REPO ? [] : ["--repo", updates.repo]),
    ...statusFlags(updates),
  ].join(" ");

export type Tone = "ok" | "warn" | "danger" | undefined;

export const headline = ({ status, latest, error }: Updates): { text: string; tone: Tone } => {
  if (status === "updateAvailable") return { text: `Update available: ${latest?.tag ?? ""}`, tone: "warn" };
  if (status === "upToDate") return { text: "Up to date", tone: "ok" };
  if (status === "devBuild") return { text: "Development build", tone: "warn" };
  if (status === "ahead") return { text: "Newer than the latest release", tone: "ok" };
  return error && !latest
    ? { text: "Couldn't reach GitHub", tone: "danger" }
    : { text: "Can't tell whether an update is available", tone: undefined };
};

const shortCommit = (sha: string) => sha.slice(0, 7);

/** One sentence on what the status means for this install. */
export const explanation = ({ status, installed, latest, canUpdate }: Updates) => {
  if (!latest) return "";
  if (status === "updateAvailable")
    return `${latest.tag} is published; this Pi runs ${installed.version}. ${canUpdate ? "Update it with the button below." : "Update it on the Pi as below."}`;
  if (status === "upToDate") return `This Pi runs ${latest.tag}, the latest published release.`;
  if (status === "devBuild")
    return (
      `This is a development build of ${installed.version} (commit ${installed.commit}), not the ${latest.tag} ` +
      `release${latest.commit ? ` (commit ${shortCommit(latest.commit)})` : ""}. The command below replaces it with the release.`
    );
  if (status === "ahead")
    return (
      `This build (${installed.version}) is newer than ${latest.tag}, the latest published release. There is ` +
      `nothing to update to, and webcontrol-update on its own leaves it alone; --version installs the release over it.`
    );
  return `The installed version (${installed.version}) or the release tag (${latest.tag}) is not a version number this page can compare.`;
};

/** A date and time as this browser writes them, or the text unchanged if it is not one. */
export const when = (value: string | number, options: Intl.DateTimeFormatOptions = { dateStyle: "medium" }) => {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? String(value) : date.toLocaleString(undefined, options);
};

/** Only an https link leaves the page. */
export const safeUrl = (url: string) => (/^https:\/\//i.test(url) ? url : undefined);
