from sku import normalize_sku


class Inventory:
    def __init__(self):
        self._quantities = {}

    def put(self, sku, quantity):
        self._quantities[normalize_sku(sku)] = quantity

    def get(self, sku):
        return self._quantities.get(sku, 0)
