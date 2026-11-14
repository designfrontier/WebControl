"""Serves the built React frontend (frontend/dist) at /next, falling back to index.html for client-side routes."""
import os
import sys

from flask import Blueprint, Response, send_from_directory

from app import base_dir

# app.base_dir is "." outside PyInstaller, which would tie /next to the process's working directory.
DEFAULT_DIST = os.path.join(
    base_dir if hasattr(sys, "_MEIPASS") else os.path.dirname(os.path.abspath(__file__)), "frontend", "dist"
)

NOT_BUILT = (
    "The new WebControl UI has not been built. Run `npm ci && npm run build` in frontend/, then reload. "
    "The classic UI is still at /."
)


def create_next_blueprint(dist_dir=DEFAULT_DIST):
    dist_dir = os.path.abspath(dist_dir)
    next_ui = Blueprint("next_ui", __name__)

    @next_ui.get("/next/", defaults={"path": ""})
    @next_ui.get("/next/<path:path>")
    def serve(path):
        index = os.path.join(dist_dir, "index.html")
        if not os.path.isfile(index):
            return Response(NOT_BUILT, status=503, mimetype="text/plain")
        if path and os.path.isfile(os.path.join(dist_dir, path)):
            return send_from_directory(dist_dir, path)
        if "." in path.rsplit("/", 1)[-1]:
            return Response("Not found", status=404, mimetype="text/plain")
        response = send_from_directory(dist_dir, "index.html")
        response.headers["Cache-Control"] = "no-cache"
        return response

    return next_ui


def init_next_ui(app, dist_dir=DEFAULT_DIST):
    app.register_blueprint(create_next_blueprint(dist_dir))
