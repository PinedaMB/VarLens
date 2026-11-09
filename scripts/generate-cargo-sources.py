#!/usr/bin/env python3
"""
Generates cargo-sources.json for Flatpak builds from Cargo.lock.
Usage: python3 scripts/generate-cargo-sources.py
"""
import tomllib
import json
from pathlib import Path

def main():
    root = Path(__file__).resolve().parent.parent
    lock_file = root / "Cargo.lock"
    out_file = root / "build-aux" / "cargo-sources.json"

    with open(lock_file, "rb") as f:
        data = tomllib.load(f)

    sources = []
    for pkg in data.get("package", []):
        if "checksum" in pkg and "source" in pkg:
            name = pkg["name"]
            version = pkg["version"]
            checksum = pkg["checksum"]
            sources.append({
                "type": "file",
                "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
                "sha256": checksum,
                "dest": f"cargo-crates/{name}-{version}"
            })

    # Cargo vendor redirection config
    config = {
        "type": "inline",
        "contents": "[source.crates-io]\nreplace-with = \"vendored-sources\"\n\n[source.vendored-sources]\ndirectory = \"cargo-crates\"\n",
        "dest-filename": ".cargo/config.toml"
    }
    sources.append(config)

    out_file.parent.mkdir(parents=True, exist_ok=True)
    with open(out_file, "w") as f:
        json.dump(sources, f, indent=2)

    print(f"==> Generated {out_file} with {len(sources)} sources.")

if __name__ == "__main__":
    main()
