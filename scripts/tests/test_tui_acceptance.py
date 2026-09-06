"""Contract tests for the captured TUI runner's local provider."""
import importlib.util
import json
from pathlib import Path
from urllib.request import Request, urlopen

spec = importlib.util.spec_from_file_location("tui_acceptance", Path(__file__).parents[1] / "tui_acceptance.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def test_provider_streams_distinct_turns_without_external_inference():
    with runner.fixture_provider() as server:
        for number in (1, 2):
            request = Request(server.url + "/v1/chat/completions", data=json.dumps({"messages": []}).encode(), headers={"Content-Type": "application/json"})
            with urlopen(request, timeout=2) as response:
                body = response.read().decode()
            assert f"TUI_FIXTURE_REPLY_{number}" in body
            assert '"finish_reason": "stop"' in body
            assert "data: [DONE]" in body
        assert server.requests == 2


def test_provider_rejects_unknown_routes():
    from urllib.error import HTTPError
    with runner.fixture_provider() as server:
        try:
            urlopen(server.url + "/unexpected", timeout=2)
        except HTTPError as error:
            assert error.code == 404
        else:
            raise AssertionError("unknown route accepted")

def test_provider_can_request_a_bounded_permission_probe():
    with runner.fixture_provider() as server:
        server.tool_path = "/tmp/fixture-only/denied.txt"
        server.requests = 2
        server.release_tool.set()
        request = Request(server.url + "/v1/chat/completions", data=b'{"messages": []}', headers={"Content-Type": "application/json"})
        with urlopen(request, timeout=2) as response:
            body = response.read().decode()
        assert '"name": "write"' in body
        assert '"finish_reason": "tool_calls"' in body
        assert "denied.txt" in body

if __name__ == "__main__":
    command = runner.tui_command(Path("/binary with spaces"), Path("/workspace"), Path("/log"), "inline", "full")
    assert "--tui" in command and "--ui" in command, "fixture must select both axes explicitly"
    assert command[command.index("--tui") + 1] == "inline"
    assert command[command.index("--ui") + 1] == "full"
    test_provider_streams_distinct_turns_without_external_inference()
    test_provider_rejects_unknown_routes()
    test_provider_can_request_a_bounded_permission_probe()
