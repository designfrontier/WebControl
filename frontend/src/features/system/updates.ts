import type { Updates } from "../../protocol";

export const DEFAULT_REPO = "designfrontier/WebControl";

/** The `webcontrol-update` line to run on the Pi for this install. */
export const updateCommand = ({ repo, status }: Pick<Updates, "repo" | "status">) =>
  [
    "webcontrol-update",
    ...(repo === DEFAULT_REPO ? [] : ["--repo", repo]),
    ...(status === "devBuild" ? ["--force"] : []),
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
export const explanation = ({ status, installed, latest }: Updates) => {
  if (!latest) return "";
  if (status === "updateAvailable")
    return `${latest.tag} is published; this Pi runs ${installed.version}. Update it on the Pi as below.`;
  if (status === "upToDate") return `This Pi runs ${latest.tag}, the latest published release.`;
  if (status === "devBuild")
    return (
      `This is a development build of ${installed.version} (commit ${installed.commit}), not the ${latest.tag} ` +
      `release${latest.commit ? ` (commit ${shortCommit(latest.commit)})` : ""}. The command below replaces it with the release.`
    );
  if (status === "ahead")
    return `This build (${installed.version}) is newer than ${latest.tag}, the latest published release. There is nothing to update to.`;
  return `The installed version (${installed.version}) or the release tag (${latest.tag}) is not a version number this page can compare.`;
};

/** A date and time as this browser writes them, or the text unchanged if it is not one. */
export const when = (value: string | number, options: Intl.DateTimeFormatOptions = { dateStyle: "medium" }) => {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? String(value) : date.toLocaleString(undefined, options);
};

/** Only an https link leaves the page. */
export const safeUrl = (url: string) => (/^https:\/\//i.test(url) ? url : undefined);
