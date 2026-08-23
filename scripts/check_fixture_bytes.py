#!/usr/bin/env python3
"""Fail if a committed fixture contains a byte that makes it platform-dependent.

The files under `tests/snapshots/` and `tests/golden/` are compared byte for byte
by the test suite, and `benchmarks/results-windows-x86_64.json` is parsed. Each
byte class below breaks one of those on one platform while leaving the other
green, which is the failure mode this guards.

Checked, in order of how easy each is to introduce by accident:

    CR (0x0d)           A checkout with `core.autocrlf=true` -- the default the
                        Windows git installer offers. `.gitattributes` is what
                        prevents this; this check is the backstop proving it
                        still works. Note that CR is *ASCII*, so the escape and
                        non-ASCII scans below cannot see it: it needs its own
                        check, and it is the most likely of these to happen.
    BOM                 A PowerShell 5.1 redirect. Breaks an insta comparison and
                        a JSON parse alike, and is invisible in most editors.
    ANSI escape (0x1b)  ariadne paints via yansi; an escape reaching a snapshot
                        is what made these snapshots platform-dependent before.
    non-ASCII           ariadne's default box-drawing characters.
                        `RenderStyle::Plain` exists to avoid them.

Why this is Python and not `grep -P`, which would be the obvious shell one-liner:
on the Windows runner, git-bash's grep gets all three of these wrong.

  * It opens files in text mode and strips CR before matching, so `grep -P '\\r'`
    never matches -- the check would be a silent no-op on the one platform whose
    line endings it exists to police.
  * In a UTF-8 locale `grep -P` matches *characters*, so `\\xef\\xbb\\xbf` looks for
    U+00EF U+00BB U+00BF rather than those three bytes and never fires on a real
    BOM.
  * Worse, `[^\\x00-\\x7f]` cannot match a byte that is not valid UTF-8 at all, so
    a UTF-16 BOM (0xff 0xfe) and a lone Latin-1 high byte both pass the non-ASCII
    scan. `LC_ALL=C` does not rescue any of this: that grep refuses `-P` outside a
    unibyte or UTF-8 locale.

Reading the bytes directly has none of those failure modes, and CI already has
python3 for the metrics check.

Usage:
    python scripts/check_fixture_bytes.py [path...]   # defaults to the fixtures
"""

from __future__ import annotations

import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

DEFAULT_TARGETS: tuple[str, ...] = (
    "tests/snapshots",
    "tests/golden",
    "benchmarks/results-windows-x86_64.json",
)

BOMS: tuple[tuple[bytes, str], ...] = (
    (b"\xef\xbb\xbf", "UTF-8"),
    (b"\xff\xfe\x00\x00", "UTF-32LE"),
    (b"\x00\x00\xfe\xff", "UTF-32BE"),
    (b"\xff\xfe", "UTF-16LE"),
    (b"\xfe\xff", "UTF-16BE"),
)

BOM_HINT = (
    "A PowerShell 5.1 redirect writes UTF-16LE, and Out-File/Tee-Object add a "
    "BOM; see benchmarks/README.md for the documented command."
)


def collect_files(targets: list[str]) -> list[Path]:
    """Expands the target paths to a sorted list of files, or exits."""

    files: set[Path] = set()
    for target in targets:
        path = Path(target) if Path(target).is_absolute() else REPO_ROOT / target
        if not path.exists():
            raise SystemExit(f"error: fixture path does not exist: {target}")
        if path.is_dir():
            files.update(child for child in path.rglob("*") if child.is_file())
        else:
            files.add(path)
    return sorted(files)


def describe(path: Path) -> str:
    """Repo-relative POSIX path, so the message matches what CI annotates."""

    try:
        return path.resolve().relative_to(REPO_ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def check_file(path: Path) -> list[str]:
    """Returns one message per byte-level problem found in `path`."""

    raw = path.read_bytes()
    name = describe(path)
    problems = []

    # Ordered most-likely-cause first, and each is reported independently: a file
    # that is both BOM'd and CRLF'd should say so once for each, because they have
    # different causes and different fixes.
    if b"\r" in raw:
        count = raw.count(b"\r")
        problems.append(
            f"{name}: {count} carriage return(s) (0x0d). Fixtures must be LF-only; "
            "check .gitattributes and your core.autocrlf setting."
        )

    for bom, encoding in BOMS:
        if raw.startswith(bom):
            problems.append(f"{name}: starts with a {encoding} BOM. {BOM_HINT}")
            break

    if b"\x1b" in raw:
        problems.append(
            f"{name}: ANSI escape byte (0x1b). Diagnostics must be rendered with "
            "RenderStyle::Plain."
        )

    # Reported with the offset of the first offender, because "non-ASCII somewhere
    # in a 200-line snapshot" is not actionable on its own.
    for offset, byte in enumerate(raw):
        if byte > 0x7F:
            problems.append(
                f"{name}: non-ASCII byte 0x{byte:02x} at offset {offset} (box "
                "drawing?). Snapshots must be pure ASCII to match on Linux and "
                "Windows alike."
            )
            break

    return problems


def main(argv: list[str]) -> int:
    targets = argv or list(DEFAULT_TARGETS)
    files = collect_files(targets)

    # A scan that finds no files would pass silently -- the same vacuous-guard bug
    # the MIN_TESTS floor exists to prevent. The fixtures are committed, so an
    # empty list means they moved or were deleted.
    if not files:
        print(
            f"error: no fixture files found under: {targets}. This guard scanned "
            "nothing -- did the fixtures move?",
            file=sys.stderr,
        )
        return 1

    problems = [problem for path in files for problem in check_file(path)]

    if problems:
        for problem in problems:
            print(f"error: {problem}", file=sys.stderr)
        print(
            f"\n{len(problems)} byte-level problem(s) in {len(files)} committed "
            "fixture(s).",
            file=sys.stderr,
        )
        return 1

    print(f"{len(files)} committed fixtures are pure ASCII: no CR, no BOM, no escapes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
