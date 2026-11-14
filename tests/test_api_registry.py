import sys

import pytest
from flask import Flask

import api


@pytest.fixture
def extra_api_dir(tmp_path, monkeypatch):
    """Adds a scratch directory to the api package so tests can drop feature modules into it."""
    monkeypatch.setattr(api, "__path__", [*api.__path__, str(tmp_path)])
    yield tmp_path
    for name in [name for name in sys.modules if name.startswith("api.zz_")]:
        del sys.modules[name]


def fresh_app():
    app = Flask(__name__)
    api.init_api(app)
    return app


def test_registers_every_feature_module_under_api(client):
    response = client.get("/api/state")
    assert response.status_code == 200


def test_a_new_module_is_mounted_without_editing_a_list(extra_api_dir):
    (extra_api_dir / "zz_widgets.py").write_text(
        "from flask import Blueprint\n"
        "blueprint = Blueprint('zz_widgets', __name__)\n"
        "@blueprint.get('/widgets')\n"
        "def widgets():\n"
        "    return {'widgets': []}\n"
    )
    app = fresh_app()
    response = app.test_client().get("/api/widgets")
    assert response.status_code == 200
    assert response.get_json() == {"widgets": []}


def test_skips_private_modules_and_modules_without_a_blueprint(extra_api_dir):
    (extra_api_dir / "_zz_private.py").write_text("raise RuntimeError('private modules are never imported')\n")
    (extra_api_dir / "zz_helpers.py").write_text("VALUE = 1\n")
    names = [blueprint.name for blueprint in api.discover_blueprints()]
    assert "state" in names
    assert not any(name.startswith(("zz_", "_zz_")) for name in names)


def test_unknown_api_paths_are_404(client):
    assert client.get("/api/nope").status_code == 404
