interface SetupFigureProps {
  folder: "gettingStarted" | "quickConfig";
  file: string;
  caption: string;
}

export const SetupFigure = ({ folder, file, caption }: SetupFigureProps) => {
  const wide = folder === "gettingStarted";
  return (
    <figure className={wide ? "setup-figure wide" : "setup-figure"}>
      <img
        src={`/static/images/${folder}/${encodeURIComponent(file)}`}
        alt={caption}
        loading="lazy"
        {...(wide ? {} : { width: 300, height: 300 })}
      />
      <figcaption>{caption}</figcaption>
    </figure>
  );
};
