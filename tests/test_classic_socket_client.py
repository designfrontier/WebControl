import re

import pytest


@pytest.mark.parametrize("route", ["/", "/logs"])
def test_classic_pages_load_a_socket_io_4_client(client, route):
    html = client.get(route).get_data(as_text=True)
    (script,) = re.findall(r'src="([^"]*socket\.io[^"]*)"', html)
    assert "socket.io-4." in script
    body = client.get(script.split("?")[0]).get_data(as_text=True)
    assert "Socket.IO v4." in body
