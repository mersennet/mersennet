#!/usr/bin/env python3

from __future__ import annotations

import argparse
import os
import shutil
from pathlib import Path

from common import render_template_command, run_command


def package_name(circuit: str) -> str:
    return f"mersennet_{circuit}_circuit"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Reference adapter for MERSENNET_BB_PROVE_ADAPTER.")
    parser.add_argument("--circuit", required=True)
    parser.add_argument("--package-dir", required=True)
    parser.add_argument("--artifacts", required=True)
    parser.add_argument("--witness", required=True)
    parser.add_argument("--proof", required=True)
    return parser.parse_args()


def find_generated_proof(package_dir: Path, circuit: str) -> Path:
    package = package_name(circuit)
    candidates = [
        package_dir / "target" / "proofs" / f"{package}.proof",
        package_dir / "proofs" / f"{package}.proof",
        package_dir / "target" / f"{package}.proof",
        package_dir / "target" / "proof.proof",
        package_dir / "target" / "proof.bin",
        package_dir / "proof.bin",
    ]
    for candidate in candidates:
        if candidate.exists():
            return candidate

    proofs = sorted(package_dir.rglob("*.proof"))
    if proofs:
        return proofs[0]
    bins = sorted(package_dir.rglob("proof*.bin"))
    if bins:
        return bins[0]
    raise SystemExit(f"could not find a generated proof under {package_dir}")


def main() -> int:
    args = parse_args()
    package_dir = Path(args.package_dir).resolve()
    artifacts_dir = Path(args.artifacts).resolve()
    witness_path = Path(args.witness).resolve()
    proof_path = Path(args.proof).resolve()

    if not witness_path.exists():
        raise SystemExit(f"missing witness file: {witness_path}")

    package_dir.mkdir(parents=True, exist_ok=True)
    destination_witness = package_dir / "Prover.toml"
    if witness_path != destination_witness:
        shutil.copyfile(witness_path, destination_witness)

    nargo_bin = os.environ.get("MERSENNET_NARGO_BIN", "nargo")
    values = {
        "nargo": nargo_bin,
        "package": package_name(args.circuit),
        "circuit": args.circuit,
        "package_dir": package_dir,
        "artifacts": artifacts_dir,
        "witness": destination_witness,
        "proof": proof_path,
    }

    template = os.environ.get("MERSENNET_BB_PROVE_TEMPLATE") or os.environ.get("MERSENNET_NARGO_PROVE_TEMPLATE")
    if template:
        run_command(render_template_command(template, values), cwd=package_dir)
    else:
        attempts = [
            [nargo_bin, "prove"],
            [nargo_bin, "prove", package_name(args.circuit)],
            [nargo_bin, "prove", "--package", package_name(args.circuit)],
        ]
        errors: list[str] = []
        for command in attempts:
            try:
                run_command(command, cwd=package_dir)
                break
            except RuntimeError as exc:
                errors.append(str(exc))
        else:
            raise SystemExit("failed to generate proof with nargo / barretenberg:\n" + "\n\n".join(errors))

    proof_path.parent.mkdir(parents=True, exist_ok=True)
    generated_proof = proof_path if proof_path.exists() else find_generated_proof(package_dir, args.circuit)
    if generated_proof != proof_path:
        shutil.copyfile(generated_proof, proof_path)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())