#!/usr/bin/env python3
"""Prints which parts of coral changed since <base> as GitHub step outputs; no base means a manual run."""

import json
import os
import subprocess
import sys
from pathlib import PurePosixPath

SERVICES = {"coral-api", "coral-bot", "coral-admin", "coral-sync", "coral-verify"}
REBUILD_EVERYTHING = {"Cargo.toml", "Cargo.lock", "Dockerfile"}


def git(*args):
    return subprocess.run(["git", *args], check=True, capture_output=True, text=True).stdout


def workspace():
    metadata = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--no-deps"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )
    root = PurePosixPath(metadata["workspace_root"].replace("\\", "/"))
    members = {package["name"] for package in metadata["packages"]}
    return {
        package["name"]: {
            "dir": PurePosixPath(package["manifest_path"].replace("\\", "/")).parent.relative_to(root),
            "deps": {dep["name"] for dep in package["dependencies"] if dep["name"] in members},
            "bins": {target["name"] for target in package["targets"] if "bin" in target["kind"]},
        }
        for package in metadata["packages"]
    }


def owner(path, packages):
    owners = [name for name, package in packages.items() if path.is_relative_to(package["dir"])]
    return max(owners, key=lambda name: len(packages[name]["dir"].parts), default=None)


def with_dependents(touched, packages):
    affected = set(touched)
    while grown := {name for name, package in packages.items() if package["deps"] & affected} - affected:
        affected |= grown
    return affected


def main():
    base = sys.argv[1] if len(sys.argv) > 1 else ""
    manual = not base
    clients = os.environ.get("CLIENTS") == "true"
    site = os.environ.get("SITE") == "true"
    packages = workspace()

    files = [] if manual or set(base) == {"0"} else git("diff", "--name-only", base, "HEAD").splitlines()
    everything = (
        os.environ.get("EVERYTHING") == "true"
        or set(base) == {"0"}
        or (manual and not clients and not site)
        or bool(REBUILD_EVERYTHING & set(files))
    )

    if everything:
        touched = set(packages)
    else:
        touched = {owner(PurePosixPath(file), packages) for file in files} - {None}
        if any(file.startswith("migrations/") for file in files):
            touched.add("database")
        if clients:
            touched.add("clients")

    affected = with_dependents(touched, packages)
    services = sorted(SERVICES & {bin for name in affected for bin in packages[name]["bins"]})

    def changed(prefix):
        return everything or any(file.startswith(prefix) for file in files)

    outputs = {
        "rust": bool(affected),
        "packages": " ".join(sorted(affected)),
        "services": " ".join(services),
        "admin_panel": changed("crates/coral-admin/ui/"),
        "coral_web": changed("coral-web/"),
        "web": site or changed("web/"),
        "deploy": changed("deploy/"),
    }
    for key, value in outputs.items():
        print(f"{key}={str(value).lower() if isinstance(value, bool) else value}")


if __name__ == "__main__":
    main()
