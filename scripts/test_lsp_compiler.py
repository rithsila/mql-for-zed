#!/usr/bin/env python3
"""Exercise the LSP stdio protocol with a local, non-remote compiler stub."""
import json
import os
from pathlib import Path
import select
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parent.parent
SERVER = ROOT / "lsp" / "target" / "debug" / "mql-lsp"


class ProtocolTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.ea = self.root / "EA.mq5"
        self.header = self.root / "Include Test.mqh"
        self.ea.write_text('#include "Include Test.mqh"\nvoid OnStart() {}\n')
        self.header.write_text('//###<EA.mq5>\nint x;\n')
        self.mode = self.root / "mode"
        self.mode.write_text("error")
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
mode="$(cat "${MQL_WORKSPACE_ROOT}/mode")"
printf '%s\n' "$job" >> "${MQL_WORKSPACE_ROOT}/calls"
if [[ "$mode" == slow* ]]; then sleep 2; fi
if [[ "$mode" == *error ]]; then
  status=compiler_errors; code=1
  diagnostics='[{"uri":"'"${HEADER_URI}"'","severity":"error","message":"bad header","line":2,"col":5,"code":"256"}]'
elif [[ "$mode" == failure ]]; then
  status=ssh_failure; code=2; diagnostics='[]'
else
  status=success; code=0; diagnostics='[]'
fi
printf '{"schema_version":1,"job_id":"%s","source_snapshot":"%s","status":"%s","diagnostics":%s}\n' "$job" "$snap" "$status" "$diagnostics"
exit "$code"
''')
        env = dict(os.environ, HEADER_URI=self.header.as_uri())
        self.proc = subprocess.Popen([str(SERVER)], stdin=subprocess.PIPE,
                                     stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env,
                                     bufsize=0)
        self.addCleanup(self.stop)
        self.send("initialize", 1, {"processId": None, "rootUri": self.root.as_uri(),
                                    "capabilities": {}, "initializationOptions": {
                                        "compilerCheck": {"enabled": True, "script": str(stub)}}})
        self.assertEqual(self.recv()["id"], 1)
        self.send("initialized", params={})
        self.send("textDocument/didOpen", params={"textDocument": {
            "uri": self.header.as_uri(), "languageId": "mql5", "version": 1,
            "text": self.header.read_text()}})

    def stop(self):
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.communicate(timeout=2)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.communicate()

    def send(self, method, ident=None, params=None):
        message = {"jsonrpc": "2.0", "method": method, "params": params or {}}
        if ident is not None:
            message["id"] = ident
        body = json.dumps(message).encode()
        self.proc.stdin.write(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
        self.proc.stdin.flush()

    def recv(self, timeout=4):
        deadline = time.monotonic() + timeout
        def read_exact(size):
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

    def save(self):
        self.send("textDocument/didSave", params={"textDocument": {"uri": self.header.as_uri()}})

    def expect_diagnostics(self, count):
        result = self.recv()
        self.assertEqual(result["method"], "textDocument/publishDiagnostics")
        self.assertEqual(result["params"]["uri"], self.header.as_uri())
        self.assertEqual(len(result["params"]["diagnostics"]), count)
        return result["params"]["diagnostics"]

    def test_publish_failure_preserves_and_clean_clears(self):
        self.save()
        diag = self.expect_diagnostics(1)[0]
        self.assertEqual(diag["code"], "256")
        self.assertEqual(diag["range"]["start"], {"line": 1, "character": 4})
        self.mode.write_text("failure")
        self.save()
        self.assertFalse(select.select([self.proc.stdout], [], [], 0.7)[0],
                         "infrastructure failure must not clear diagnostics")
        self.mode.write_text("success")
        self.save()
        self.expect_diagnostics(0)

    def test_rapid_saves_coalesce(self):
        for _ in range(3):
            self.save()
        self.expect_diagnostics(1)
        self.assertEqual(len((self.root / "calls").read_text().splitlines()), 1)

    def test_slow_check_does_not_block_requests_or_publish_stale_results(self):
        self.mode.write_text("slowerror")
        self.save()
        time.sleep(0.45)  # past debounce; stub is sleeping
        self.send("textDocument/hover", 2, {"textDocument": {"uri": self.header.as_uri()},
                                            "position": {"line": 1, "character": 0}})
        self.send("textDocument/completion", 3, {"textDocument": {"uri": self.header.as_uri()},
                                                 "position": {"line": 1, "character": 0}})
        start = time.monotonic()
        self.assertEqual({self.recv(timeout=1.4)["id"], self.recv(timeout=1.4)["id"]}, {2, 3})
        self.assertLess(time.monotonic() - start, 1.5)
        self.mode.write_text("success")
        self.send("textDocument/didChange", params={"textDocument": {
            "uri": self.header.as_uri(), "version": 2}, "contentChanges": [{"text": self.header.read_text()}]})
        self.save()
        self.assertFalse(select.select([self.proc.stdout], [], [], 3.3)[0],
                         "stale error and clean check should not publish diagnostics")


if __name__ == "__main__":
    unittest.main()
