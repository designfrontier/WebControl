import json

from flask import request

from app import socketio

NAMESPACE = "/MaslowCNC"


def emit_to_client(command, data, sid=None):
    """Sends one `message` envelope to a single socket client (the requester by default) instead of broadcasting."""
    socketio.emit(
        "message",
        {"command": command, "data": json.dumps(data), "dataFormat": "json"},
        namespace=NAMESPACE,
        to=sid or request.sid,
    )
