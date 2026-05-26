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


if __name__ == "__main__":
    unittest.main()