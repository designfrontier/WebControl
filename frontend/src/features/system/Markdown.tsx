import Markdown, { defaultUrlTransform } from "react-markdown";
import type { Components } from "react-markdown";
import { Link } from "react-router";
import remarkGfm from "remark-gfm";
import { helpAssetUrl } from "./api";
import { helpRoute } from "./helpLinks";
import { safeUrl } from "./updates";

const EXTERNAL = /^[a-z][a-z0-9+.-]*:/i;

const NewTab: Components["a"] = ({ href, children }) =>
  href ? (
    <a href={href} target="_blank" rel="noopener noreferrer">
      {children}
    </a>
  ) : (
    <>{children}</>
  );

const httpsOnly = (url: string) => safeUrl(url) ?? "";

const docsComponents: Components = {
  a: ({ href = "", children }) => {
    if (EXTERNAL.test(href)) return <NewTab href={href}>{children}</NewTab>;
    const route = helpRoute(href);
    return route ? <Link to={route}>{children}</Link> : <a href={href}>{children}</a>;
  },
  img: ({ src, alt }) => {
    const source = typeof src === "string" ? src : "";
    return <img src={EXTERNAL.test(source) ? source : helpAssetUrl(source)} alt={alt ?? ""} loading="lazy" />;
  },
};

const notesComponents: Components = { a: NewTab };

/** Renders docs and release notes; raw HTML is dropped, never injected, and release notes link only to https. */
export const MarkdownView = ({ children, docs = false }: { children: string; docs?: boolean }) => (
  <div className="markdown">
    <Markdown
      remarkPlugins={[remarkGfm]}
      skipHtml
      urlTransform={docs ? defaultUrlTransform : httpsOnly}
      components={docs ? docsComponents : notesComponents}
    >
      {children}
    </Markdown>
  </div>
);
