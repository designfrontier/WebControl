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

## At a tool change

The prompt shows the library's description of the tool the controller asks for, or "Insert Tool 3 (not in your tool
library)" when the library has no tool 3. Any other text from the controller stays above that line. A tool you add while the prompt is open shows at once.
