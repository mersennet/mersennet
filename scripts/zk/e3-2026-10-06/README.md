# E3 transcript, 6 Oct 2026

One real SP1 prove + verify of the current `programs/state-transition` ELF,
`MERSENNET_SP1_MODE=local`. Supersedes the 10 Jun transcript in
`scripts/zk/` (vkey `0013c6c7…`, `core` proof): six commits had touched
`programs/` since, so that ELF and its verifying key no longer matched the code.

| | |
|---|---|
| Source | `mersennet/mersennet` at `3660b23` (`programs/` unchanged since `dc83ec9`) |
| ELF build | `cargo prove build --docker --tag v6.2.2 --workspace-directory ../..` in `programs/state-transition` |
| ELF | `programs/state-transition/target/elf-compilation/docker/riscv64im-succinct-zkvm-elf/release/mersennet-state-transition`, 720,480 bytes |
| ELF sha256 | `03370b219d11ed928ef3ba3df471bd8d05029887a179ea8ccbeac09a50440eb7` |
| Verifying-key hash | `008cd0050b67a4fe3800a848c1c22166b36b696b795ec0d4681fc7678dc1a9a4` (`cargo prove vkey --elf …`), pinned in `crates/zkp/params/sp1/state-transition.vk.hash` |
| Input | block 1, zero state and nullifier roots, no transactions: `prove-request.json`, rendered by `examples/render_prove_request.rs` from the 10 Jun request with `blockProgramInputHex` cleared (the old bytes no longer deserialize into today's `BlockProgramInput`) |
| Prove | `prove-response.json`: `proof_system` `compressed`, 1.27 MB proof, 240 bytes of public values |
| Verify | `verify-request.json` (`examples/render_verify_request.rs`) → `verify-response.json`: `{"verified": true}` |

## What it takes to run

Hetzner `ccx43` (16 dedicated vCPU, 64 GB), Ubuntu 24.04, with the host built
`--release --features real-sp1` on Ubuntu 24.04 (glibc 2.39) and copied over.

- With SP1's default worker pools the prover went past 63 GB and was killed.
- With every pool at one worker (`SP1_WORKER_NUM_CORE_WORKERS=1`, `…_SETUP_…`,
  `…_SPLICING_…`, `…_RECURSION_EXECUTOR_…`, `…_RECURSION_PROVER_…`,
  `…_PREPARE_REDUCE_…`, `…_DEFERRED_…` and their `…_BUFFER_SIZE=1`,
  `SP1_WORKER_NUMBER_OF_GAS_EXECUTORS=1`) it peaked at 48.9 GB and took 1 h 14 min.
- The same single-worker run under a 34 GB cgroup cap on a 47 GB workstation
  was killed at the cap after 5.5 min; uncapped, it froze WSL.
- Verify: 30 s, 7.3 GB peak.

Use a 64 GB machine with the single-worker settings, or more memory for the
defaults. `prove.log` and `verify.log` keep only the `time -v` summaries:
the successful prove shared its log file with two restarts that were killed.
