#!/usr/bin/env python3
"""Record a native ARM64 release build without modifying its application sources."""
import hashlib
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile


def output(*command):
    return subprocess.check_output(command, text=True, env={**os.environ, "LC_ALL": "C"}).strip()


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    source, artifacts = (Path(value).resolve(strict=True) for value in sys.argv[1:])
    tools_root = Path(__file__).resolve().parents[1]
    tag = os.environ["RELEASE_TAG"]
    version = os.environ["SIDERA_VERSION"]
    if not re.fullmatch(r"v3\.0-Electro-testing-custom[0-9]{8}\.[0-9]+", tag):
        raise ValueError("Invalid release tag")
    if version != tag[1:].replace("-custom", "+custom"):
        raise ValueError("Package version does not match release tag")
    commit = output("git", "-C", str(source), "rev-parse", "HEAD")
    if commit != output("git", "-C", str(source), "rev-parse", f"refs/tags/{tag}^{{commit}}"):
        raise ValueError("Application checkout does not match release tag")
    support_commit = output("git", "-C", str(tools_root), "rev-parse", "HEAD")
    package = artifacts / f"sidera_{version}_arm64.deb"
    if output("dpkg-deb", "-f", str(package), "Architecture") != "arm64":
        raise ValueError("Package is not ARM64")
    if output("dpkg-deb", "-f", str(package), "Version") != version:
        raise ValueError("Package version mismatch")
    binary = source / "rust/target/release/sidera"
    header = output("readelf", "-h", str(binary))
    if not re.search(r"Machine:\s+AArch64\s*$", header, re.MULTILINE):
        raise ValueError("Package binary is not AArch64")
    dependencies = output("dpkg-deb", "-f", str(package), "Depends")
    tests = (artifacts / "STARTUP-TESTS-arm64.txt").read_text(encoding="utf-8")
    passed = re.search(r"Ran (\d+) tests in [\d.]+s\s+OK\s*$", tests)
    if not passed or int(passed[1]) < 1:
        raise ValueError("Successful native startup test report is missing")

    # Include the packaging/verification sources alongside the already-published
    # tagged application + vendored dependency archive. No executable is changed.
    with tarfile.open(artifacts / "BUILD-SCRIPTS-arm64.tar.gz", "w:gz") as archive:
        for name in ("rust/build_deb.sh", "tools/deb_dependencies.py",
                     "tools/run_isolated.py", "tools/release_arm64_metadata.py",
                     ".github/scripts/prepare-isolation.sh", ".github/workflows/release-arm64.yml"):
            archive.add(tools_root / name, arcname=name)
    report = f"""# ARM64 build supplement for {tag}

This supplements the existing release; the application sources are unchanged.
The original amd64 asset and its BUILDINFO.md remain unchanged.

| Item | Value |
| --- | --- |
| Application source | `{commit}` |
| Packaging / verification source | `{support_commit}` |
| Architecture | Native ARM64 / AArch64 (no x86 relabeling or emulation) |
| Build OS | {output('lsb_release', '-ds')} |
| System libc | {output('getconf', 'GNU_LIBC_VERSION')} |
| Rust | {output('rustc', '+' + os.environ['RUST_VERSION'], '--version')} |
| Cargo | {output('cargo', '+' + os.environ['RUST_VERSION'], '--version')} |
| Package version | `{version}` |
| Actual DEB dependencies | `{dependencies}` |
| Cargo.lock SHA256 | `{sha256(source / 'rust/Cargo.lock')}` |
| DEB SHA256 | `{sha256(package)}` |
| Executable SHA256 | `{sha256(binary)}` |
| Native isolated startup tests | {passed[1]} passed; report in STARTUP-TESTS-arm64.txt |
| Workflow | https://github.com/{os.environ['GITHUB_REPOSITORY']}/actions/runs/{os.environ['GITHUB_RUN_ID']} |

## Rebuild

Use an ARM64 Ubuntu 22.04 build environment with Rust 1.93.1 and the dependencies
listed in the workflow. Check out the application source commit above; the
existing release source-with-dependencies archive contains the same Rust sources
and locked dependency sources. Run `cargo +1.93.1 build --release --locked` in
`rust/` (add `--offline` when using the vendored source archive).

The archived `rust/build_deb.sh` and `tools/deb_dependencies.py` replace only the
old packaging helpers. They derive dependencies from the actual ELF with
dpkg-shlibdeps. Copy them into the corresponding paths of the source checkout,
then run `SIDERA_VERSION={version} bash rust/build_deb.sh arm64` from its root.
See the archived workflow for exact build, package extraction and test commands.
These records describe provenance; byte-for-byte reproducibility is not claimed.

## Verification and limits

The binary was compiled and executed on a native ARM64 runner. The package's ELF
architecture/version and extracted binary match the build; dependency resolution
was simulated by APT. Startup checks use a private home, network and Xvfb/Wayland
protocol fixtures. No real classroom desktop, touchscreen or WPS was exercised.
An ARM64 package does not by itself establish compatibility with every Kylin
release or GNOME version. Match both architecture and the listed dependencies.
GNOME extension metadata still applies. WPS and system graphics fixes are not
bundled. Later custom branch features are not backported into this release.

The project remains GPL v3. Dependency notices are in THIRD_PARTY_NOTICES.md;
the accompanying source archive is still applicable to this application build.
SHA256SUMS-arm64 is an unsigned integrity manifest for this supplement.
"""
    (artifacts / "BUILDINFO-arm64.md").write_text(report, encoding="utf-8")
    files = sorted(path for path in artifacts.iterdir() if path.is_file() and path.name != "SHA256SUMS-arm64")
    (artifacts / "SHA256SUMS-arm64").write_text(
        "".join(f"{sha256(path)}  {path.name}\n" for path in files), encoding="utf-8")
    print(report)


if __name__ == "__main__":
    main()
