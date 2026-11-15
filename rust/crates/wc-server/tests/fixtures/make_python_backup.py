"""Builds the Python-written backup fixture the Rust restore test reads, as Actions.zipfolder writes one."""
import json
import os
import sys
import tempfile
import zipfile

target = sys.argv[1]
with tempfile.TemporaryDirectory() as home:
    folder = os.path.join(home, ".WebControl")
    os.makedirs(os.path.join(folder, "gcode", "jobs"))
    os.makedirs(os.path.join(folder, "boards"))
    with open(os.path.join(folder, "webcontrol.json"), "w") as out:
        json.dump({"Maslow Settings": [{"key": "COMport", "value": "/dev/ttyPYTHON", "type": "string"}]}, out,
                  sort_keys=True, indent=4)
    with open(os.path.join(folder, "gcode", "jobs", "leg.nc"), "w") as out:
        out.write("G21\nG0 X1 Y1\n")
    with open(os.path.join(folder, "boards", "shelf.brd"), "w") as out:
        out.write("{}\n")
    zipobj = zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED)
    rootlen = len(folder) + 1
    for base, dirs, files in sorted(os.walk(folder)):
        for file in sorted(files):
            fn = os.path.join(base, file)
            zipobj.write(fn, fn[rootlen:])
    zipobj.close()
