# The tool library

Run after `jobs.md` section 3 (the air cut), router off and bit out. CI covers the API and the prompt text against the
simulator; this checks them on the Pi.

## 1. The library

- [ ] Settings ▸ Tools on the desktop: add tool 1, `1/4" 2 flute endmill`, 6.35 mm, 2 flutes. It lists as `T1`
      with "6.35 mm · 2 flutes". On the Pi, `~/.WebControl/tools.json` holds it.
- [ ] Edit it from a phone: the desktop's list follows without a reload. Add tool 1 again: refused with "Your tool
      library already has a tool 1." Delete asks a second time before it removes anything.
- [ ] Restart `webcontrol-rs`: the library is still there; `tools.json.bak` holds the version before the last save.

## 2. At a tool change

- [ ] Run the air-cut file (`jobs.md`) with tool 2 in the library as `60° v-bit`: at `T2 M6` the prompt reads
      `Tool 2 — 60° v-bit` above Resume. Delete tool 2 and run it again: the prompt reads
      `Tool 2 (not in your tool library)`.
- [ ] While the tool change waits, add tool 2 in Settings ▸ Tools: it saves (the library is not locked by a job), the
      prompt follows, and Resume still continues the job.
