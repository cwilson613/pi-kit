#!/usr/bin/env python3
"""Prepare isolated, recorded operator trials in installed macOS terminals."""
from __future__ import annotations

import argparse
import datetime
import json
import os
from pathlib import Path
import plistlib
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time

from tui_acceptance import digest, fixture_provider, prepare_fixture_workspace

APPS = {
    "ghostty": "/Applications/Ghostty.app",
    "wezterm": "/Applications/WezTerm.app",
    "iterm": "/Applications/iTerm.app",
    "kitty": "/Applications/kitty.app",
    "terminal": "/System/Applications/Utilities/Terminal.app",
}
TERMINAL_ENV = ("TERM", "COLORTERM", "TERM_PROGRAM", "TERM_PROGRAM_VERSION", "LANG", "LC_CTYPE", "TERMINFO", "TERMINFO_DIRS", "KITTY_WINDOW_ID", "WEZTERM_PANE", "GHOSTTY_RESOURCES_DIR")


def inventory():
    found = {}
    for client, app in APPS.items():
        info = Path(app) / "Contents/Info.plist"
        if info.exists():
            values = plistlib.loads(info.read_bytes())
            version = values.get("CFBundleShortVersionString", "unknown")
            if client == "wezterm":
                version = subprocess.check_output([str(Path(app) / "Contents/MacOS/wezterm"), "--version"], text=True, timeout=10).strip()
            found[client] = {"app": app, "version": version,
                             "bundle_id": values.get("CFBundleIdentifier")}
    return found


def launch_command(client, app, command):
    if client == "ghostty":
        return ["/usr/bin/open", "-na", app, "--args", "-e", str(command)]
    if client == "wezterm":
        return [str(Path(app) / "Contents/MacOS/wezterm"), "start", "--always-new-process", "--no-auto-connect", "--", str(command)]
    if client == "kitty":
        return ["/usr/bin/open", "-na", app, "--args", str(command)]
    if client == "iterm":
        script = 'on run argv\n tell application id "com.googlecode.iterm2"\n  activate\n  create window with default profile command (item 1 of argv)\n end tell\nend run'
        return ["/usr/bin/osascript", "-e", script, shlex.join([str(command)])]
    if client == "terminal":
        return ["/usr/bin/open", "-a", app, str(command)]
    raise ValueError(f"unsupported terminal: {client}")


def clean_environment(root, inherited):
    environment = {key: inherited[key] for key in TERMINAL_ENV if key in inherited}
    environment.update({"PATH": inherited["PATH"], "HOME": str(root), "SHELL": "/bin/bash",
                        "OMEGON_HOME": str(root / "omegon-home"), "XDG_CONFIG_HOME": str(root / ".config"),
                        "ASCIINEMA_CONFIG_HOME": str(root / "asciinema"), "OMEGON_CHILD": "1",
                        "OPENAI_API_KEY": "local-only", "OMEGON_PROJECT_ENDPOINT_616363657074616E6365_TOKEN": "local-only"})
    return environment


def write_command(path, arguments):
    # .command files are a native macOS operator entry point. Quote every argument.
    path.write_text("#!/bin/bash\nexec " + shlex.join([str(arg) for arg in arguments]) + "\n")
    path.chmod(0o755)


def prepare(binary, output):
    output = output.resolve()
    checkout = Path(__file__).resolve().parents[1]
    if output.is_relative_to(checkout):
        raise ValueError("operator bundles must be outside the checkout")
    recorder = shutil.which("asciinema")
    if not recorder:
        raise RuntimeError("asciinema is required for native terminal recordings")
    clients = inventory()
    if not clients:
        raise RuntimeError("no supported terminal clients found")
    output.mkdir(parents=True, exist_ok=False)
    (output / "support").mkdir()
    (output / "runs").mkdir()
    shutil.copy2(binary, output / "omegon")
    for name in ("tui_operator_test.py", "tui_acceptance.py"):
        shutil.copy2(Path(__file__).with_name(name), output / "support" / name)
    metadata = {"created": time.time(), "source": str(checkout),
                "revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=checkout, text=True).strip(),
                "dirty": subprocess.check_output(["git", "status", "--porcelain"], cwd=checkout, text=True),
                "binary_sha256": digest(output / "omegon"), "python": sys.executable,
                "recorder": recorder, "recorder_version": subprocess.check_output([recorder, "--version"], text=True, timeout=10).strip(), "clients": clients,
                "support_sha256": {p.name: digest(p) for p in (output / "support").iterdir()}}
    (output / "bundle.json").write_text(json.dumps(metadata, indent=2) + "\n")
    for client in clients:
        folder = output / client
        folder.mkdir()
        base = [sys.executable, output / "support/tui_operator_test.py"]
        write_command(folder / "Launch.command", [*base, "launch", "--bundle", output, "--client", client])
        write_command(folder / "Run.command", [*base, "run", "--bundle", output, "--client", client])
    (output / "README.md").write_text(CHECKLIST)
    print(output)


def verify_bundle(bundle):
    metadata = json.loads((bundle / "bundle.json").read_text())
    if digest(bundle / "omegon") != metadata["binary_sha256"]:
        raise RuntimeError("the prepared executable changed; prepare a new bundle")
    for name, expected in metadata["support_sha256"].items():
        if digest(bundle / "support" / name) != expected:
            raise RuntimeError("the prepared runner changed; prepare a new bundle")
    return metadata


def process_table():
    table = {}
    for line in subprocess.check_output(["ps", "-axo", "pid=,ppid=,lstart="], text=True, timeout=5).splitlines():
        parts = line.split()
        if len(parts) >= 7:
            table[int(parts[0])] = (int(parts[1]), tuple(parts[2:]))
    return table


def cleanup_tree(pid):
    """Clean up only descendants of this invocation, including separate sessions."""
    table = process_table()
    owned = {pid} if pid in table else set()
    while True:
        expanded = owned | {child for child, (parent, _) in table.items() if parent in owned}
        if expanded == owned:
            break
        owned = expanded
    identities = {child: table[child][1] for child in owned}
    for sig in (signal.SIGTERM, signal.SIGKILL):
        current = process_table()
        for child, identity in identities.items():
            if child in current and current[child][1] == identity:
                try:
                    os.kill(child, sig)
                except ProcessLookupError:
                    pass
        if sig == signal.SIGTERM and owned:
            time.sleep(1)


def run(bundle, client):
    if not sys.stdin.isatty() or not sys.stdout.isatty():
        raise RuntimeError("run this inside a terminal, or use its Launch.command")
    metadata = verify_bundle(bundle)
    if client not in metadata["clients"]:
        raise ValueError("terminal is not in this bundle's inventory")
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    output = bundle / "runs" / f"{client}-{stamp}"
    output.mkdir()
    print(f"Omegon terminal trial: {client}\nEvidence: {output}\n")
    print("F2: Sessions/Work browser. Enter: inspect. Escape: back.\n"
          "Send two prompts. Third prompt requests a write after 5 seconds:\n"
          "open F2/Work during the delay, then press N to deny.\n"
          "Use /session-export scrollback; resize; test selection and paste.\n"
          "For cancellation, start a separate run and Ctrl+C during that delay.\n"
          "Exit with /quit. This records output locally; nothing is uploaded.\n")
    ledger = {"client": client, "installed_client": metadata["clients"][client], "started": time.time(),
              "binary_sha256": metadata["binary_sha256"], "revision": metadata["revision"],
              "support_sha256": metadata["support_sha256"],
              "terminal_environment": {key: os.environ.get(key) for key in TERMINAL_ENV},
              "initial_size": list(os.get_terminal_size()), "multiplexer": bool(os.environ.get("TMUX") or os.environ.get("STY")), "status": "starting"}
    recorder = None
    stopping = threading.Event()
    old_handlers = {}
    try:
        with tempfile.TemporaryDirectory(prefix="og-tui-", dir="/tmp") as temporary, fixture_provider() as provider:
            root = Path(temporary)
            workspace = prepare_fixture_workspace(root, provider)
            environment = clean_environment(root, os.environ)
            def release_probe():
                while not stopping.wait(0.1):
                    if provider.tool_waiting.is_set():
                        if not stopping.wait(5):
                            provider.release_tool.set()
                        return
            worker = threading.Thread(target=release_probe, daemon=True)
            worker.start()
            command = [str(bundle / "omegon"), "--cwd", str(workspace), "--model", "openai:gpt-5.4",
                       "--fresh", "--no-splash", "--log-level", "debug", "--log-file", str(output / "omegon.log")]
            child = [metadata["python"], str(bundle / "support/tui_operator_test.py"), "child",
                     "--identity", str(output / "process.json"), "--", *command]
            # asciinema's documented command API takes shell text, so use shlex quoting.
            recorder = subprocess.Popen([metadata["recorder"], "rec", "-q", "-c", shlex.join(child),
                                         "-e", ",".join(TERMINAL_ENV), str(output / "terminal.cast")], env=environment, cwd=workspace)
            def stop(_sig, _frame):
                stopping.set()
                cleanup_tree(recorder.pid)
            for sig in (signal.SIGHUP, signal.SIGTERM, signal.SIGINT):
                old_handlers[sig] = signal.signal(sig, stop)
            ledger["recorder_pid"] = recorder.pid
            ledger["recorder_exit_code"] = recorder.wait()
            ledger["provider_requests"] = provider.requests
            ledger["fixture_write_exists"] = Path(provider.tool_path).exists()
            log = output / "omegon.log"
            ledger["tui_started"] = log.exists() and "terminal input boundary acquired" in log.read_text()
            ledger["status"] = "recorded; operator assessment required" if ledger["tui_started"] else "failed before TUI readiness"
            stopping.set()
            worker.join(timeout=1)
    finally:
        stopping.set()
        if recorder is not None and recorder.poll() is None:
            cleanup_tree(recorder.pid)
            recorder.wait(timeout=5)
        for sig, old in old_handlers.items():
            signal.signal(sig, old)
        ledger["finished"] = time.time()
        ledger["artifact_sha256"] = {p.name: digest(p) for p in output.iterdir() if p.is_file()}
        (output / "manifest.json").write_text(json.dumps(ledger, indent=2) + "\n")
        (output / "RESULTS.md").write_text("# Operator result\n\nStatus: NOT ASSESSED\n\n" + CHECKLIST)
    print(f"\nRecording saved: {output}\nFill in RESULTS.md; replay with asciinema play terminal.cast.")
    input("Press Enter to close this trial… ")


CHECKLIST = """# Omegon terminal compatibility trial

Open a client's **Launch.command** in Finder. It opens a fresh window in that
client; the helper Terminal window can be closed after launch. Run.command is
for use *inside the intended client*, not for double-clicking in Finder.

Each run uses the same frozen executable and fixture, with isolated HOME,
configuration and workspace. Real terminal settings, fonts, colors and keyboard
mappings are preserved. No tmux layer is inserted. asciinema records local output
and resize events; no upload or paid provider is used. Private projects are not
opened. The stable installed Omegon launcher is not changed.

Test the same sequence in Ghostty, WezTerm, iTerm2, kitty and Apple Terminal:

- [ ] F2 opens Project browser; Tab changes Sessions/Work; Enter opens details.
- [ ] Escape returns to a draft without losing it (Mac media-key settings may require Fn+F2).
- [ ] Send two prompts and see two distinct replies.
- [ ] Send a third prompt, open F2 then Work within five seconds, and deny the write with N.
- [ ] The Work tab returns; Escape returns to the completed conversation.
- [ ] /session-export scrollback prints to native history and restores the complete TUI.
- [ ] Resize wide/narrow and tall/short; inspect clipping, colors and glyphs.
- [ ] Check paste, multiline input (Shift+Enter/Option+Enter), text selection and clipboard copy.
- [ ] Exit with /quit; check that the shell's cursor, echo and keyboard behave normally.
- [ ] Optional separate run: Ctrl+C during the third prompt's five-second delay; draft survives.

Save a native window screenshot with macOS Shift+Cmd+4, then Space; move the image
into the run directory. Output recordings prove terminal bytes, not font or GUI
rendering fidelity. Record terminal profile/font, viewport, failed step and notes
in the run's RESULTS.md. Runs and recordings are under runs/<client>-<timestamp>.
Replay: asciinema play /absolute/path/to/terminal.cast

Known cross-client issue: the Project browser shows the generic / search hint,
but search is not wired in this browser increment. Record it as known rather than
a terminal-specific regression.

Work currently shows Workbench summaries. Populated execution/evidence drill-down
and persistent inline layout remain outside this first browser increment.
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    p = sub.add_parser("prepare")
    p.add_argument("--binary", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    for action in ("launch", "run"):
        p = sub.add_parser(action)
        p.add_argument("--bundle", type=Path, required=True)
        p.add_argument("--client", choices=APPS, required=True)
    p = sub.add_parser("child")
    p.add_argument("--identity", type=Path, required=True)
    p.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.action == "prepare":
        prepare(args.binary.resolve(strict=True), args.output)
    elif args.action == "launch":
        bundle = args.bundle.resolve(strict=True)
        metadata = verify_bundle(bundle)
        command = launch_command(args.client, metadata["clients"][args.client]["app"], bundle / args.client / "Run.command")
        if args.client == "wezterm":
            # The WezTerm CLI becomes its GUI process. Detach that new instance
            # so closing the launcher window cannot terminate the trial.
            with (bundle / "wezterm-launch.log").open("ab") as log:
                process = subprocess.Popen(command, start_new_session=True, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
            time.sleep(0.5)
            if process.poll() not in (None, 0):
                raise RuntimeError("WezTerm could not launch; see wezterm-launch.log")
        else:
            subprocess.run(command, check=True)
    elif args.action == "run":
        run(args.bundle.resolve(strict=True), args.client)
    else:
        command = args.command[1:] if args.command[:1] == ["--"] else args.command
        if not command:
            parser.error("child requires an executable")
        args.identity.write_text(json.dumps({"pid": os.getpid(), "process_group": os.getpgrp(),
                                            "executable": command[0], "started": time.time(),
                                            "command": command}, indent=2) + "\n")
        os.execv(command[0], command)


if __name__ == "__main__":
    main()
