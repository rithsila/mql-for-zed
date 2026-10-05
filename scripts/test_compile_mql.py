#!/usr/bin/env python3
"""Focused contract tests; remote commands are replaced by local stubs."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parent


class CompileTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / "Example.mq5"
        self.source.write_text("void OnStart() {}\n")
        self.log = self.root / "compile.log"

    def run_script(self, script, *args, env=None):
        return subprocess.run([str(SCRIPTS / script), *map(str, args)],
                              text=True, capture_output=True, env=env)

    def assert_json(self, result, status, exit_code):
        self.assertEqual(result.returncode, exit_code, result.stderr)
        data = json.loads(result.stdout)
        self.assertEqual(data["status"], status)
        self.assertEqual(data["schema_version"], 1)
        self.assertEqual(data["job_id"], "job")
        self.assertEqual(data["source_snapshot"], "snapshot")
        return data

    def test_parser_summary_and_diagnostics(self):
        self.log.write_bytes("C:\\work\\Example.mq5(4,7) : error 123: bad\r\n1 errors, 0 warnings\r\n".encode("utf-16"))
        result = self.run_script("parse-mql-log.py", "--json", "--job-id", "job",
                                 "--snapshot", "snapshot", self.log,
                                 "--map", f"C:/work={self.root}")
        data = self.assert_json(result, "compiler_errors", 1)
        self.assertEqual(data["diagnostics"][0], {
            "uri": self.source.as_uri(), "severity": "error", "message": "bad",
            "line": 4, "col": 7, "code": "123"})

    def test_utf8_header_path_with_spaces_and_clickable_output(self):
        header = self.root / "Include Files" / "a.mqh"
        self.log.write_text("C:\\Program Files\\MQL5\\Include Files\\a.mqh(2,5) : warning 456: careful\n0 errors, 1 warnings\n")
        options = (self.log, "--map", f"C:/Program Files/MQL5={self.root}")
        data = self.assert_json(self.run_script("parse-mql-log.py", "--json", "--job-id", "job",
                                                "--snapshot", "snapshot", *options), "success", 0)
        self.assertEqual(data["diagnostics"][0]["uri"], header.as_uri())
        self.assertEqual(data["diagnostics"][0]["code"], "456")
        clickable = self.run_script("parse-mql-log.py", *options)
        self.assertEqual(clickable.returncode, 0)
        self.assertIn(f"{header}:2:5: warning: careful", clickable.stdout)

    def test_parser_log_failures_and_success(self):
        for content in ("", "Example.mq5(1,1) : warning: caution", "garbage"):
            self.log.write_text(content)
            result = self.run_script("parse-mql-log.py", "--json", "--job-id", "job",
                                     "--snapshot", "snapshot", self.log)
            self.assert_json(result, "log_failure", 2)
        self.log.write_bytes(b"\xff\xfe\xff")
        self.assert_json(self.run_script("parse-mql-log.py", "--json", "--job-id", "job",
                                         "--snapshot", "snapshot", self.log), "log_failure", 2)
        self.log.write_text("0 errors, 0 warnings\n")
        self.assert_json(self.run_script("parse-mql-log.py", "--json", "--job-id", "job",
                                         "--snapshot", "snapshot", self.log), "success", 0)
        self.log.unlink()
        self.assert_json(self.run_script("parse-mql-log.py", "--json", "--job-id", "job",
                                         "--snapshot", "snapshot", self.log), "log_failure", 2)

    def test_runner_stages_and_no_deploy(self):
        bin_dir = self.root / "bin"
        bin_dir.mkdir()
        calls = self.root / "calls"
        (bin_dir / "ssh").write_text("#!/bin/sh\necho ssh >> \"$CALLS\"\ncase \"$*\" in *start*) exit \"$LAUNCH_EXIT\";; esac\nexit \"$SYNC_EXIT\"\n")
        (bin_dir / "scp").write_text("#!/bin/sh\necho scp >> \"$CALLS\"\n[ \"$SCP_EXIT\" = 0 ] || exit \"$SCP_EXIT\"\nfor arg do dest=\"$arg\"; done\nprintf '0 errors, 0 warnings\\n' > \"$dest\"\n")
        for name in ("ssh", "scp"):
            (bin_dir / name).chmod(0o755)
        base = dict(os.environ, PATH=f"{bin_dir}:{os.environ['PATH']}", CALLS=str(calls),
                    MQL_WORKSPACE_ROOT=str(self.root), MQL_CONFIG=str(self.root / "missing"),
                    MQL_VM_MQL5_ROOT="C:/MQL5")
        for sync, launch, scp, status, code, expected in (
            (1, 0, 0, "ssh_failure", 2, "ssh\n"),
            (0, 255, 0, "ssh_failure", 2, "ssh\nssh\n"),
            (0, 1, 1, "launch_failure", 2, "ssh\nssh\nscp\n"),
            (0, 0, 1, "log_failure", 2, "ssh\nssh\nscp\n"),
            (0, 1, 0, "launch_failure", 2, "ssh\nssh\nscp\n"),
            (0, 0, 0, "success", 0, "ssh\nssh\nscp\n"),
        ):
            with self.subTest(status=status, launch=launch):
                calls.unlink(missing_ok=True)
                env = dict(base, SYNC_EXIT=str(sync), LAUNCH_EXIT=str(launch), SCP_EXIT=str(scp))
                result = self.run_script("compile-mql.sh", "--check", "--json", "--job-id", "job",
                                         "--snapshot", "snapshot", self.source, env=env)
                self.assert_json(result, status, code)
                self.assertEqual(calls.read_text(), expected)


if __name__ == "__main__":
    unittest.main()
