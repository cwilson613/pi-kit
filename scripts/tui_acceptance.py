#!/usr/bin/env python3
"""Drive the actual Omegon TUI through a private tmux server; no paid inference."""
from __future__ import annotations

import argparse
from contextlib import contextmanager
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import shlex
import signal
import subprocess
import tempfile
import threading
import time
import uuid


@contextmanager
def fixture_provider():
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def do_GET(self):
            if self.path == "/v1/models":
                body = {"data": [{"id": "gpt-5.4", "object": "model"}]}
            elif self.path == "/api/tags":
                body = {"models": [{"name": "gpt-5.4"}]}
            else:
                self.send_error(404)
                return
            self.send_response(200)
            self.end_headers()
            self.wfile.write(json.dumps(body).encode())

        def do_POST(self):
            self.connection.settimeout(3)
            length = int(self.headers.get("Content-Length", "0"))
            if self.path != "/v1/chat/completions" or not 0 <= length <= 8 * 1024 * 1024:
                self.send_error(400)
                return
            json.loads(self.rfile.read(length))
            with server.request_lock:
                server.requests += 1
                number = server.requests
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.end_headers()
            for delta, finish in [({"content": f"TUI_FIXTURE_REPLY_{number}"}, None), ({}, "stop")]:
                event = {"choices": [{"index": 0, "delta": delta, "finish_reason": finish}]}
                self.wfile.write(("data: " + json.dumps(event) + "\n\n").encode())
                self.wfile.flush()
            self.wfile.write(b"data: [DONE]\n\n")

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.requests = 0
    server.request_lock = threading.Lock()
    server.url = f"http://127.0.0.1:{server.server_port}"
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield server
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


def digest(path):
    with Path(path).open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def run(binary: Path, output: Path):
    binary = binary.resolve(strict=True)
    checkout = Path(__file__).resolve().parents[1]
    if output.resolve().is_relative_to(checkout):
        raise ValueError("capture evidence must be outside the checkout")
    output.mkdir(parents=True, exist_ok=False)
    socket = "omegon-acceptance-" + uuid.uuid4().hex
    ledger = {"binary": str(binary), "binary_sha256": digest(binary), "started": time.time(),
              "revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=checkout, text=True).strip(),
              "dirty": subprocess.check_output(["git", "status", "--porcelain"], cwd=checkout, text=True),
              "captures": [], "passed": False}

    def tmux(*args, check=True):
        return subprocess.run(["tmux", "-L", socket, *args], check=check, capture_output=True, text=True, timeout=10).stdout

    def action(*args):
        ledger.setdefault("actions", []).append({"time": time.time(), "tmux": args})
        return tmux(*args)

    def screen():
        return tmux("capture-pane", "-p", "-t", "run:0.0")

    def capture(name):
        path = output / (name + ".txt")
        path.write_text(screen())
        ledger["captures"].append({"name": name, "time": time.time(), "sha256": digest(path),
                                   "geometry": tmux("display-message", "-p", "-t", "run:0.0", "#{pane_width}x#{pane_height}").strip()})

    def wait_for(predicate, label):
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            if predicate():
                return
            time.sleep(0.05)
        capture("failure")
        raise TimeoutError(label)

    with tempfile.TemporaryDirectory(prefix="omegon-tui-") as temporary, fixture_provider() as provider:
        root = Path(temporary)
        workspace = root / "workspace"
        config = workspace / ".omegon"
        config.mkdir(parents=True)
        (config / "profile.json").write_text("{}\n")
        (config / "inference.toml").write_text(f'''schema_version = 1
[[endpoints]]
id = "acceptance"
adapter = "chat-completions"
secret_refs = ["OMEGON_PROJECT_ENDPOINT_616363657074616E6365_TOKEN"]
[endpoints.transport]
kind = "http"
base_url = "{provider.url}/v1"
[[offerings]]
id = "openai:gpt-5.4"
endpoint = "acceptance"
native_model_id = "gpt-5.4"
input_modalities = ["text"]
output_modalities = ["text"]
[offerings.capabilities]
tools = true
reasoning = true
''')
        log = output / "omegon.log"
        command = [str(binary), "--cwd", str(workspace), "--model", "openai:gpt-5.4", "--no-splash", "--fresh", "--log-level", "debug", "--log-file", str(log)]
        # Start with an explicit environment, so real credentials/plugins cannot leak into the fixture.
        environment = {"PATH": os.environ["PATH"], "HOME": str(root), "OMEGON_HOME": str(root / "omegon-home"),
                       "XDG_CONFIG_HOME": str(root / ".config"), "TERM": "xterm-256color", "LANG": "en_US.UTF-8",
                       "OMEGON_CHILD": "1", "NO_COLOR": "1", "OPENAI_API_KEY": "local-only",
                       "OMEGON_PROJECT_ENDPOINT_616363657074616E6365_TOKEN": "local-only"}
        launch = ["env", "-i", *(f"{key}={value}" for key, value in environment.items()), *command]
        ledger["command"] = command
        try:
            tmux("-f", "/dev/null", "new-session", "-d", "-s", "run", "-c", str(workspace), "-x", "120", "-y", "40", shlex.join(launch))
            ledger["pid"] = tmux("display-message", "-p", "-t", "run:0.0", "#{pane_pid}").strip()
            ledger["process_group"] = os.getpgid(int(ledger["pid"]))
            if ledger["process_group"] != int(ledger["pid"]):
                raise RuntimeError("terminal child must own its process group")
            ledger["process"] = subprocess.check_output(["ps", "-p", ledger["pid"], "-o", "pid=,lstart=,command="], text=True)
            if str(binary) not in ledger["process"]:
                raise RuntimeError("running process does not identify the requested binary")
            wait_for(lambda: log.exists() and "terminal input boundary acquired" in log.read_text(), "TUI startup")
            capture("01-startup")
            for number in (1, 2):
                action("send-keys", "-t", "run:0.0", "-l", f"fixture turn {number}")
                action("send-keys", "-t", "run:0.0", "Enter")
                wait_for(lambda: f"TUI_FIXTURE_REPLY_{number}" in screen(), f"visible reply {number}")
                capture(f"0{number + 1}-turn-{number}")
            action("resize-window", "-t", "run:0", "-x", "90", "-y", "30")
            wait_for(lambda: "TUI_FIXTURE_REPLY_2" in screen() and "ready · idle" in screen(), "reply survives resize and runtime becomes idle")
            capture("04-resize")
            assert provider.requests == 2, f"unexpected inference requests: {provider.requests}"
            if digest(binary) != ledger["binary_sha256"]:
                raise RuntimeError("binary changed during capture")
            ledger["passed"] = True
        finally:
            ledger["provider_requests"] = provider.requests
            ledger["finished"] = time.time()
            # Only this invocation's private terminal server is terminated.
            tmux("kill-server", check=False)
            group = ledger.get("process_group")
            if group:
                deadline = time.monotonic() + 3
                while time.monotonic() < deadline:
                    try:
                        os.killpg(group, 0)
                    except ProcessLookupError:
                        break
                    time.sleep(0.05)
                else:
                    os.killpg(group, signal.SIGKILL)
                    ledger["cleanup_forced"] = True
            (output / "manifest.json").write_text(json.dumps(ledger, indent=2) + "\n")
    print(json.dumps({"passed": ledger["passed"], "evidence": str(output), "provider_requests": ledger["provider_requests"]}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True, help="freshly built Omegon executable")
    parser.add_argument("--output", type=Path, required=True, help="new evidence directory outside the checkout")
    arguments = parser.parse_args()
    run(arguments.binary, arguments.output.resolve())
