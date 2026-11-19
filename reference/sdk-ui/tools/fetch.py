#!/usr/bin/env python3
"""Reproduce BerryOS' private SDK/UI official-reference cache.

Usage:
  python3 reference/sdk-ui/tools/fetch.py --list
  python3 reference/sdk-ui/tools/fetch.py --status
  python3 reference/sdk-ui/tools/fetch.py --sync
  python3 reference/sdk-ui/tools/fetch.py --sync openharmony-docs fuchsia
  python3 reference/sdk-ui/tools/fetch.py --check-links

The cache is intentionally ignored by Git. The manifest and this script are
tracked instead, so the private repository can reproduce the cache without
committing third-party blobs.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path
from typing import Any

SDK_ROOT = Path(__file__).resolve().parents[1]
MANIFEST = SDK_ROOT / "sources.toml"
CACHE = SDK_ROOT / "cache"


def load_manifest() -> dict[str, Any]:
    with MANIFEST.open("rb") as stream:
        return tomllib.load(stream)


def selected_sources(manifest: dict[str, Any], ids: list[str]) -> list[dict[str, Any]]:
    sources = manifest.get("source", [])
    if not ids:
        return sources
    by_id = {source["id"]: source for source in sources}
    missing = [item for item in ids if item not in by_id]
    if missing:
        raise SystemExit(f"unknown source id(s): {', '.join(missing)}")
    return [by_id[item] for item in ids]


def run(command: list[str], *, capture: bool = False) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        command,
        text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture else None,
        check=False,
    )
    if result.returncode != 0:
        detail = (result.stderr or result.stdout or "").strip()
        raise RuntimeError(f"command failed ({result.returncode}): {' '.join(command)}\n{detail}")
    return result




def commit_of(path: Path) -> str | None:
    if not (path / ".git").exists():
        return None
    result = subprocess.run(
        ["git", "-C", str(path), "rev-parse", "HEAD"],
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        return None
    return result.stdout.strip()


def archive_commit(source: dict[str, Any]) -> str | None:
    marker = CACHE / source["id"] / "SOURCE.txt"
    if not marker.exists():
        return None
    for line in marker.read_text().splitlines():
        if line.startswith("commit="):
            return line.split("=", 1)[1].strip()
    return None


def cache_size(path: Path) -> int:
    if not path.exists():
        return 0
    total = 0
    for root, _dirs, files in os.walk(path):
        for name in files:
            try:
                total += (Path(root) / name).stat().st_size
            except FileNotFoundError:
                pass
    return total


def write_sparse_patterns(source: dict[str, Any]) -> str:
    patterns = source.get("paths", [])
    if not patterns:
        return ""
    fd, name = tempfile.mkstemp(prefix="berry-sdk-ui-sparse-")
    with os.fdopen(fd, "w") as stream:
        stream.write("\n".join(patterns) + "\n")
    return name


def clone_git(source: dict[str, Any], destination: Path) -> None:
    if destination.exists():
        raise RuntimeError(f"cache already exists: {destination}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    ref = source.get("ref", "main")
    base = [
        "git",
        "-c",
        "http.version=HTTP/1.1",
        "clone",
        "--depth",
        "1",
        "--filter=blob:none",
        "--no-checkout",
        "--branch",
        ref,
        source["source"],
        str(destination),
    ]
    print(f"clone {source['id']} ({ref})")
    run(base)
    pattern_file = write_sparse_patterns(source)
    try:
        if pattern_file:
            run(["git", "-C", str(destination), "sparse-checkout", "init", "--no-cone"])
            with open(pattern_file) as stream:
                passed = subprocess.run(
                    ["git", "-C", str(destination), "sparse-checkout", "set", "--stdin"],
                    stdin=stream,
                    text=True,
                )
                if passed.returncode != 0:
                    raise RuntimeError(passed.stderr or "sparse-checkout set failed")
        run(["git", "-C", str(destination), "checkout"])
    finally:
        if pattern_file:
            os.unlink(pattern_file)


def gitiles_repo(source: dict[str, Any]) -> str:
    value = source["source"]
    marker = "/+archive/"
    if marker not in value:
        raise RuntimeError(f"not a Gitiles archive URL: {value}")
    return value.split(marker, 1)[0]


def resolve_gitiles_commit(source: dict[str, Any]) -> str:
    ref = source.get("ref", "main")
    result = run(
        ["git", "ls-remote", gitiles_repo(source), f"refs/heads/{ref}"],
        capture=True,
    )
    return result.stdout.strip().split()[0]


def fetch_archive(source: dict[str, Any], destination: Path) -> None:
    if destination.exists():
        raise RuntimeError(f"cache already exists: {destination}")
    destination.mkdir(parents=True)
    fd, archive_name = tempfile.mkstemp(prefix="berry-sdk-ui-archive-", suffix=".tar.gz")
    os.close(fd)
    try:
        run(["curl", "-fL", "--retry", "3", source["source"], "-o", archive_name])
        run(["tar", "-xzf", archive_name, "-C", str(destination)])
        commit = resolve_gitiles_commit(source)
        (destination / "SOURCE.txt").write_text(
            "url=" + gitiles_repo(source) + "\n"
            "ref=refs/heads/" + source.get("ref", "main") + "\n"
            "commit=" + commit + "\n"
            "fetched_at=manual\n"
            "format=Gitiles +archive\n"
        )
    finally:
        if os.path.exists(archive_name):
            os.unlink(archive_name)


def sync(source: dict[str, Any]) -> None:
    if not source.get("enabled", False):
        print(f"skip disabled {source['id']}")
        return
    destination = CACHE / source["id"]
    if destination.exists():
        print(f"exists {source['id']} @{commit_of(destination) or archive_commit(source)}")
        return
    kind = source["cache"]
    if kind in {"git", "sparse-git"}:
        clone_git(source, destination)
    elif kind == "gitiles-archive":
        fetch_archive(source, destination)
    else:
        raise RuntimeError(f"cannot sync cache kind {kind!r} for {source['id']}")
    print(f"ready {source['id']} @{commit_of(destination) or archive_commit(source)}")


def status(sources: list[dict[str, Any]], *, json_output: bool = False) -> int:
    rows = []
    for source in sources:
        path = CACHE / source["id"]
        current = commit_of(path) or archive_commit(source)
        pinned = source.get("commit")
        state = "absent"
        if path.exists():
            if current and pinned and current != pinned:
                state = "drifted"
            else:
                state = "present"
        rows.append(
            {
                "id": source["id"],
                "vendor": source.get("vendor", ""),
                "cache": source["cache"],
                "enabled": source.get("enabled", False),
                "state": state,
                "pinned": pinned,
                "current": current,
                "bytes": cache_size(path),
                "license": source.get("license", ""),
            }
        )
    if json_output:
        print(json.dumps(rows, ensure_ascii=False, indent=2))
    else:
        for row in rows:
            size = row["bytes"] / (1024 * 1024)
            print(
                f"{row['state']:8} {row['id'] + ':':38} "
                f"{row['current'] or '-':40} {size:8.1f} MiB {row['license']}"
            )
    return 1 if any(row["state"] == "drifted" for row in rows) else 0


def check_links(sources: list[dict[str, Any]]) -> int:
    failed = []
    for source in sources:
        if not source.get("official"):
            continue
        result = subprocess.run(
            [
                "curl",
                "-L",
                "-s",
                "-o",
                "/dev/null",
                "-w",
                "%{http_code}",
                "--max-time",
                "30",
                source["official"],
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        code = result.stdout.strip() or "0"
        ok = code.startswith("2") or code.startswith("3") or code == "405"
        print(f"{code:4} {source['id']}: {source['official']}")
        if code == "405":
            print("      note: host rejects command-line GET; browser/clone access still valid")
        if not ok:
            failed.append(source["id"])
    return 1 if failed else 0


def list_sources(sources: list[dict[str, Any]]) -> None:
    for source in sources:
        flag = "cache" if source.get("enabled", False) else source["cache"]
        print(
            f"{source['id'] + ':':38} {flag:15} "
            f"{source.get('vendor', '') + '/' + source.get('category', ''):24} {source['title']}"
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--list", action="store_true", help="list manifest entries")
    parser.add_argument("--status", action="store_true", help="show local cache status")
    parser.add_argument("--json", action="store_true", help="status/list JSON output")
    parser.add_argument("--sync", nargs="*", metavar="ID", help="sync all or selected cache entries")
    parser.add_argument("--check-links", action="store_true", help="HEAD/GET official URLs")
    parser.add_argument("ids", nargs="*", metavar="ID", help="selection fallback")
    args = parser.parse_args()

    manifest = load_manifest()
    ids = args.sync if args.sync is not None else args.ids
    sources = selected_sources(manifest, ids)

    if args.list:
        list_sources(sources)
        return 0
    if args.status:
        return status(sources, json_output=args.json)
    if args.check_links:
        return check_links(sources)
    if args.sync is not None:
        for source in sources:
            sync(source)
        return status([item for item in sources if item.get("enabled", False)])
    parser.print_help()
    return 2


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (RuntimeError, OSError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
