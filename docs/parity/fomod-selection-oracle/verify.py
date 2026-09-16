from pathlib import Path
import json
result = json.loads(Path(__file__).with_name("result.json").read_text())
defaults = {"SelectAny": [False, False], "SelectExactlyOne": [True, False], "SelectAtMostOne": [False, False], "SelectAtLeastOne": [True, False], "SelectAll": [True, True]}
assert len(result["Groups"]) == len(defaults)
for group in result["Groups"]:
    assert group["Defaults"] == defaults[group["Group"]]
    assert group["InitialValidation"] is None
    assert group["Final"] == [False, True]
    assert bool(group["FinalValidation"]) == (group["Group"] == "SelectAll")
assert result["Required"] == {"DirectClear": False, "SelectSibling": True, "Selected": [False, True], "Validation": None}
print("Five group defaults and required-sibling conflict reproduced.")
