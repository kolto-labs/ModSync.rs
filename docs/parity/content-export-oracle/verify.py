"""Verify the saved C# import result against the original Markdown fixture."""
from pathlib import Path
import json
root = Path(__file__).resolve().parent
source = (root.parent / "markdown-warning-oracle/guide.md").read_text()
warning = json.loads((root.parent / "markdown-warning-oracle/result.json").read_text())
results = json.loads((root / "result.json").read_text())
assert [r["Format"] for r in results] == ["toml", "yaml"]
for result in results:
    assert result["Count"] == 2
    assert result["PreambleContent"] == source.split("## Mod List")[0].rstrip()
    for key in ["EpilogueContent", "WidescreenWarningContent"]:
        assert result[key] == warning[key]
print("Both C# imports preserve component count and exact wizard content.")
