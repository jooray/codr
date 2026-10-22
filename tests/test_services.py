import os
from pathlib import Path
import plistlib
import sys
import tempfile
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
sys.path.insert(0, str(SCRIPTS))
import services


class LaunchdTests(unittest.TestCase):
    def manager(self):
        def write(path, text):
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
            path.chmod(0o600)
        return services.LaunchdServices(lambda argv: "", write, "/unused")

    def test_plist_restarts_failures_and_supplies_a_service_path(self):
        payload = self.manager().payload(services.Service(
            "herdr-white-noise-bridge", "worker", ["/bin/echo", "--config", "/tmp/config.toml"],
            environment={"CODR_TEST": "1"}))
        self.assertEqual(payload["Label"], "io.github.tidley.codr.herdr-white-noise-bridge")
        self.assertEqual(payload["ProgramArguments"], ["/bin/echo", "--config", "/tmp/config.toml"])
        self.assertTrue(payload["RunAtLoad"])
        self.assertEqual(payload["KeepAlive"], {"SuccessfulExit": False})
        self.assertIn("/opt/homebrew/bin", payload["EnvironmentVariables"]["PATH"])
        self.assertEqual(payload["EnvironmentVariables"]["CODR_TEST"], "1")
        self.assertNotIn("--unix-socket", " ".join(payload["ProgramArguments"]))

    def test_written_plist_is_valid_and_private(self):
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {"CODR_LAUNCH_AGENTS_DIR": temporary}):
            manager = self.manager()
            target = manager.install(services.Service("codr-phonon", "speech", ["/bin/echo", "serve"]))
            self.assertEqual(target.stat().st_mode & 0o777, 0o600)
            parsed = plistlib.loads(target.read_bytes())
            self.assertEqual(parsed["Label"], "io.github.tidley.codr.codr-phonon")

    def test_changed_plist_is_not_overwritten(self):
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {"CODR_LAUNCH_AGENTS_DIR": temporary}):
            manager = self.manager()
            target = manager.path(services.Service("codr-phonon", "speech", ["/bin/echo"]))
            manager.install(services.Service("codr-phonon", "speech", ["/bin/echo"]))
            with self.assertRaises(services.ServiceError):
                manager.install(services.Service("codr-phonon", "speech", ["/bin/echo", "changed"]))

    def test_plugin_start_uses_the_same_label(self):
        text = (SCRIPTS.parent / "plugin-start.sh").read_text()
        self.assertIn(services.LABEL_PREFIX + ".herdr-white-noise-bridge", text)

    def test_runtime_directory_can_be_redirected(self):
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {"XDG_RUNTIME_DIR": temporary}):
            self.assertEqual(services.runtime_dir(), Path(temporary))
            self.assertEqual(services.runtime_dir().stat().st_mode & 0o777, 0o700)

    def test_a_long_socket_path_is_rejected_on_macos(self):
        with patch.object(services.sys, "platform", "darwin"):
            with self.assertRaises(ValueError):
                services.check_socket_path("/" + "a" * 120)

    def test_for_host_honours_the_override(self):
        with patch.dict(os.environ, {"CODR_SERVICE_MANAGER": "launchd"}):
            self.assertIsInstance(services.for_host(lambda argv: "", lambda path, text: None, "/tmp"), services.LaunchdServices)
        with patch.dict(os.environ, {"CODR_SERVICE_MANAGER": "systemd"}):
            self.assertIsInstance(services.for_host(lambda argv: "", lambda path, text: None, "/tmp"), services.SystemdServices)


class SystemdTests(unittest.TestCase):
    def test_bounded_python_selection_skips_313_for_the_intel_mac_runtime(self):
        versions = {"/python3.13": (3, 13), "/python3.10": (3, 10)}
        with patch.dict(os.environ, {"CODR_PYTHON": ""}), patch.object(services.shutil, "which", side_effect=lambda name: {"python3.13": "/python3.13", "python3.10": "/python3.10"}.get(name)), patch.object(services, "python_version", side_effect=lambda path: versions.get(path)):
            self.assertEqual(services.find_python((3, 10), preferred=("python3.13", "python3.10"), maximum=(3, 12)), "/python3.10")

    def test_directives_are_replaced_once(self):
        text = services._replace_directive("A=1\nExecStart=old\nB=2\n", "ExecStart=", "ExecStart=new")
        self.assertEqual(text, "A=1\nExecStart=new\nB=2\n")
        with self.assertRaises(services.ServiceError):
            services._replace_directive("A=1\n", "ExecStart=", "ExecStart=new")

    def test_arguments_are_escaped_for_systemd(self):
        self.assertEqual(services.systemd_argument("/config/${PROFILE}%"), '"/config/$${PROFILE}%%"')


if __name__ == "__main__":
    unittest.main()
