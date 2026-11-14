"""Resolves user-supplied relative paths inside an allowed root, rejecting anything that would escape it."""
import os
from urllib.parse import unquote


class UnsafePathError(ValueError):
    pass


def _fully_decoded(text):
    decoded = unquote(text)
    return decoded if decoded == text else _fully_decoded(decoded)


def _escapes(relative):
    variants = {relative, _fully_decoded(relative)}
    normalised = [variant.replace("\\", "/") for variant in variants]
    return any(
        os.path.isabs(path) or path.startswith("/") or (len(path) > 1 and path[1] == ":") or ".." in path.split("/")
        for path in normalised
    )


def resolveInside(root, *parts):
    """The path of `parts` joined under `root`; raises UnsafePathError if it is absolute or leaves `root`."""
    texts = [str(part) for part in parts if str(part) != ""]
    relative = "/".join(texts)
    if "\x00" in relative or any(_escapes(text) for text in texts):
        raise UnsafePathError(relative)
    candidate = os.path.normpath(os.path.join(root, relative))
    realRoot = os.path.realpath(root)
    realCandidate = os.path.realpath(candidate)
    if realCandidate != realRoot and not realCandidate.startswith(realRoot + os.sep):
        raise UnsafePathError(relative)
    return candidate


def libraryRoot(config, kind):
    return os.path.join(config.getHome(), ".WebControl", kind)


def docsRoot(data):
    base = data.platformHome if data.platform == "PYINSTALLER" else os.path.dirname(os.path.dirname(__file__))
    return os.path.join(base, "docs")
