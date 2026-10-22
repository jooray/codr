"""Install or reuse codr's private, memory-limited Phonon-2 worker."""
import json
from contextlib import closing
import hashlib
import http.client
import os
from pathlib import Path
import platform
import shutil
import sys
import tempfile
import time
import urllib.request

import services
from transcribe import UnixHTTPConnection

ROOT = Path(__file__).resolve().parents[1]
VERSION = "0.2.12"


def attach_worker(private_write, manager):
    """Make systemd start the speech worker with the bridge. launchd starts all agents itself."""
    if manager.backend != services.SYSTEMD:
        return
    path = manager.unit_root / "herdr-white-noise-bridge.service.d/phonon.conf"
    unit = "[Unit]\nWants=codr-phonon.service\nAfter=codr-phonon.service\n"
    if path.exists() and path.read_text() != unit:
        raise ValueError("Existing Phonon service dependency differs; inspect it before changing setup")
    if not path.exists():
        private_write(path, unit)


def ready(socket_path):
    try:
        with closing(UnixHTTPConnection(socket_path, timeout=2)) as connection:
            connection.request("GET", "/health")
            response = connection.getresponse()
            payload = json.loads(response.read(16384))
            return response.status == 200 and payload.get("kind") == "speech" and payload.get("repo") == "FermionResearch/Phonon-2"
    except (OSError, ValueError, http.client.HTTPException):
        return False


def ffmpeg_path():
    found = shutil.which("ffmpeg")
    if not found and sys.platform == "darwin":
        for directory in ("/opt/homebrew/bin", "/usr/local/bin"):
            candidate = Path(directory) / "ffmpeg"
            if candidate.is_file() and os.access(candidate, os.X_OK):
                found = str(candidate)
    if not found:
        raise ValueError("Audio support needs FFmpeg. Install it, then run setup again")
    return found


def install_packages(runner, python):
    """Phonon runs through MLX on Apple silicon and through torch elsewhere."""
    if sys.platform == "darwin":
        if platform.machine() in ("arm64", "aarch64"):
            runner([str(python), "-m", "pip", "install", f"fermion-research[mlx]=={VERSION}"])
        else:
            runner([str(python), "-m", "pip", "install", f"fermion-research=={VERSION}"])
        return
    runner([str(python), "-m", "pip", "install", "--index-url", "https://download.pytorch.org/whl/cpu", "torch==2.8.0"])
    runner([str(python), "-m", "pip", "install", f"fermion-research=={VERSION}"])


def configure_phonon(runner, private_write, unit_argument=None):
    home = Path.home()
    config_root = Path(os.environ.get("XDG_CONFIG_HOME", home / ".config"))
    manager = services.for_host(runner, private_write, config_root, error=ValueError)
    root = home / ".local/share/codr/phonon"
    root.mkdir(parents=True, mode=0o700, exist_ok=True)
    speech_socket = services.check_socket_path(services.runtime_dir() / "codr-phonon/phonon.sock")
    speech_socket.parent.mkdir(parents=True, mode=0o700, exist_ok=True)
    helper = root / "transcribe.py"
    private_write(helper, (ROOT / "scripts/transcribe.py").read_text())
    ffmpeg = ffmpeg_path()
    command = [sys.executable, str(helper), "--socket", str(speech_socket), "--ffmpeg", ffmpeg]
    if ready(speech_socket):
        if manager.backend == services.SYSTEMD and (manager.unit_root / "codr-phonon.service").exists():
            attach_worker(private_write, manager)
            manager.reload()
        return command
    print("Phonon-2: local CPU or MLX worker, approximately 164 MB model download.", flush=True)
    maximum = (3, 12) if sys.platform == "darwin" and platform.machine() not in ("arm64", "aarch64") else None
    python = services.find_python((3, 10), maximum=maximum)
    if not python:
        requirement = "Python 3.10 to 3.12" if maximum else "Python 3.10 or newer"
        raise ValueError(f"Audio support needs {requirement}; install one (for example: brew install python@3.12), then run setup again")
    venv = root / "venv"
    interpreter = venv / "bin/python"
    if not interpreter.exists():
        runner([python, "-m", "venv", "--without-pip", str(venv)])
    if runner([str(interpreter), "-c", "import importlib.util; print(importlib.util.find_spec('pip') is not None)"]).strip() != "True":
        # Debian and macOS venvs can lack ensurepip. Bootstrap the standard pip
        # wheel in this private environment, verified against PyPI's digest.
        with urllib.request.urlopen("https://pypi.org/pypi/pip/25.2/json", timeout=30) as response:
            release = json.load(response)
        wheel = next(file for file in release["urls"] if file["filename"] == "pip-25.2-py3-none-any.whl")
        with tempfile.TemporaryDirectory(prefix="codr-pip-") as temporary:
            path = Path(temporary) / wheel["filename"]
            with urllib.request.urlopen(wheel["url"], timeout=60) as response:
                payload = response.read(3 * 1024 * 1024)
            if hashlib.sha256(payload).hexdigest() != wheel["digests"]["sha256"]:
                raise ValueError("pip bootstrap checksum did not match PyPI")
            path.write_bytes(payload)
            runner(["env", f"PYTHONPATH={path}", str(interpreter), "-m", "pip", "install", "--no-index", str(path)])
    fermion = venv / "bin/fermion"
    if not fermion.exists():
        install_packages(runner, interpreter)
    runner([str(interpreter), "-c", f"import importlib.metadata; assert importlib.metadata.version('fermion-research') == '{VERSION}'"])
    runner(["env", f"FERMION_CACHE_DIR={root / 'models'}", f"HF_HOME={root / 'huggingface'}",
            str(fermion), "transcribe", "phonon-2", "--download-only", "unused.wav"])
    manager.install(services.Service("codr-phonon", "codr local Phonon-2 speech worker",
        [fermion, "serve", "phonon-2", "--unix-socket", speech_socket, "--threads", "4"],
        environment={"FERMION_CACHE_DIR": str(root / "models"), "HF_HOME": str(root / "huggingface")}, throttle=10))
    attach_worker(private_write, manager)
    manager.reload()
    manager.start("codr-phonon")
    for _ in range(60):
        if ready(speech_socket):
            return command
        time.sleep(1)
    raise ValueError("Phonon-2 did not become ready; inspect the codr-phonon service log")


if __name__ == "__main__":
    from setup import private_write, unit_argument, run
    print(json.dumps(configure_phonon(run, private_write, unit_argument)))
