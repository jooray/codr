#!/usr/bin/env python3
"""Apply the published connector patch only to its pinned, clean upstream source."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def prepare(source):
    settings = tomllib.loads((ROOT / "connector.toml").read_text())
    source = Path(source).resolve()
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip()
    if revision != settings["revision"]:
        raise RuntimeError("Connector source does not match the pinned revision")
    if subprocess.check_output(["git", "status", "--porcelain"], cwd=source, text=True).strip():
        raise RuntimeError("Connector source has local changes; nothing was replaced")
    patch = ROOT / settings["patch"]
    subprocess.run(["git", "apply", "--check", str(patch)], cwd=source, check=True)
    subprocess.run(["git", "apply", str(patch)], cwd=source, check=True)
    release = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    provenance = dict(codr_version=release, repository=settings["repository"], revision=revision,
                      upstream_version=settings["version"], patch_sha256=hashlib.sha256(patch.read_bytes()).hexdigest())
    (source / "CODR-CONNECTOR.json").write_text(json.dumps(provenance, indent=2) + "\n")
    return provenance


if __name__ == "__main__":
    print(json.dumps(prepare(sys.argv[1])))
