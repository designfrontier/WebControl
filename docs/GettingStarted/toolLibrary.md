---
layout: default
title: Tool Library
nav_order: 2
parent: Getting Started
---
# Tool Library

When a file changes tools (`T1 M6`), the controller pauses and asks for the tool by number. The tool library tells you
which bit that number is, so the prompt reads "Insert Tool 1 — 1/4" 2 flute endmill" rather than just "Please insert tool
1".

## Settings ▸ Tools

Open **Settings ▸ Tools** to list, add, edit and delete tools. Each tool has:

* **Tool number**: the T number your CAM program gives it. Each number is used once.
* **Description**: what you will read at the tool change, e.g. `1/4" 2 flute endmill`.
* **Diameter** in mm or inches, **Flutes** and **Notes**, all optional.

The library is kept in `~/.WebControl/tools.json` on the Pi, with the previous version as `tools.json.bak`. You can
edit it during a job, for example to add a tool while a tool change waits; it only changes what the prompts say,
never what is sent to the controller.

## Tools named in a file

Many CAM programs describe each tool in comments at the top of the file. WebControl reads:

* Fusion 360: `(T1 D=6.35 CR=0. - ZMIN=-3. - flat end mill)`, read as "6.35 mm (1/4") flat end mill".
* Carbide Create: `(TOOL/MILL,3.18,0.00,0.00,0.00)` before `M6T102`, and the tool number in `M0 ;T102`.
* Estlcam: `;Change tool: 3mm 2 flute endmill` before the tool change.
* Vectric VCarve and Aspire: the "Tools used in this file" list, e.g. `(1 = End Mill (6 mm))`.
* Plain notes such as `(T1 1/4" 2 flute endmill)`, `(Tool 2: v-bit)` or `; T3 ball nose`.

Under the 3D view, beside the loaded file's name, **Tools: T1, T6** lists the tools the file uses; open it to see
each one.

## Which description you see

At a tool change and in that list:

* If the file describes the tool, you see the file's description, since the file is what is about to cut.
  * When your library has no such tool, a note says so and **Add to tool library** offers the file's description.
  * When your library describes that number differently, a note shows what your library says and **Update tool
    library** offers to replace it with the file's.
* If the file does not describe the tool, you see your library's description.
* If neither does, you see "Tool 3 (not in your tool library)".

At a tool change that line starts with "Insert", e.g. "Insert Tool 3 — 1/8" ball end mill", in place of the
controller's "Please insert tool 3"; any other text from the controller stays above it.

Adding or updating opens the entry for editing first; nothing in your library changes until you save it.
