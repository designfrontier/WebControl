import { screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { jsonResponse } from "../../test/fakeFetch";
import { currentPath, renderApp } from "../../test/render";
import { helpPage, systemFetch } from "../../test/systemFixtures";

const FIRST_STEPS = helpPage(
  "GettingStarted/firstSteps.md",
  [
    "# First Steps",
    "",
    "See [triangular calibration](Actions/Calibration-Setup/triangularCalibration.md) and [Maslow](https://maslowcnc.com).",
    "",
    "![Chains Off Top](../assets/gettingStarted/chainOffSprocketsTop.png)",
    "",
    "|Result|Cause|",
    "|---|---|",
    "|Fail|Cable|",
    "",
    "[Download](files/x.zip) <b>raw</b>",
  ].join("\n"),
  "First Steps",
);

const article = () => within(document.querySelector("article")!);
const index = () => within(screen.getByRole("navigation", { name: "Help pages" }));

const renderHelp = (route = "/system/help", routes = {}) =>
  renderApp(route, {
    fetch: systemFetch({ "/api/system/help?path=GettingStarted%2FfirstSteps.md": FIRST_STEPS, ...routes }),
  });

describe("HelpPanel", () => {
  it("shows the docs home with the help index alongside", async () => {
    renderHelp();
    expect(await article().findByRole("heading", { name: "WebControl" })).toBeInTheDocument();
    expect(index().getByRole("link", { name: "First Steps" })).toHaveAttribute(
      "href",
      "/system/help/GettingStarted/firstSteps.md",
    );
    expect(index().getByRole("link", { name: "Home" })).toHaveAttribute("href", "/system/help");
  });

  it("scrolls to the page only after a link is followed, home included", async () => {
    const scrollIntoView = vi.fn();
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, value: scrollIntoView });
    const { user } = renderHelp();
    await article().findByRole("heading", { name: "WebControl" });
    expect(scrollIntoView).not.toHaveBeenCalled();

    await user.click(index().getByRole("link", { name: "First Steps" }));
    await article().findByRole("heading", { name: "First Steps" });
    expect(scrollIntoView).toHaveBeenCalledTimes(1);

    await user.click(index().getByRole("link", { name: "Home" }));
    await article().findByRole("heading", { name: "WebControl" });
    expect(scrollIntoView).toHaveBeenCalledTimes(2);
    Reflect.deleteProperty(HTMLElement.prototype, "scrollIntoView");
  });

  it("follows doc links client-side and renders tables, images and external links", async () => {
    const { user } = renderHelp();
    await user.click(await index().findByRole("link", { name: "First Steps" }));
    expect(currentPath()).toBe("/system/help/GettingStarted/firstSteps.md");

    expect(await article().findByRole("heading", { name: "First Steps" })).toBeInTheDocument();
    expect(article().getByRole("link", { name: "triangular calibration" })).toHaveAttribute(
      "href",
      "/system/help/Actions/Calibration-Setup/triangularCalibration.md",
    );
    expect(article().getByRole("link", { name: "Maslow" })).toHaveAttribute("target", "_blank");
    expect(article().getByRole("link", { name: "Download" })).toHaveAttribute("href", "files/x.zip");
    expect(article().getByRole("img", { name: "Chains Off Top" })).toHaveAttribute(
      "src",
      "/api/system/help/assets/gettingStarted/chainOffSprocketsTop.png",
    );
    expect(article().getByRole("cell", { name: "Cable" })).toBeInTheDocument();
    expect(article().queryByText("raw", { selector: "b" })).not.toBeInTheDocument();
  });

  it("keeps absolute image URLs as they are", async () => {
    renderHelp("/system/help/GettingStarted/firstSteps.md", {
      "/api/system/help?path=GettingStarted%2FfirstSteps.md": helpPage("x", "![Logo](https://example.com/l.png)"),
    });
    expect(await article().findByRole("img", { name: "Logo" })).toHaveAttribute("src", "https://example.com/l.png");
  });

  it("reports a missing page and retries", async () => {
    let missing = true;
    const respond = () => (missing ? jsonResponse({ error: "nope" }, 404) : jsonResponse(helpPage("x", "# Found")));
    const { user } = renderHelp("/system/help/Gone.md", { "/api/system/help?path=Gone.md": respond });
    expect(await article().findByRole("alert")).toHaveTextContent("Could not load Gone.md");
    missing = false;
    await user.click(article().getByRole("button", { name: "Retry" }));
    expect(await article().findByRole("heading", { name: "Found" })).toBeInTheDocument();
  });

  it("says so when the index cannot load", async () => {
    renderHelp("/system/help", { "/api/system/help?path=assets%2FhelpPages.md": () => jsonResponse({}, 500) });
    expect(await index().findByText("The help index is unavailable.")).toBeInTheDocument();
  });
});
