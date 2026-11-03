#!/usr/bin/env python3
"""Bounded subprocess contract checks for the development MCP mock only.

No physical device discovery is requested. PASS establishes stdio/mock host
behavior, not hardware, full P0 qualification, or independent SDK-client interop.
"""
import argparse
from collections import deque
import json
from pathlib import Path
import queue
import subprocess
import threading
import time

VERSIONS = ("2025-11-25", "2026-07-28")
TOOLS = {"ak820_devices", "ak820_session", "ak820_read", "ak820_tests", "ak820_trace", "ak820_operation", "ak820_backup", "ak820_prepare"}


class Client:
    def __init__(self, binary, version, backend="mock"):
        flags = ["--mcp-stdio"] if backend is None else ["--mcp-stdio", "--mcp-access=read-only", f"--mcp-backend={backend}"]
        self.process = subprocess.Popen(
            [str(binary), *flags],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True, bufsize=1)
        self.lines = queue.Queue()
        self.stderr = deque(maxlen=64)
        self.closed = False
        self.next_id = 0
        self.version = version
        self.modern = version == "2026-07-28"
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.err_reader = threading.Thread(target=self._read_stderr, daemon=True)
        self.reader.start()
        self.err_reader.start()
        try:
            if self.modern:
                # rmcp 3.8 implements SEP-2575 discovery/per-request metadata:
                # modern protocol has no initialize/initialized lifecycle.
                discovered = self.request("server/discover", {})
                assert version in discovered.get("supportedVersions", []), (
                    f"Modern discovery does not advertise {version}: {discovered}")
                assert discovered.get("resultType") == "complete", discovered
                assert "ttlMs" in discovered and "cacheScope" in discovered, discovered
                assert discovered["capabilities"].get("tools") is not None, discovered
            else:
                initialized = self.request("initialize", {"protocolVersion": version,
                    "capabilities": {}, "clientInfo": {"name": "ak820-stdio-contract", "version": "1"}})
                if initialized.get("protocolVersion") != version:
                    raise AssertionError(f"Protocol-version mismatch: requested {version}, got {initialized.get('protocolVersion')}")
                self.notify("notifications/initialized")
        except BaseException:
            self.close(require_success=False)
            raise

    def _read(self):
        try:
            for line in self.process.stdout:
                self.lines.put(line)
        finally:
            self.lines.put(None)

    def _read_stderr(self):
        for line in self.process.stderr:
            self.stderr.append(line[-1024:])

    def raw(self, text):
        self.process.stdin.write(text)
        self.process.stdin.flush()

    def notify(self, method, params=None):
        self.raw(json.dumps({"jsonrpc": "2.0", "method": method, "params": params or {}}) + "\n")

    @staticmethod
    def decode(line):
        try:
            message = json.loads(line)
        except (ValueError, TypeError) as exc:
            raise AssertionError(f"Non-protocol stdout: {str(line)[:200]!r}") from exc
        if not isinstance(message, dict) or message.get("jsonrpc") != "2.0":
            raise AssertionError(f"Non-JSON-RPC stdout: {message!r}")
        return message

    def request(self, method, params, reject_unrelated=False):
        self.next_id += 1
        ident = self.next_id
        if self.modern:
            params = dict(params)
            params["_meta"] = {
                "io.modelcontextprotocol/protocolVersion": self.version,
                "io.modelcontextprotocol/clientInfo": {"name": "ak820-stdio-contract", "version": "1"},
                "io.modelcontextprotocol/clientCapabilities": {},
            }
        self.raw(json.dumps({"jsonrpc": "2.0", "id": ident, "method": method, "params": params}) + "\n")
        deadline = time.monotonic() + 10
        while True:
            try:
                line = self.lines.get(timeout=max(0.001, deadline - time.monotonic()))
            except queue.Empty as exc:
                raise AssertionError(f"Timed out waiting for {method}") from exc
            if line is None:
                raise AssertionError(f"Server closed while awaiting {method}: {''.join(self.stderr)[-4096:]}")
            message = self.decode(line)
            if reject_unrelated and message.get("id") != ident:
                raise AssertionError(f"Unexpected response to discarded frame: {message}")
            if message.get("id") == ident:
                if "error" in message:
                    raise AssertionError(f"{method} JSON-RPC error: {message['error']}")
                result = message["result"]
                if self.modern:
                    assert result.get("resultType") == "complete", f"Missing modern result discriminator for {method}: {result}"
                else:
                    assert "resultType" not in result, f"Modern result leaked into legacy {method}: {result}"
                return result
            if time.monotonic() >= deadline:
                raise AssertionError(f"Timed out waiting for {method} amid unrelated messages")

    def request_error(self, method, params):
        self.next_id += 1
        ident = self.next_id
        if self.modern:
            params = dict(params, _meta={
                "io.modelcontextprotocol/protocolVersion": self.version,
                "io.modelcontextprotocol/clientInfo": {"name": "ak820-stdio-contract", "version": "1"},
                "io.modelcontextprotocol/clientCapabilities": {},
            })
        self.raw(json.dumps({"jsonrpc": "2.0", "id": ident, "method": method, "params": params}) + "\n")
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            try:
                line = self.lines.get(timeout=max(0.001, deadline - time.monotonic()))
            except queue.Empty as exc:
                raise AssertionError(f"Timed out waiting for error from {method}") from exc
            assert line is not None, f"Server closed awaiting {method}"
            message = self.decode(line)
            if message.get("id") == ident:
                assert "error" in message, message
                return message["error"]
        raise AssertionError(f"Timed out waiting for error from {method}")

    def tool(self, name, arguments, error=None):
        result = self.request("tools/call", {"name": name, "arguments": arguments})
        value = result.get("structuredContent")
        assert isinstance(value, dict), f"Missing structuredContent: {result}"
        texts = [json.loads(item["text"]) for item in result.get("content", []) if item.get("type") == "text"]
        assert value in texts, "Text fallback disagrees with structuredContent"
        assert value.get("format") == "ak820-mcp-result-1"
        assert value.get("backend") == "mock" and value.get("evidence") == "host_model"
        assert value["verification"]["setters_sent"] == 0
        assert value["verification"]["physical_behavior_validated"] is False
        if error:
            assert result.get("isError") is True, result
            assert value.get("error", {}).get("code") == error, value
        else:
            assert not result.get("isError") and value.get("error") is None, value
        return value

    def resource(self, uri):
        result = self.request("resources/read", {"uri": uri})
        contents = result.get("contents", [])
        assert contents, result
        return json.loads(contents[0]["text"])

    def close(self, require_success=True):
        if self.closed:
            return
        self.closed = True
        try:
            self.process.stdin.close()
        except (BrokenPipeError, OSError):
            pass
        try:
            code = self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=5)
            raise AssertionError("Server did not exit within five seconds after EOF")
        finally:
            self.reader.join(timeout=1)
            self.err_reader.join(timeout=1)
        while not self.lines.empty():
            line = self.lines.get_nowait()
            if line is not None:
                self.decode(line)
        if require_success:
            assert code == 0, f"Server exited {code} after EOF: {''.join(self.stderr)[-4096:]}"


def exercise(binary, version):
    client = Client(binary, version)
    try:
        tools = client.request("tools/list", {})["tools"]
        assert {tool["name"] for tool in tools} == TOOLS
        assert all(tool.get("inputSchema") and tool.get("outputSchema") for tool in tools)
        resources = client.request("resources/list", {})["resources"]
        assert {"ak820://server", "ak820://inputs"}.issubset({r["uri"] for r in resources})
        assert client.resource("ak820://server")["access"] == "read-only"
        assert client.resource("ak820://inputs")["inputs"] == []
        catalog = next(r["uri"] for r in resources if r["uri"].startswith("ak820://catalog/"))
        assert len(client.resource(catalog)["profiles"]) == 10
        # rmcp 3.8's bounded transport discards malformed/oversize lines and
        # continues. Verify recovery, not an invented JSON-RPC parse-error reply.
        client.raw('{"unterminated":\n')
        assert len(client.request("tools/list", {})["tools"]) == len(TOOLS)
        client.raw('{"jsonrpc":"2.0","id":-101,"id":-102,"method":"tools/list"}\n')
        client.raw('{"jsonrpc":"2.0","id":-103,"method":"tools/call",'
                   '"params":{"name":"ak820_devices","arguments":{"refresh":false,"refresh":true}}}\n')
        client.raw('{"jsonrpc":"2.0","id":-104,"method":"tools/call",'
                   '"params":{"name":"ak820_devices","arguments":{"refresh":false,"\\u0072efresh":true}}}\n')
        assert len(client.request("tools/list", {}, reject_unrelated=True)["tools"]) == len(TOOLS)
        client.raw("x" * 65537 + "\n")
        assert len(client.request("tools/list", {})["tools"]) == len(TOOLS)
        assert client.request_error("unknown/method", {})["code"] == -32601
        client.notify("notifications/cancelled", {"requestId": 999999, "reason": "contract test"})
        assert len(client.request("tools/list", {})["tools"]) == len(TOOLS)
        client.tool("ak820_nonexistent", {}, "INVALID_ARGUMENT")
        client.tool("ak820_devices", {"refresh": "yes"}, "INVALID_ARGUMENT")
        client.tool("ak820_devices", {"refresh": False, "raw_path": "/forbidden"}, "INVALID_ARGUMENT")
        inventory = client.tool("ak820_devices", {"refresh": True})
        assert [d["device_id"] for d in inventory["data"]["devices"]] == ["mock-1"]
        opened = client.tool("ak820_session", {"action": "open", "device_id": "mock-1", "load": "diagnostics"})
        assert opened["status"] == "accepted"
        operation = opened["data"]["operation_id"]
        deadline = time.monotonic() + 5
        while True:
            state = client.tool("ak820_operation", {"action": "status", "operation_id": operation})["data"]
            if state["state"] != "running":
                assert state["state"] == "completed", state
                opened = state["result"]
                break
            assert time.monotonic() < deadline, "Open operation deadline exceeded"
            time.sleep(0.01)
        session = opened["device_session_id"]
        generation = opened["session_generation"]
        prepare = {"device_session_id": session, "expected_generation": generation, "backup_artifact_id": "unknown", "target_release": "v0.6.0-beta.5", "target_profile": "main", "settings_mode": "defaults", "prerelease_opt_in": True}
        client.tool("ak820_prepare", {**prepare, "path": "forbidden"}, "INVALID_ARGUMENT")
        preparation = client.tool("ak820_prepare", prepare)
        prepare_id = preparation["data"]["operation_id"]
        deadline = time.monotonic() + 5
        while True:
            prepare_state = client.tool("ak820_operation", {"action": "status", "operation_id": prepare_id})["data"]
            if prepare_state["state"] != "running":
                assert prepare_state["state"] == "failed"
                assert prepare_state["result"]["error"]["code"] == "BACKUP_INVALID"
                break
            assert time.monotonic() < deadline, "Prepare operation deadline exceeded"
            time.sleep(0.01)
        client.tool("ak820_backup", {"device_session_id": session, "expected_generation": generation, "path": "forbidden"}, "INVALID_ARGUMENT")
        backup = client.tool("ak820_backup", {"device_session_id": session, "expected_generation": generation})
        backup_id = backup["data"]["operation_id"]
        deadline = time.monotonic() + 5
        while True:
            backup_state = client.tool("ak820_operation", {"action": "status", "operation_id": backup_id})["data"]
            if backup_state["state"] != "running":
                assert backup_state["state"] == "failed"
                assert backup_state["result"]["error"]["code"] == "UNSUPPORTED_FEATURE"
                break
            assert time.monotonic() < deadline, "Backup operation deadline exceeded"
            time.sleep(0.01)
        assert client.tool("ak820_session", {"action": "status", "device_session_id": session})["data"]["state"] == "diagnostic"
        assert client.tool("ak820_read", {"device_session_id": session, "section": "identity"})["image"]["status"] == "synthetic"
        suites = client.tool("ak820_tests", {"action": "list"})["data"]["suites"]
        assert {s["suite_id"] for s in suites} == {"protocol.smoke", "protocol.errors"}
        errors = client.tool("ak820_tests", {"action": "run", "suite_id": "protocol.errors"})["data"]
        assert errors["synthetic"] is True and len(errors["checks"]) >= 10
        assert all(c["status"] == "PASS" for c in errors["checks"])
        smoke = client.tool("ak820_tests", {"action": "run", "suite_id": "protocol.smoke", "device_session_id": session})["data"]
        assert smoke["full_acceptance"] is False
        assert any(c["status"] == "SKIP" for c in smoke["checks"])
        report = client.resource("ak820://report/" + smoke["report_id"])
        assert report["data"]["suite_id"] == "protocol.smoke"
        trace = client.tool("ak820_trace", {"action": "start", "device_session_id": session, "mode": "metadata", "max_bytes": 4096, "max_seconds": 10})["data"]
        trace_id = trace["trace_id"]
        client.tool("ak820_read", {"device_session_id": session, "section": "storage"})
        running = client.tool("ak820_trace", {"action": "status", "trace_id": trace_id})["data"]
        assert running["records"] >= 1 and running["replayable"] is False
        page = client.resource(trace["resource_uri"])
        assert page["records"] and all("payload" not in record for record in page["records"])
        stopped = client.tool("ak820_trace", {"action": "stop", "trace_id": trace_id})["data"]
        assert stopped["state"] == "stopped"
        client.tool("ak820_read", {"device_session_id": session, "section": "connection"})
        assert client.tool("ak820_trace", {"action": "status", "trace_id": trace_id})["data"]["records"] == stopped["records"]
        client.tool("ak820_operation", {"action": "cancel", "operation_id": operation})
        client.tool("ak820_session", {"action": "close", "device_session_id": session})
        assert client.tool("ak820_operation", {"action": "status", "operation_id": operation})["data"]["lease_held"] is False
        client.tool("ak820_read", {"device_session_id": session, "section": "storage"}, "SESSION_STALE")
    finally:
        client.close()


def exercise_hid_metadata(binary, version, default=False):
    """Launch the real backend without requesting discovery or any device access."""
    client = Client(binary, version, backend=None if default else "hid")
    try:
        assert {tool["name"] for tool in client.request("tools/list", {})["tools"]} == TOOLS
        resources = client.request("resources/list", {})["resources"]
        server = client.resource("ak820://server")
        assert server["backend"] == "hid" and server["access"] == "read-only", server
        catalog = next(r["uri"] for r in resources if r["uri"].startswith("ak820://catalog/"))
        assert len(client.resource(catalog)["profiles"]) == 10
        result = client.request("tools/call", {"name": "ak820_tests", "arguments": {"action": "list"}})
        assert not result.get("isError"), result
        value = result["structuredContent"]
        assert value["backend"] == "hid" and value["error"] is None, value
        assert value["verification"]["setters_sent"] == 0
        assert {suite["suite_id"] for suite in value["data"]["suites"]} == {"protocol.smoke"}
    finally:
        client.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--production-binary", type=Path,
                        help="Optional mcp build without the development mock feature")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    for version in VERSIONS:
        exercise(binary, version)
        exercise_hid_metadata(binary, version)
        exercise_hid_metadata(binary, version, default=True)
    for flags in [("--mcp-stdio", "--mcp-backend=mock", "--mcp-backend=hid"),
                  ("--mcp-stdio", "--mcp-access=write"),
                  ("--mcp-stdio", "--mcp-access=read-only", "--mcp-access=read-only"),
                  ("--mcp-stdio", "--mcp-backend=mock", "--mcp-backend=mock"),
                  ("--mcp-backend=mock",),
                  ("--mcp-stdio", "--mcp-stdio"),
                  ("--mcp-stdio", "--mcp-arbitrary=1")]:
        invalid = subprocess.run([str(binary), *flags], input="", capture_output=True, text=True, timeout=5)
        assert invalid.returncode != 0 and not invalid.stdout.strip(), f"Invalid startup accepted or contaminated stdout: {flags}"
    if args.production_binary:
        production = args.production_binary.resolve(strict=True)
        for version in VERSIONS:
            exercise_hid_metadata(production, version)
        rejected = subprocess.run([str(production), "--mcp-stdio", "--mcp-backend=mock"],
                                  input="", capture_output=True, text=True, timeout=5)
        assert rejected.returncode != 0 and not rejected.stdout.strip(), "Production build accepted mock backend or contaminated stdout"
        print("Production MCP metadata launch and mock-backend rejection passed; no hardware calls requested")
    print("MCP stdio synthetic contract and HID metadata-only checks passed for " + ", ".join(VERSIONS))


if __name__ == "__main__":
    main()
