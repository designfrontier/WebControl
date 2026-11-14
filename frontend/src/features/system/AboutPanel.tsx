import type { SystemInfo } from "../../protocol";

const REPO = "https://github.com/WebControlCNC/WebControl/";

export const AboutPanel = ({ info }: { info: SystemInfo | null }) => (
  <section aria-labelledby="about-title" className="system-panel about">
    <h2 id="about-title">About WebControl</h2>
    <dl className="facts">
      <dt>Version</dt>
      <dd>{info ? `v${info.version}` : "…"}</dd>
      <dt>Platform</dt>
      <dd>{info?.platform ?? "…"}</dd>
      <dt>Controller firmware</dt>
      <dd>{info ? (info.controllerFirmwareVersion > 0 ? info.controllerFirmwareVersion : "Not reported") : "…"}</dd>
    </dl>
    <h3>License</h3>
    <p>WebControl is licensed under the GNU General Public License v3.0.</p>
    <p className="muted">
      Permissions of this strong copyleft license are conditioned on making available complete source code of licensed
      works and modifications, which include larger works using a licensed work, under the same license. Copyright and
      license notices must be preserved. Contributors provide an express grant of patent rights.
    </p>
    <p>
      <a href={REPO} target="_blank" rel="noreferrer">
        {REPO}
      </a>
    </p>
  </section>
);
