"""Smoke-test the packaged Stencilator executables on the target OS."""

import argparse
import hashlib
import json
import stat
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import cast
from zipfile import ZipFile

from .package_release import DOCUMENTS


def smoke(binary: Path, version: str, workspace: Path) -> None:
    reported = subprocess.check_output(
        [binary, "--version"], encoding="utf-8", timeout=15
    )
    assert reported.strip() == f"stencilator {version}", reported

    request: dict[str, object] = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientCapabilities": {},
            },
            "name": "check_stencil",
            "arguments": {"design": {"source": {"kind": "text", "text": "O"}}},
        },
    }
    completed = subprocess.run(
        [binary, "mcp", "--workspace", workspace],
        input=json.dumps(request) + "\n",
        capture_output=True,
        encoding="utf-8",
        timeout=20,
        check=False,
    )
    assert completed.returncode == 0, completed.stderr
    response = cast(dict[str, object], json.loads(completed.stdout))
    assert response["id"] == request["id"], response
    result = cast(dict[str, object], response["result"])
    assert result["isError"] is False, result
    content = cast(dict[str, object], result["structuredContent"])
    assert content["mode"] == "freeform", content
    assert not any(workspace.iterdir()), "Checking a stencil wrote to the workspace"


def check(bundle: Path) -> None:
    standalone = bundle.with_suffix(".zip")
    checksums = bundle.with_name(bundle.stem + "-sha256.txt")
    lines = checksums.read_text(encoding="ascii").splitlines()
    expected = {
        name: digest for digest, name in (line.split("  ", 1) for line in lines)
    }
    for archive in (standalone, bundle):
        with archive.open("rb") as source:
            assert (
                hashlib.file_digest(source, "sha256").hexdigest()
                == expected[archive.name]
            ), archive

    executable = "stencilator.exe" if sys.platform == "win32" else "stencilator"
    entry = f"server/{executable}"
    with ZipFile(standalone) as archive, ZipFile(bundle) as extension:
        manifest = cast(dict[str, object], json.loads(extension.read("manifest.json")))
        version = manifest["version"]
        assert isinstance(version, str)
        assert manifest["manifest_version"] == "0.3", manifest
        assert manifest["compatibility"] == {"platforms": [sys.platform]}, manifest
        assert manifest["server"] == {
            "type": "binary",
            "entry_point": entry,
            "mcp_config": {
                "command": f"${{__dirname}}/{entry}",
                "args": ["mcp", "--workspace", "${user_config.workspace}"],
            },
        }, manifest
        workspace_config = cast(dict[str, dict[str, object]], manifest["user_config"])[
            "workspace"
        ]
        assert (
            workspace_config["type"] == "directory"
            and workspace_config["required"] is True
        )
        assert archive.read(executable) == extension.read(entry)
        for document in (*DOCUMENTS, "THIRD-PARTY-LICENSES.txt"):
            assert extension.read(document) == archive.read(document) != b"", document

        with tempfile.TemporaryDirectory(prefix="Stencilator package ") as temporary:
            root = Path(temporary)
            workspace = root / "Artwork folder"
            workspace.mkdir()
            for package, member, folder in (
                (archive, executable, "standalone"),
                (extension, entry, "bundle"),
            ):
                mode = stat.S_IMODE(package.getinfo(member).external_attr >> 16)
                assert mode == 0o755, member
                binary = Path(package.extract(member, root / folder))
                if sys.platform != "win32":
                    binary.chmod(mode)
                smoke(binary, version, workspace)
    print(f"{bundle.stem}: both checksums, archives and MCP executables passed")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    _ = parser.add_argument(
        "bundle", type=Path, help="The MCP bundle beside its ZIP and checksums."
    )
    check(cast(Path, parser.parse_args().bundle))
