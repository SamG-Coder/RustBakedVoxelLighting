"""Collect published Cargo dependency notices for the binary distribution."""
import json
from pathlib import Path
import shutil
import subprocess
import sys

output = Path(sys.argv[1])
output.mkdir(parents=True, exist_ok=True)
metadata = json.loads(subprocess.check_output([
    "cargo", "metadata", "--locked", "--format-version", "1",
    "--filter-platform", "x86_64-pc-windows-msvc",
], text=True))
resolved = {node["id"] for node in metadata["resolve"]["nodes"]}
index = []
missing = []
for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
    if package["id"] not in resolved or package["source"] is None:
        continue
    name = f'{package["name"]}-{package["version"]}'
    root = Path(package["manifest_path"]).parent
    notices = [p for p in root.iterdir() if p.is_file() and
               p.name.lower().startswith(("license", "licence", "copying", "notice", "copyright"))]
    if package.get("license_file"):
        notices.append(root / package["license_file"])
    if not notices:
        missing.append(name)
    for source in set(notices):
        destination = output / name / source.name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)
    index.append({"crate": name, "license": package.get("license"),
                  "repository": package.get("repository")})
(output / "index.json").write_text(json.dumps(index, indent=2) + "\n", encoding="utf-8")
if missing:
    raise SystemExit("Missing dependency license files: " + ", ".join(missing))
print(f"Collected notices for {len(index)} Cargo dependencies")
