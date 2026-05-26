#!/usr/bin/env python3

from __future__ import annotations

import json
import os
import shlex
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Iterable


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def split_command(template: str) -> list[str]:
    return shlex.split(template, posix=os.name != "nt")


def render_template_command(template: str, values: dict[str, object]) -> list[str]:
    rendered_values = {key: str(value) for key, value in values.items()}
    return [part.format(**rendered_values) for part in split_command(template)]


def host_executor() -> str:
    executor = os.environ.get("PRIME_SP1_HOST_EXECUTOR", "native").strip().lower()
    if executor not in {"native", "wsl"}:
        raise RuntimeError(
            "PRIME_SP1_HOST_EXECUTOR must be one of: native, wsl"
        )
    return executor


def to_wsl_path(path: Path | str) -> str:
    raw = str(path)
    normalized = raw.replace("\\", "/")
    if len(normalized) >= 3 and normalized[1:3] == ":/":
        drive = normalized[0].lower()
        return f"/mnt/{drive}/{normalized[3:]}"
    return normalized


def with_wsl_paths(values: dict[str, object], path_keys: Iterable[str]) -> dict[str, object]:
    rendered = dict(values)
    for key in path_keys:
        value = rendered.get(key)
        if value:
            rendered[key] = to_wsl_path(value)
    return rendered


def build_wsl_command(command: Iterable[str], cwd: Path) -> list[str]:
    wsl = shutil.which("wsl.exe") or shutil.which("wsl")
    if not wsl:
        raise RuntimeError(
            "PRIME_SP1_HOST_EXECUTOR=wsl requires wsl.exe to be installed and available on PATH"
        )

    distro = os.environ.get("PRIME_SP1_WSL_DISTRO", "").strip()
    script = f"cd {shlex.quote(to_wsl_path(cwd))} && {shlex.join(list(command))}"

    wrapped = [wsl]
    if distro:
        wrapped.extend(["-d", distro])
    wrapped.extend(["bash", "-lc", script])
    return wrapped


def run_command(
    command: Iterable[str],
    cwd: Path | None = None,
    *,
    executor: str = "native",
) -> subprocess.CompletedProcess[str]:
    command_list = list(command)
    working_dir = cwd or repo_root()

    if executor == "wsl":
        command_list = build_wsl_command(command_list, working_dir)
        working_dir = None

    result = subprocess.run(
        command_list,
        cwd=str(working_dir) if working_dir else None,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        cmd = " ".join(result.args)
        stderr = result.stderr.strip()
        stdout = result.stdout.strip()
        message = stderr or stdout or f"command exited with {result.returncode}"
        raise RuntimeError(f"command failed: {cmd}\n{message}")
    return result


def read_json(path: Path) -> dict[str, object]:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def write_json(path: Path, payload: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2, sort_keys=True)
        handle.write("\n")


def fail(message: str) -> "NoReturn":
    print(message, file=sys.stderr)
    raise SystemExit(1)