#!/usr/bin/env python3
"""Validate ACCESS schemas and checked-in configuration instances."""

from __future__ import annotations

import json
from pathlib import Path

from jsonschema import Draft202012Validator, FormatChecker


ROOT = Path(__file__).resolve().parents[1]
SCHEMA_DIR = ROOT / "schemas"
VALIDATION_PAIRS = (
    (
        SCHEMA_DIR / "access-protocol-profile.schema.json",
        ROOT / "config/access/access-protocol-profile.json",
    ),
    (
        SCHEMA_DIR / "access-authorization-policy-bundle.schema.json",
        ROOT / "config/access/access-authorization-policy-bundle.json",
    ),
    (
        SCHEMA_DIR / "client-trust-bundle.schema.json",
        ROOT / "config/access/simulation-client-trust-bundle.json",
    ),
)


def load_json(path: Path) -> object:
    with path.open(encoding="utf-8") as stream:
        return json.load(stream)


def validate_schema(path: Path) -> None:
    Draft202012Validator.check_schema(load_json(path))


def validate_instance(schema_path: Path, instance_path: Path) -> None:
    schema = load_json(schema_path)
    validator = Draft202012Validator(schema, format_checker=FormatChecker())
    errors = sorted(validator.iter_errors(load_json(instance_path)), key=lambda error: list(error.path))
    if errors:
        details = "\n".join(
            f"  {instance_path.relative_to(ROOT)}:{'/'.join(map(str, error.path)) or '<root>'}: {error.message}"
            for error in errors
        )
        raise ValueError(f"configuration validation failed:\n{details}")


def validate_policy_source() -> None:
    manifest_path = ROOT / "config/access/access-authorization-policy-bundle.json"
    manifest = load_json(manifest_path)
    source_path = ROOT / manifest["source_file"]
    if not source_path.is_file():
        raise ValueError(
            f"authorization policy source does not exist: {source_path.relative_to(ROOT)}"
        )


def main() -> None:
    schema_paths = sorted(SCHEMA_DIR.glob("*.schema.json"))
    if not schema_paths:
        raise ValueError("no ACCESS schemas found")
    for schema_path in schema_paths:
        validate_schema(schema_path)
    for schema_path, instance_path in VALIDATION_PAIRS:
        validate_instance(schema_path, instance_path)
    validate_policy_source()
    print(
        f"validated {len(schema_paths)} schemas and "
        f"{len(VALIDATION_PAIRS)} configuration instances"
    )


if __name__ == "__main__":
    main()
