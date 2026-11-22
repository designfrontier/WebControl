# The tool library

Run after `jobs.md` section 3 (the air cut), router off and bit out. CI covers the comment parsing, the API and the
prompt text against the simulator; this checks them on the Pi with real files.

## 1. The library

- [ ] Settings ▸ Tools on the desktop: add tool 1, `1/4" 2 flute endmill`, 6.35 mm, 2 flutes. It lists as `T1`
      with "6.35 mm · 2 flutes". On the Pi, `~/.WebControl/tools.json` holds it.
- [ ] Edit it from a phone: the desktop's list follows without a reload. Add tool 1 again: refused with "Your tool
      library already has a tool 1." Delete asks a second time before it removes anything.
- [ ] Restart `webcontrol-rs`: the library is still there; `tools.json.bak` holds the version before the last save.

## 2. A file's tools at a tool change

- [ ] Open a Fusion 360 file, e.g. `nullcase.final/2.clearing.25.endmill.nullcase.nc`. Under the 3D view, "Tools: T6"
      opens to `Tool 6 — 6.35 mm (1/4") flat end mill` with "From this file; not in your tool library."
- [ ] Open a Carbide Create file with `M0 ;T10` (e.g. `planck/planckBottomRecessFor12mmFeet.25.endmill.nc`): the
      viewer caption shows "Tools: T10" and, opened, `Tool 10 (not in your tool library)`.
- [ ] Run the air-cut file with tool 2 in the library as `60° v-bit`: at `T2 M6` the prompt reads
      `Insert Tool 2 — 60° v-bit` above Resume, with no separate "Please insert tool 2". Add
      `(T2 D=3.175 CR=0 - ZMIN=-1 - flat end mill)` to the file's top and run it again: the prompt shows
      `Insert Tool 2 — 3.175 mm (1/8") flat end mill` and "From this file; your library says 60° v-bit." Press Update
      tool library, save: the note goes and Settings ▸ Tools shows the file's description.
- [ ] While the tool change waits, add a tool in Settings ▸ Tools: it saves (the library is not locked by a job), and
      Resume still continues the job.
