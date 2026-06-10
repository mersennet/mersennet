#!/usr/bin/env python3

from __future__ import annotations

import argparse
import os
import shutil
import tempfile
from pathlib import Path

from common import repo_root, run_command


CIRCUITS: tuple[str, ...] = (
    "spend",
    "output",
    "join_split",
    "order_place",
    "liquidate_claim",
    "liquidate_execute",
)


WRAPPERS: dict[str, str] = {
    "spend": """mod lib;
mod poseidon;
mod merkle;
mod note;
mod spend;

use crate::spend;

fn main(
    root: pub Field,
    nullifier: pub Field,
    new_commitment: pub Field,
    public_amount: pub Field,
    spent_note: spend::SpentNote,
    output_note: spend::OutputNote,
    merkle_path: [Field; 32],
    merkle_index_bits: [Field; 32],
    spend_sk: Field,
) {
    spend::verify(
        root,
        nullifier,
        new_commitment,
        public_amount,
        spent_note,
        output_note,
        merkle_path,
        merkle_index_bits,
        spend_sk,
    );
}
""",
    "output": """mod lib;
mod poseidon;
mod merkle;
mod note;
mod output;

use crate::note::Note;

fn main(
    commitment: pub Field,
    asset_id: pub Field,
    public_amount: pub Field,
    note: Note,
) {
    output::verify(commitment, asset_id, public_amount, note);
}
""",
    "join_split": """mod lib;
mod poseidon;
mod merkle;
mod note;
mod join_split;

use crate::note::Note;

fn main(
    root: pub Field,
    nullifier1: pub Field,
    nullifier2: pub Field,
    commitment1: pub Field,
    commitment2: pub Field,
    public_amount: pub Field,
    input1: Note,
    input2: Note,
    input1_path: [Field; 32],
    input1_index_bits: [Field; 32],
    input2_path: [Field; 32],
    input2_index_bits: [Field; 32],
    output1: Note,
    output2: Note,
    spend_sk: Field,
    input2_used: Field,
    output2_used: Field,
) {
    join_split::verify(
        root,
        nullifier1,
        nullifier2,
        commitment1,
        commitment2,
        public_amount,
        input1,
        input2,
        input1_path,
        input1_index_bits,
        input2_path,
        input2_index_bits,
        output1,
        output2,
        spend_sk,
        input2_used,
        output2_used,
    );
}
""",
    "order_place": """mod lib;
mod poseidon;
mod merkle;
mod note;
mod order_place;

use crate::note::Note;

fn main(
    root: pub Field,
    nullifier: pub Field,
    new_commitment: pub Field,
    market_id: pub Field,
    side_hash: pub Field,
    price_band: pub Field,
    size_band: pub Field,
    oracle_price: pub Field,
    imm_required: pub Field,
    spent: Note,
    output: Note,
    spent_path: [Field; 32],
    spent_index_bits: [Field; 32],
    spend_sk: Field,
    side_salt: Field,
    side: Field,
) {
    order_place::verify(
        root,
        nullifier,
        new_commitment,
        market_id,
        side_hash,
        price_band,
        size_band,
        oracle_price,
        imm_required,
        spent,
        output,
        spent_path,
        spent_index_bits,
        spend_sk,
        side_salt,
        side,
    );
}
""",
    "liquidate_claim": """mod lib;
mod poseidon;
mod merkle;
mod note;
mod liquidate_claim;

use crate::note::Note;

fn main(
    root: pub Field,
    market_id: pub Field,
    oracle_price: pub Field,
    liquidator_id: pub Field,
    claim_tag: pub Field,
    victim: Note,
    victim_path: [Field; 32],
    victim_index_bits: [Field; 32],
    notional: Field,
    maintenance_required: Field,
    equity: Field,
) {
    liquidate_claim::verify(
        root,
        market_id,
        oracle_price,
        liquidator_id,
        claim_tag,
        victim,
        victim_path,
        victim_index_bits,
        notional,
        maintenance_required,
        equity,
    );
}
""",
    "liquidate_execute": """mod lib;
mod poseidon;
mod merkle;
mod note;
mod liquidate_execute;

use crate::note::Note;

fn main(
    root: pub Field,
    victim_nullifier: pub Field,
    bounty_commitment: pub Field,
    insurance_commitment: pub Field,
    winning_bid: pub Field,
    market_id: pub Field,
    oracle_price: pub Field,
    victim: Note,
    victim_path: [Field; 32],
    victim_index_bits: [Field; 32],
    victim_spend_sk: Field,
    bounty: Note,
    insurance: Note,
) {
    liquidate_execute::verify(
        root,
        victim_nullifier,
        bounty_commitment,
        insurance_commitment,
        winning_bid,
        market_id,
        oracle_price,
        victim,
        victim_path,
        victim_index_bits,
        victim_spend_sk,
        bounty,
        insurance,
    );
}
""",
}


ROTATION_OFFSETS = (
    0,
    1,
    62,
    28,
    27,
    36,
    44,
    6,
    55,
    20,
    3,
    10,
    43,
    25,
    39,
    41,
    45,
    15,
    21,
    8,
    18,
    2,
    61,
    56,
    14,
)

ROUND_CONSTANTS = (
    0x0000000000000001,
    0x0000000000008082,
    0x800000000000808A,
    0x8000000080008000,
    0x000000000000808B,
    0x0000000080000001,
    0x8000000080008081,
    0x8000000000008009,
    0x000000000000008A,
    0x0000000000000088,
    0x0000000080008009,
    0x000000008000000A,
    0x000000008000808B,
    0x800000000000008B,
    0x8000000000008089,
    0x8000000000008003,
    0x8000000000008002,
    0x8000000000000080,
    0x000000000000800A,
    0x800000008000000A,
    0x8000000080008081,
    0x8000000000008080,
    0x0000000080000001,
    0x8000000080008008,
)


def rotl64(value: int, shift: int) -> int:
    shift %= 64
    return ((value << shift) | (value >> (64 - shift))) & ((1 << 64) - 1)


def keccak_f1600(state: list[int]) -> None:
    for round_constant in ROUND_CONSTANTS:
        c = [state[x] ^ state[x + 5] ^ state[x + 10] ^ state[x + 15] ^ state[x + 20] for x in range(5)]
        d = [c[(x - 1) % 5] ^ rotl64(c[(x + 1) % 5], 1) for x in range(5)]
        for x in range(5):
            for y in range(5):
                state[x + 5 * y] ^= d[x]

        b = [0] * 25
        for x in range(5):
            for y in range(5):
                index = x + 5 * y
                new_x = y
                new_y = (2 * x + 3 * y) % 5
                b[new_x + 5 * new_y] = rotl64(state[index], ROTATION_OFFSETS[index])

        for x in range(5):
            for y in range(5):
                state[x + 5 * y] = b[x + 5 * y] ^ ((~b[((x + 1) % 5) + 5 * y]) & b[((x + 2) % 5) + 5 * y])

        state[0] ^= round_constant


def keccak256(data: bytes) -> bytes:
    rate = 136
    padded = bytearray(data)
    padded.append(0x01)
    while len(padded) % rate != rate - 1:
        padded.append(0)
    padded.append(0x80)

    state = [0] * 25
    for offset in range(0, len(padded), rate):
        block = padded[offset : offset + rate]
        for lane_index in range(rate // 8):
            lane = int.from_bytes(block[lane_index * 8 : (lane_index + 1) * 8], "little")
            state[lane_index] ^= lane
        keccak_f1600(state)

    output = bytearray()
    while len(output) < 32:
        for lane in state[: rate // 8]:
            output.extend(lane.to_bytes(8, "little"))
            if len(output) >= 32:
                return bytes(output[:32])
        keccak_f1600(state)
    return bytes(output[:32])


def hash_directory(path: Path) -> bytes:
    digest_input = bytearray()
    files = sorted(child for child in path.rglob("*") if child.is_file())
    for child in files:
        relative = child.relative_to(path).as_posix().encode("utf-8")
        contents = child.read_bytes()
        digest_input.extend(relative)
        digest_input.extend(len(contents).to_bytes(8, "little"))
        digest_input.extend(contents)
    return keccak256(bytes(digest_input))


def package_name(circuit: str) -> str:
    return f"prime_{circuit}_circuit"


def materialize_package(tmp_dir: Path, circuits_src_dir: Path, circuit: str) -> Path:
    package_dir = tmp_dir / circuit
    src_dir = package_dir / "src"
    src_dir.mkdir(parents=True, exist_ok=True)

    for source_file in circuits_src_dir.glob("*.nr"):
        shutil.copy2(source_file, src_dir / source_file.name)

    nargo_toml = (
        "[package]\n"
        f"name = \"{package_name(circuit)}\"\n"
        'type = "bin"\n'
        'authors = ["PrimeNumbers Labs"]\n'
        'compiler_version = ">=0.30.0"\n\n'
        "[dependencies]\n"
    )
    (package_dir / "Nargo.toml").write_text(nargo_toml, encoding="utf-8")
    (src_dir / "main.nr").write_text(WRAPPERS[circuit], encoding="utf-8")
    return package_dir


def compile_circuit(nargo_bin: str, circuits_src_dir: Path, artifacts_dir: Path, circuit: str) -> Path:
    with tempfile.TemporaryDirectory(prefix=f"mersennet-{circuit}-") as tmp_name:
        package_dir = materialize_package(Path(tmp_name), circuits_src_dir, circuit)
        run_command([nargo_bin, "compile"], cwd=package_dir)

        target_dir = package_dir / "target"
        if not target_dir.exists():
            raise RuntimeError(f"nargo compile did not create {target_dir}")

        destination = artifacts_dir / circuit
        if destination.exists():
            shutil.rmtree(destination)
        shutil.copytree(target_dir, destination)
        (destination / "vk.hash").write_text(hash_directory(destination).hex(), encoding="utf-8")
        return destination


def parse_args() -> argparse.Namespace:
    root = repo_root()
    parser = argparse.ArgumentParser(description="Compile per-circuit Noir artifacts and write deterministic vk.hash files.")
    parser.add_argument("--circuit", action="append", choices=CIRCUITS, help="Only compile a specific circuit. Repeat for more than one.")
    parser.add_argument(
        "--circuits-dir",
        default=str(root / "crates" / "zkp" / "circuits"),
        help="Path to the shared Noir circuits directory.",
    )
    parser.add_argument(
        "--artifacts-dir",
        default=os.environ.get("PRIME_NOIR_ARTIFACTS_DIR", str(root / "crates" / "zkp" / "params" / "noir")),
        help="Destination for compiled per-circuit artifacts.",
    )
    parser.add_argument(
        "--nargo-bin",
        default=os.environ.get("PRIME_NARGO_BIN", "nargo"),
        help="Path to the nargo executable.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    circuits = args.circuit or list(CIRCUITS)
    circuits_dir = Path(args.circuits_dir).resolve()
    circuits_src_dir = circuits_dir / "src"
    artifacts_dir = Path(args.artifacts_dir).resolve()
    artifacts_dir.mkdir(parents=True, exist_ok=True)

    if not circuits_src_dir.exists():
        raise SystemExit(f"missing circuits source directory: {circuits_src_dir}")

    verified_empty = keccak256(b"")
    if verified_empty.hex() != "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470":
        raise SystemExit("keccak256 self-test failed")

    for circuit in circuits:
        destination = compile_circuit(args.nargo_bin, circuits_src_dir, artifacts_dir, circuit)
        print(f"compiled {circuit} -> {destination}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())