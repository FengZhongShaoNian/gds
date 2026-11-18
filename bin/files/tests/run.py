"""Run the driven zgui E2E suite and native headless Weston smoke check; retain artifacts."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time

CHECKPOINTS = [
    "01-open",
    "02-rename-undo-redo",
    "03-new-folder",
    "04-trash-restore",
    "05-external-watch",
    "06-grid-switch",
    "07-tab-drag",
    "08-search-ime",
    "09-details-terminal",
    "10-location-mouse-history",
]


def wait_for(predicate, timeout_secs: float, message: str) -> None:
    deadline = time.monotonic() + timeout_secs
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(0.05)
    raise SystemExit(message)


def build_base_env(
    home: Path,
    runtime: Path,
    artifacts: Path,
    weston: Path,
) -> dict[str, str]:
    env = os.environ.copy()
    for key in ("DISPLAY", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS"):
        env.pop(key, None)
    env.update(
        HOME=str(home),
        XDG_CONFIG_HOME=str(home / ".config"),
        XDG_DATA_HOME=str(home / ".local/share"),
        XDG_STATE_HOME=str(home / ".local/state"),
        XDG_CACHE_HOME=str(home / ".cache"),
        XDG_RUNTIME_DIR=str(runtime),
        DBUS_SYSTEM_BUS_ADDRESS=f"unix:path={runtime}/unavailable-system-bus",
        GDS_E2E_ARTIFACTS=str(artifacts),
        GDS_E2E_SCREENSHOT=str(weston.with_name("weston-screenshooter")),
    )
    return env


def run_driven(
    binary: Path,
    work: Path,
    artifacts: Path,
    base_env: dict[str, str],
) -> Path:
    env = base_env.copy()
    env["GDS_E2E_MODE"] = "driven"
    command = [
        "timeout",
        "--kill-after=5",
        "90",
        "dbus-run-session",
        f"--config-file={artifacts / 'dbus.conf'}",
        "--",
        str(binary),
        str(work),
    ]
    log_path = artifacts / "driven.log"
    with log_path.open("w", encoding="utf-8") as log:
        result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)
    (artifacts / "driven-exit-code.txt").write_text(f"{result.returncode}\n", encoding="utf-8")
    if result.returncode != 0:
        raise SystemExit(f"Driven E2E failed (exit {result.returncode}); inspect {log_path}")

    passed_path = artifacts / "driven-passed.txt"
    if not passed_path.is_file():
        raise SystemExit(f"Driven E2E did not write {passed_path}; inspect {log_path}")

    for name in CHECKPOINTS:
        png = artifacts / f"{name}.png"
        a11y = artifacts / f"{name}.accessibility.txt"
        if not png.is_file() or png.stat().st_size == 0:
            raise SystemExit(f"Missing or empty PNG artifact {png}; inspect {log_path}")
        if not a11y.is_file() or a11y.stat().st_size == 0:
            raise SystemExit(f"Missing or empty accessibility dump {a11y}; inspect {log_path}")

    return passed_path


def run_wayland_smoke(
    binary: Path,
    weston: Path,
    work: Path,
    runtime: Path,
    artifacts: Path,
    base_env: dict[str, str],
) -> Path:
    socket_name = "files-e2e-wayland"
    socket_path = runtime / socket_name
    weston_log_path = artifacts / "weston.log"
    app_log_path = artifacts / "wayland-app.log"
    screenshooter = weston.with_name("weston-screenshooter")

    weston_env = base_env.copy()
    weston_cmd = [
        "dbus-run-session",
        f"--config-file={artifacts / 'dbus.conf'}",
        "--",
        str(weston),
        "--backend=headless",
        "--renderer=pixman",
        f"--socket={socket_name}",
        "--width=1280",
        "--height=900",
        "--fake-seat",
        "--no-config",
        "--idle-time=0",
        "--debug",
    ]

    with weston_log_path.open("w", encoding="utf-8") as weston_log:
        weston_proc = subprocess.Popen(
            weston_cmd,
            env=weston_env,
            stdout=weston_log,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        try:
            wait_for(
                lambda: socket_path.exists() or weston_proc.poll() is not None,
                15.0,
                f"Timed out waiting for Weston Wayland socket {socket_path}; inspect {weston_log_path}",
            )
            if weston_proc.poll() is not None:
                raise SystemExit(f"Weston exited early ({weston_proc.returncode}); inspect {weston_log_path}")

            app_env = base_env.copy()
            app_env["WAYLAND_DISPLAY"] = socket_name
            app_env["GDS_E2E_MODE"] = "wayland"

            with app_log_path.open("w", encoding="utf-8") as app_log:
                app_proc = subprocess.Popen(
                    [str(binary), str(work)],
                    env=app_env,
                    stdout=app_log,
                    stderr=subprocess.STDOUT,
                    start_new_session=True,
                )
                try:
                    wait_for(
                        lambda: (
                            app_proc.poll() is not None
                            or "Files ready"
                            in app_log_path.read_text(encoding="utf-8", errors="replace")
                        ),
                        20.0,
                        f"Native Wayland Files session did not become ready; inspect {app_log_path}",
                    )
                    if app_proc.poll() is not None:
                        raise SystemExit(
                            f"Native Wayland app exited early ({app_proc.returncode}); inspect {app_log_path}"
                        )

                    time.sleep(0.3)

                    shot_env = app_env.copy()
                    before = set(artifacts.glob("wayland-screenshot*.png"))
                    shot_res = subprocess.run(
                        [str(screenshooter)],
                        cwd=artifacts,
                        env=shot_env,
                        capture_output=True,
                        text=True,
                        timeout=15,
                    )
                    if shot_res.returncode != 0:
                        raise SystemExit(
                            f"weston-screenshooter failed ({shot_res.returncode}): {shot_res.stderr}"
                        )
                    after = set(artifacts.glob("wayland-screenshot*.png")) - before
                    if not after:
                        after = set(artifacts.glob("*.png")) - {
                            artifacts / f"{name}.png" for name in CHECKPOINTS
                        }
                    if not after:
                        raise SystemExit(f"weston-screenshooter produced no PNG in {artifacts}")
                    newest = max(after, key=lambda p: p.stat().st_mtime)
                    wayland_png = artifacts / "wayland-window.png"
                    shutil.copy2(newest, wayland_png)
                    if wayland_png.stat().st_size == 0:
                        raise SystemExit(f"Captured Wayland PNG is empty: {wayland_png}")
                finally:
                    if app_proc.poll() is None:
                        app_proc.send_signal(signal.SIGTERM)
                        try:
                            app_proc.wait(timeout=10)
                        except subprocess.TimeoutExpired:
                            app_proc.kill()
                            app_proc.wait(timeout=5)
        finally:
            if weston_proc.poll() is None:
                try:
                    os.killpg(weston_proc.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                try:
                    weston_proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    try:
                        os.killpg(weston_proc.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    weston_proc.wait(timeout=5)

    app_log_text = app_log_path.read_text(encoding="utf-8", errors="replace")
    if "timed out waiting on app_will_quit" in app_log_text:
        raise SystemExit(f"Shutdown did not drain cleanly; inspect {app_log_path}")
    return wayland_png


def main() -> None:
    repository = Path(__file__).resolve().parents[3]
    binary = Path(sys.argv[1]).resolve()
    weston = Path(sys.argv[2]).resolve()
    output = repository / "target/files-e2e"
    output.mkdir(parents=True, exist_ok=True)
    artifacts = Path(tempfile.mkdtemp(prefix="run-", dir=output))

    home = artifacts / "home"
    work = home / "work"
    work.mkdir(parents=True)
    (home / ".config/gds/app/files").mkdir(parents=True)
    (home / ".local/share").mkdir(parents=True)
    (home / ".local/state").mkdir(parents=True)
    (home / ".cache").mkdir()
    runtime = artifacts / "runtime"
    runtime.mkdir(mode=0o700)

    (artifacts / "dbus.conf").write_text(
        f'''<busconfig>
  <type>session</type><listen>unix:tmpdir={runtime}</listen>
  <policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy>
</busconfig>
''',
        encoding="utf-8",
    )
    (work / "original.txt").write_bytes(b"original payload\n")
    (home / ".config/gds/app/files/settings.toml").write_text(
        'language = "en-US"\nanimations_enabled = false\n[appearance]\nsource = "builtin"\nmode = "dark"\n',
        encoding="utf-8",
    )

    with binary.open("rb") as binary_file:
        binary_sha256 = hashlib.file_digest(binary_file, "sha256").hexdigest()
    (artifacts / "manifest.json").write_text(
        json.dumps(
            {
                "binary": str(binary),
                "weston": str(weston),
                "binary_sha256": binary_sha256,
                "revision": subprocess.check_output(
                    ["git", "rev-parse", "HEAD"], cwd=repository, text=True
                ).strip(),
                "replay": [sys.executable, str(Path(__file__).resolve()), str(binary), str(weston)],
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    (artifacts / "source.diff").write_bytes(
        subprocess.check_output(
            ["git", "diff", "--", "Cargo.toml", "bin/files", "src"],
            cwd=repository,
        )
    )
    subprocess.run(
        [
            "tar",
            "czf",
            str(artifacts / "source.tar.gz"),
            "Cargo.toml",
            "Cargo.lock",
            "build.rs",
            "src",
            "bin/files/src",
            "bin/files/tests",
            "assets",
            "macros",
            "packaging/nix",
            "shell.nix",
            "flake.lock",
        ],
        cwd=repository,
        check=True,
    )

    print(f"Files E2E artifacts: {artifacts}", flush=True)
    base_env = build_base_env(home, runtime, artifacts, weston)

    driven_passed = run_driven(binary, work, artifacts, base_env)
    wayland_png = run_wayland_smoke(binary, weston, work, runtime, artifacts, base_env)

    summary = (
        driven_passed.read_text(encoding="utf-8", errors="replace").rstrip()
        + f"\nPASS wayland screenshot={wayland_png}\n"
    )
    passed_path = artifacts / "passed.txt"
    passed_path.write_text(summary, encoding="utf-8")
    print(summary, end="")


if __name__ == "__main__":
    main()
