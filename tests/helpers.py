import json

NAMESPACE = "/MaslowCNC"


def drain(queue):
    items = []
    while not queue.empty():
        items.append(queue.get())
    return items


def decode_ui_message(raw):
    message = json.loads(raw)
    return message["command"], message["message"], json.loads(message["data"])


def envelope(packet):
    """The test client records an emitted "message" event as a send, so its args are not wrapped in a list."""
    args = packet["args"]
    return args[0] if isinstance(args, list) else args


def messages(test_client, command=None):
    """Decoded `message` envelopes a socket test client has received, optionally filtered by command."""
    received = [envelope(packet) for packet in test_client.get_received(NAMESPACE) if packet["name"] == "message"]
    decoded = [
        {**envelope, "data": json.loads(envelope["data"]) if envelope.get("dataFormat") == "json" else envelope["data"]}
        for envelope in received
    ]
    return [envelope for envelope in decoded if command is None or envelope["command"] == command]
