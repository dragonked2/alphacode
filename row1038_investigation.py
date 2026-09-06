from PIL import Image
import numpy as np
from ecdsa import SigningKey, SECP256k1
import hashlib

# Load the image
img = Image.open('puzzle_image.png')
img_array = np.array(img)
grayscale = img_array[:, :, 0]

print("=== Investigating Row 1038 ===")

# Get row 1038
row = grayscale[1038, :]
print(f"Row 1038 length: {len(row)}")

# Find non-standard values (not 0, not 255)
non_std = [(i, int(v)) for i, v in enumerate(row) if v not in [0, 255]]
print(f"Non-standard values: {len(non_std)}")

# Print positions and values
print("\nPositions and values:")
for i, (pos, val) in enumerate(non_std):
    print(f"  {i}: position {pos}, value {val} (0x{val:02x})")

# The values are all in range 0-255
# 64 values * 1 hex char each = 64 hex chars = 32 bytes = private key!
# But we need to figure out the mapping

# Let's try different interpretations
print("\n=== Trying different interpretations ===")

vals = [v for _, v in non_std]
positions = [p for _, p in non_std]

# Method 1: Direct hex encoding (2 hex chars per value)
hex_2char = ''.join(f'{v:02x}' for v in vals)
print(f"\nMethod 1 (2 hex chars per value): {hex_2char}")
print(f"Length: {len(hex_2char)} hex chars = {len(hex_2char)//2} bytes")

# Method 2: 1 hex char per value (only lower nibble)
hex_1char = ''.join(f'{v % 16:x}' for v in vals)
print(f"\nMethod 2 (1 hex char per value, lower nibble): {hex_1char}")
print(f"Length: {len(hex_1char)} hex chars = {len(hex_1char)//2} bytes")

# Method 3: Try values as-is but check if they could be ASCII
print(f"\nMethod 3 (as ASCII):")
ascii_str = ''.join(chr(v) if 32 <= v <= 126 else f'[{v}]' for v in vals)
print(f"  {ascii_str}")

# Method 4: Maybe the values represent nibbles directly
# Group into pairs and convert to bytes
print(f"\nMethod 4 (paired nibbles):")
if len(vals) % 2 == 0:
    bytes_data = bytes(vals)
    print(f"  As bytes: {bytes_data.hex()}")
    print(f"  Length: {len(bytes_data)} bytes")
    
    # Check if it could be a valid private key (32 bytes)
    if len(bytes_data) == 32:
        key_int = int.from_bytes(bytes_data, 'big')
        secp256k1_order = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
        if 0 < key_int < secp256k1_order:
            print(f"  Valid private key!")
            # Derive address
            sk = SigningKey.from_string(bytes_data, curve=SECP256k1)
            vk = sk.get_verifying_key()
            public_key = b'\x04' + vk.to_string()
            keccak = hashlib.sha3_256(public_key).digest()
            address = '0x' + keccak[-20:].hex()
            print(f"  Derived address: {address}")
            print(f"  Prize address: 0xFF2142E98E09b5344994F9bEB9C56C95506B9F17")
            print(f"  Match: {address.lower() == '0xff2142e98e09b5344994f9eb9c56c95506b9f17'.lower()}")

# Method 5: Maybe only specific positions matter
# The README mentioned "strange non-zero pixels over mostly zero (white) line"
# Row 1038 might not be the right row - let's check other rows too
print("\n=== Checking all rows for exactly 64 non-standard values ===")
for row_idx in range(grayscale.shape[0]):
    row_data = grayscale[row_idx, :]
    non_std_count = sum(1 for v in row_data if v not in [0, 255])
    if non_std_count == 64:
        print(f"Row {row_idx}: 64 non-standard values!")
        non_std_vals = [int(v) for v in row_data if v not in [0, 255]]
        # Try as private key
        if len(non_std_vals) == 32:
            key_bytes = bytes(non_std_vals)
            key_int = int.from_bytes(key_bytes, 'big')
            secp256k1_order = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
            if 0 < key_int < secp256k1_order:
                print(f"  Valid private key candidate!")
                sk = SigningKey.from_string(key_bytes, curve=SECP256k1)
                vk = sk.get_verifying_key()
                public_key = b'\x04' + vk.to_string()
                keccak = hashlib.sha3_256(public_key).digest()
                address = '0x' + keccak[-20:].hex()
                print(f"  Derived address: {address}")

# Method 6: Look at column patterns too
print("\n=== Checking columns for patterns ===")
for col_idx in range(grayscale.shape[1]):
    col_data = grayscale[:, col_idx]
    non_std_count = sum(1 for v in col_data if v not in [0, 255])
    if non_std_count == 32 or non_std_count == 64:
        print(f"Column {col_idx}: {non_std_count} non-standard values")
        non_std_vals = [int(v) for v in col_data if v not in [0, 255]]
        if len(non_std_vals) == 32:
            key_bytes = bytes(non_std_vals)
            key_int = int.from_bytes(key_bytes, 'big')
            secp256k1_order = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
            if 0 < key_int < secp256k1_order:
                print(f"  Valid private key candidate!")
                sk = SigningKey.from_string(key_bytes, curve=SECP256k1)
                vk = sk.get_verifying_key()
                public_key = b'\x04' + vk.to_string()
                keccak = hashlib.sha3_256(public_key).digest()
                address = '0x' + keccak[-20:].hex()
                print(f"  Derived address: {address}")
