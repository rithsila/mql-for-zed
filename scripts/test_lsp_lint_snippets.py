#!/usr/bin/env python3
"""Exercise local linting and snippet completions over LSP JSON-RPC."""
import json
import os
import select
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SERVER = ROOT / "lsp" / "target" / "debug" / "mql-lsp"


class LintAndSnippetTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.ea = self.root / "TestEA.mq5"
        self.ea.write_text("void OnTick() {}\n")

        self.proc = subprocess.Popen(
            [str(SERVER)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            bufsize=0,
        )
        self.addCleanup(self.stop)

    def stop(self):
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.communicate(timeout=2)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.communicate()

    def send(self, method, ident=None, params=None):
        assert self.proc.stdin is not None
        message = {"jsonrpc": "2.0", "method": method, "params": params or {}}
        if ident is not None:
            message["id"] = ident
        body = json.dumps(message).encode()
        self.proc.stdin.write(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
        self.proc.stdin.flush()

    def recv(self, timeout=4):
        assert self.proc.stdout is not None
        assert self.proc.stderr is not None
        deadline = time.monotonic() + timeout

        def read_exact(size):
            assert self.proc.stdout is not None
            assert self.proc.stderr is not None
            result = bytearray()
            while len(result) < size:
                remaining = deadline - time.monotonic()
                if remaining <= 0 or not select.select([self.proc.stdout], [], [], remaining)[0]:
                    self.fail(f"timed out waiting for LSP output (server exit={self.proc.poll()})")
                chunk = os.read(self.proc.stdout.fileno(), size - len(result))
                if not chunk:
                    self.fail("LSP exited: " + self.proc.stderr.read().decode())
                result.extend(chunk)
            return bytes(result)

        header = bytearray()
        while not header.endswith(b"\r\n\r\n"):
            header.extend(read_exact(1))
        length = int(header.decode().split("Content-Length: ")[1].split("\r\n")[0])
        return json.loads(read_exact(length))

    def init_lsp(self, lint_options=None):
        init_opts = {}
        if lint_options is not None:
            init_opts["lint"] = lint_options
        self.send(
            "initialize",
            1,
            {
                "processId": None,
                "rootUri": self.root.as_uri(),
                "capabilities": {},
                "initializationOptions": init_opts,
            },
        )
        resp = self.recv()
        self.assertEqual(resp["id"], 1)
        self.send("initialized", params={})

    def test_lint_on_open_and_change_local(self):
        self.init_lsp()
        # Open document with an ignored trade call
        code_with_lint = "void OnTick() {\n   trade.Buy(0.1);\n}\n"
        self.send(
            "textDocument/didOpen",
            params={
                "textDocument": {
                    "uri": self.ea.as_uri(),
                    "languageId": "mql5",
                    "version": 1,
                    "text": code_with_lint,
                }
            },
        )
        msg = self.recv()
        self.assertEqual(msg["method"], "textDocument/publishDiagnostics")
        self.assertEqual(msg["params"]["uri"], self.ea.as_uri())
        diags = msg["params"]["diagnostics"]
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0]["source"], "zed-mql-lint")
        self.assertEqual(diags[0]["code"], "ignored-trade-result")

        # Edit document to suppress the lint inline
        suppressed_code = "void OnTick() {\n   // zed-mql-lint: disable ignored-trade-result fire and forget\n   trade.Buy(0.1);\n}\n"
        self.send(
            "textDocument/didChange",
            params={
                "textDocument": {"uri": self.ea.as_uri(), "version": 2},
                "contentChanges": [{"text": suppressed_code}],
            },
        )
        msg = self.recv()
        self.assertEqual(msg["method"], "textDocument/publishDiagnostics")
        self.assertEqual(len(msg["params"]["diagnostics"]), 0)

    def test_lint_configuration_disabled(self):
        self.init_lsp(lint_options={"enabled": False})
        code_with_lint = "void OnTick() {\n   trade.Buy(0.1);\n}\n"
        self.send(
            "textDocument/didOpen",
            params={
                "textDocument": {
                    "uri": self.ea.as_uri(),
                    "languageId": "mql5",
                    "version": 1,
                    "text": code_with_lint,
                }
            },
        )
        # Should not publish any diagnostics because lint is disabled
        assert self.proc.stdout is not None
        self.assertFalse(
            select.select([self.proc.stdout], [], [], 0.5)[0],
            "disabled lint must not publish diagnostics",
        )

    def test_lint_severity_override(self):
        self.init_lsp(
            lint_options={
                "enabled": True,
                "rules": {
                    "ignored-trade-result": {
                        "severity": "error"
                    }
                },
            }
        )
        code = "void OnTick() {\n   trade.Buy(0.1);\n}\n"
        self.send(
            "textDocument/didOpen",
            params={
                "textDocument": {
                    "uri": self.ea.as_uri(),
                    "languageId": "mql5",
                    "version": 1,
                    "text": code,
                }
            },
        )
        msg = self.recv()
        diags = msg["params"]["diagnostics"]
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0]["severity"], 1)  # DiagnosticSeverity::ERROR is 1

    def test_snippet_completions(self):
        self.init_lsp()
        self.send(
            "textDocument/didOpen",
            params={
                "textDocument": {
                    "uri": self.ea.as_uri(),
                    "languageId": "mql5",
                    "version": 1,
                    "text": "void OnStart() {}\n",
                }
            },
        )
        self.send(
            "textDocument/completion",
            2,
            {
                "textDocument": {"uri": self.ea.as_uri()},
                "position": {"line": 0, "character": 0},
            },
        )
        resp = self.recv()
        self.assertEqual(resp["id"], 2)
        items = resp["result"]
        labels = [item["label"] for item in items]
        expected_snippets = [
            "OnInit",
            "OnTick",
            "OnDeinit",
            "ea-skeleton",
            "trade-setup",
            "indicator-init",
            "copy-buffer",
            "trade-check",
            "new-bar",
            "OnTester",
        ]
        for snippet_label in expected_snippets:
            self.assertIn(snippet_label, labels, f"Missing snippet completion {snippet_label}")

        # Check snippet format
        on_init_item = next(item for item in items if item["label"] == "OnInit")
        self.assertEqual(on_init_item["kind"], 15)  # CompletionItemKind::SNIPPET is 15
        self.assertEqual(on_init_item["insertTextFormat"], 2)  # InsertTextFormat::SNIPPET is 2
        self.assertIn("INIT_SUCCEEDED", on_init_item["insertText"])


    def test_coexistence_with_compiler_diagnostics(self):
        # Set up stub script for compiler diagnostics
        stub = self.root / "stub.sh"
        stub.write_text('''#!/usr/bin/env bash
set -eu
job=""; snap=""
while (($#)); do
  case "$1" in
    --job-id) job="$2"; shift 2 ;;
    --snapshot) snap="$2"; shift 2 ;;
    *) shift ;;
  esac
done
printf '{"schema_version":1,"job_id":"%s","source_snapshot":"%s","status":"compiler_errors","diagnostics":[{"uri":"%s","severity":"error","message":"compiler syntax error","line":1,"col":1,"code":"256"}]}\n' "$job" "$snap" "$TEST_URI"
exit 1
''')
        os.chmod(stub, 0o755)

        self.stop()
        env = dict(os.environ, TEST_URI=self.ea.as_uri())
        self.proc = subprocess.Popen(
            [str(SERVER)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
            bufsize=0,
        )

        self.send(
            "initialize",
            1,
            {
                "processId": None,
                "rootUri": self.root.as_uri(),
                "capabilities": {},
                "initializationOptions": {
                    "compilerCheck": {"enabled": True, "script": str(stub)},
                    "lint": {"enabled": True},
                },
            },
        )
        self.assertEqual(self.recv()["id"], 1)
        self.send("initialized", params={})

        # Open file with lint violation
        code_with_lint = "void OnTick() {\n   trade.Buy(0.1);\n}\n"
        self.ea.write_text(code_with_lint)
        self.send(
            "textDocument/didOpen",
            params={
                "textDocument": {
                    "uri": self.ea.as_uri(),
                    "languageId": "mql5",
                    "version": 1,
                    "text": code_with_lint,
                }
            },
        )
        msg1 = self.recv()
        self.assertEqual(len(msg1["params"]["diagnostics"]), 1)
        self.assertEqual(msg1["params"]["diagnostics"][0]["source"], "zed-mql-lint")

        # Save document to trigger compiler check
        self.send("textDocument/didSave", params={"textDocument": {"uri": self.ea.as_uri()}})
        msg2 = self.recv()
        sources = {d["source"] for d in msg2["params"]["diagnostics"]}
        self.assertIn("zed-mql-lint", sources, "Lint diagnostic was erased by compiler diagnostic")
        self.assertIn("MetaEditor", sources, "Compiler diagnostic was missing")
        self.assertEqual(len(msg2["params"]["diagnostics"]), 2)


if __name__ == "__main__":
    unittest.main()
