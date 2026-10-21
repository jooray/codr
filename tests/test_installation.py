import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
sys.path.insert(0, str(SCRIPTS))
import install as installer

spec = importlib.util.spec_from_file_location("codr_setup", SCRIPTS / "setup.py")
setup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(setup)


class InstallerTests(unittest.TestCase):
    def test_supported_architectures_and_rejection(self):
        self.assertEqual(installer.platform_name("Linux", "x86_64"), "linux-x86_64")
        self.assertEqual(installer.platform_name("Linux", "arm64"), "linux-aarch64")
        with self.assertRaises(installer.InstallError):
            installer.platform_name("Darwin", "arm64")

    def test_downloaded_archive_is_checked_before_installation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / "source.tar.gz"
            binary = b"#!/bin/sh\nprintf 'test binary\\n'\n"
            with tarfile.open(archive, "w:gz") as bundle:
                entry = tarfile.TarInfo("herdr-white-noise-bridge")
                entry.size = len(binary)
                bundle.addfile(entry, io.BytesIO(binary))
            payload = archive.read_bytes()
            checksum = hashlib.sha256(payload).hexdigest().encode() + b"  source.tar.gz\n"

            def download(url, destination):
                destination.write_bytes(checksum if url.endswith(".sha256") else payload)

            target = root / "bin/bridge"
            with patch.object(installer, "download", side_effect=download):
                installer.install_bundle("https://github.com/example/release.tar.gz", "herdr-white-noise-bridge", target)
            self.assertEqual(target.read_bytes(), binary)
            self.assertEqual(target.stat().st_mode & 0o777, 0o755)
            checksum = b"0" * 64 + b"\n"
            with patch.object(installer, "download", side_effect=download):
                with self.assertRaises(installer.InstallError):
                    installer.install_bundle("https://github.com/example/release.tar.gz", "herdr-white-noise-bridge", target)
            self.assertEqual(target.read_bytes(), binary)

    def test_symlink_archive_member_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / "source.tar.gz"
            with tarfile.open(archive, "w:gz") as bundle:
                entry = tarfile.TarInfo("bridge")
                entry.type = tarfile.SYMTYPE
                entry.linkname = "/etc/passwd"
                bundle.addfile(entry)
            payload = archive.read_bytes()

            def download(url, destination):
                destination.write_bytes(hashlib.sha256(payload).hexdigest().encode() if url.endswith(".sha256") else payload)

            with patch.object(installer, "download", side_effect=download):
                with self.assertRaises(installer.InstallError):
                    installer.install_bundle("https://github.com/example/release.tar.gz", "bridge", root / "installed")
            self.assertFalse((root / "installed").exists())


class SetupTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.home = Path(self.temporary.name)
        self.environment = patch.dict(os.environ, {"HOME": str(self.home), "XDG_CONFIG_HOME": str(self.home / "config space %"), "XDG_STATE_HOME": str(self.home / "state"), "CODR_OWNER_NPUB": ""}, clear=True)
        self.environment.start()
        self.addCleanup(self.environment.stop)
        self.addCleanup(self.temporary.cleanup)
        self.bridge = self.home / ".local/bin/herdr-white-noise-bridge"
        self.bridge.parent.mkdir(parents=True)
        self.bridge.write_text("fixture")
        self.socket = self.home / "named-herdr.sock"
        self.calls = []
        self.bootstrap_count = 0
        self.config = self.home / "config space %/herdr-white-noise-bridge/config.toml"

    def runner(self, argv):
        self.calls.append([str(a) for a in argv])
        if "bootstrap" in argv:
            self.bootstrap_count += 1
            return json.dumps({"account_id_hex": "ab" * 32, "npub": "npub1bridge", "label": "herdr-bridge", "key_package_published": True})
        if "--version" in argv:
            return "fixture version"
        return ""

    def mocks(self):
        return patch.multiple(setup,
            find_executable=lambda name, candidates=(): "/usr/bin/" + name,
            ensure_mdk=lambda runner: Path("/usr/bin/wn-agent"),
            read_snapshot=lambda path: {"agents": []},
            local_accounts=lambda path: [])

    def test_fresh_setup_generates_private_portable_config_and_preserves_on_repeat(self):
        with self.mocks():
            result = setup.configure("npub1fixture", self.socket, self.runner)
            self.assertTrue(self.config.exists())
            self.assertEqual(self.config.stat().st_mode & 0o777, 0o600)
            config = setup.tomllib.loads(self.config.read_text())
            self.assertEqual(config["phone_npub"], "npub1fixture")
            self.assertEqual(config["herdr_socket"], str(self.socket))
            self.assertEqual(config["account_id_hex"], "ab" * 32)
            unit = self.config.parents[1] / "systemd/user/herdr-white-noise-bridge.service"
            self.assertIn("%%", unit.read_text())
            self.assertIn("--config", unit.read_text())
            original = self.config.read_bytes()
            setup.configure("npub1fixture", self.socket, self.runner)
            self.assertEqual(self.config.read_bytes(), original)
            self.assertEqual(self.bootstrap_count, 1)
            self.assertNotIn("restart", [part for call in self.calls for part in call])

    def test_wrong_owner_cannot_change_an_existing_installation(self):
        with self.mocks():
            setup.configure("npub1fixture", self.socket, self.runner)
            original = self.config.read_bytes()
            with self.assertRaises(setup.SetupError):
                setup.configure("npub1different", self.socket, self.runner)
            self.assertEqual(self.config.read_bytes(), original)

    def test_invalid_owner_stops_before_services_or_identity_creation(self):
        def runner(argv):
            self.calls.append(argv)
            raise setup.SetupError("Invalid owner")
        with self.mocks():
            with self.assertRaises(setup.SetupError):
                setup.configure("npub1invalid", self.socket, runner)
        self.assertFalse(self.config.exists())
        self.assertFalse((self.home / ".marmot-agents/herdr").exists())
        self.assertTrue(all("systemctl" not in str(call) for call in self.calls))

    def test_resume_selects_existing_account_without_creating_another(self):
        with self.mocks(), patch.object(setup, "local_accounts", return_value=[{"label": "herdr-bridge", "account_id_hex": "ab" * 32, "local_signing": True}]):
            setup.configure("npub1fixture", self.socket, self.runner)
        bootstrap = next(call for call in self.calls if "bootstrap" in call)
        self.assertIn("--no-create", bootstrap)
        self.assertIn("--account-id-hex", bootstrap)

    def test_concurrent_setup_cannot_create_a_second_identity(self):
        with setup.setup_lock(self.config.parent), self.mocks():
            with self.assertRaises(setup.SetupError):
                setup.configure("npub1fixture", self.socket, self.runner)
        self.assertEqual(self.bootstrap_count, 0)

    def test_existing_install_cannot_silently_change_the_selected_server(self):
        with self.mocks():
            setup.configure("npub1fixture", self.socket, self.runner)
            with self.assertRaises(setup.SetupError):
                setup.configure("npub1fixture", self.home / "different.sock", self.runner)

    def test_unit_arguments_escape_literal_systemd_environment_references(self):
        self.assertEqual(setup.unit_argument("/config/${CODR_PROFILE}%"), '"/config/$${CODR_PROFILE}%%"')

    def test_retry_after_worker_start_failure_returns_the_same_bridge_identity(self):
        def fail_worker_start(argv):
            if list(argv) == ["systemctl", "--user", "start", "herdr-white-noise-bridge.service"]:
                raise setup.SetupError("Worker start failed")
            return self.runner(argv)
        with self.mocks():
            with self.assertRaises(setup.SetupError):
                setup.configure("npub1fixture", self.socket, fail_worker_start)
            self.assertTrue(self.config.exists())
            result = setup.configure("npub1fixture", self.socket, self.runner)
        self.assertEqual(result["bridge_npub"], "npub1bridge")
        self.assertEqual(self.bootstrap_count, 1)


if __name__ == "__main__":
    unittest.main()
