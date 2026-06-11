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


LOCAL_REAL_SP1_ENV_DEFAULTS = {
    "MERSENNET_SP1_PROOF_SYSTEM": "core",
    "MERSENNET_SP1_INLINE_VERIFY": "0",
    "MERSENNET_SP1_DEFERRED_PROOF_VERIFICATION": "0",
    "RAYON_NUM_THREADS": "2",
    "SP1_WORKER_NUM_CORE_WORKERS": "2",
    "SP1_WORKER_NUM_SETUP_WORKERS": "2",
    "SP1_WORKER_NUM_PREPARE_REDUCE_WORKERS": "1",
    "SP1_WORKER_NUM_RECURSION_EXECUTOR_WORKERS": "1",
    "SP1_WORKER_NUM_RECURSION_PROVER_WORKERS": "1",
    "SP1_WORKER_NUM_DEFERRED_WORKERS": "1",
    "SP1_WORKER_NUM_SPLICING_WORKERS": "1",
}


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def split_command(template: str) -> list[str]:
    return shlex.split(template, posix=os.name != "nt")


def render_template_command(template: str, values: dict[str, object]) -> list[str]:
    rendered_values = {key: str(value) for key, value in values.items()}
    return [part.format(**rendered_values) for part in split_command(template)]


def host_executor() -> str:
    executor = os.environ.get("MERSENNET_SP1_HOST_EXECUTOR", "native").strip().lower()
    if executor not in {"native", "wsl"}:
        raise RuntimeError(
            "MERSENNET_SP1_HOST_EXECUTOR must be one of: native, wsl"
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


def build_wsl_command(command: Iterable[str], cwd: Path, env_overrides: dict[str, str] | None = None) -> list[str]:
    wsl = shutil.which("wsl.exe") or shutil.which("wsl")
    if not wsl:
        raise RuntimeError(
            "MERSENNET_SP1_HOST_EXECUTOR=wsl requires wsl.exe to be installed and available on PATH"
        )

    distro = os.environ.get("MERSENNET_SP1_WSL_DISTRO", "").strip()
    exports = ""
    if env_overrides:
        exports = " ".join(
            f"{name}={shlex.quote(value)}" for name, value in env_overrides.items()
        ) + " "
    script = f"cd {shlex.quote(to_wsl_path(cwd))} && {exports}{shlex.join(list(command))}"

    wrapped = [wsl]
    if distro:
        wrapped.extend(["-d", distro])
    wrapped.extend(["bash", "-lc", script])
    return wrapped


def normalize_real_sp1_command(command: Iterable[str]) -> list[str]:
    command_list = list(command)
    if len(command_list) < 2:
        return command_list

    if command_list[0] != "cargo" or command_list[1] != "run":
        return command_list

    if "real-sp1" not in command_list:
        return command_list

    if "--release" in command_list or "--profile" in command_list:
        return command_list

    return [command_list[0], command_list[1], "--release", *command_list[2:]]


def local_real_sp1_env_defaults(command: Iterable[str]) -> dict[str, str]:
    command_list = list(command)
    if "real-sp1" not in command_list:
        return {}

    if "--prove-request" not in command_list:
        return {}

    if os.environ.get("MERSENNET_SP1_MODE", "local").strip().lower() != "local":
        return {}

    return {
        name: value
        for name, value in LOCAL_REAL_SP1_ENV_DEFAULTS.items()
        if not os.environ.get(name, "").strip()
    }


def run_command(
    command: Iterable[str],
    cwd: Path | None = None,
    *,
    executor: str = "native",
) -> subprocess.CompletedProcess[str]:
    command_list = normalize_real_sp1_command(command)
    env_overrides = local_real_sp1_env_defaults(command_list)
    working_dir = cwd or repo_root()
    env = os.environ.copy()
    env.update(env_overrides)

    if executor == "wsl":
        command_list = build_wsl_command(command_list, working_dir, env_overrides)
        working_dir = None

    result = subprocess.run(
        command_list,
        cwd=str(working_dir) if working_dir else None,
        env=env,
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