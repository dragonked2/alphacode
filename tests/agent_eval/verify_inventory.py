"""Independent checks for SKU normalization and inventory lookup."""
from pathlib import Path
import sys

sys.path.insert(0, str(Path(sys.argv[1]).resolve()))
from inventory import Inventory
from sku import normalize_sku


def rejects_invalid_sku(operation):
    for invalid in (None, 12, b"sku", "", " \t\n"):
        try:
            operation(invalid)
        except ValueError:
            continue
        raise AssertionError(f"Accepted invalid SKU {invalid!r}")


assert normalize_sku(" \tStraße-1\n") == "strasse-1"
stock = Inventory()
stock.put(" Straße-1 ", 7)
assert stock.get("STRASSE-1") == 7
stock.put("strasse-1", 11)
assert stock.get(" Straße-1 ") == 11
stock.put("other-1", 3)
assert stock.get("Missing") == 0
rejects_invalid_sku(normalize_sku)
rejects_invalid_sku(stock.get)
rejects_invalid_sku(lambda invalid: stock.put(invalid, 99))
assert stock.get("STRASSE-1") == 11
assert stock.get("OTHER-1") == 3
