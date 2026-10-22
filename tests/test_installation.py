import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import plistlib
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
sys.path.insert(0, str(SCRIPTS))
import install as installer
import services

spec = importlib.util.spec_from_file_location("codr_setup", SCRIPTS / "setup.py")
setup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(setup)


class InstallerTests(unittest.TestCase):
    def test_supported_architectures_and_rejection(self):
        self.assertEqual(installer.platform_name("Linux", "x86_64"), "linux-x86_64")
        self.assertEqual(installer.platform_name("Linux", "arm64"), "linux-aarch64")
        self.assertEqual(installer.platform_name("Darwin", "arm64"), "macos-aarch64")
        self.assertEqual(installer.platform_name("Darwin", "x86_64"), "macos-x86_64")
        self.assertEqual(installer.mdk_platform_name("Darwin", "arm64"), "darwin-aarch64")
        with self.assertRaises(installer.InstallError):
            installer.platform_name("Windows", "x86_64")

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


class ManifestTests(unittest.TestCase):
    def test_manifest_declares_both_host_platforms(self):
        manifest = setup.tomllib.loads((SCRIPTS.parent / "herdr-plugin.toml").read_text())
        self.assertIn("linux", manifest["platforms"])
        self.assertIn("macos", manifest["platforms"])
        self.assertEqual(manifest["version"], setup.tomllib.loads((SCRIPTS.parent / "Cargo.toml").read_text())["package"]["version"])


class SetupTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.home = Path(self.temporary.name)
        self.environment = patch.dict(os.environ, {"HOME": str(self.home), "XDG_CONFIG_HOME": str(self.home / "config space %"), "XDG_STATE_HOME": str(self.home / "state"), "CODR_OWNER_NPUB": "", "CODR_SERVICE_MANAGER": "systemd"}, clear=True)
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
            local_accounts=lambda path: [],
            configure_phonon=lambda *args: ["/usr/bin/python3", "/example/transcribe.py", "--socket", "/example/phonon.sock"])

    def test_fresh_setup_generates_private_portable_config_and_preserves_on_repeat(self):
        with self.mocks():
            result = setup.configure("npub1fixture", self.socket, self.runner)
            self.assertTrue(self.config.exists())
            self.assertEqual(self.config.stat().st_mode & 0o777, 0o600)
            config = setup.tomllib.loads(self.config.read_text())
            self.assertEqual(config["phone_npub"], "npub1fixture")
            self.assertEqual(config["herdr_socket"], str(self.socket.resolve()))
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

    def test_enabling_phonon_on_an_existing_text_only_install_keeps_one_valid_command(self):
        with self.mocks():
            setup.configure("npub1fixture", self.socket, self.runner, enable_audio=False)
            setup.configure("npub1fixture", self.socket, self.runner, enable_audio=True)
        config = setup.tomllib.loads(self.config.read_text())
        self.assertEqual(config["phone_npub"], "npub1fixture")
        self.assertEqual(config["transcribe_command"][0], "/usr/bin/python3")
        self.assertEqual(self.bootstrap_count, 1)

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

    def test_codex_only_install_does_not_require_opencode(self):
        def find(name, candidates=()):
            if name in ("opencode", "claude"):
                raise setup.SetupError("Not installed")
            return "/usr/bin/" + name
        with self.mocks(), patch.object(setup, "find_executable", side_effect=find):
            setup.configure("npub1fixture", self.socket, self.runner)
        config = setup.tomllib.loads(self.config.read_text())
        self.assertEqual(config["opencode_bin"], "opencode")
        self.assertEqual(config["codex_home"], str((self.home / ".codex").resolve()))

    def test_claude_only_install_does_not_require_opencode_or_codex(self):
        def find(name, candidates=()):
            if name in ("opencode", "codex"):
                raise setup.SetupError("Not installed")
            return "/usr/bin/" + name
        with self.mocks(), patch.object(setup, "find_executable", side_effect=find):
            result = setup.configure("npub1fixture", self.socket, self.runner)
        config = setup.tomllib.loads(self.config.read_text())
        self.assertEqual(config["claude_home"], str((self.home / ".claude").resolve()))
        self.assertEqual(result["supported_agents"], ["claude"])

    def test_macos_setup_writes_launch_agents_and_uses_launchctl(self):
        agents = self.home / "LaunchAgents"
        with self.mocks(), patch.dict(os.environ, {"CODR_SERVICE_MANAGER": "launchd", "CODR_LAUNCH_AGENTS_DIR": str(agents)}), \
                patch.object(services, "_capture", return_value="state = running"):
            result = setup.configure("npub1fixture", self.socket, self.runner)
        self.assertEqual(result["bridge_npub"], "npub1bridge")
        self.assertTrue((agents / "io.github.tidley.codr.wn-agent-herdr.plist").exists())
        self.assertTrue((agents / "io.github.tidley.codr.herdr-white-noise-bridge.plist").exists())
        self.assertFalse((self.config.parents[1] / "systemd").exists())
        self.assertTrue(all("systemctl" not in " ".join(call) for call in self.calls))
        self.assertIn(["launchctl", "kickstart", f"gui/{os.getuid()}/io.github.tidley.codr.herdr-white-noise-bridge"], self.calls)
        plist = plistlib.loads((agents / "io.github.tidley.codr.herdr-white-noise-bridge.plist").read_bytes())
        self.assertIn("--config", plist["ProgramArguments"])
        self.assertEqual(plist["KeepAlive"], {"SuccessfulExit": False})

    def test_broken_unrelated_cli_does_not_block_an_observed_native_agent(self):
        def find(name, candidates=()):
            if name in ("codex", "claude"):
                raise setup.SetupError("Not on PATH")
            return "/usr/bin/" + name
        def runner(argv):
            if argv == ["/usr/bin/opencode", "--version"]:
                raise setup.SetupError("Broken unrelated installation")
            return self.runner(argv)
        with self.mocks(), patch.object(setup, "find_executable", side_effect=find), patch.object(setup,"read_snapshot", return_value={"agents":[{"agent":"claude","agent_session":{"agent":"claude"}}]}):
            result=setup.configure("npub1fixture", self.socket, runner)
        self.assertEqual(result["supported_agents"],["claude"])


if __name__ == "__main__":
    unittest.main()
