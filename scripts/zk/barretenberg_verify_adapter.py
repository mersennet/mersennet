#!/usr/bin/env python3

from __future__ import annotations

import argparse
import os
import tempfile
from pathlib import Path

from common import render_template_command, run_command


def package_name(circuit: str) -> str:
    return f"mersennet_{circuit}_circuit"


def find_acir_artifact(artifacts_dir: Path, circuit: str) -> Path:
    explicit = os.environ.get("MERSENNET_BB_ACIR_PATH")
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
    explicit = os.environ.get("MERSENNET_BB_VK_PATH")
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
    bb_bin = os.environ.get("MERSENNET_BB_BIN", "bb")
    values = {
        "bb": bb_bin,
        "acir": acir_path,
        "vk": vk_path,
        "artifacts": artifacts_dir,
        "circuit": circuit,
    }
    template = os.environ.get("MERSENNET_BB_WRITE_VK_TEMPLATE")
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
    parser = argparse.ArgumentParser(description="Reference adapter for MERSENNET_BB_VERIFY_ADAPTER.")
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

        # Normalize the public inputs into Barretenberg's canonical
        # binary layout: 32-byte big-endian field elements concatenated.
        # The caller may hand us either that binary form already, or a
        # text file of one big-endian hex field per line (the encoding
        # `BarretenbergVerifier` writes). We detect which and convert.
        raw = public_inputs_path.read_bytes()
        public_inputs_bin = tmp_dir / "public_inputs.bin"
        fields_hex: list[str] = []
        try:
            text = raw.decode("utf-8")
            lines = [line.strip() for line in text.splitlines() if line.strip()]
            looks_hex = bool(lines) and all(
                all(ch in "0123456789abcdefABCDEFx" for ch in line) for line in lines
            )
        except UnicodeDecodeError:
            looks_hex = False
            lines = []

        if looks_hex:
            field_bytes = bytearray()
            for line in lines:
                token = line[2:] if line[:2].lower() == "0x" else line
                value = bytes.fromhex(token.rjust(64, "0"))
                if len(value) != 32:
                    raise SystemExit(f"public input field is not 32 bytes: {line}")
                field_bytes.extend(value)
                fields_hex.append(token.rjust(64, "0"))
            public_inputs_bin.write_bytes(bytes(field_bytes))
        else:
            if len(raw) % 32 != 0:
                raise SystemExit("binary public inputs length is not a multiple of 32 bytes")
            public_inputs_bin.write_bytes(raw)
            for offset in range(0, len(raw), 32):
                fields_hex.append(raw[offset : offset + 32].hex())

        public_inputs_json = tmp_dir / "public_inputs.json"
        public_inputs_json.write_text(
            "[\n" + ",\n".join(f'  "0x{value}"' for value in fields_hex) + "\n]\n",
            encoding="utf-8",
        )

        bb_bin = os.environ.get("MERSENNET_BB_BIN", "bb")
        values = {
            "bb": bb_bin,
            "circuit": args.circuit,
            "artifacts": artifacts_dir,
            "acir": acir_path,
            "vk": vk_path,
            "proof": proof_path,
            "public_inputs": public_inputs_bin,
            "public_inputs_json": public_inputs_json,
        }

        template = os.environ.get("MERSENNET_BB_VERIFY_TEMPLATE")
        if template:
            run_command(render_template_command(template, values))
            return 0

        attempts = [
            [bb_bin, "verify", "-k", str(vk_path), "-p", str(proof_path), "-i", str(public_inputs_bin)],
            [bb_bin, "verify", "--vk", str(vk_path), "--proof", str(proof_path), "--public-inputs", str(public_inputs_bin)],
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