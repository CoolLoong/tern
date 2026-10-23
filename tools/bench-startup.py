#!/usr/bin/env python3
"""Measure launch-to-map on Niri, not first paint or a disk-cold boot."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import socket
import statistics
import subprocess
import tempfile
import time


def stop(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
    process.wait()


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=root / "target/release/tern")
    parser.add_argument("--samples", type=int, default=7)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--fresh-process", action="store_true")
    parser.add_argument("--label", default="startup")
    parser.add_argument("--timeout", type=float, default=15)
    args = parser.parse_args()
    if args.samples < 1 or args.warmups < 0 or args.timeout <= 0:
        parser.error("samples and timeout must be positive; warmups must be nonnegative")
    if not re.fullmatch(r"[a-zA-Z0-9_-]+", args.label):
        parser.error("label must contain only letters, digits, underscores or hyphens")
    binary = args.binary.resolve(strict=True)
    folder = root / "target/startup-bench"
    folder.mkdir(parents=True, exist_ok=True)
    # Each run owns its profile and child processes, including on timeout/failure.
    run = Path(tempfile.mkdtemp(prefix=f"{args.label}-", dir=folder))
    config = run / "config/tern"
    config.mkdir(parents=True)
    original = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")) / "tern/settings.json"
    if original.exists():
        shutil.copy2(original, config / "settings.json")
    runtime = Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}"))
    niri_socket = os.environ.get("NIRI_SOCKET")
    if not niri_socket:
        sockets = list(runtime.glob("niri.*.sock"))
        if len(sockets) != 1:
            parser.error("set NIRI_SOCKET to the compositor socket")
        niri_socket = str(sockets[0])
    env = dict(os.environ, XDG_RUNTIME_DIR=str(runtime), NIRI_SOCKET=niri_socket,
               DBUS_SESSION_BUS_ADDRESS=os.environ.get("DBUS_SESSION_BUS_ADDRESS", f"unix:path={runtime}/bus"),
               WAYLAND_DISPLAY=os.environ.get("WAYLAND_DISPLAY", Path(niri_socket).name.split(".")[1]),
               XDG_CONFIG_HOME=str(config.parent), XDG_CACHE_HOME=str(run / "cache"))
    results, warmups, children = [], [], []
    leader = None
    try:
        for index in range(args.samples + args.warmups):
            with socket.socket(socket.AF_UNIX) as stream:
                stream.connect(niri_socket)
                stream.sendall(b'"EventStream"\n')
                deadline = time.monotonic() + args.timeout
                with stream.makefile("r") as events:
                    def event():
                        remaining = deadline - time.monotonic()
                        if remaining <= 0:
                            raise TimeoutError(f"window event timed out; inspect {run}")
                        stream.settimeout(remaining)
                        line = events.readline()
                        if not line:
                            raise RuntimeError("Niri event stream closed")
                        return json.loads(line)

                    while True:
                        initial = event()
                        if "WindowsChanged" in initial:
                            existing = {w["id"] for w in initial["WindowsChanged"]["windows"]}
                            break
                    log_path = run / f"{index}.log"
                    with log_path.open("w") as log:
                        started = time.monotonic_ns()
                        client = subprocess.Popen([str(binary), str(root)], env=env,
                                                  stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
                        children.append(client)
                        if leader is None:
                            leader = client
                        while True:
                            window = event().get("WindowOpenedOrChanged", {}).get("window")
                            if window and window["id"] not in existing and window.get("pid") == leader.pid:
                                elapsed = (time.monotonic_ns() - started) / 1e6
                                break
                            if client.poll() not in (None, 0) or leader.poll() is not None:
                                raise RuntimeError(f"Tern exited before mapping; inspect {log_path}")
                        sample = {"sample": index, "mapped_ms": round(elapsed, 3), "pid": leader.pid}
                        (warmups if index < args.warmups else results).append(sample)
                        print(json.dumps(dict(sample, warmup=index < args.warmups)), flush=True)
                        time.sleep(0.3)
                        subprocess.run(["niri", "msg", "action", "close-window", "--id", str(window["id"])],
                                       env=env, stdout=subprocess.DEVNULL, check=True, timeout=5)
                        while event().get("WindowClosed", {}).get("id") != window["id"]:
                            pass
                        if args.fresh_process:
                            stop(leader)
                            leader = None
                        elif client is not leader and client.wait(timeout=5) != 0:
                            raise RuntimeError(f"forwarding failed; inspect {log_path}")
        report = {
            "measurement": __doc__, "binary": str(binary),
            "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "fresh_process": args.fresh_process, "warmups": warmups, "samples": results,
            "median_ms": round(statistics.median(s["mapped_ms"] for s in results), 3),
            "min_ms": min(s["mapped_ms"] for s in results), "max_ms": max(s["mapped_ms"] for s in results),
            "run_directory": str(run),
        }
        if leader is not None:
            for line in Path(f"/proc/{leader.pid}/status").read_text().splitlines():
                if line.startswith("VmRSS:"):
                    report["resident_kib"] = int(line.split()[1])
        for destination in [run / "results.json", folder / f"{args.label}.json"]:
            destination.write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps(report), flush=True)
    finally:
        for child in children:
            stop(child)


if __name__ == "__main__":
    main()
