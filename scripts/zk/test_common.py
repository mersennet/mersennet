import os
import unittest
from pathlib import Path
from unittest.mock import patch

import common


class CommonParityTests(unittest.TestCase):
    def test_to_wsl_path_translates_windows_drive_paths(self) -> None:
        self.assertEqual(
            common.to_wsl_path(r"C:\Users\rod_o\Documents\projects\personal\prime-chain\request.json"),
            "/mnt/c/Users/rod_o/Documents/projects/personal/prime-chain/request.json",
        )

    def test_with_wsl_paths_only_rewrites_selected_keys(self) -> None:
        values = {
            "request": r"C:\temp\request.json",
            "response": r"C:\temp\response.json",
            "mode": "local",
        }

        translated = common.with_wsl_paths(values, ("request", "response"))

        self.assertEqual(translated["request"], "/mnt/c/temp/request.json")
        self.assertEqual(translated["response"], "/mnt/c/temp/response.json")
        self.assertEqual(translated["mode"], "local")

    @patch.dict(os.environ, {}, clear=True)
    @patch("common.shutil.which", return_value=r"C:\Windows\System32\wsl.exe")
    def test_build_wsl_command_wraps_repo_command(self, _which: object) -> None:
        command = [
            "cargo",
            "run",
            "--manifest-path",
            "programs/state-transition-host/Cargo.toml",
            "--features",
            "real-sp1",
            "--",
            "--prove-request",
            "/mnt/c/tmp/request.json",
            "--prove-response",
            "/mnt/c/tmp/response.json",
        ]

        wrapped = common.build_wsl_command(command, Path(r"C:\repo\prime-chain"))

        self.assertEqual(wrapped[:3], [r"C:\Windows\System32\wsl.exe", "bash", "-lc"])
        self.assertIn("cd /mnt/c/repo/prime-chain && cargo run", wrapped[3])
        self.assertIn("--features real-sp1", wrapped[3])
        self.assertIn("/mnt/c/tmp/request.json", wrapped[3])

    @patch.dict(os.environ, {}, clear=True)
    @patch("common.shutil.which", return_value=r"C:\Windows\System32\wsl.exe")
    def test_build_wsl_command_includes_env_overrides(self, _which: object) -> None:
        wrapped = common.build_wsl_command(
            ["cargo", "run", "--release"],
            Path(r"C:\repo\prime-chain"),
            {"PRIME_SP1_PROOF_SYSTEM": "core", "RAYON_NUM_THREADS": "1"},
        )

        self.assertIn("PRIME_SP1_PROOF_SYSTEM=core", wrapped[3])
        self.assertIn("RAYON_NUM_THREADS=1", wrapped[3])

    def test_normalize_real_sp1_command_adds_release_profile(self) -> None:
        command = [
            "cargo",
            "run",
            "--manifest-path",
            "programs/state-transition-host/Cargo.toml",
            "--features",
            "real-sp1",
            "--",
            "--prove-request",
            "request.json",
            "--prove-response",
            "response.json",
        ]

        normalized = common.normalize_real_sp1_command(command)

        self.assertEqual(normalized[0:3], ["cargo", "run", "--release"])
        self.assertIn("real-sp1", normalized)

    def test_normalize_real_sp1_command_preserves_explicit_profile(self) -> None:
        command = [
            "cargo",
            "run",
            "--profile",
            "dev",
            "--manifest-path",
            "programs/state-transition-host/Cargo.toml",
            "--features",
            "real-sp1",
        ]

        normalized = common.normalize_real_sp1_command(command)

        self.assertEqual(normalized, command)

    @patch.dict(os.environ, {"PRIME_SP1_MODE": "local"}, clear=True)
    def test_local_real_sp1_env_defaults_added_for_local_prove(self) -> None:
        command = [
            "cargo",
            "run",
            "--release",
            "--manifest-path",
            "programs/state-transition-host/Cargo.toml",
            "--features",
            "real-sp1",
            "--",
            "--prove-request",
            "request.json",
            "--prove-response",
            "response.json",
        ]

        defaults = common.local_real_sp1_env_defaults(command)

        self.assertEqual(defaults["PRIME_SP1_PROOF_SYSTEM"], "core")
        self.assertEqual(defaults["PRIME_SP1_INLINE_VERIFY"], "0")
        self.assertEqual(defaults["RAYON_NUM_THREADS"], "2")
        self.assertEqual(defaults["SP1_WORKER_NUM_CORE_WORKERS"], "2")
        self.assertEqual(defaults["SP1_WORKER_NUM_SETUP_WORKERS"], "2")
        self.assertEqual(defaults["SP1_WORKER_NUM_RECURSION_PROVER_WORKERS"], "1")

    @patch.dict(
        os.environ,
        {
            "PRIME_SP1_MODE": "local",
            "PRIME_SP1_PROOF_SYSTEM": "compressed",
            "RAYON_NUM_THREADS": "4",
        },
        clear=True,
    )
    def test_local_real_sp1_env_defaults_preserve_explicit_overrides(self) -> None:
        command = [
            "cargo",
            "run",
            "--release",
            "--manifest-path",
            "programs/state-transition-host/Cargo.toml",
            "--features",
            "real-sp1",
            "--",
            "--prove-request",
            "request.json",
            "--prove-response",
            "response.json",
        ]

        defaults = common.local_real_sp1_env_defaults(command)

        self.assertNotIn("PRIME_SP1_PROOF_SYSTEM", defaults)
        self.assertNotIn("RAYON_NUM_THREADS", defaults)
        self.assertEqual(defaults["PRIME_SP1_INLINE_VERIFY"], "0")


if __name__ == "__main__":
    unittest.main()