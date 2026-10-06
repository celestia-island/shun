#!/usr/bin/env python3
"""Demo pairing `request` script: mints a display code from local
entropy (no network) and answers the pane's stdin JSON.

This file rides the PAYLOAD (payload = ../examples/demo_payload): the
shell stages the payload's installer/ subtree from the embedded archive
the first time the lane runs, so payload-root-relative paths like this
one execute from any CWD.
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
