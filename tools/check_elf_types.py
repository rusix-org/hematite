#!/usr/bin/env python3
"""Compile actual ELF-plan clients and reject forged metadata or escaped borrows."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

from check_frame_types import run
from check_mapping_types import rejected_only_for

PREAMBLE = '''#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::elf::{Error, Image, Permissions, Segment, MAX_LOAD_SEGMENTS};
'''
PROBES = {
    "valid_parse_use": ('''pub fn client(input: &[u8]) -> Result<(u64, usize), Error> {
        let image = Image::parse(input)?;
        for segment in image.segments() {
            let _ = (segment.virtual_address(), segment.memory_size(),
                segment.file_bytes(), segment.page_start(), segment.page_end(),
                segment.permissions().readable(), segment.permissions().writable(),
                segment.permissions().executable());
        }
        Ok((image.entry(), image.page_count()))
    }''', None),
    "valid_segment_copy": ('''pub fn client(input: &[u8]) -> Result<Segment<'_>, Error> {
        let image = Image::parse(input)?;
        Ok(image.segments()[0])
    }''', None),
    "valid_bytes_after_image_drop": ('''pub fn client(input: &[u8]) -> Result<&[u8], Error> {
        let image = Image::parse(input)?;
        let bytes = image.segments()[0].file_bytes();
        drop(image);
        Ok(bytes)
    }''', None),
    "forge_image": ('''pub fn client(input: &[u8]) {
        let image = Image::parse(input).unwrap();
        let segment = image.segments()[0];
        let _ = Image { entry: 0, segments: [segment; MAX_LOAD_SEGMENTS], count: 1, pages: 1 };
    }''', "E0451"),
    "forge_segment": ('''pub fn client(input: &[u8]) {
        let _ = Segment { virtual_address: 0, memory_size: 1, file_bytes: input,
            permissions: Permissions::ReadExecute, page_start: 0, page_end: 4096 };
    }''', "E0451"),
    "private_image_entry": ('''pub fn client(mut image: Image<'_>) {
        image.entry = 0;
    }''', "E0616"),
    "private_segment_permissions": ('''pub fn client(mut segment: Segment<'_>) {
        segment.permissions = Permissions::ReadWrite;
    }''', "E0616"),
    "image_outlive_input": ('''pub fn client() -> Image<'static> {
        let input = [0u8; 4096];
        Image::parse(&input).unwrap()
    }''', "E0515"),
    "mutate_input_while_image_live": ('''pub fn client(input: &mut [u8]) -> u64 {
        let image = Image::parse(input).unwrap();
        input[0] = 0;
        image.entry()
    }''', "E0506"),
    "file_bytes_outlive_input": ('''pub fn client() -> &'static [u8] {
        let input = [0u8; 4096];
        let image = Image::parse(&input).unwrap();
        image.segments()[0].file_bytes()
    }''', "E0515"),
    "mutate_file_bytes": ('''pub fn client(image: &Image<'_>) {
        let bytes = image.segments()[0].file_bytes();
        bytes[0] = 0;
    }''', "E0594"),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--library", type=Path, required=True)
    parser.add_argument("--target", choices=["aarch64-unknown-none", "x86_64-unknown-none"], required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    library, output = args.library.resolve(), args.output.resolve()
    if not library.is_file():
        parser.error("build the hm-kernel library first")
    if output.exists() and any(output.iterdir()):
        parser.error("use a fresh output directory to preserve earlier results")
    output.mkdir(parents=True, exist_ok=True)
    (output / "input.json").write_text(json.dumps({
        "library": str(library), "sha256": hashlib.sha256(library.read_bytes()).hexdigest(),
        "target": args.target, "expected_probes": list(PROBES),
    }, indent=2) + "\n")
    results = {}
    for name, (body, rejection) in PROBES.items():
        source, metadata = output / f"{name}.rs", output / f"{name}.rmeta"
        source.write_text(PREAMBLE + body + "\n")
        command = ["rustc", "--edition=2024", "--crate-type=lib", "--emit=metadata",
                   "--error-format=json", "--target", args.target, "-C", "panic=abort",
                   "--extern", f"hm_kernel={library}", "-L", f"dependency={library.parent / 'deps'}",
                   "-o", str(metadata), str(source)]
        code, diagnostics = run(command, output, name)
        passed = (code == 0 and metadata.is_file()) if rejection is None else (
            code is not None and code != 0 and rejected_only_for(diagnostics, rejection))
        results[name] = {"passed": passed, "returncode": code, "expected_rejection": rejection}
    passed = len(results) == len(PROBES) and all(result["passed"] for result in results.values())
    result = {"passed": passed, "scope": "actual safe ELF metadata and input lifetime boundaries; no execution or isolation claim",
              "target": args.target, "probes": results}
    (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
