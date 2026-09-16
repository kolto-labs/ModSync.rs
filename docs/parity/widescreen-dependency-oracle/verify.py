import json
from pathlib import Path
result = json.loads(Path(__file__).with_name("result.json").read_text())
assert result == {"Bulk": "Success", "AfterBulk": None, "StateAfterBulk": "Pending", "Narrow": "DependencyViolation", "AfterNarrow": None, "Full": "Success", "AfterFull": "second"}, result
print("Reference phase dependency result verified")
