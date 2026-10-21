"""Install or reuse codr's private, memory-limited Phonon-2 worker."""
import json
from contextlib import closing
import hashlib
import http.client
import os
from pathlib import Path
import shutil
import sys
import tempfile
import time
import urllib.request

from transcribe import UnixHTTPConnection

ROOT = Path(__file__).resolve().parents[1]
VERSION = "0.2.12"


def attach_worker(private_write):
    root = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")) / "systemd/user"
    path = root / "herdr-white-noise-bridge.service.d/phonon.conf"
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


def configure_phonon(runner, private_write, unit_argument):
    home = Path.home()
    root = home / ".local/share/codr/phonon"
    root.mkdir(parents=True, mode=0o700, exist_ok=True)
    runtime = Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}"))
    speech_socket = runtime / "codr-phonon/phonon.sock"
    helper = root / "transcribe.py"
    private_write(helper, (ROOT / "scripts/transcribe.py").read_text())
    ffmpeg = shutil.which("ffmpeg")
    if not ffmpeg:
        raise ValueError("Audio support needs FFmpeg. Install it, then run setup again")
    command = [sys.executable, str(helper), "--socket", str(speech_socket), "--ffmpeg", ffmpeg]
    if ready(speech_socket):
        unit_root = Path(os.environ.get("XDG_CONFIG_HOME", home / ".config")) / "systemd/user"
        if (unit_root / "codr-phonon.service").exists():
            attach_worker(private_write)
            runner(["systemctl", "--user", "daemon-reload"])
        return command
    print("Phonon-2: local CPU worker, approximately 164 MB model download, 1 GiB service memory limit.", flush=True)
    venv = root / "venv"
    python = venv / "bin/python"
    if not python.exists():
        runner([sys.executable, "-m", "venv", "--without-pip", str(venv)])
    if runner([str(python), "-c", "import importlib.util; print(importlib.util.find_spec('pip') is not None)"]).strip() != "True":
        # Debian can provide venv without ensurepip. Bootstrap the standard pip
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
            runner(["env", f"PYTHONPATH={path}", str(python), "-m", "pip", "install", "--no-index", str(path)])
    fermion = venv / "bin/fermion"
    if not fermion.exists():
        runner([str(python), "-m", "pip", "install", "--index-url", "https://download.pytorch.org/whl/cpu", "torch==2.8.0"])
        runner([str(python), "-m", "pip", "install", f"fermion-research=={VERSION}"])
    runner([str(python), "-c", f"import importlib.metadata; assert importlib.metadata.version('fermion-research') == '{VERSION}'"])
    runner(["env", f"FERMION_CACHE_DIR={root / 'models'}", f"HF_HOME={root / 'huggingface'}",
            str(fermion), "transcribe", "phonon-2", "--download-only", "unused.wav"])
    unit = (ROOT / "systemd/codr-phonon.service").read_text()
    unit = unit.replace(next(line for line in unit.splitlines() if line.startswith("ExecStart=")),
                        "ExecStart=" + " ".join(unit_argument(arg) for arg in [fermion, "serve", "phonon-2", "--unix-socket", speech_socket, "--threads", "4"]))
    unit_path = Path(os.environ.get("XDG_CONFIG_HOME", home / ".config")) / "systemd/user/codr-phonon.service"
    if unit_path.exists() and unit_path.read_text() != unit:
        raise ValueError("Existing codr-phonon.service differs; inspect it before changing the worker")
    if not unit_path.exists():
        private_write(unit_path, unit)
    attach_worker(private_write)
    runner(["systemctl", "--user", "daemon-reload"])
    runner(["systemctl", "--user", "start", "codr-phonon.service"])
    for _ in range(60):
        if ready(speech_socket):
            return command
        time.sleep(1)
    raise ValueError("Phonon-2 did not become ready within its 1 GiB memory limit; inspect codr-phonon.service")


if __name__ == "__main__":
    from setup import private_write, unit_argument, run
    print(json.dumps(configure_phonon(run, private_write, unit_argument)))
