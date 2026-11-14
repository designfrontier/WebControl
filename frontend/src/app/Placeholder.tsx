interface Props {
  title: string;
  /** Where the classic UI implements this screen, for whoever ports it. */
  portedFrom: string;
}

export const Placeholder = ({ title, portedFrom }: Props) => (
  <section className="placeholder" aria-labelledby="placeholder-title">
    <h1 id="placeholder-title">{title}</h1>
    <p>This screen has not been ported to the new UI yet.</p>
    <p className="muted">
      Classic UI source: <code>{portedFrom}</code>
    </p>
    <p>
      <a href="/">Open the classic UI</a>
    </p>
  </section>
);
