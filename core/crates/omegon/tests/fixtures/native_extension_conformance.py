#!/usr/bin/env python3
import json
import os
import subprocess
import sys
import threading
import time


MODE = os.environ.get("OMEGON_FIXTURE_MODE", "compatible")
CONTROL = os.environ.get("OMEGON_FIXTURE_CONTROL")
MARKER = os.environ.get("OMEGON_FIXTURE_MARKER")
COUNTER = os.environ.get("OMEGON_FIXTURE_COUNTER")
ACTIVE = {}
ACTIVE_LOCK = threading.Lock()
OUTPUT_LOCK = threading.Lock()


def modes():
    value = MODE
    if CONTROL:
        try:
            with open(CONTROL, encoding="utf-8") as stream:
                value = stream.read().strip() or value
        except FileNotFoundError:
            pass
    return set(value.split(","))


def respond(request, *, result=None, error=None):
    response = {"jsonrpc": "2.0", "id": request.get("id")}
    if error is None:
        response["result"] = result
    else:
        response["error"] = {"code": -32603, "message": error}
    with OUTPUT_LOCK:
        print(json.dumps(response, separators=(",", ":")), flush=True)


def execute_delayed(request, child):
    cancelled = threading.Event()
    request_id = request.get("id")
    with ACTIVE_LOCK:
        ACTIVE[request_id] = (cancelled, child)
    if MARKER:
        with open(MARKER, "w", encoding="utf-8") as stream:
            json.dump({"pid": os.getpid(), "child_pid": child.pid if child else None}, stream)
    was_cancelled = cancelled.wait(300)
    with ACTIVE_LOCK:
        ACTIVE.pop(request_id, None)
    if was_cancelled:
        if child:
            child.terminate()
            child.wait(timeout=2)
        respond(request, error="fixture request cancelled")
    else:
        respond(request, result={"content": [{"type": "text", "text": "ok"}]})


for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    active_modes = modes()
    if method == "notifications/cancelled":
        request_id = request.get("params", {}).get("request_id")
        with ACTIVE_LOCK:
            active = ACTIVE.get(request_id)
        if active:
            active[0].set()
        continue
    if method == "initialize":
        info = {"name": "native-conformance-fixture", "version": "0.1.0"}
        if "missing_sdk" not in active_modes:
            info["sdk_version"] = "99.0" if "unsupported_sdk" in active_modes else "0.25"
        respond(request, result={"protocol_version": 2, "extension_info": info})
    elif method == "get_tools":
        tool_name = "fixture_changed" if "changed_tool_shape" in active_modes else "fixture_echo"
        tools = (
            [{"description": "missing name"}]
            if "malformed_tools" in active_modes
            else [
                {
                    "name": tool_name,
                    "description": "Echo fixture input",
                    "inputSchema": {"type": "object", "properties": {}},
                }
            ]
        )
        respond(request, result=tools)
    elif method == "bootstrap_config":
        if "bootstrap_failure" in active_modes:
            respond(request, error="fixture rejected bootstrap")
        else:
            respond(request, result={"acknowledged": True})
    elif method == "fixture/status":
        if "readiness_child_process" in active_modes:
            child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(300)"])
            if MARKER:
                with open(MARKER, "w", encoding="utf-8") as stream:
                    json.dump({"pid": os.getpid(), "child_pid": child.pid}, stream)
        if "readiness_failure" in active_modes:
            respond(request, error="fixture is not ready")
        else:
            respond(request, result={"ready": True})
    elif method == "execute_tool":
        if COUNTER:
            with open(COUNTER, "a", encoding="utf-8") as stream:
                stream.write("owner-entered\n")
        child = None
        if "child_process" in active_modes:
            child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(300)"])
        if "crash" in active_modes:
            os._exit(70)
        if "delay" in active_modes:
            threading.Thread(
                target=execute_delayed, args=(request, child), daemon=True
            ).start()
            continue
        if MARKER:
            with open(MARKER, "w", encoding="utf-8") as stream:
                json.dump({"pid": os.getpid(), "child_pid": child.pid if child else None}, stream)
        respond(
            request,
            result={
                "content": [{"type": "text", "text": "ok"}],
                "fixture_pid": os.getpid(),
                "child_pid": child.pid if child else None,
            },
        )
    else:
        respond(request, error=f"method not found: {method}")
