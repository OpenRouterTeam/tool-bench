#!/usr/bin/env python3
"""Command-line MCP client for one pilot slot. Set PILOT_SLOT to the slot number.

  mcp.py servers                                  the MCP servers
  mcp.py tools <server> [--grep REGEX]            tool names, one line each
  mcp.py schema <server> <tool>                   full description and input schema
  mcp.py call <server> <tool> '<json arguments>'  call a tool (arguments: JSON object, @file or -)

Exit status: 0 success, 1 the tool reported an error, 2 usage, HTTP or protocol error.
"""

import argparse
import difflib
import fcntl
import json
import os
import random
import re
import sys
import urllib.error
import urllib.request

SLOTS = "/tmp/tb-pilot/slots"
PROTOCOL = "2025-06-18"
SUMMARY_WIDTH = 120
BASE_HEADERS = {"Content-Type": "application/json", "Accept": "application/json, text/event-stream"}


class Failure(Exception):
    """An error to report plainly, without a traceback."""


def slot_directory():
    slot = os.environ.get("PILOT_SLOT", "")
    if not slot.isdigit():
        raise Failure("set PILOT_SLOT to your slot number, for example: export PILOT_SLOT=3")
    return "%s/%d" % (SLOTS, int(slot))


def servers():
    try:
        with open(slot_directory() + "/servers.json") as handle:
            return json.load(handle)["servers"]
    except FileNotFoundError:
        raise Failure("slot %s is not running" % os.environ["PILOT_SLOT"])


def events(response):
    """Yields the JSON message of each server-sent event as it arrives."""
    data = []
    for raw in response:
        line = raw.decode("utf-8", "replace").rstrip("\r\n")
        if line.startswith("data:"):
            data.append(line[5:][1:] if line[5:].startswith(" ") else line[5:])
        elif not line and data:
            yield "\n".join(data)
            data = []
    if data:
        yield "\n".join(data)


class SessionLost(Failure):
    """The server no longer knows the session id that was sent."""


class Session:
    """One MCP session per server over streamable HTTP, kept for the life of the slot.

    An agent harness holds a single session with each server for a whole trial, and servers
    may keep state in it, so the session id is saved between invocations of this program.
    """

    def __init__(self, server, timeout):
        self.name, self.url, self.timeout = server["name"], server["url"], timeout
        self.saved = "%s/sessions/%s.json" % (slot_directory(), self.name)
        # Never route loopback traffic through a system or environment proxy.
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        # Invocations share one session, so request ids must not repeat between them
        # (ids stay below 2**31 for servers that parse them as 32-bit integers).
        self.last_id = random.randrange(1, 2**30)
        self.reopened = False
        if not self.resume():
            self.open()

    def open(self, lost=None):
        """Starts a session, unless a concurrent invocation has just saved a usable one."""
        try:
            os.makedirs(os.path.dirname(self.saved), exist_ok=True)
            lock = open(self.saved + ".lock", "w")
        except OSError:
            return self.initialize()
        with lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            if not self.resume() or self.headers.get("Mcp-Session-Id") == lost:
                self.initialize()

    def resume(self):
        try:
            with open(self.saved) as handle:
                saved = json.load(handle)
            self.headers = dict(BASE_HEADERS, **saved["headers"])
            self.instructions = saved["instructions"]
        except (OSError, ValueError, KeyError, TypeError):
            return False
        return "Mcp-Session-Id" in self.headers

    def initialize(self):
        self.headers = dict(BASE_HEADERS)
        result = self.exchange(
            "initialize",
            {
                "protocolVersion": PROTOCOL,
                "capabilities": {},
                "clientInfo": {"name": "pilot-cli", "version": "1"},
            },
        )
        self.instructions = result.get("instructions") or ""
        version = result.get("protocolVersion")
        if isinstance(version, str):
            self.headers["MCP-Protocol-Version"] = version
        self.post({"jsonrpc": "2.0", "method": "notifications/initialized"}, None)
        kept = {name: value for name, value in self.headers.items() if name not in BASE_HEADERS}
        try:
            temporary = "%s.%d" % (self.saved, os.getpid())
            with open(temporary, "w") as handle:
                json.dump({"headers": kept, "instructions": self.instructions}, handle)
            os.replace(temporary, self.saved)
        except OSError:
            pass  # The session still works for this invocation.

    def post(self, payload, want):
        """Sends one message; returns the response whose id is `want` (None: a notification)."""
        request = urllib.request.Request(
            self.url, data=json.dumps(payload).encode(), headers=self.headers, method="POST"
        )
        what = "%s (%s)" % (self.name, payload["method"])
        try:
            with self.opener.open(request, timeout=self.timeout) as response:
                session = response.headers.get("Mcp-Session-Id")
                if session:
                    self.headers["Mcp-Session-Id"] = session
                if want is None:
                    return None
                if response.headers.get_content_type() == "text/event-stream":
                    bodies = events(response)
                else:
                    bodies = [response.read().decode("utf-8", "replace")]
                for body in bodies:
                    try:
                        parsed = json.loads(body)
                    except ValueError:
                        raise Failure(
                            "protocol error from %s: the response is not JSON: %s"
                            % (what, body[:500] or "(empty body)")
                        )
                    for message in parsed if isinstance(parsed, list) else [parsed]:
                        if (
                            isinstance(message, dict)
                            and message.get("id") == want
                            and ("result" in message or "error" in message)
                        ):
                            return message
        except urllib.error.HTTPError as error:
            body = error.read().decode("utf-8", "replace").strip()
            raise (SessionLost if error.code == 404 and "Mcp-Session-Id" in self.headers else Failure)(
                "HTTP %s %s from %s: %s" % (error.code, error.reason, what, body[:2000] or "(empty body)")
            )
        except urllib.error.URLError as error:
            raise Failure("cannot reach %s at %s: %s" % (self.name, self.url, error.reason))
        except OSError as error:
            raise Failure("connection to %s failed: %s" % (what, error or type(error).__name__))
        raise Failure("protocol error from %s: the server sent no response to the request" % what)

    def request(self, method, params=None):
        try:
            return self.exchange(method, params)
        except SessionLost:
            if self.reopened:
                raise
        # The saved session is gone (the servers restarted); open a new one and send again.
        self.reopened = True
        self.open(lost=self.headers.get("Mcp-Session-Id"))
        return self.exchange(method, params)

    def exchange(self, method, params=None):
        self.last_id += 1
        payload = {"jsonrpc": "2.0", "id": self.last_id, "method": method}
        if params is not None:
            payload["params"] = params
        message = self.post(payload, self.last_id)
        if "error" in message:
            error = message["error"] if isinstance(message["error"], dict) else {"message": message["error"]}
            detail = "MCP error %s from %s (%s): %s" % (
                error.get("code", "?"), self.name, method, error.get("message", ""))
            if error.get("data") is not None:
                data = error["data"]
                detail += "\n" + (data if isinstance(data, str) else json.dumps(data, indent=2))
            raise Failure(detail)
        if not isinstance(message["result"], dict):
            raise Failure("protocol error from %s (%s): the result is not an object" % (self.name, method))
        return message["result"]

    def tools(self):
        found, cursor = [], None
        while True:
            result = self.request("tools/list", {"cursor": cursor} if cursor else None)
            found.extend(result.get("tools") or [])
            cursor = result.get("nextCursor")
            if not cursor:
                return found


def connect(name, timeout):
    known = servers()
    for server in known:
        if server["name"] == name:
            return Session(server, timeout)
    raise Failure("unknown server %r; the servers are: %s" % (name, ", ".join(s["name"] for s in known)))


def summary(description):
    for line in (description or "").splitlines():
        line = " ".join(line.split())
        if line:
            return line if len(line) <= SUMMARY_WIDTH else line[: SUMMARY_WIDTH - 1].rstrip() + "…"
    return "(no description)"


def command_servers(arguments):
    for server in servers():
        print(server["name"])


def command_tools(arguments):
    session = connect(arguments.server, arguments.timeout)
    if arguments.instructions:
        print(session.instructions.strip() or "(this server provides no instructions)")
        return
    tools = session.tools()
    shown = tools
    if arguments.grep:
        try:
            pattern = re.compile(arguments.grep, re.IGNORECASE)
        except re.error as error:
            raise Failure("invalid --grep pattern: %s" % error)
        shown = [
            tool for tool in tools
            if pattern.search(tool.get("name", "")) or pattern.search(tool.get("description") or "")
        ]
    # A harness shows a server's instructions to its agent up front; here they are one command away.
    if session.instructions.strip():
        print("NOTE: this server gives usage instructions (%d characters). Read them with: tools %s --instructions\n"
              % (len(session.instructions.strip()), arguments.server))
    for tool in shown:
        print("%s: %s" % (tool.get("name"), summary(tool.get("description"))))
    count = "%d tools" % len(tools) if shown is tools else "%d of %d tools match" % (len(shown), len(tools))
    print("\n%s. Details: schema %s <tool>" % (count, arguments.server))


def command_schema(arguments):
    tools = connect(arguments.server, arguments.timeout).tools()
    for tool in tools:
        if tool.get("name") == arguments.tool:
            break
    else:
        names = [tool.get("name", "") for tool in tools]
        close = difflib.get_close_matches(arguments.tool, names, n=5, cutoff=0.5)
        hint = "; similar: " + ", ".join(close) if close else ""
        raise Failure("server %r has no tool %r%s" % (arguments.server, arguments.tool, hint))
    print("tool: %s" % tool.get("name"))
    if tool.get("title"):
        print("title: %s" % tool["title"])
    if tool.get("annotations"):
        print("annotations: %s" % json.dumps(tool["annotations"], ensure_ascii=False))
    print("\ndescription:\n%s" % ((tool.get("description") or "").strip() or "(none)"))
    print("\ninput schema:\n%s" % json.dumps(tool.get("inputSchema", {}), indent=2, ensure_ascii=False))


def call_arguments(text):
    if text == "-":
        text = sys.stdin.read()
    elif text.startswith("@"):
        try:
            with open(text[1:]) as handle:
                text = handle.read()
        except OSError as error:
            raise Failure("cannot read the arguments file: %s" % error)
    try:
        value = json.loads(text)
    except ValueError as error:
        raise Failure("the arguments are not valid JSON (%s); pass one JSON object such as "
                      "'{\"query\": \"x\"}', @file, or - for standard input" % error)
    if not isinstance(value, dict):
        raise Failure("the arguments must be a JSON object, not %s" % type(value).__name__)
    return value


def repeats(structured, texts):
    """True when the structured content only repeats what a text part already shows."""
    for text in texts:
        if structured == {"result": text}:
            return True
        try:
            parsed = json.loads(text)
        except ValueError:
            continue
        if structured == parsed or structured == {"result": parsed}:
            return True
    return False


def render(result):
    parts, texts = [], []
    for part in result.get("content") or []:
        kind = part.get("type") if isinstance(part, dict) else None
        if kind == "text":
            texts.append(part.get("text", ""))
            parts.append(part.get("text", ""))
        elif kind == "resource" and isinstance(part.get("resource"), dict):
            resource = part["resource"]
            if "text" in resource:
                parts.append("[resource %s]\n%s" % (resource.get("uri", ""), resource["text"]))
            else:
                parts.append("[resource %s, %s, %d base64 characters not shown]" % (
                    resource.get("uri", ""), resource.get("mimeType", "unknown type"),
                    len(resource.get("blob", ""))))
        elif kind == "resource_link":
            parts.append("[resource link %s %s]" % (part.get("uri", ""), part.get("name", "")))
        elif kind in ("image", "audio"):
            parts.append("[%s, %s, %d base64 characters not shown]" % (
                kind, part.get("mimeType", "unknown type"), len(part.get("data", ""))))
        else:
            parts.append(json.dumps(part, ensure_ascii=False))
    structured = result.get("structuredContent")
    if structured is not None and not repeats(structured, texts):
        parts.append("structured content:\n" + json.dumps(structured, indent=2, ensure_ascii=False))
    return "\n".join(parts) if parts else "(the result has no content)"


def command_call(arguments):
    values = call_arguments(arguments.arguments)
    session = connect(arguments.server, arguments.timeout)
    result = session.request("tools/call", {"name": arguments.tool, "arguments": values})
    failed = result.get("isError") is True
    if arguments.raw:
        print(json.dumps(result, ensure_ascii=False))
    else:
        if failed:
            print("TOOL ERROR: %s.%s reported isError=true. Its message follows." % (arguments.server, arguments.tool))
        print(render(result))
    return 1 if failed else 0


def main():
    parser = argparse.ArgumentParser(
        prog="mcp.py", description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--timeout", type=float, default=120.0, help="seconds to wait for each response (default 120)")
    commands = parser.add_subparsers(dest="command", metavar="{servers,tools,schema,call}")
    commands.required = True
    commands.add_parser("servers", help="list the MCP servers").set_defaults(run=command_servers)
    tools = commands.add_parser("tools", help="list a server's tools, one line each")
    tools.add_argument("server")
    tools.add_argument("--grep", metavar="REGEX", help="only tools whose name or description matches (case-insensitive)")
    tools.add_argument("--instructions", action="store_true", help="print the server's own usage instructions instead")
    tools.set_defaults(run=command_tools)
    schema = commands.add_parser("schema", help="print a tool's full description and input schema")
    schema.add_argument("server")
    schema.add_argument("tool")
    schema.set_defaults(run=command_schema)
    call = commands.add_parser("call", help="call a tool")
    call.add_argument("server")
    call.add_argument("tool")
    call.add_argument("arguments", nargs="?", default="{}", help="a JSON object, @file, or - for standard input (default {})")
    call.add_argument("--raw", action="store_true", help="print the whole result object as JSON")
    call.set_defaults(run=command_call)
    arguments = parser.parse_args()
    for stream in (sys.stdout, sys.stderr):
        stream.reconfigure(encoding="utf-8", errors="replace")
    try:
        status = arguments.run(arguments) or 0
        sys.stdout.flush()
    except Failure as failure:
        sys.stdout.flush()
        print("ERROR: %s" % failure, file=sys.stderr)
        status = 2
    except BrokenPipeError:
        os.dup2(os.open(os.devnull, os.O_WRONLY), sys.stdout.fileno())
        status = 0
    except KeyboardInterrupt:
        status = 130
    sys.exit(status)


if __name__ == "__main__":
    main()
