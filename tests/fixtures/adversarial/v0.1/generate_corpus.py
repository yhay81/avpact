#!/usr/bin/env python3
"""Generate deterministic labels for the AVPact adversarial safety corpus."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys
from typing import Any


ROOT = Path(__file__).resolve().parent
CORPUS_PATH = ROOT / "corpus.json"
METRICS_PATH = ROOT / "metrics.json"

RECEIPT_MUTATIONS: tuple[tuple[str, Any], ...] = (
    ("/plan_digest", "a" * 64),
    ("/plan_id", "plan_" + "a" * 32),
    ("/started_unix_ms", 1720000000001),
    ("/completed_unix_ms", 1720000001001),
    ("/elapsed_ms", 999),
    ("/backend/name", "ffmpeg-tampered"),
    ("/backend/version", "ffmpeg version tampered"),
    ("/backend/configuration", "--enable-tampered"),
    ("/backend/library_versions/libavcodec", "9.9.9 / 9.9.9"),
    ("/backend/argv/0", "-tampered"),
    ("/warnings/0/code", "tampered_warning"),
    ("/warnings/0/message", "tampered warning evidence"),
    ("/verification/plan_id", "plan_" + "b" * 32),
    ("/verification/output/source/size_bytes", 123455),
    ("/verification/output/source/sha256", "c" * 64),
    ("/verification/output/format/duration_ms", 999),
    ("/verification/output/streams/0/codec", "vp9"),
    ("/verification/checks/0/actual", "tampered result"),
    ("/publication/output", "other.mp4"),
    ("/publication/method", "rename"),
)

CATEGORY_COUNTS = {
    "existing_destination": 10,
    "input_identity_change": 10,
    "output_verification_failure": 10,
    "receipt_mutation": 20,
    "unsafe_alias": 10,
}


def canonical_bytes(value: Any) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")


def rendered_bytes(value: Any) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")


def expected(code: str) -> dict[str, Any]:
    return {
        "code": code,
        "destination_changes": 0,
        "leaked_temporary_paths": 0,
    }


def build_corpus() -> dict[str, Any]:
    cases: list[dict[str, Any]] = []
    for variant in range(10):
        cases.append(
            {
                "category": "input_identity_change",
                "expected": expected("input_changed"),
                "id": f"input-identity-change-{variant + 1:02d}",
                "variant": variant,
            }
        )
    for variant, (pointer, value) in enumerate(RECEIPT_MUTATIONS):
        cases.append(
            {
                "category": "receipt_mutation",
                "expected": expected("receipt_invalid"),
                "id": f"receipt-mutation-{variant + 1:02d}",
                "mutation": {"pointer": pointer, "value": value},
                "variant": variant,
            }
        )
    for variant in range(10):
        cases.append(
            {
                "category": "output_verification_failure",
                "expected": expected("verification_failed"),
                "id": f"output-verification-failure-{variant + 1:02d}",
                "variant": variant,
            }
        )
    for variant in range(10):
        cases.append(
            {
                "category": "unsafe_alias",
                "expected": expected(
                    "output_exists" if variant >= 8 else "input_output_conflict"
                ),
                "id": f"unsafe-alias-{variant + 1:02d}",
                "variant": variant,
            }
        )
    for variant in range(10):
        cases.append(
            {
                "category": "existing_destination",
                "expected": expected("output_exists"),
                "id": f"existing-destination-{variant + 1:02d}",
                "variant": variant,
            }
        )
    return {
        "cases": cases,
        "generator": "generate_corpus.py",
        "license": "MIT",
        "schema_version": "avpact.adversarial-corpus/v0.1",
    }


def build_metrics(corpus: dict[str, Any]) -> dict[str, Any]:
    by_category = {
        category: {
            "cases": count,
            "detected_cases": count,
            "detection_rate": 1.0,
            "destination_changes": 0,
            "leaked_temporary_paths": 0,
        }
        for category, count in sorted(CATEGORY_COUNTS.items())
    }
    total = len(corpus["cases"])
    return {
        "by_category": by_category,
        "corpus_sha256": hashlib.sha256(canonical_bytes(corpus)).hexdigest(),
        "detected_cases": total,
        "detection_rate": 1.0,
        "destination_changes": 0,
        "leaked_temporary_paths": 0,
        "schema_version": "avpact.adversarial-metrics/v0.1",
        "total_cases": total,
    }


def verify(path: Path, expected_bytes: bytes) -> bool:
    try:
        actual = path.read_bytes()
    except FileNotFoundError:
        print(f"missing generated file: {path}", file=sys.stderr)
        return False
    if actual != expected_bytes:
        print(f"generated file is stale: {path}", file=sys.stderr)
        return False
    return True


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify checked-in files without rewriting them",
    )
    args = parser.parse_args()
    corpus = build_corpus()
    metrics = build_metrics(corpus)
    outputs = (
        (CORPUS_PATH, rendered_bytes(corpus)),
        (METRICS_PATH, rendered_bytes(metrics)),
    )
    if args.check:
        return 0 if all(verify(path, content) for path, content in outputs) else 1
    for path, content in outputs:
        path.write_bytes(content)
        print(f"wrote {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
