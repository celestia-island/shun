#!/usr/bin/env python3
"""Demo pairing `await` script: answers pending with the live remaining
time on the shared demo window (the real gateway parks ~20s per call;
this demo answers at once — the pane's own 1s pacing provides the tick).
An operator "accepts" by creating an accept marker next to this script:
the demo of the control-panel side without a network.
"""
import json
import os
import sys
import time

BASE = os.path.dirname(os.path.abspath(__file__))


def main() -> int:
    answers = json.load(sys.stdin)
    deadline_file = os.path.join(BASE, ".demo_deadline")
    accept_file = os.path.join(BASE, ".demo_accept")
    if os.path.exists(accept_file):
        os.remove(accept_file)
        print(json.dumps({
            "status": "claimed",
            "node_id": answers.get("node_id", "demo"),
            "device_secret": "demo-secret-do-not-ship",
            "owner": "ops@demo.example",
            "pairing_code": answers.get("code", ""),
        }))
        return 0
    # First call for a code arms the demo deadline.
    if not os.path.exists(deadline_file):
        with open(deadline_file, "w") as fh:
            fh.write(str(time.time() + 300))
    with open(deadline_file) as fh:
        remaining = max(0, int(float(fh.read()) - time.time()))
    if remaining <= 0:
        os.remove(deadline_file)
        print(json.dumps({"status": "unknown"}))
        return 0
    print(json.dumps({"status": "pending", "expires_in": remaining}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
