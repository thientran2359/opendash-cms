#!/usr/bin/env python3
"""OpenDash local developer launcher for Windows and macOS.

Run: python dev.py
Requires Python 3.10+ with Tk, Node/npm and Rust/Cargo.
The window owns the server: closing it stops build processes and OpenDash.
"""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import queue
import shutil
import signal
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
import webbrowser


if sys.platform == "win32":
    # PowerShell 5.1 may launch Python with a legacy console encoding.
    for stream in (sys.stdout, sys.stderr):
        if stream is not None and hasattr(stream, "reconfigure"):
            stream.reconfigure(encoding="utf-8", errors="replace")


ROOT = Path(__file__).resolve().parent
WATCH_PATHS = (
    "cms/admin/src", "cms/admin/index.html", "cms/admin/vite.config.ts",
    "cms/admin/package.json", "cms/admin/package-lock.json",
    "cms/crates", "cms/migrations", "cms/Cargo.toml", "cms/Cargo.lock",
)


def command(name: str) -> str:
    path = shutil.which(name)
    if not path:
        raise RuntimeError(f"Không tìm thấy {name}. Hãy cài công cụ này và thêm vào PATH.")
    return path


def health(port: int) -> bool:
    try:
        with urllib.request.urlopen(
            f"http://127.0.0.1:{port}/api/health", timeout=1.5
        ) as response:
            import json

            payload = json.load(response)
            return response.status == 200 and payload.get("status") == "ok"
    except (OSError, ValueError, urllib.error.URLError):
        return False


def port_owner(port: int) -> int | None:
    """Find the actual listening process, without relying on network response alone."""
    if sys.platform == "win32":
        script = (
            f"Get-NetTCPConnection -State Listen -LocalPort {port} "
            "-ErrorAction SilentlyContinue | "
            "Where-Object { $_.LocalAddress -in @('127.0.0.1','::1') } | "
            "Select-Object -ExpandProperty OwningProcess -Unique"
        )
        result = subprocess.run(
            ["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
            capture_output=True, text=True, timeout=8, check=False,
        )
    elif sys.platform == "darwin":
        result = subprocess.run(
            ["lsof", "-nP", f"-iTCP:{port}", "-sTCP:LISTEN", "-t"],
            capture_output=True, text=True, timeout=8, check=False,
        )
    else:
        raise RuntimeError("dev.py chỉ hỗ trợ Windows và macOS.")
    pids = {int(value) for value in result.stdout.split() if value.isdigit()}
    if len(pids) > 1:
        raise RuntimeError(f"Cổng {port} có nhiều tiến trình lắng nghe: {sorted(pids)}.")
    return next(iter(pids), None)


def is_opendash_process(pid: int) -> bool:
    """Reject unrelated port owners before attempting a takeover."""
    if sys.platform == "win32":
        script = (
            f"(Get-CimInstance Win32_Process -Filter 'ProcessId={pid}' "
            "-ErrorAction SilentlyContinue).ExecutablePath"
        )
        result = subprocess.run(
            ["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
            capture_output=True, text=True, timeout=8, check=False,
        )
    else:
        result = subprocess.run(
            ["ps", "-p", str(pid), "-o", "comm="],
            capture_output=True, text=True, timeout=8, check=False,
        )
    executable = result.stdout.strip().strip('"')
    return Path(executable).stem.lower() == "opendash"


def stop_process_tree(process: subprocess.Popen | None) -> None:
    if process is None or process.poll() is not None:
        return
    if sys.platform == "win32":
        subprocess.run(
            ["taskkill", "/PID", str(process.pid), "/T", "/F"],
            capture_output=True, timeout=10, check=False,
        )
    else:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        if sys.platform != "win32":
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        process.wait(timeout=5)


def stop_port_owner(pid: int) -> None:
    if sys.platform == "win32":
        subprocess.run(
            ["taskkill", "/PID", str(pid), "/T", "/F"],
            capture_output=True, text=True, timeout=10, check=True,
        )
    else:
        os.kill(pid, signal.SIGTERM)


def take_over_port(port: int, log) -> None:
    owner = port_owner(port)
    if owner is None:
        return
    if not is_opendash_process(owner) or not health(port):
        raise RuntimeError(
            f"Cổng {port} đang được PID {owner} sử dụng nhưng chưa xác minh "
            "được đây là OpenDash. Không tự động dừng tiến trình lạ."
        )
    if port_owner(port) != owner:
        raise RuntimeError("Chủ sở hữu cổng vừa thay đổi; không dừng tiến trình.")
    log(f"Dừng OpenDash cũ PID {owner} trên cổng {port}...")
    stop_port_owner(owner)
    for _ in range(40):
        if port_owner(port) is None:
            return
        time.sleep(0.25)
    raise RuntimeError(f"Cổng {port} vẫn còn bị chiếm sau khi dừng OpenDash cũ.")


def source_snapshot(root: Path) -> dict[str, tuple[int, int]]:
    result: dict[str, tuple[int, int]] = {}
    for relative in WATCH_PATHS:
        path = root / relative
        if path.is_file():
            candidates = (path,)
        elif path.is_dir():
            candidates = (item for item in path.rglob("*") if item.is_file())
        else:
            continue
        for item in candidates:
            if any(part.startswith(".") for part in item.relative_to(root).parts):
                continue
            try:
                stat = item.stat()
                result[str(item)] = (stat.st_mtime_ns, stat.st_size)
            except FileNotFoundError:
                continue
    return result


class DevRunner:
    def __init__(self, root: Path, site_root: Path, port: int, watch: bool,
                 events: queue.Queue, open_browser: bool) -> None:
        self.root = root.resolve()
        self.site_root = site_root.resolve()
        self.port = port
        self.watch = watch
        self.events = events
        self.open_browser = open_browser
        self.stop_event = threading.Event()
        self.rebuild_event = threading.Event()
        self.lock = threading.Lock()
        self.active: subprocess.Popen | None = None
        self.server: subprocess.Popen | None = None
        self.worker: threading.Thread | None = None
        self.browser_opened = False
        self.npm_install_signature: int | None = None

    def emit(self, kind: str, message: str) -> None:
        self.events.put((kind, message))

    def stop_active(self) -> None:
        with self.lock:
            process = self.active
        stop_process_tree(process)

    def stop(self) -> None:
        self.stop_event.set()
        self.stop_active()

    def start(self) -> None:
        self.worker = threading.Thread(target=self.run, daemon=True)
        self.worker.start()

    def spawn(self, args: list[str], cwd: Path, env: dict[str, str] | None = None) -> subprocess.Popen:
        options = {"cwd": str(cwd), "env": env, "stdin": subprocess.DEVNULL,
                   "stdout": subprocess.PIPE, "stderr": subprocess.STDOUT,
                   "text": True, "encoding": "utf-8", "errors": "replace"}
        if sys.platform == "win32":
            options["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP | subprocess.CREATE_NO_WINDOW
        else:
            options["start_new_session"] = True
        return subprocess.Popen(args, **options)

    def read_output(self, process: subprocess.Popen) -> None:
        assert process.stdout is not None
        for line in process.stdout:
            self.emit("log", line.rstrip())

    def execute(self, args: list[str], cwd: Path) -> None:
        if self.stop_event.is_set():
            raise InterruptedError()
        self.emit("log", f"> {' '.join(args)}")
        process = self.spawn(args, cwd)
        with self.lock:
            self.active = process
        reader = threading.Thread(target=self.read_output, args=(process,), daemon=True)
        reader.start()
        try:
            while process.poll() is None:
                if self.stop_event.wait(0.2):
                    stop_process_tree(process)
                    raise InterruptedError()
            reader.join(timeout=2)
            if process.returncode != 0:
                raise RuntimeError(f"Lệnh thất bại (exit {process.returncode}): {' '.join(args)}")
        finally:
            with self.lock:
                if self.active is process:
                    self.active = None

    def build(self) -> None:
        self.emit("status", "Đang build React Admin + Rust (debug)...")
        admin = self.root / "cms" / "admin"
        # A pre-existing node_modules can belong to an older main and miss new
        # dependencies. Always npm ci once at startup; repeat on lockfile edits.
        lock_signature = (admin / "package-lock.json").stat().st_mtime_ns
        if self.npm_install_signature != lock_signature or not (admin / "node_modules").is_dir():
            self.execute([command("npm"), "ci"], admin)
            self.npm_install_signature = lock_signature
        self.execute([command("npm"), "run", "build"], admin)
        self.execute([command("cargo"), "build", "--locked", "-p", "opendash-server"],
                     self.root / "cms")

    def launch_server(self) -> None:
        if self.stop_event.is_set():
            raise InterruptedError()
        self.site_root.mkdir(parents=True, exist_ok=True)
        env = os.environ.copy()
        env["OPENDASH_PORT"] = str(self.port)
        env["OPENDASH_SITE_ROOT"] = str(self.site_root)
        exe = self.root / "cms" / "target" / "debug" / ("opendash.exe" if sys.platform == "win32" else "opendash")
        if not exe.is_file():
            raise RuntimeError(f"Không tìm thấy binary vừa build: {exe}")
        process = self.spawn([str(exe)], self.root, env)
        with self.lock:
            self.server = process
            self.active = process
        threading.Thread(target=self.read_output, args=(process,), daemon=True).start()
        for _ in range(90):
            if self.stop_event.is_set():
                raise InterruptedError()
            if process.poll() is not None:
                raise RuntimeError(f"OpenDash thoát sớm (exit {process.returncode}).")
            if health(self.port):
                url = f"http://127.0.0.1:{self.port}/admin/#/posts"
                self.emit("status", f"Đang chạy: {url}")
                self.emit("ready", url)
                if self.open_browser and not self.browser_opened:
                    webbrowser.open(url)
                    self.browser_opened = True
                return
            if self.stop_event.wait(0.2):
                raise InterruptedError()
        raise RuntimeError("OpenDash chưa sẵn sàng sau khi khởi chạy.")

    def stop_server(self) -> None:
        with self.lock:
            process = self.server
        stop_process_tree(process)
        with self.lock:
            if self.active is process:
                self.active = None
            self.server = None

    def run(self) -> None:
        try:
            self.emit("log", f"Source: {self.root}")
            self.emit("log", f"Site data: {self.site_root}")
            self.emit("log", f"URL: http://127.0.0.1:{self.port}/admin/")
            take_over_port(self.port, lambda message: self.emit("log", message))
            while not self.stop_event.is_set():
                self.stop_server()
                self.build()
                self.launch_server()
                baseline = source_snapshot(self.root)
                pending = None
                while not self.stop_event.wait(0.7):
                    with self.lock:
                        server = self.server
                    if server is not None and server.poll() is not None:
                        raise RuntimeError(f"OpenDash đã thoát (exit {server.returncode}).")
                    if self.rebuild_event.is_set():
                        self.rebuild_event.clear()
                        break
                    if not self.watch:
                        continue
                    latest = source_snapshot(self.root)
                    if latest == baseline:
                        pending = None
                    elif pending is None or latest != pending[0]:
                        pending = (latest, time.monotonic())
                    elif time.monotonic() - pending[1] >= 1.2:
                        self.emit("log", "Phát hiện thay đổi mã nguồn. Build lại...")
                        break
        except InterruptedError:
            pass
        except Exception as exc:
            self.emit("status", f"Lỗi: {exc}")
            self.emit("log", f"LỖI: {exc}")
        finally:
            self.stop_server()
            self.stop_active()
            self.emit("stopped", "OpenDash đã dừng.")


def main() -> int:
    parser = argparse.ArgumentParser(description="OpenDash desktop dev launcher (Windows/macOS)")
    parser.add_argument("--port", type=int, default=int(os.environ.get("OPENDASH_PORT", "18766")))
    parser.add_argument("--site-root", type=Path, default=Path(os.environ.get("OPENDASH_SITE_ROOT", ROOT)))
    parser.add_argument("--no-watch", action="store_true", help="Không tự build lại khi file thay đổi")
    parser.add_argument("--no-browser", action="store_true", help="Không tự mở trình duyệt")
    parser.add_argument("--headless", action="store_true", help="Dùng terminal, Ctrl+C để dừng")
    args = parser.parse_args()
    if not 1 <= args.port <= 65535:
        parser.error("Port phải trong khoảng 1..65535.")
    messages: queue.Queue = queue.Queue()
    runner = DevRunner(ROOT, args.site_root, args.port, not args.no_watch, messages, not args.no_browser)

    if args.headless:
        runner.start()
        try:
            while runner.worker and runner.worker.is_alive():
                try:
                    kind, value = messages.get(timeout=0.2)
                    print(value, flush=True)
                    if kind == "status" and value.startswith("Lỗi:"):
                        return 1
                except queue.Empty:
                    continue
        except KeyboardInterrupt:
            runner.stop()
        finally:
            runner.stop()
            if runner.worker:
                runner.worker.join(timeout=12)
        return 0

    try:
        import tkinter as tk
        from tkinter.scrolledtext import ScrolledText
    except ImportError as exc:
        raise SystemExit("Python thiếu Tkinter. Cài Python có Tk hoặc chạy python dev.py --headless.") from exc

    window = tk.Tk()
    window.title("OpenDash — Dev")
    window.geometry("760x520")
    window.minsize(560, 350)
    frame = tk.Frame(window, padx=18, pady=14)
    frame.pack(fill="both", expand=True)
    tk.Label(frame, text="OpenDash CMS — Development", font=("Arial", 16, "bold"), anchor="w").pack(fill="x")
    status = tk.StringVar(value="Đang khởi động...")
    tk.Label(frame, textvariable=status, anchor="w", wraplength=690).pack(fill="x", pady=(8, 4))
    tk.Label(frame, text=f"Port {args.port}  |  Site: {runner.site_root}", anchor="w", wraplength=690).pack(fill="x")
    toolbar = tk.Frame(frame)
    toolbar.pack(fill="x", pady=(10, 8))
    tk.Button(toolbar, text="Mở Admin", command=lambda: webbrowser.open(f"http://127.0.0.1:{args.port}/admin/#/posts")).pack(side="left", padx=(0, 8))
    tk.Button(toolbar, text="Mở website", command=lambda: webbrowser.open(f"http://127.0.0.1:{args.port}/")).pack(side="left", padx=(0, 8))
    tk.Button(toolbar, text="Build lại", command=runner.rebuild_event.set).pack(side="left")
    tk.Label(frame, text="Đóng cửa sổ này để dừng OpenDash. Thay đổi mã nguồn sẽ tự build lại.", anchor="w").pack(fill="x")
    logs = ScrolledText(frame, height=20, wrap="word", state="disabled")
    logs.pack(fill="both", expand=True, pady=(8, 0))
    closing = False

    def refresh() -> None:
        while True:
            try:
                kind, value = messages.get_nowait()
            except queue.Empty:
                break
            if kind == "status":
                status.set(value)
            elif kind == "stopped" and not closing:
                status.set(value)
            elif kind == "log":
                logs.configure(state="normal")
                logs.insert("end", value + "\n")
                logs.see("end")
                logs.configure(state="disabled")
        if closing:
            if runner.worker is None or not runner.worker.is_alive():
                window.destroy()
            else:
                window.after(100, refresh)
        else:
            window.after(100, refresh)

    def close() -> None:
        nonlocal closing
        if closing:
            return
        closing = True
        status.set("Đang dừng build/server...")
        runner.stop()

    window.protocol("WM_DELETE_WINDOW", close)
    runner.start()
    window.after(100, refresh)
    window.mainloop()
    return 0


if __name__ == "__main__":
    sys.exit(main())
