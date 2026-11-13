"""Independent checks; argv[1] is the agent's copied workspace."""
import math
from pathlib import Path
import sys

sys.path.insert(0, str(Path(sys.argv[1]).resolve()))
from quantity import parse_quantity

for value, expected in [(0, 0.0), (2, 2.0), (0.125, 0.125), (" 2.25 ", 2.25), ("0", 0.0)]:
    actual = parse_quantity(value)
    assert isinstance(actual, float) and math.isfinite(actual) and actual == expected, repr(value)
for value in [True, False, -1, "-0.5", float("nan"), float("inf"), float("-inf"), "nan", "inf", "", "bad", None, [], {}]:
    try:
        parse_quantity(value)
    except ValueError:
        continue
    raise AssertionError(f"Expected ValueError for {value!r}")
print("Quantity validation passed")
