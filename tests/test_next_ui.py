import pytest
from flask import Flask

from next_ui import NOT_BUILT, init_next_ui


@pytest.fixture
def dist(tmp_path):
    (tmp_path / "assets").mkdir()
    (tmp_path / "index.html").write_text("<div id=root></div>")
    (tmp_path / "assets" / "index-abc.js").write_text("console.log('app')")
    return tmp_path


def client_for(dist_dir):
    app = Flask(__name__)
    init_next_ui(app, str(dist_dir))
    # Flask's test client only weakly references the app.
    client_for.app = app
    return app.test_client()


def test_bare_next_redirects_to_the_app_root(dist):
    response = client_for(dist).get("/next")
    assert response.status_code == 308
    assert response.headers["Location"].endswith("/next/")


@pytest.mark.parametrize("path", ["/next/", "/next/settings", "/next/machine/calibration"])
def test_serves_index_for_the_app_and_its_client_side_routes(dist, path):
    response = client_for(dist).get(path)
    assert response.status_code == 200
    assert b"<div id=root>" in response.data
    assert response.headers["Cache-Control"] == "no-cache"


def test_serves_built_assets(dist):
    response = client_for(dist).get("/next/assets/index-abc.js")
    assert response.status_code == 200
    assert b"console.log" in response.data


def test_missing_asset_is_404_not_index(dist):
    assert client_for(dist).get("/next/assets/gone.js").status_code == 404


def test_explains_how_to_build_when_dist_is_missing(tmp_path):
    response = client_for(tmp_path / "nothing-here").get("/next/")
    assert response.status_code == 503
    assert response.get_data(as_text=True) == NOT_BUILT


def test_main_app_mounts_next_without_touching_the_classic_routes(app):
    rules = {rule.rule for rule in app.url_map.iter_rules()}
    assert {"/next/", "/next/<path:path>"} <= rules
    assert {"/", "/controls", "/text", "/logs", "/uploadGCode"} <= rules


def test_does_not_serve_files_outside_dist(dist):
    (dist.parent / "secret.txt").write_text("nope")
    response = client_for(dist).get("/next/..%2Fsecret.txt")
    assert response.status_code == 404
    assert b"nope" not in response.data
