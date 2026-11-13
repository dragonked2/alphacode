"""Independent checks; argv[1] is the agent's copied workspace."""
from pathlib import Path
import sys

sys.path.insert(0, str(Path(sys.argv[1]).resolve()))
from ordered import unique_in_order

for values, expected in [
    (["b", "a", "b", "c"], ["b", "a", "c"]),
    (["猫", "a", "猫", "🦀"], ["猫", "a", "🦀"]),
    ([1, "1", None, 1], [1, "1", None]),
    ([], []),
]:
    assert unique_in_order(values) == expected
assert unique_in_order(value % 3 for value in range(10)) == [0, 1, 2]
print("Ordered deduplication passed")
