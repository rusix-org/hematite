#!/usr/bin/env python3
"""Check bootstrap trust-boundary hygiene; Rust and review establish enforcement.

This deliberately checks the dependency-free workspace policy. It is not a Rust
parser or a soundness audit. Rust's forbid lint is authoritative when compiling
safe crate roots; nearby SAFETY comments are only a review aid.
"""

from pathlib import Path
import re
import sys
import tomllib


ROOT = Path(__file__).resolve().parents[1]
UNSAFE = re.compile(r"\bunsafe\s*(?:\{|fn\b|extern\b|impl\b|trait\b|\()|\bglobal_asm\s*!")
FORBID = re.compile(r"#!\[\s*forbid\s*\(\s*unsafe_code\s*\)\s*\]")


def leading_forbid(text):
    """Require a real, standalone inner lint in the crate's attribute preamble."""
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("//"):
            continue
        if FORBID.fullmatch(line):
            return True
        if line.startswith("#![") and line.endswith("]"):
            continue
        return False
    return False


def crate_roots(directory, manifest):
    roots = set()
    if "lib" in manifest:
        roots.add(directory / manifest["lib"].get("path", "src/lib.rs"))
    elif (directory / "src/lib.rs").exists():
        roots.add(directory / "src/lib.rs")
    for kind, folder in (("bin", "src/bin"), ("test", "tests"),
                         ("example", "examples"), ("bench", "benches")):
        for target in manifest.get(kind, []):
            if "path" not in target:
                raise ValueError(f"{directory.name}: explicit {kind} target needs an explicit path")
            roots.add(directory / target["path"])
        automatic = manifest["package"].get("auto" + kind + "s", True)
        if automatic:
            base = directory / folder
            roots.update(base.glob("*.rs"))
            roots.update(base.glob("*/main.rs"))
    if manifest["package"].get("autobins", True) and (directory / "src/main.rs").exists():
        roots.add(directory / "src/main.rs")
    return roots


def safety_comments(path):
    errors = []
    lines = path.read_text().splitlines()
    # Search code lines, not prose in ordinary // comments. This check is only
    # comment hygiene; compiler lints and independent review remain mandatory.
    code = "\n".join(line.split("//", 1)[0] for line in lines)
    for match in UNSAFE.finditer(code):
        index = code.count("\n", 0, match.start())
        comments = []
        for previous in reversed(lines[max(0, index - 6):index]):
            previous = previous.strip()
            if not previous:
                continue
            if not previous.startswith("//"):
                break
            comments.append(previous)
        if not any(re.search(r"//\s*SAFETY(?::| contract:)\s*\S.{9,}", c)
                   for c in comments):
            errors.append(f"{path.relative_to(ROOT)}:{index + 1}: missing nearby explanatory SAFETY comment")
    return errors


def check():
    errors = []
    workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]
    members = workspace.get("members", [])
    if not members:
        return ["Workspace has no members"]
    names = set()
    for member in members:
        if any(char in member for char in "*?["):
            errors.append(f"Workspace members must be explicit paths: {member}")
            continue
        directory = (ROOT / member).resolve()
        if not directory.is_relative_to(ROOT) or not directory.is_dir():
            errors.append(f"Missing or external workspace member: {member}")
            continue
        manifest = tomllib.loads((directory / "Cargo.toml").read_text())
        name = manifest["package"]["name"]
        if name in names:
            errors.append(f"Duplicate package name: {name}")
        names.add(name)
        roots = crate_roots(directory, manifest)
        if not roots:
            errors.append(f"{member}: no crate roots found")
        for path in roots:
            if not path.resolve().is_relative_to(directory) or not path.is_file():
                errors.append(f"{member}: missing or external crate root {path}")
            elif name != "hm-core" and not leading_forbid(path.read_text()):
                errors.append(f"{path.relative_to(ROOT)}: require #![forbid(unsafe_code)] in crate preamble")
        build = manifest["package"].get("build")
        if name != "hm-core" and ((directory / "build.rs").exists() or build not in (None, False)):
            errors.append(f"{member}: build scripts outside hm-core are not permitted")
        if name == "hm-core":
            for path in directory.rglob("*.rs"):
                errors.extend(safety_comments(path))
    if "hm-core" not in names:
        errors.append("Trusted hm-core workspace member is missing")
    packages = tomllib.loads((ROOT / "Cargo.lock").read_text()).get("package", [])
    locked_names = {package["name"] for package in packages}
    if locked_names != names or len(packages) != len(names):
        errors.append("Cargo.lock must contain exactly the workspace packages; dependency review required")
    if any("source" in package for package in packages):
        errors.append("External dependencies are not approved for this bootstrap")
    return errors


def main():
    try:
        errors = check()
    except (OSError, KeyError, TypeError, ValueError) as error:
        errors = [f"Cannot validate workspace: {error}"]
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("PASS: safe crate roots, dependency-free lockfile, core SAFETY-comment hygiene")
    print("Scope: structural hygiene only; compile all targets and review core soundness separately")
    return 0


if __name__ == "__main__":
    sys.exit(main())
