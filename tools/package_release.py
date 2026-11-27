"""Package a Stencilator binary as a ZIP and a Claude Desktop MCP bundle."""

import argparse
import hashlib
import json
import shutil
import stat
import tempfile
import tomllib
from pathlib import Path
from typing import cast
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo

ROOT = Path(__file__).resolve().parents[1]
TARGETS = {
    "x86_64-unknown-linux-gnu": ("linux", "stencilator"),
    "aarch64-apple-darwin": ("darwin", "stencilator"),
    "x86_64-pc-windows-msvc": ("win32", "stencilator.exe"),
    "x86_64-pc-windows-gnu": ("win32", "stencilator.exe"),
}
DOCUMENTS = (
    "README.md",
    "LICENSE",
    "docs/mcp.md",
    "docs/projects.md",
    "docs/svg-paints.md",
    "fonts/README.md",
    "fonts/OFL-NotoSans.txt",
    "fonts/OFL-AllertaStencil.txt",
)


def manifest(version: str, platform: str, entry_point: str) -> dict[str, object]:
    return {
        "manifest_version": "0.3",
        "name": "stencilator",
        "display_name": "Stencilator",
        "version": version,
        "description": "Check artwork and make 3D-printable painting stencils.",
        "author": {"name": "Lukasz Liniewicz"},
        "homepage": "https://stencilator.liniewicz.eu",
        "repository": {
            "type": "git",
            "url": "https://github.com/lukaszliniewicz/stencilator",
        },
        "license": "MIT",
        "compatibility": {"platforms": [platform]},
        "server": {
            "type": "binary",
            "entry_point": entry_point,
            "mcp_config": {
                "command": f"${{__dirname}}/{entry_point}",
                "args": ["mcp", "--workspace", "${user_config.workspace}"],
            },
        },
        "user_config": {
            "workspace": {
                "type": "directory",
                "title": "Stencilator workspace",
                "description": "Folder for the artwork, editable projects and exports.",
                "required": True,
            },
        },
        "tools": [
            {
                "name": "check_stencil",
                "description": "Inspect a design without writing files.",
            },
            {
                "name": "export_stencil",
                "description": "Export a new stencil, chart or project file.",
            },
        ],
    }


def archive_info(name: str, executable: bool = False) -> ZipInfo:
    info = ZipInfo(name)
    info.create_system = 3  # Record Unix permissions even when packaging on Windows.
    info.external_attr = (stat.S_IFREG | (0o755 if executable else 0o644)) << 16
    info.compress_type = ZIP_DEFLATED
    return info


def package(
    archive: Path,
    binary: Path,
    notices: bytes,
    entry_point: str,
    bundle_manifest: dict[str, object] | None,
) -> None:
    with ZipFile(archive, mode="x") as output:
        if bundle_manifest is not None:
            output.writestr(
                archive_info("manifest.json"),
                json.dumps(bundle_manifest, indent=2) + "\n",
            )
        with (
            binary.open("rb") as source,
            output.open(archive_info(entry_point, executable=True), "w") as target,
        ):
            shutil.copyfileobj(source, target)
        for name in DOCUMENTS:
            output.writestr(archive_info(name), (ROOT / name).read_bytes())
        output.writestr(archive_info("THIRD-PARTY-LICENSES.txt"), notices)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    _ = parser.add_argument("--target", choices=TARGETS, required=True)
    _ = parser.add_argument(
        "--binary", type=Path, required=True, help="The already built release binary."
    )
    _ = parser.add_argument(
        "--notices", type=Path, required=True, help="The cargo-about licence report."
    )
    _ = parser.add_argument("--output", type=Path, required=True)
    options = parser.parse_args()
    binary = cast(Path, options.binary)
    if not binary.is_file():
        parser.error(f"Binary not found: {binary}")
    notice_file = cast(Path, options.notices)
    if not notice_file.is_file() or notice_file.stat().st_size == 0:
        parser.error(f"Licence report not found or empty: {notice_file}")
    notices = notice_file.read_bytes()
    target = cast(str, options.target)
    platform, name = TARGETS[target]
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    version = cast(str, cargo["workspace"]["package"]["version"])
    stem = f"stencilator-{version}-{target}"
    directory = cast(Path, options.output)
    directory.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=directory) as temporary:
        staging = Path(temporary)
        archive = staging / f"{stem}.zip"
        bundle = staging / f"{stem}.mcpb"
        checksums = staging / f"{stem}-sha256.txt"
        entry_point = f"server/{name}"
        package(archive, binary, notices, name, None)
        package(
            bundle,
            binary,
            notices,
            entry_point,
            manifest(version, platform, entry_point),
        )
        with checksums.open("w", encoding="ascii", newline="\n") as sums:
            for product in (archive, bundle):
                with product.open("rb") as source:
                    digest = hashlib.file_digest(source, "sha256").hexdigest()
                _ = sums.write(f"{digest}  {product.name}\n")
        for product in (archive, bundle, checksums):
            print(product.replace(directory / product.name))


if __name__ == "__main__":
    main()
