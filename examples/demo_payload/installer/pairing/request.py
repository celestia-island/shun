#!/usr/bin/env python3
"""Demo pairing `request` script: mints a display code from local
entropy (no network) and answers the pane's stdin JSON.

This file rides the PAYLOAD (payload = ../examples/demo_payload), so it
exists on disk inside the install dir at pairing time — the scripts-lane
path resolution story this demo establishes: config-relative paths are
resolved against the payload's installer/ prefix at build time.
"""
import json
import os
import secrets
import sys


def main() -> int:
    answers = json.load(sys.stdin)
    # The node identity rides back as `echo` so the operator can see the
    # pane's stdin answers arrived (the code itself is random).
    seed = answers.get("node_id", "demo") + answers.get("name", "")
    alphabet = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789"
    code = "".join(secrets.choice(alphabet) for _ in range(8))
    print(json.dumps({"code": code, "expires_in": 300, "echo": seed[:16]}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
