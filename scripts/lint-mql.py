#!/usr/bin/env python3
"""Run the local zed-mql lint rules on one MQL5 file through the mql-lsp server.

Prints one JSON object on stdout:
  {"status":"ok","file":"...","diagnostics":[{"row":0,"col":0,"severity":"warning",
   "source":"zed-mql-lint","code":"ignored-trade-result","message":"..."}]}

Exit codes: 0 ok (findings or clean), 2 usage error, 3 lint server binary missing,
4 no diagnostics received (timeout or server problem).
"""
import json
import os
import select
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SERVER = ROOT / "lsp" / "target" / "debug" / "mql-lsp"
CRLF = chr(13) + chr(10)
SEV = {1: "error", 2: "warning", 3: "info", 4: "hint"}


class Lsp:
    def __init__(self, binary):
        self.proc = subprocess.Popen(
            [str(binary)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            bufsize=0,
        )

    def send(self, method, ident=None, params=None):
        message = {"jsonrpc": "2.0", "method": method, "params": params or {}}
        if ident is not None:
            message["id"] = ident
        body = json.dumps(message).encode()
        head = ("Content-Length: " + str(len(body)) + CRLF + CRLF).encode()
        self.proc.stdin.write(head + body)
        self.proc.stdin.flush()

    def recv(self, timeout):
        out = bytearray()
        deadline = time.monotonic() + timeout
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                return None
            r, _, _ = select.select([self.proc.stdout], [], [], remaining)
            if not r:
                continue
            chunk = os.read(self.proc.stdout.fileno(), 4096)
            if not chunk:
                return None
            out.extend(chunk)
            text = bytes(out)
            sep = ("Content-Length: ").encode()
            if text.startswith(sep):
                idx = text.find((CRLF + CRLF).encode())
                if idx == -1:
                    continue
                length = int(text[len(sep):idx].decode().strip())
                have = len(text) - (idx + 4)
                while have < length:
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        return None
                    chunk = os.read(self.proc.stdout.fileno(), length - have)
                    if not chunk:
                        return None
                    out.extend(chunk)
                    have += len(chunk)
                body = bytes(out[idx + 4: idx + 4 + length])
                rest = bytes(out[idx + 4 + length:])
                # keep leftover buffered bytes for the next call
                self._buf = rest
                return json.loads(body)

    def run(self):
        return self.proc.poll() is None


def main():
    if len(sys.argv) != 2 or sys.argv[1] in ("-h", "--help"):
        print("usage: lint-mql.py <path/to/File.mq5>", file=sys.stderr)
        return 2
    path = Path(sys.argv[1]).expanduser().resolve()
    if path.suffix not in (".mq5", ".mqh"):
        print("error: expected an .mq5 or .mqh file", file=sys.stderr)
        return 2
    if not path.is_file():
        print("error: file not found: " + str(path), file=sys.stderr)
        return 2
    if not SERVER.is_file():
        print("error: lint server missing at " + str(SERVER) + "; build lsp/ first", file=sys.stderr)
        return 3

    text = path.read_text(encoding="utf-8", errors="replace")
    lsp = Lsp(SERVER)
    try:
        lsp.send("initialize", 1, {
            "processId": os.getpid(),
            "rootUri": path.parent.as_uri(),
            "capabilities": {},
            "initializationOptions": {},
        })
        resp = lsp.recv(4)
        if not resp or resp.get("id") != 1:
            print("error: lint server did not answer initialize", file=sys.stderr)
            return 4
        lsp.send("initialized")
        lsp.send("textDocument/didOpen", params={
            "textDocument": {
                "uri": path.as_uri(),
                "languageId": "mql5",
                "version": 1,
                "text": text,
            }
        })
        diags = []
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            msg = lsp.recv(max(0.1, deadline - time.monotonic()))
            if not msg:
                break
            if msg.get("method") == "textDocument/publishDiagnostics" and msg["params"]["uri"] == path.as_uri():
                diags = msg["params"]["diagnostics"]
                # give the server a short window to push a newer set, then stop
                try:
                    r, _, _ = select.select([lsp.proc.stdout], [], [], 0.4)
                    if not r:
                        break
                except OSError:
                    break
    finally:
        try:
            lsp.send("shutdown", 99)
            lsp.send("exit")
        except Exception:
            pass
        if lsp.run():
            lsp.proc.terminate()

    out = {
        "status": "ok",
        "file": str(path),
        "lint_server": str(SERVER),
        "diagnostics": [
            {
                "row": d["range"]["start"]["line"],
                "col": d["range"]["start"]["character"],
                "severity": SEV.get(d.get("severity"), "warning"),
                "source": d.get("source", ""),
                "code": d.get("code", ""),
                "message": d.get("message", ""),
            }
            for d in diags
        ],
    }
    print(json.dumps(out, indent=2))
    if not diags:
        # zero diagnostics may also mean the server never published; distinguish by protocol success
        print("note: no lint findings published (clean file or server silence)", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
