import json

from flask import has_request_context, request

from app import socketio

NAMESPACE = "/MaslowCNC"


def emit_to_client(command, data, sid=None):
    """Sends one `message` envelope to a single socket client instead of broadcasting.

    `sid` defaults to the requester inside a Socket.IO handler; an HTTP route has none, so it raises there.
    """
    sid = sid or (getattr(request, "sid", None) if has_request_context() else None)
    if sid is None:
        raise ValueError("emit_to_client needs a sid outside a Socket.IO handler")
    socketio.emit(
        "message",
        {"command": command, "data": json.dumps(data), "dataFormat": "json"},
        namespace=NAMESPACE,
        to=sid,
    )
