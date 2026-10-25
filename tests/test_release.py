import re
import tomllib
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent


class ReleasePreparationTests(unittest.TestCase):
    def test_package_plugin_lockfile_and_notes_agree(self):
        package = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]
        plugin = tomllib.loads((ROOT / "herdr-plugin.toml").read_text())
        packages = tomllib.loads((ROOT / "Cargo.lock").read_text())["package"]
        locked = next(item for item in packages if item["name"] == package["name"])
        self.assertEqual(package["version"], plugin["version"])
        self.assertEqual(package["version"], locked["version"])
        notes = (ROOT / "docs" / "releases" / f'{package["version"]}.md').read_text()
        self.assertEqual(notes.splitlines()[0], f'# codr {package["version"]}')
        self.assertIn("## Contributors", notes)

    def test_release_publishes_reviewed_notes_from_the_exact_source(self):
        workflow = (ROOT / ".github" / "workflows" / "build.yml").read_text()
        release = workflow.split("\n  release:\n", 1)[1]
        self.assertIn("actions/checkout@v4", release)
        self.assertIn("ref: ${{ github.sha }}", release)
        self.assertIn("--notes-file", release)
        self.assertNotIn("--generate-notes", release)
        self.assertIn("needs: binaries", release)
        self.assertIn("--verify-tag", release)

    def test_install_examples_select_this_release(self):
        version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
        for name in ["README.md", "docs/INSTALL.md"]:
            with self.subTest(path=name):
                refs = re.findall(r"herdr plugin install tidley/codr --ref v([0-9.]+)", (ROOT / name).read_text())
                self.assertTrue(refs)
                self.assertEqual(set(refs), {version})


if __name__ == "__main__":
    unittest.main()
