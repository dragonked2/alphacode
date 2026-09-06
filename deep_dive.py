from PIL import Image
import numpy as np
from ecdsa import SigningKey, SECP256k1
import hashlib

# Load the image
img = Image.open('puzzle_image.png')
img_array = np.array(img)
grayscale = img_array[:, :, 0]
alpha = img_array[:, :, 1]

print("=== Deep analysis of the puzzle image ===")
print(f"Image size: {grayscale.shape}")
print(f"Total pixels: {grayscale.size}")

# The cHRM chunk is just standard PNG metadata, not a hidden key
# Let's focus on the pixel content

# 1. Look at the first row more carefully - the README says there are "strange non-zero pixels"
print("\n=== First row analysis (potential hidden data) ===")
first_row = grayscale[0, :]
# Find non-background pixels (background appears to be 255/white)
non_bg = [(i, v) for i, v in enumerate(first_row) if v != 255]
print(f"Non-255 pixels in first row: {len(non_bg)}")
# These could encode data!
# Extract the values
values = [v for _, v in non_bg]
positions = [i for _, i in non_bg]
print(f"Values: {values}")
print(f"Positions: {positions}")

# Try to interpret as hex characters
# Values 0-255 could be mapped to hex in various ways
# Maybe values modulo 16?
print(f"\nValues mod 16: {[v % 16 for v in values]}")
print(f"Values mod 16 as hex: {''.join(f'{v % 16:x}' for v in values)}")

# 2. Look at rows around the building area (rows 700-900)
# These rows have many value jumps, which could hide data
print("\n=== Building region analysis (rows 700-900) ===")
building_area = grayscale[700:900, 0:300]

# For each row, extract non-standard values
for row_offset in range(0, building_area.shape[0], 10):
    row = building_area[row_offset, :]
    # Values that are not 0 or 255
    non_std = [(i, v) for i, v in enumerate(row) if v not in [0, 255]]
    if non_std:
        vals = [v for _, v in non_std]
        # Try different interpretations
        # Maybe XOR with a constant?
        # Or maybe they represent something when sorted or arranged

# 3. Look at the boat region
# Based on the image description, there's a "biggest boat"
# Let's scan the image to find it
print("\n=== Looking for the biggest boat ===")

# The boat should be a region with many non-standard pixel values
# Let's scan different regions
best_region = None
best_score = 0

for y in range(0, grayscale.shape[0] - 100, 50):
    for x in range(0, grayscale.shape[1] - 200, 50):
        region = grayscale[y:y+100, x:x+200]
        # Count non-standard values (not 0, not 255)
        non_std = np.sum((region != 0) & (region != 255))
        if non_std > best_score:
            best_score = non_std
            best_region = (y, x)

print(f"Region with most non-standard values: rows {best_region[0]}-{best_region[0]+100}, cols {best_region[1]}-{best_region[1]+200}")
print(f"Score: {best_score}")

# Look at this region more carefully
region = grayscale[best_region[0]:best_region[0]+100, best_region[1]:best_region[1]+200]
print(f"Unique values in region: {len(np.unique(region))}")

# 4. Try to extract data from specific patterns
print("\n=== Looking for hex-encoded data ===")

# The private key is 32 bytes = 64 hex characters
# Look for 64 consecutive values that could be hex

# Check if any row contains exactly 64 non-standard values
for row in range(grayscale.shape[0]):
    row_data = grayscale[row, :]
    non_std = [(i, v) for i, v in enumerate(row_data) if v not in [0, 255]]
    if len(non_std) == 64:
        vals = [v for _, v in non_std]
        # Try as hex
        hex_str = ''.join(f'{v:02x}' for v in vals)
        print(f"Row {row}: 64 non-std values -> hex: {hex_str}")
        # Check if valid private key
        try:
            key_int = int(hex_str, 16)
            secp256k1_order = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
            if 0 < key_int < secp256k1_order:
                print(f"  Valid private key!")
                # Derive address
                sk = SigningKey.from_string(bytes.fromhex(hex_str), curve=SECP256k1)
                vk = sk.get_verifying_key()
                public_key = b'\x04' + vk.to_string()
                keccak = hashlib.sha3_256(public_key).digest()
                address = '0x' + keccak[-20:].hex()
                print(f"  Derived address: {address}")
                print(f"  Prize address: 0xFF2142E98E09b5344994F9bEB9C56C95506B9F17")
                print(f"  Match: {address.lower() == '0xff2142e98e09b5344994f9eb9c56c95506b9f17'.lower()}")
        except:
            pass

# 5. Check if the data might be encoded differently
# Maybe each pixel value represents a nibble (4 bits) of the key
print("\n=== Nibble encoding approach ===")
# Take non-255, non-0 values and treat as nibbles
all_vals = grayscale.flatten()
non_std_all = [v for v in all_vals if v not in [0, 255]]
print(f"Total non-standard values: {len(non_std_all)}")

# Try grouping into nibbles
if len(non_std_all) >= 64:
    # Take first 64 values
    nibbles = [v % 16 for v in non_std_all[:64]]
    hex_str = ''.join(f'{n:x}' for n in nibbles)
    print(f"First 64 nibbles as hex: {hex_str}")
    try:
        key_int = int(hex_str, 16)
        secp256k1_order = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
        if 0 < key_int < secp256k1_order:
            print(f"Valid private key!")
    except:
        pass

# 6. Check if the key is hidden in the differences between adjacent pixels
print("\n=== Difference encoding ===")
flat = grayscale.flatten()
diffs = [int(flat[i+1]) - int(flat[i]) for i in range(min(1000, len(flat)-1))]
print(f"First 50 diffs: {diffs[:50]}")

# Try to interpret diffs as data
# Positive diffs could be 1, negative could be 0
bits = [1 if d > 0 else 0 for d in diffs]
# Convert to bytes
bytes_list = []
for i in range(0, len(bits) - 7, 8):
    byte = 0
    for j in range(8):
        byte = (byte << 1) | bits[i + j]
    bytes_list.append(byte)

# Check for printable ASCII
printable = ''.join(chr(b) if 32 <= b <= 126 else '.' for b in bytes_list[:50])
print(f"Diff-encoded text: {printable}")

print("\n=== Conclusion ===")
print("Standard steganography approaches haven't found the key.")
print("The puzzle might require:")
print("1. Visual inspection with specific contrast settings")
print("2. Understanding the specific artistic technique used")
print("3. A non-standard encoding method")
print("4. Combining information from multiple parts of the image")
