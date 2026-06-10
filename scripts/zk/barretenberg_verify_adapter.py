#!/usr/bin/env python3

from __future__ import annotations

import argparse
import os
import tempfile
from pathlib import Path

from common import render_template_command, run_command


def package_name(circuit: str) -> str:
    return f"prime_{circuit}_circuit"


def find_acir_artifact(artifacts_dir: Path, circuit: str) -> Path:
    explicit = os.environ.get("PRIME_BB_ACIR_PATH")
    if explicit:
        path = Path(explicit).resolve()
        if path.exists():
            return path

    candidates = [
        artifacts_dir / f"{package_name(circuit)}.json",
        artifacts_dir / f"{circuit}.json",
        artifacts_dir / "program.json",
    ]
    for candidate in candidates:
        if candidate.exists():
            return candidate

    all_json = sorted(path for path in artifacts_dir.glob("*.json") if path.name != "vk.hash")
    if all_json:
        return all_json[0]
    raise SystemExit(f"could not find an ACIR json artifact under {artifacts_dir}")


def find_or_create_vk(circuit: str, artifacts_dir: Path, acir_path: Path) -> Path:
    explicit = os.environ.get("PRIME_BB_VK_PATH")
    if explicit:
        path = Path(explicit).resolve()
        if path.exists():
            return path

    candidates = [
        artifacts_dir / f"{package_name(circuit)}.vk",
        artifacts_dir / f"{circuit}.vk",
        artifacts_dir / "vk",
        artifacts_dir / "vk.bin",
    ]
    for candidate in candidates:
        if candidate.exists():
            return candidate

    vk_path = artifacts_dir / f"{package_name(circuit)}.vk"
    bb_bin = os.environ.get("PRIME_BB_BIN", "bb")
    values = {
        "bb": bb_bin,
        "acir": acir_path,
        "vk": vk_path,
        "artifacts": artifacts_dir,
        "circuit": circuit,
    }
    template = os.environ.get("PRIME_BB_WRITE_VK_TEMPLATE")
    if template:
        run_command(render_template_command(template, values))
        if vk_path.exists():
            return vk_path
        raise SystemExit(f"vk template completed without producing {vk_path}")

    attempts = [
        [bb_bin, "write_vk", "-b", str(acir_path), "-o", str(vk_path)],
        [bb_bin, "write_vk", "--bytecode", str(acir_path), "--output", str(vk_path)],
    ]
    errors: list[str] = []
    for command in attempts:
        try:
            run_command(command)
            if vk_path.exists():
                return vk_path
        except RuntimeError as exc:
            errors.append(str(exc))
    raise SystemExit("failed to generate verifying key:\n" + "\n\n".join(errors))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Reference adapter for PRIME_BB_VERIFY_ADAPTER.")
    parser.add_argument("--circuit", required=True)
    parser.add_argument("--artifacts", required=True)
    parser.add_argument("--proof", required=True)
    parser.add_argument("--public-inputs", required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    artifacts_dir = Path(args.artifacts).resolve()
    proof_path = Path(args.proof).resolve()
    public_inputs_path = Path(args.public_inputs).resolve()
    acir_path = find_acir_artifact(artifacts_dir, args.circuit)
    vk_path = find_or_create_vk(args.circuit, artifacts_dir, acir_path)

    if not proof_path.exists():
        raise SystemExit(f"missing proof file: {proof_path}")
    if not public_inputs_path.exists():
        raise SystemExit(f"missing public inputs file: {public_inputs_path}")

    with tempfile.TemporaryDirectory(prefix="mersennet-bb-") as tmp_name:
        tmp_dir = Path(tmp_name)
        public_inputs_json = tmp_dir / "public_inputs.json"
        public_inputs = [line.strip() for line in public_inputs_path.read_text(encoding="utf-8").splitlines() if line.strip()]
        public_inputs_json.write_text("[\n" + ",\n".join(f'  "{value}"' for value in public_inputs) + "\n]\n", encoding="utf-8")

        bb_bin = os.environ.get("PRIME_BB_BIN", "bb")
        values = {
            "bb": bb_bin,
            "circuit": args.circuit,
            "artifacts": artifacts_dir,
            "acir": acir_path,
            "vk": vk_path,
            "proof": proof_path,
            "public_inputs": public_inputs_path,
            "public_inputs_json": public_inputs_json,
        }

        template = os.environ.get("PRIME_BB_VERIFY_TEMPLATE")
        if template:
            run_command(render_template_command(template, values))
            return 0

        attempts = [
            [bb_bin, "verify", "-k", str(vk_path), "-p", str(proof_path), "-i", str(public_inputs_path)],
            [bb_bin, "verify", "--vk", str(vk_path), "--proof", str(proof_path), "--public-inputs", str(public_inputs_path)],
            [bb_bin, "verify", "--vk", str(vk_path), "--proof", str(proof_path), "--public-inputs", str(public_inputs_json)],
            [bb_bin, "verify", "-k", str(vk_path), "-p", str(proof_path)],
            [bb_bin, "verify", "--vk", str(vk_path), "--proof", str(proof_path)],
        ]
        errors: list[str] = []
        for command in attempts:
            try:
                run_command(command)
                return 0
            except RuntimeError as exc:
                errors.append(str(exc))

    raise SystemExit("failed to verify proof with barretenberg:\n" + "\n\n".join(errors))


if __name__ == "__main__":
    raise SystemExit(main())