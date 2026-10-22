import importlib.util
import io
import json
from pathlib import Path
import socket
import sys
import tempfile
import threading
import unittest
import wave
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
sys.path.insert(0, str(SCRIPTS))


class PhononTests(unittest.TestCase):
    def test_normalised_audio_goes_to_the_private_worker_and_returns_plain_text(self):
        import transcribe
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            audio = root / "voice.m4a"
            audio.write_bytes(b"fixture")
            server = socket.socket(socket.AF_UNIX)
            server.bind(str(root / "phonon.sock"))
            server.listen()
            requests = []
            def handle():
                with server.accept()[0] as connection:
                    reader = connection.makefile("rb")
                    head = reader.readline()
                    headers = {}
                    while line := reader.readline().strip():
                        key, value = line.split(b":", 1)
                        headers[key.lower()] = value.strip()
                    payload = reader.read(int(headers[b"content-length"]))
                    requests.append((head, payload))
                    body = json.dumps({"text": "Use the blue button."}).encode()
                    connection.sendall(b"HTTP/1.1 200 OK\r\nContent-Length: " + str(len(body)).encode() + b"\r\n\r\n" + body)
            thread = threading.Thread(target=handle, daemon=True)
            thread.start()
            def convert(argv, **kwargs):
                with wave.open(str(argv[-1]), "wb") as output:
                    output.setparams((1, 2, 16000, 0, "NONE", "not compressed"))
                    output.writeframes(b"\0\0" * 16000)
            try:
                with patch.object(transcribe.subprocess, "run", side_effect=convert):
                    result = transcribe.transcribe(audio, root / "phonon.sock", "ffmpeg")
                self.assertEqual(result, "Use the blue button.")
                self.assertIn(b"POST /v1/audio/transcriptions", requests[0][0])
                self.assertIn(b"RIFF", requests[0][1])
                self.assertIn(b"phonon-2", requests[0][1])
            finally:
                server.close()
                thread.join(timeout=3)

    def test_healthy_existing_worker_is_reused_without_installing_or_restarting(self):
        import phonon
        import setup
        with tempfile.TemporaryDirectory() as temporary, patch.dict("os.environ", {"HOME": temporary, "XDG_RUNTIME_DIR": temporary}):
            calls = []
            with patch.object(phonon, "ready", return_value=True), patch.object(phonon.shutil, "which", return_value="/usr/bin/ffmpeg"):
                command = phonon.configure_phonon(lambda argv: calls.append(argv), setup.private_write, setup.unit_argument)
            self.assertEqual(calls, [])
            self.assertIn("--socket", command)
            self.assertTrue(Path(command[1]).is_file())

    def test_healthy_worker_keeps_the_setup_helper_interpreter_without_discovery(self):
        import phonon
        import setup
        with tempfile.TemporaryDirectory() as temporary, patch.dict("os.environ", {"HOME": temporary, "XDG_RUNTIME_DIR": temporary, "CODR_SERVICE_MANAGER": "systemd"}):
            with patch.object(phonon, "ready", return_value=True), patch.object(phonon.shutil, "which", return_value="/usr/bin/ffmpeg"), patch.object(phonon.sys, "executable", "/existing/python3.11"), patch.object(phonon.services, "find_python", return_value="/preferred/python3.12") as discover:
                command = phonon.configure_phonon(lambda argv: self.fail("Healthy worker must not be changed"), setup.private_write)
            self.assertEqual(command[0], "/existing/python3.11")
            discover.assert_not_called()

    def test_launchd_speech_worker_uses_the_model_cache_prepared_by_setup(self):
        import phonon
        import services
        import setup
        import plistlib
        with tempfile.TemporaryDirectory() as temporary, patch.dict("os.environ", {"HOME": temporary, "XDG_RUNTIME_DIR": temporary}):
            root = Path(temporary) / ".local/share/codr/phonon"
            binaries = root / "venv/bin"
            binaries.mkdir(parents=True)
            for name in ("python", "fermion"):
                (binaries / name).write_text("fixture")
            def runner(argv):
                return "True" if "find_spec('pip')" in str(argv[-1]) else ""
            manager = services.LaunchdServices(runner, setup.private_write, Path(temporary) / ".config")
            with patch.object(phonon.services, "for_host", return_value=manager), patch.object(phonon.services, "find_python", return_value="/python3.12"), patch.object(phonon, "ready", side_effect=[False, True]), patch.object(phonon, "ffmpeg_path", return_value="/ffmpeg"), patch.object(manager, "start"):
                phonon.configure_phonon(runner, setup.private_write)
            payload = plistlib.loads(manager.path("codr-phonon").read_bytes())
            self.assertEqual(payload["EnvironmentVariables"]["FERMION_CACHE_DIR"], str(root / "models"))
            self.assertEqual(payload["EnvironmentVariables"]["HF_HOME"], str(root / "huggingface"))

    def test_runtime_selection_limits_python_only_on_intel_macos(self):
        import phonon
        import setup
        for system, machine, maximum in (("darwin", "x86_64", (3, 12)), ("darwin", "arm64", None), ("linux", "x86_64", None)):
            with self.subTest(system=system, machine=machine), tempfile.TemporaryDirectory() as temporary:
                with patch.dict("os.environ", {"HOME": temporary, "XDG_RUNTIME_DIR": temporary, "CODR_SERVICE_MANAGER": "systemd"}), patch.object(phonon.sys, "platform", system), patch.object(phonon.platform, "machine", return_value=machine), patch.object(phonon, "ready", return_value=False), patch.object(phonon, "ffmpeg_path", return_value="/ffmpeg"), patch.object(phonon.services, "find_python", return_value=None) as discover:
                    with self.assertRaisesRegex(ValueError, "Audio support needs Python"):
                        phonon.configure_phonon(lambda argv: self.fail("No runtime selected"), setup.private_write)
                    discover.assert_called_once_with((3, 10), maximum=maximum)

    def test_apple_silicon_installs_the_mlx_runtime(self):
        import phonon
        calls = []
        with patch.object(phonon.sys, "platform", "darwin"), patch.object(phonon.platform, "machine", return_value="arm64"):
            phonon.install_packages(lambda argv: calls.append(argv), "/venv/bin/python")
        self.assertEqual(calls, [["/venv/bin/python", "-m", "pip", "install", "fermion-research[mlx]==0.2.12"]])

    def test_intel_mac_uses_the_plain_fermion_wheel(self):
        import phonon
        calls = []
        with patch.object(phonon.sys, "platform", "darwin"), patch.object(phonon.platform, "machine", return_value="x86_64"):
            phonon.install_packages(lambda argv: calls.append(argv), "/venv/bin/python")
        self.assertEqual(calls, [["/venv/bin/python", "-m", "pip", "install", "fermion-research==0.2.12"]])

    def test_linux_installs_the_cpu_torch_wheel_first(self):
        import phonon
        calls = []
        with patch.object(phonon.sys, "platform", "linux"):
            phonon.install_packages(lambda argv: calls.append(argv), "/venv/bin/python")
        self.assertEqual(len(calls), 2)
        self.assertIn("https://download.pytorch.org/whl/cpu", calls[0])

    def test_worker_has_a_one_gibibyte_limit_and_a_private_socket(self):
        unit = (SCRIPTS.parent / "systemd/codr-phonon.service").read_text()
        self.assertIn("MemoryMax=1G", unit)
        self.assertIn("MemorySwapMax=0", unit)
        self.assertIn("RuntimeDirectoryMode=0700", unit)
        self.assertIn("--unix-socket", unit)
        self.assertNotIn("0.0.0.0", unit)
