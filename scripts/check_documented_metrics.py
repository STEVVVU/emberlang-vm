#!/usr/bin/env python3
"""Fail if the deterministic benchmark counters drift, or if the prose disagrees.

`benchmarks/results-windows-x86_64.json` is the committed evidence artifact, and
it is the single source of truth for every figure this repo publishes. This script
is the only place that reads it and compares it against everything else, so a
number exists in exactly one file.

Three distinct checks, in increasing order of what they can catch:

1. **The artifact agrees with itself.** Its top-level deterministic counters must
   equal the `run_to_run_spread.identical_in_every_run` block, which is the claim
   that those fields did not move across 32 runs. If a regenerated artifact
   updated one and not the other, the honesty claim would be quietly false.

2. **The prose agrees with the artifact.** `README.md` and `benchmarks/README.md`
   quote 336 / 402 / 722 / 5,000,400 / 8,441 / 81 in tables and in running text.
   Regenerating the artifact and forgetting the prose leaves both files
   self-consistent and the documentation wrong -- the failure mode no
   `git diff --exit-code` can see.

3. **A fresh run reproduces the artifact.** Pass a benchmark run's JSON as an
   argument and its deterministic counters are compared against the committed
   ones. This is what CI does; it replaces four constants that used to be
   hard-coded a second time inside the workflow.

Timings are deliberately NOT asserted anywhere here. The measurement box is a
power-managed laptop and the observed spread reaches 12.2x on p95, so a timing
gate would be a flake generator rather than a guard. See `benchmarks/README.md`
for why only the counters are held fixed.

Usage:
    python scripts/check_documented_metrics.py                  # artifact + prose
    python scripts/check_documented_metrics.py fresh-run.json   # also compare a run
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
ARTIFACT = REPO_ROOT / "benchmarks" / "results-windows-x86_64.json"

# Every field the benchmark bin is contracted to emit. A refactor that renames or
# drops one should fail loudly here rather than silently publish a smaller doc.
REQUIRED_FIELDS: tuple[str, ...] = (
    "schema",
    "profile",
    "source_bytes",
    "compile_iterations",
    "compile_min_us",
    "compile_median_us",
    "compile_p95_us",
    "bytecode_instructions",
    "constants_folded",
    "dead_instructions_removed",
    "vm_total_steps",
    "vm_steps_per_second",
)

# Properties of the compiler, not of the clock. These are the fields the artifact
# claims were byte-identical in all 32 runs, so they are exactly the fields a
# fresh run must reproduce.
DETERMINISTIC_FIELDS: tuple[str, ...] = (
    "bytecode_instructions",
    "constants_folded",
    "dead_instructions_removed",
    "vm_total_steps",
    "source_bytes",
    "source_functions",
)

# (artifact field, documenting file, the exact substring the prose uses).
#
# The substrings include the surrounding words on purpose: matching a bare "81"
# would hit "1.81" or a line number, so each claim is anchored to the sentence
# that makes it. Values are formatted from the artifact, never written out here,
# which is what keeps the artifact authoritative.
PROSE_CLAIMS: tuple[tuple[str, str, str], ...] = (
    ("constants_folded", "README.md", "| Constants folded per compile | {:,} |"),
    ("dead_instructions_removed", "README.md", "| Dead instructions removed per compile | {:,} |"),
    ("bytecode_instructions", "README.md", "| Resulting bytecode instructions | {:,} |"),
    ("vm_total_steps", "README.md", "| VM throughput ({:,} instructions) |"),
    ("source_bytes", "README.md", "deterministic {:,}-byte,"),
    ("source_functions", "README.md", "{:,}-function input."),
    ("constants_folded", "README.md", "folded {:,} constants"),
    ("dead_instructions_removed", "README.md", "removed {:,} instructions"),
    ("bytecode_instructions", "README.md", "producing {:,} bytecode instructions"),
    ("constants_folded", "benchmarks/README.md", "({:,} constants folded"),
    ("dead_instructions_removed", "benchmarks/README.md", "{:,} dead instructions removed)"),
    ("bytecode_instructions", "benchmarks/README.md", "({:,} instructions)"),
    ("vm_total_steps", "benchmarks/README.md", "({:,}) were **byte-identical"),
    ("source_bytes", "benchmarks/README.md", "deterministic {:,}-byte program"),
)


ENCODING_HINT = (
    "If this was produced by a shell redirect on Windows, PowerShell 5.1 writes "
    "UTF-16LE (and `Out-File`/`Tee-Object` add a BOM); see benchmarks/README.md "
    "for the documented command."
)


def load_json(path: Path) -> dict[str, object]:
    """Reads a JSON document, reporting a parse failure against its path.

    A UTF-16 artifact is a real hazard on Windows: `benchmark > results.json` in
    PowerShell 5.1 writes UTF-16LE. That is checked for by inspecting the bytes
    rather than by catching a decode error, because UTF-16LE ASCII decodes
    *successfully* as UTF-8 -- every other byte is a NUL, which is valid UTF-8 --
    so it surfaces as a baffling JSONDecodeError about column 2 instead. Name the
    cause rather than the symptom.
    """

    raw = path.read_bytes()
    if raw.startswith((b"\xff\xfe", b"\xfe\xff")) or b"\x00" in raw[:512]:
        raise SystemExit(f"{path}: looks like UTF-16, not UTF-8. {ENCODING_HINT}")
    if raw.startswith(b"\xef\xbb\xbf"):
        raise SystemExit(f"{path}: starts with a UTF-8 BOM. {ENCODING_HINT}")

    try:
        return json.loads(raw.decode("utf-8"))
    except UnicodeDecodeError as error:
        raise SystemExit(f"{path}: not valid UTF-8 ({error}). {ENCODING_HINT}") from error
    except json.JSONDecodeError as error:
        raise SystemExit(f"{path}: not well-formed JSON ({error})") from error


def check_required_fields(document: dict[str, object], label: str) -> list[str]:
    missing = [field for field in REQUIRED_FIELDS if field not in document]
    if missing:
        return [f"{label}: benchmark JSON lost fields: {missing}"]
    return []


def check_artifact_self_consistency(artifact: dict[str, object]) -> list[str]:
    """The top-level counters must match the `identical_in_every_run` claim."""

    spread = artifact.get("run_to_run_spread")
    if not isinstance(spread, dict):
        return ["artifact: missing the run_to_run_spread block"]
    claimed = spread.get("identical_in_every_run")
    if not isinstance(claimed, dict):
        return ["artifact: run_to_run_spread lost identical_in_every_run"]

    failures = []
    for field, value in claimed.items():
        actual = artifact.get(field)
        if actual != value:
            failures.append(
                f"artifact: identical_in_every_run.{field} is {value!r} but the "
                f"top-level {field} is {actual!r}"
            )
    # The other direction: a deterministic field that stopped being claimed.
    for field in DETERMINISTIC_FIELDS:
        if field not in claimed:
            failures.append(
                f"artifact: {field} is deterministic but is no longer listed under "
                "identical_in_every_run"
            )
    return failures


def check_prose(artifact: dict[str, object]) -> list[str]:
    """Every documented figure must be rendered from the artifact's value."""

    failures = []
    for field, relative_path, template in PROSE_CLAIMS:
        value = artifact.get(field)
        if not isinstance(value, int):
            failures.append(f"artifact: {field} is {value!r}, expected an integer")
            continue
        expected = template.format(value)
        document = (REPO_ROOT / relative_path).read_text(encoding="utf-8")
        if expected not in document:
            failures.append(
                f"{relative_path}: does not contain {expected!r} "
                f"(from the artifact's {field}={value}). Update the prose and the "
                "artifact together."
            )
    return failures


def check_fresh_run(artifact: dict[str, object], fresh: dict[str, object]) -> list[str]:
    """A fresh run must reproduce the committed deterministic counters exactly."""

    failures = check_required_fields(fresh, "fresh run")
    for field in DETERMINISTIC_FIELDS:
        expected = artifact.get(field)
        actual = fresh.get(field)
        if actual != expected:
            failures.append(
                f"fresh run: {field} is {actual!r} but the committed artifact "
                f"records {expected!r}. This is a deterministic compiler property, "
                "so a change here is a real change -- not measurement noise."
            )
    return failures


def main(argv: list[str]) -> int:
    artifact = load_json(ARTIFACT)

    failures = check_required_fields(artifact, "artifact")
    failures += check_artifact_self_consistency(artifact)
    failures += check_prose(artifact)

    checked = f"{ARTIFACT.relative_to(REPO_ROOT).as_posix()} + documented prose"
    if argv:
        fresh_path = Path(argv[0])
        failures += check_fresh_run(artifact, load_json(fresh_path))
        checked += f" + a fresh run ({fresh_path})"

    if failures:
        for failure in failures:
            print(f"error: {failure}", file=sys.stderr)
        print(f"\n{len(failures)} documented-metric check(s) failed.", file=sys.stderr)
        return 1

    print(f"documented metrics OK: {checked}")
    for field in DETERMINISTIC_FIELDS:
        print(f"  {field} = {artifact[field]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
