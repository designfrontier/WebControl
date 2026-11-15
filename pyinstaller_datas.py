"""The data files main.spec and main-onedir.spec bundle, as PyInstaller (source, destination) pairs."""
import os

REQUIRED = ["templates", "static", "tools", "docs", "defaultwebcontrol.json"]
# contract/ is only read by the tests, so it stays out of the bundle.
OPTIONAL = ["firmware"]
FRONTEND_DIST = os.path.join("frontend", "dist")

MISSING_DIST = (
    "frontend/dist/index.html is missing, so the new UI at /next would not be bundled. "
    "Build it first: cd frontend && npm ci && npm run build"
)


def bundle_datas(root, log=print):
    if not os.path.isfile(os.path.join(root, FRONTEND_DIST, "index.html")):
        raise SystemExit(MISSING_DIST)
    missing = [name for name in REQUIRED if not os.path.exists(os.path.join(root, name))]
    if missing:
        raise SystemExit("missing from the source tree: " + ", ".join(missing))
    present = [name for name in OPTIONAL if os.path.isdir(os.path.join(root, name))]
    for name in sorted(set(OPTIONAL) - set(present)):
        log(f"WARNING: {name}/ not found, so this build has no bundled {name}")
    return [
        (os.path.join(root, name), "." if os.path.isfile(os.path.join(root, name)) else name)
        for name in REQUIRED + present + [FRONTEND_DIST]
    ]
