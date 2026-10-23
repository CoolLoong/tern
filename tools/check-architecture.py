#!/usr/bin/env python3
"""Check Tern's scene layout without compiling or opening a window."""
from pathlib import Path
import re

root = Path(__file__).resolve().parents[1]
src = root / "src"
scenes = {"app", "workspace", "browser", "files", "navigation", "settings", "platform"}
errors = []

if {path.name for path in src.glob("*.rs")} != {"main.rs"}:
    errors.append("src/ must contain only main.rs; put implementation in its owning scene")
declared = set(re.findall(r"^mod (\w+);$", (src / "main.rs").read_text(), re.MULTILINE))
if declared != scenes:
    errors.append(f"main.rs scene declarations differ from the documented layout: {declared}")
for scene in scenes:
    if not (src / scene / "mod.rs").is_file():
        errors.append(f"missing scene entry point: {scene}/mod.rs")

# The filesystem/model layer must stay usable without a GPUI App or Window.
# All app-local references in this layer belong to files itself.
for path in (src / "files").rglob("*.rs"):
    source = path.read_text()
    if re.search(r"\b(gpui_kit|gpui_dock|gpui_transfer|menubar)::", source):
        errors.append(f"{path.relative_to(root)} depends on the UI")
    for dependency in re.findall(r"\bcrate::(\w+)", source):
        if dependency != "files":
            errors.append(f"{path.relative_to(root)} depends on scene {dependency}")

if errors:
    raise SystemExit("\n".join(errors))
print("Architecture OK: 7 scenes, main.rs-only source root, UI-independent files layer.")
