import { useEffect, useRef } from "react";
import { useParams } from "react-router";
import type { Fetch } from "../../api/http";
import { getHelpPage } from "./api";
import { LazyMarkdown as MarkdownView } from "./LazyMarkdown";
import { errorText, useLoad } from "./useLoad";

const INDEX_PAGE = "assets/helpPages.md";
const loadPage = (fetchImpl: Fetch, path: string) => getHelpPage(fetchImpl, path);

/** Offline help: docs/*.md rendered client-side, with the classic help index alongside. */
export const HelpPanel = () => {
  const path = useParams()["*"] || "index.md";
  const page = useLoad(loadPage, path);
  const index = useLoad(loadPage, INDEX_PAGE);
  const article = useRef<HTMLElement>(null);

  useEffect(() => {
    if (page.status === "ready") article.current?.scrollIntoView?.({ block: "start" });
  }, [page.status, path]);

  return (
    <section aria-labelledby="help-title" className="system-panel help">
      <h2 id="help-title">Help</h2>
      <p className="muted">
        Also online at{" "}
        <a href="https://webcontrolcnc.github.io/WebControl/" target="_blank" rel="noreferrer">
          webcontrolcnc.github.io/WebControl
        </a>
        .
      </p>
      <div className="help-layout">
        <nav aria-label="Help pages" className="help-index">
          {index.status === "ready" ? (
            <MarkdownView docs>{index.data.markdown}</MarkdownView>
          ) : (
            index.status === "error" && <p className="muted">The help index is unavailable.</p>
          )}
        </nav>
        <article ref={article} className="help-page" aria-busy={page.status === "loading"}>
          {page.status === "loading" && <p role="status">Loading help…</p>}
          {page.status === "error" && (
            <div className="system-error" role="alert">
              <p>
                Could not load {path}: {errorText(page.error)}
              </p>
              <button type="button" onClick={page.reload}>
                Retry
              </button>
            </div>
          )}
          {page.status === "ready" && <MarkdownView docs>{page.data.markdown}</MarkdownView>}
        </article>
      </div>
    </section>
  );
};
