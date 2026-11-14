"""JSON API for the new frontend, mounted at /api.

Each module here that defines a module-level `blueprint` is registered under /api automatically, so a feature adds
endpoints by adding api/<feature>.py, without editing a shared list.
"""
import importlib
import pkgutil

from flask import Blueprint


def discover_blueprints():
    modules = [
        importlib.import_module(f"{__name__}.{info.name}")
        for info in pkgutil.iter_modules(__path__)
        if not info.name.startswith("_")
    ]
    return [module.blueprint for module in modules if isinstance(getattr(module, "blueprint", None), Blueprint)]


def create_api_blueprint():
    api = Blueprint("api", __name__, url_prefix="/api")
    for child in discover_blueprints():
        api.register_blueprint(child)
    return api


def init_api(app):
    app.register_blueprint(create_api_blueprint())
