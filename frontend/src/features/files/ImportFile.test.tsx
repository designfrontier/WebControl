import { fireEvent, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch } from "../../test/fakeFetch";
import { expectPath, recorder } from "../../test/filesBoards";
import { renderApp } from "../../test/render";

const file = (name: string) => new File(["{}"], name);

describe.each([
  ["restore", "/importRestoreWebControl", "Restore WebControl", "Restore", "backup.zip", ".zip"],
  ["groundcontrol", "/importFile", "Import groundcontrol.ini", "Import", "groundcontrol.ini", ".ini"],
  ["webcontrol", "/importFileWCJSON", "Import webcontrol.json", "Import", "webcontrol.json", ".json"],
])("import %s", (kind, route, title, button, fileName, accept) => {
  const setup = (post = recorder()) => ({
    ...renderApp(`/files/import/${kind}`, { fetch: fakeFetch({ [`POST ${route}`]: post.respond }) }),
    post,
  });
  const page = () => within(screen.getByRole("region", { name: title }));

  it(`posts the file to ${route} and confirms`, async () => {
    const { user, post } = setup();
    expect(screen.getByLabelText("File")).toHaveAttribute("accept", accept);
    await user.upload(screen.getByLabelText("File"), file(fileName));
    await user.click(screen.getByRole("button", { name: button }));

    expect((post.forms()[0].file as File).name).toBe(fileName);
    expect(await page().findByText(`${fileName} was imported.`)).toBeInTheDocument();
    expect(page().getByRole("link", { name: "Done" })).toHaveAttribute("href", "/files");
  });

  it("needs a file", async () => {
    const { user, post } = setup();
    await user.click(screen.getByRole("button", { name: button }));
    expect(screen.getByRole("alert")).toHaveTextContent("Choose a file first.");
    expect(post.calls).toEqual([]);
  });

  it("explains a rejected file", async () => {
    const { user } = setup(recorder(500));
    await user.upload(screen.getByLabelText("File"), file(fileName));
    await user.click(screen.getByRole("button", { name: button }));
    expect(await screen.findByRole("alert")).toHaveTextContent(`Could not use ${fileName}.`);
    expect(page().getByRole("link", { name: "Cancel" })).toBeInTheDocument();
  });
});

describe("import", () => {
  it("clears the confirmation when another file is chosen", async () => {
    const { user } = renderApp("/files/import/webcontrol", {
      fetch: fakeFetch({ "POST /importFileWCJSON": recorder().respond }),
    });
    await user.upload(screen.getByLabelText("File"), file("a.json"));
    await user.click(screen.getByRole("button", { name: "Import" }));
    expect(await screen.findByText("a.json was imported.")).toBeInTheDocument();

    await user.upload(screen.getByLabelText("File"), file("b.json"));
    expect(screen.queryByText(/was imported/)).not.toBeInTheDocument();
  });

  it("forgets the file when the selection is cleared", async () => {
    const { user } = renderApp("/files/import/restore");
    await user.upload(screen.getByLabelText("File"), file("a.zip"));
    fireEvent.change(screen.getByLabelText("File"), { target: { files: [] } });
    await user.click(screen.getByRole("button", { name: "Restore" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Choose a file first.");
  });
});

describe("import route", () => {
  it.each(["nope", "toString", "constructor"])("sends /files/import/%s back to the files screen", async (kind) => {
    renderApp(`/files/import/${kind}`);
    await expectPath("/files");
  });
});
