from conftest import serial_open_calls
from helpers import NAMESPACE, decode_ui_message, drain, messages


def test_connecting_broadcasts_the_host_address_and_retries_the_serial_port(socket_client, data):
    assert socket_client.is_connected(NAMESPACE)
    assert [m["data"] for m in messages(socket_client, "hostAddress")] == [{"hostAddress": data.hostAddress}]
    assert serial_open_calls


def test_setting_request_answers_with_requested_setting(socket_client, data):
    socket_client.get_received(NAMESPACE)
    socket_client.emit("settingRequest", {"data": {"section": "Computed Settings", "setting": "units"}}, namespace=NAMESPACE)
    assert [m["data"] for m in messages(socket_client, "requestedSetting")] == [
        {"setting": "units", "value": data.config.getValue("Computed Settings", "units")}
    ]


def test_action_event_reaches_process_action(socket_client, data):
    data.uploadFlag = 0
    socket_client.emit("action", {"data": {"command": "move", "arg": "right", "arg1": "3"}}, namespace=NAMESPACE)
    assert drain(data.gcode_queue)[1] == "G00 X3.0 "


def test_check_for_gcode_update_queues_units_then_gcode(socket_client, data):
    socket_client.emit("checkForGCodeUpdate", {"data": "Please"}, namespace=NAMESPACE)
    assert [decode_ui_message(raw)[1] for raw in drain(data.ui_queue1)] == ["unitsUpdate", "gcodeUpdate"]
