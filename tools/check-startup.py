#!/usr/bin/env python3
"""Niri smoke test for single-instance, window activation and workspace isolation."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=root / "target/release/tern")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    runtime = Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}"))
    sock = os.environ.get("NIRI_SOCKET")
    if not sock:
        sockets = list(runtime.glob("niri.*.sock"))
        if len(sockets) != 1:
            parser.error("set NIRI_SOCKET")
        sock = str(sockets[0])
    base = root / "target/startup-check"
    base.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(dir=base))
    env = dict(os.environ, XDG_RUNTIME_DIR=str(runtime), NIRI_SOCKET=sock,
               DBUS_SESSION_BUS_ADDRESS=os.environ.get("DBUS_SESSION_BUS_ADDRESS", f"unix:path={runtime}/bus"),
               WAYLAND_DISPLAY=os.environ.get("WAYLAND_DISPLAY", Path(sock).name.split(".")[1]),
               XDG_CONFIG_HOME=str(run / "config"), XDG_CACHE_HOME=str(run / "cache"))
    first, second = run / "first", run / "second"
    first.mkdir()
    second.mkdir()
    (first / "中文 文件.txt").write_text("fixture")
    children = []
    report = {"binary": str(binary), "run_directory": str(run), "checks": []}

    def ipc(payload):
        with socket.socket(socket.AF_UNIX) as stream:
            stream.settimeout(5)
            stream.connect(sock)
            stream.sendall(json.dumps(payload).encode() + b"\n")
            with stream.makefile("r") as response:
                reply = json.loads(response.readline())
                if "Err" in reply:
                    raise RuntimeError(reply["Err"])
                return reply["Ok"]

    def windows():
        return [w for w in ipc("Windows")["Windows"] if w.get("pid") == leader.pid]

    def wait_count(count):
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            found = windows()
            if len(found) == count:
                return found
            if leader.poll() is not None:
                raise RuntimeError("primary exited")
            time.sleep(0.03)
        raise AssertionError(f"expected {count} windows, got {len(found)}")

    def launch(*paths):
        client = subprocess.Popen([str(binary), *map(str, paths)], env=env, stdout=log, stderr=subprocess.STDOUT)
        children.append(client)
        if client.wait(timeout=10) != 0:
            raise AssertionError("secondary failed")

    def close(window):
        ipc({"Action": {"CloseWindow": {"id": window["id"]}}})

    with (run / "app.log").open("w") as log:
        leader = subprocess.Popen([str(binary), str(first)], env=env, stdout=log, stderr=subprocess.STDOUT)
        children.append(leader)
        try:
            a = wait_count(1)[0]
            launch(second)
            b = next(w for w in wait_count(2) if w["id"] != a["id"])
            close(b)
            wait_count(1)
            workspace = run / "config/tern/workspace.json"
            saved = workspace.read_text()
            assert str(second) in saved and str(first) not in saved, "another window overwrote the closed workspace"
            report["checks"].append("closing a window saves only its own workspace")
            launch()
            time.sleep(0.3)
            assert [w["id"] for w in windows()] == [a["id"]], "no-argument launch duplicated a surviving window"
            report["checks"].append("no-argument launch reuses a surviving older window")
            close(a)
            wait_count(0)
            assert leader.poll() is None
            launch()
            restored = wait_count(1)[0]
            report["checks"].append("close-last-window retains primary; reopening keeps its PID")
            close(restored)
            wait_count(0)
            with ThreadPoolExecutor(max_workers=8) as pool:
                list(pool.map(lambda _: launch(first), range(8)))
            opened = wait_count(8)
            report["checks"].append("8 concurrent launches create 8 windows in the same primary")
            for window in opened:
                close(window)
            wait_count(0)
            # Call the exported interface on this isolated profile, never on the
            # user's global org.freedesktop.FileManager1 owner.
            config = str(run / "config/tern").encode()
            name_hash = 0xcbf29ce484222325
            for byte in config:
                name_hash = ((name_hash ^ byte) * 0x100000001b3) & ((1 << 64) - 1)
            name = f"io.github.coolloong.Tern.Profile{name_hash:016x}"
            subprocess.run(["gdbus", "call", "--session", "--dest", name,
                            "--object-path", "/org/freedesktop/FileManager1", "--method",
                            "org.freedesktop.FileManager1.ShowItems", repr([(first / "中文 文件.txt").as_uri()]), ""],
                           env=env, check=True, timeout=10, stdout=subprocess.DEVNULL)
            shown = wait_count(1)[0]
            report["checks"].append("FileManager1 ShowItems with an encoded Chinese URI uses the same primary")
            close(shown)
            wait_count(0)
            report["ok"] = True
        except Exception as error:
            report["ok"] = False
            report["error"] = str(error)
            raise
        finally:
            (run / "results.json").write_text(json.dumps(report, indent=2) + "\n")
            print(json.dumps(report), flush=True)
            for child in children:
                if child.poll() is None:
                    child.terminate()
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()


if __name__ == "__main__":
    main()
