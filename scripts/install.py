#!/usr/bin/env python3
"""Install the binary matching this plugin checkout's release version."""

import hashlib
import os
from pathlib import Path
import platform
import re
import shutil
import sys
import tarfile
import tempfile
import tomllib
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
MAX_DOWNLOAD = 256 * 1024 * 1024


class InstallError(Exception):
    pass


def platform_name(system=None, machine=None):
    system = system or platform.system()
    machine = machine or platform.machine()
    architectures = {"x86_64": "x86_64", "aarch64": "aarch64", "arm64": "aarch64"}
    if system != "Linux" or machine not in architectures:
        raise InstallError("Supported platforms: Linux x86_64 and aarch64")
    return "linux-" + architectures[machine]


def version():
    value = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", value):
        raise InstallError("The checkout does not select a numeric release")
    return value


def download(url, destination):
    try:
        request = urllib.request.Request(url, headers={"User-Agent": "codr-installer"})
        with urllib.request.urlopen(request, timeout=60) as response, destination.open("wb") as output:
            if not response.url.startswith("https://"):
                raise InstallError("Release download redirected outside HTTPS")
            size = 0
            while chunk := response.read(1024 * 1024):
                size += len(chunk)
                if size > MAX_DOWNLOAD:
                    raise InstallError("Release download exceeds 256 MiB")
                output.write(chunk)
    except (OSError, ValueError) as error:
        raise InstallError(f"Could not download release asset: {url}") from error


def install_bundle(url, member, destination):
    """Verify the archive, then install only its expected regular executable."""
    with tempfile.TemporaryDirectory(prefix="codr-download-") as temporary:
        archive = Path(temporary) / "binary.tar.gz"
        checksum = Path(temporary) / "checksum"
        download(url, archive)
        download(url + ".sha256", checksum)
        fields = checksum.read_text().split()
        if not fields or not re.fullmatch(r"[a-fA-F0-9]{64}", fields[0]):
            raise InstallError("Invalid release checksum")
        with archive.open("rb") as source:
            actual = hashlib.file_digest(source, "sha256").hexdigest()
        if actual != fields[0].lower():
            raise InstallError("Release checksum mismatch; nothing was installed")
        with tarfile.open(archive, "r:gz") as bundle:
            entries = [entry for entry in bundle.getmembers() if entry.name == member]
            if len(entries) != 1 or not entries[0].isfile() or not 0 < entries[0].size <= MAX_DOWNLOAD:
                raise InstallError("Release lacks the expected regular executable")
            destination.parent.mkdir(parents=True, exist_ok=True)
            with tempfile.NamedTemporaryFile(dir=destination.parent, delete=False) as output:
                temporary_binary = Path(output.name)
                try:
                    with bundle.extractfile(entries[0]) as source:
                        shutil.copyfileobj(source, output)
                    output.flush()
                    os.fsync(output.fileno())
                    os.chmod(temporary_binary, 0o755)
                    os.replace(temporary_binary, destination)
                finally:
                    temporary_binary.unlink(missing_ok=True)


def main():
    try:
        release = version()
        asset = f"codr-v{release}-{platform_name()}.tar.gz"
        url = f"https://github.com/tidley/codr/releases/download/v{release}/{asset}"
        destination = Path.home() / ".local/bin/herdr-white-noise-bridge"
        install_bundle(url, "herdr-white-noise-bridge", destination)
        print(f"Installed codr {release}: {destination}")
        print("Next: run the plugin's Setup action, or follow docs/INSTALL.md for non-interactive setup.")
        print("For an existing installation, restart only the codr worker when it is safe to apply this binary update.")
    except (InstallError, OSError, tarfile.TarError) as error:
        print(f"codr install: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
