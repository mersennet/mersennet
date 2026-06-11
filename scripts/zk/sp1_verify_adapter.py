#!/usr/bin/env python3

from __future__ import annotations

import argparse
import os
from pathlib import Path

from common import host_executor, read_json, render_template_command, repo_root, run_command, with_wsl_paths


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Reference adapter for MERSENNET_SP1_VERIFY_ADAPTER.")
    parser.add_argument("--request", required=True)
    parser.add_argument("--response", required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    request_path = Path(args.request).resolve()
    response_path = Path(args.response).resolve()
    request = read_json(request_path)

    default_template = (
        'cargo run --manifest-path programs/state-transition-host/Cargo.toml '
        '-- --verify-request {request} --verify-response {response}'
    )
    template = os.environ.get("MERSENNET_SP1_VERIFY_TEMPLATE", default_template)
    if not template:
        scaffold = repo_root() / "programs" / "state-transition" / "README.md"
        raise SystemExit(
            "MERSENNET_SP1_VERIFY_TEMPLATE is not set. The repo still ships the SP1 program as a scaffold; "
            f"materialize/build that program and point this adapter at a real verifier command. See {scaffold}."
        )

    values = {
        "request": request_path,
        "response": response_path,
        "program_elf": request.get("programElfPath") or os.environ.get("MERSENNET_SP1_PROGRAM_ELF", ""),
        "prev_state_root_hex": request.get("prevStateRootHex", ""),
        "new_state_root_hex": request.get("newStateRootHex", ""),
        "prev_nullifier_root_hex": request.get("prevNullifierRootHex", ""),
        "new_nullifier_root_hex": request.get("newNullifierRootHex", ""),
        "block_height": request.get("blockHeight", ""),
        "block_hash_hex": request.get("blockHashHex", ""),
        "new_market_state_hash_hex": request.get("newMarketStateHashHex", ""),
        "tx_count": request.get("txCount", ""),
        "vkey_hash_hex": request.get("vkeyHashHex", ""),
        "public_values_hex": request.get("publicValuesHex", ""),
        "proof_bytes_hex": request.get("proofBytesHex", ""),
        "proof_system": request.get("proofSystem", ""),
        "mode": os.environ.get("MERSENNET_SP1_MODE", "local"),
    }
    executor = host_executor()
    if executor == "wsl":
        values = with_wsl_paths(values, ("request", "response", "program_elf"))
    run_command(render_template_command(template, values), cwd=repo_root(), executor=executor)
    if not response_path.exists():
        raise SystemExit(f"verify command finished without creating {response_path}")
    response = read_json(response_path)
    if "verified" not in response:
        raise SystemExit("verify response is missing required key: verified")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())