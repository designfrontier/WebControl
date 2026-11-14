import pytest

import main
from api.client_emit import NAMESPACE, emit_to_client
from helpers import envelope, messages


@main.socketio.on("testOnlyEcho", namespace=NAMESPACE)
def _echo(payload):
    emit_to_client("echo", payload)


def test_reaches_only_the_requesting_client(connect_socket):
    asker, bystander = connect_socket(), connect_socket()
    asker.get_received(NAMESPACE)
    bystander.get_received(NAMESPACE)

    asker.emit("testOnlyEcho", {"hello": "world"}, namespace=NAMESPACE)

    assert [m["data"] for m in messages(asker, "echo")] == [{"hello": "world"}]
    assert messages(bystander, "echo") == []


def test_wraps_data_in_the_legacy_message_envelope(connect_socket):
    asker = connect_socket()
    asker.get_received(NAMESPACE)
    asker.emit("testOnlyEcho", [1, 2], namespace=NAMESPACE)
    packet = [p for p in asker.get_received(NAMESPACE) if p["name"] == "message"][0]
    assert envelope(packet) == {"command": "echo", "data": "[1, 2]", "dataFormat": "json"}


def test_outside_a_socket_handler_it_needs_a_sid(app):
    with app.test_request_context("/api/state"):
        with pytest.raises(ValueError, match="needs a sid"):
            emit_to_client("echo", {})
    with pytest.raises(ValueError, match="needs a sid"):
        emit_to_client("echo", {})
