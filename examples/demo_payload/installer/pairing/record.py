#!/usr/bin/env python3
"""Demo pairing `record` script: persists the claim next to itself —
the scripts lane's own credential landing (the demo's answer to the
gateway lane's env-file)."""
import json
import os
import sys

BASE = os.path.dirname(os.path.abspath(__file__))


def main() -> int:
    claim = json.load(sys.stdin)
    with open(os.path.join(BASE, "pairing.json"), "w") as fh:
        json.dump(claim, fh, indent=2)
    return 0


if __name__ == "__main__":
    sys.exit(main())
