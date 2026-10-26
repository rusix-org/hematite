#!/usr/bin/env python3
"""Check Phase 0 record completeness, without claiming approval or boot success."""
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
REQUIRED = (
    "AGENTS.md", "PROGRESS.md", "README.md",
    "docs/adr/0001-project-direction.md", "docs/adr/0002-license.md",
    "docs/adr/0003-name.md", "docs/adr/0004-core-architecture.md",
)


def main():
    errors = []
    for name in REQUIRED:
        path = ROOT / name
        if not path.is_file() or not path.read_text().strip():
            errors.append(f"Missing or empty required record: {name}")
            continue
        if name.startswith("docs/adr/"):
            body = path.read_text()
            if "status" not in body.lower():
                errors.append(f"Missing decision status: {name}")
            if not body.startswith("# "):
                errors.append(f"Missing title: {name}")
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("PASS: Phase 0 records present; approval and runtime gates not assessed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
