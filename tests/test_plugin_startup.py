import json
import os
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[1]


class PluginStartupTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.home = Path(self.temporary.name)
        self.bin = self.home / "bin"
        self.bin.mkdir()
        self.log = self.home / "commands.jsonl"
        self.herdr = self.bin / "herdr fixture"
        self.fake(self.bin / "systemctl", "WORKER_FAIL")
        self.fake(self.herdr, "MENU_FAIL")
        self.environment = dict(os.environ, HOME=str(self.home),
                                PATH=str(self.bin) + os.pathsep + os.environ.get("PATH", ""),
                                HERDR_BIN_PATH=str(self.herdr), STARTUP_LOG=str(self.log),
                                XDG_RUNTIME_DIR=str(self.home / "runtime"),
                                DBUS_SESSION_BUS_ADDRESS="unix:path=fixture")

    def fake(self, path, failure_flag):
        path.write_text(
            "#!/usr/bin/env python3\nimport json,os,sys\n"
            "from pathlib import Path\n"
            "with open(os.environ['STARTUP_LOG'],'a') as log:\n"
            "    log.write(json.dumps([Path(sys.argv[0]).name,*sys.argv[1:]])+'\\n')\n"
            f"sys.exit(int(os.environ.get('{failure_flag}','0')))\n"
        )
        path.chmod(0o700)

    def run_start(self, *args, **environment):
        result = subprocess.run(["sh", str(ROOT / "plugin-start.sh"), *args],
                                env=dict(self.environment, **environment),
                                capture_output=True, text=True, timeout=10)
        calls = [json.loads(line) for line in self.log.read_text().splitlines()]
        return result, calls

    def test_startup_opens_the_forwarding_popup_after_starting_the_worker(self):
        manifest = tomllib.loads((ROOT / "herdr-plugin.toml").read_text())
        startup = next(hook for hook in manifest["startup"] if hook["command"][:2] == ["sh", "plugin-start.sh"])
        result, calls = self.run_start(*startup["command"][2:])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls, [
            ["systemctl", "--user", "start", "herdr-white-noise-bridge.service"],
            ["herdr fixture", "plugin", "pane", "open", "--plugin", "local.white-noise", "--entrypoint", "forwarding", "--focus"],
        ])

    def test_manual_worker_start_does_not_open_another_popup(self):
        result, calls = self.run_start()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0][0], "systemctl")

    def test_unavailable_popup_does_not_fail_a_successful_worker_start(self):
        result, calls = self.run_start("--show-menu", MENU_FAIL="1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(calls), 2)
        self.assertIn("forwarding", result.stderr)

    def test_worker_failure_is_reported_without_opening_the_popup(self):
        result, calls = self.run_start("--show-menu", WORKER_FAIL="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(len(calls), 1)


if __name__ == "__main__":
    unittest.main()
