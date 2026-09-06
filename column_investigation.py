from PIL import Image
import numpy as np
from ecdsa import SigningKey, SECP256k1
import hashlib

# Load the image
img = Image.open('puzzle_image.png')
img_array = np.array(img)
grayscale = img_array[:, :, 0]

print("=== Testing Method 2: Lower nibble from Row 1038 ===")

# Get row 1038
row = grayscale[1038, :]
non_std = [int(v) for v in row if v not in [0, 255]]

# Method 2: 1 hex char per value (lower nibble)
hex_str = ''.join(f'{v % 16:x}' for v in non_std)
print(f"Hex string: {hex_str}")
print(f"Length: {len(hex_str)} hex chars")

# Convert to bytes
key_bytes = bytes.fromhex(hex_str)
print(f"Key bytes: {key_bytes.hex()}")
print(f"Length: {len(key_bytes)} bytes")

# Check if valid private key
if len(key_bytes) == 32:
    key_int = int.from_bytes(key_bytes, 'big')
    secp256k1_order = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
    print(f"Is valid private key: {0 < key_int < secp256k1_order}")
    
    if 0 < key_int < secp256k1_order:
        # Derive address
        sk = SigningKey.from_string(key_bytes, curve=SECP256k1)
        vk = sk.get_verifying_key()
        public_key = b'\x04' + vk.to_string()
        keccak = hashlib.sha3_256(public_key).digest()
        address = '0x' + keccak[-20:].hex()
        print(f"Derived address: {address}")
        print(f"Prize address: 0xFF2142E98E09b5344994F9bEB9C56C95506B9F17")
        print(f"Match: {address.lower() == '0xff2142e98e09b5344994f9eb9c56c95506b9f17'.lower()}")

# Now let's investigate columns with 64 non-standard values
print("\n=== Investigating columns with 64 non-standard values ===")

cols_64 = [886, 898, 905, 916]
for col_idx in cols_64:
    col_data = grayscale[:, col_idx]
    non_std_vals = [int(v) for v in col_data if v not in [0, 255]]
    
    if len(non_std_vals) == 64:
        print(f"\nColumn {col_idx}: 64 values")
        print(f"Values: {non_std_vals}")
        
        # Try as 32-byte key using lower nibbles
        hex_lower = ''.join(f'{v % 16:x}' for v in non_std_vals)
        key_bytes_lower = bytes.fromhex(hex_lower)
        
        if len(key_bytes_lower) == 32:
            key_int = int.from_bytes(key_bytes_lower, 'big')
            if 0 < key_int < secp256k1_order:
                print(f"  Lower nibble method: Valid key!")
                sk = SigningKey.from_string(key_bytes_lower, curve=SECP256k1)
                vk = sk.get_verifying_key()
                public_key = b'\x04' + vk.to_string()
                keccak = hashlib.sha3_256(public_key).digest()
                address = '0x' + keccak[-20:].hex()
                print(f"  Derived address: {address}")
                print(f"  Match: {address.lower() == '0xff2142e98e09b5344994f9eb9c56c95506b9f17'.lower()}")
        
        # Try as direct bytes
        key_bytes_direct = bytes(non_std_vals[:32])
        key_int = int.from_bytes(key_bytes_direct, 'big')
        if 0 < key_int < secp256k1_order:
            print(f"  Direct bytes method: Valid key!")
            sk = SigningKey.from_string(key_bytes_direct, curve=SECP256k1)
            vk = sk.get_verifying_key()
            public_key = b'\x04' + vk.to_string()
            keccak = hashlib.sha3_256(public_key).digest()
            address = '0x' + keccak[-20:].hex()
            print(f"  Derived address: {address}")
            print(f"  Match: {address.lower() == '0xff2142e98e09b5344994f9eb9c56c95506b9f17'.lower()}")

# Also check the 32-value columns more carefully
print("\n=== Re-checking 32-value columns ===")
cols_32 = [13, 14, 806]
for col_idx in cols_32:
    col_data = grayscale[:, col_idx]
    non_std_vals = [int(v) for v in col_data if v not in [0, 255]]
    
    if len(non_std_vals) == 32:
        print(f"\nColumn {col_idx}: 32 values")
        print(f"Values: {non_std_vals}")
        print(f"As hex: {''.join(f'{v:02x}' for v in non_std_vals)}")
        
        # Try as private key
        key_bytes = bytes(non_std_vals)
        key_int = int.from_bytes(key_bytes, 'big')
        if 0 < key_int < secp256k1_order:
            sk = SigningKey.from_string(key_bytes, curve=SECP256k1)
            vk = sk.get_verifying_key()
            public_key = b'\x04' + vk.to_string()
            keccak = hashlib.sha3_256(public_key).digest()
            address = '0x' + keccak[-20:].hex()
            print(f"Derived address: {address}")
            print(f"Match: {address.lower() == '0xff2142e98e09b5344994f9eb9c56c95506b9f17'.lower()}")

# Let's also look at the positions of non-standard values in columns
print("\n=== Position analysis for 32-value columns ===")
for col_idx in cols_32:
    col_data = grayscale[:, col_idx]
    positions = [i for i, v in enumerate(col_data) if v not in [0, 255]]
    values = [int(v) for i, v in enumerate(col_data) if v not in [0, 255]]
    print(f"\nColumn {col_idx}:")
    print(f"  Positions: {positions}")
    print(f"  Values: {values}")

# Let's try a different approach - maybe the key is encoded in the positions
print("\n=== Trying position-based encoding ===")
for col_idx in cols_32:
    col_data = grayscale[:, col_idx]
    positions = [i for i, v in enumerate(col_data) if v not in [0, 255]]
    
    # Try using positions as key bytes
    key_bytes = bytes(positions)
    if len(key_bytes) == 32:
        key_int = int.from_bytes(key_bytes, 'big')
        if 0 < key_int < secp256k1_order:
            print(f"Column {col_idx} positions as key: Valid!")
            sk = SigningKey.from_string(key_bytes, curve=SECP256k1)
            vk = sk.get_verifying_key()
            public_key = b'\x04' + vk.to_string()
            keccak = hashlib.sha3_256(public_key).digest()
            address = '0x' + keccak[-20:].hex()
            print(f"Derived address: {address}")
            print(f"Match: {address.lower() == '0xff2142e98e09b5344994f9eb9c56c95506b9f17'.lower()}")
