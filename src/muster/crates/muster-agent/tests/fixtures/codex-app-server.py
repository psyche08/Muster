#!/usr/bin/env python3
"""Deterministic `codex app-server` fixture for composer predictions; never
runs a model. The source turn id picks the scenario: `fail` ends the turn
failed, `hang` never answers, any other turn answers a suggestion after the
client has declined an approval request."""
import json
import sys

if sys.argv[1:] != ["app-server", "-c", "mcp_servers={}"]:
    sys.exit(2)


def send(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()


def notify(method, params):
    send({"jsonrpc": "2.0", "method": method, "params": params})


def answer():
    notify("item/completed", {"threadId": "other-thread", "item": {"type": "agentMessage", "text": "{\"suggestion\":\"wrong thread\"}"}})
    notify("item/completed", {"threadId": "fork-1", "item": {"type": "agentMessage", "text": "{\"suggestion\":\"Commit and install it\"}"}})
    notify("turn/completed", {"threadId": "fork-1", "turn": {"id": "prediction-turn", "status": "completed"}})


for line in sys.stdin:
    message = json.loads(line)
    method, params = message.get("method"), message.get("params") or {}
    if method == "initialize":
        assert params["capabilities"]["experimentalApi"] is True
        notify("remoteControl/status/changed", {})
        send({"jsonrpc": "2.0", "id": message["id"], "result": {}})
    elif method == "thread/fork":
        assert params["threadId"] == "source-thread"
        assert params["path"] == "/tmp/rollout.jsonl"
        assert params["ephemeral"] is True and params["permissions"] == ":read-only"
        assert params["approvalPolicy"] == "never"
        send({"jsonrpc": "2.0", "id": message["id"], "result": {"thread": {"id": "fork-1", "ephemeral": True}}})
        notify("thread/started", {"thread": {"id": "fork-1"}})
    elif method == "turn/start":
        assert params["threadId"] == "fork-1"
        assert "suggestion" in params["input"][0]["text"]
        turn = params["responsesapiClientMetadata"]["prediction_source_turn_id"]
        send({"jsonrpc": "2.0", "id": message["id"], "result": {"turn": {"id": "prediction-turn", "status": "inProgress"}}})
        if turn == "fail":
            notify("turn/completed", {"threadId": "fork-1", "turn": {"id": "prediction-turn", "status": "failed"}})
        elif turn != "hang":
            send({"jsonrpc": "2.0", "id": "approval-1", "method": "item/commandExecution/requestApproval", "params": {"threadId": "fork-1"}})
    elif message.get("id") == "approval-1":
        assert "error" in message
        answer()
sys.exit(0)
