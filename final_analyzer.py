from PIL import Image
import numpy as np
from ecdsa import SigningKey, SECP256k1
import hashlib

# Load the image
img = Image.open('puzzle_image.png')
img_array = np.array(img)
grayscale = img_array[:, :, 0]

# The image is 1600x1105 = 1,768,000 pixels
# A private key is 32 bytes = 256 bits
# The hint says "format does not matter" - meaning we need to extract 32 bytes

# Let's try a completely different approach:
# What if the private key is encoded in specific pixel values?

# First, let's check if there's a specific region with unusual pixel distributions
# that could encode the key

print("=== Approach: Looking at pixel value distributions in detail ===")

# Let's try to find the private key by looking at the cHRM chunk differently
# The cHRM chunk in PNG spec contains: white point x,y, red x,y, green x,y, blue x,y
# Each value is 4 bytes big-endian
# But what if the author used this chunk to store the key?

chrm_hex = '00007a26000080840000fa00000080e8000075300000ea6000003a9800001770'
chrm_bytes = bytes.fromhex(chrm_hex)

# Let's try various transformations on this data
print("\n=== Trying transformations on cHRM data ===")

# Try XOR with something
# Try byte swapping
# Try shifting

# Original
print(f"Original: {chrm_hex}")

# Reversed bytes
reversed_hex = chrm_hex[::-1]
print(f"Reversed: {reversed_hex}")

# Swapped pairs
swapped = ''
for i in range(0, len(chrm_hex), 2):
    swapped = chrm_hex[i:i+2] + swapped
print(f"Swapped pairs: {swapped}")

# Try as series of 16-bit values (big endian)
values_16 = []
for i in range(0, 32, 2):
    val = int.from_bytes(chrm_bytes[i:i+2], 'big')
    values_16.append(val)
print(f"16-bit values: {values_16}")

# Now let's try a totally different approach
# What if the hidden key is somewhere in the pixel data using a specific extraction pattern?

# Let's try to look at specific rows/columns that have unusual patterns
print("\n=== Analyzing rows with many value jumps ===")

# Rows around 746-870 had lots of jumps - this is where the buildings are
# Let's extract data from these rows

# Focus on the building area
building_region = grayscale[700:900, 0:400]
print(f"Building region shape: {building_region.shape}")

# Check for unusual value patterns
for row_offset in range(building_region.shape[0]):
    row = building_region[row_offset, :]
    # Look for sequences that could encode hex
    unusual_vals = [(i, v) for i, v in enumerate(row) if v not in [0, 255] and 0 < v < 200]
    if len(unusual_vals) > 5:
        vals = [v for _, v in unusual_vals]
        # Try to interpret as hex
        hex_str = ''.join(f'{v:02x}' for v in vals[:64])
        # Check if it could be a valid private key
        try:
            key_int = int(hex_str, 16)
            secp256k1_order = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
            if 0 < key_int < secp256k1_order:
                print(f"Row {700 + row_offset}: Potential key! Hex: {hex_str[:64]}")
        except:
            pass

# Let's try yet another approach - look for the key in the non-standard alpha values
print("\n=== Analyzing non-standard alpha values ===")
alpha = img_array[:, :, 1]
non_standard = (alpha > 0) & (alpha < 255)
non_standard_vals = alpha[non_standard]
print(f"Non-standard alpha count: {len(non_standard_vals)}")
print(f"Non-standard alpha values: {non_standard_vals}")

# There are only 434 non-standard alpha values, with values 253-254
# These don't seem to encode much

# Let's try looking at specific pixel neighborhoods around the boat
# The boat should be in the center-left of the image
print("\n=== Looking at boat region more carefully ===")
# Let's look at different regions where the boat might be
# Based on typical AR puzzle images, the boat is usually in the lower portion

for y_start in range(200, 1000, 100):
    for x_start in range(0, 1200, 200):
        region = grayscale[y_start:y_start+100, x_start:x_start+200]
        # Check for text-like patterns (values between 32-126 that form words)
        flat = region.flatten()
        # Look for ASCII printable ranges
        printable_count = sum(1 for v in flat if 32 <= v <= 126)
        if printable_count > 1000:
            # Try to read as text
            text = ''.join(chr(v) if 32 <= v <= 126 else '.' for v in flat[:200])
            if 'key' in text.lower() or 'priv' in text.lower() or '0x' in text:
                print(f"Region ({y_start},{x_start}): {text[:100]}")

# Let's try one more approach - maybe the key is split across multiple channels
# or encoded using specific bit manipulation
print("\n=== Trying bit manipulation approaches ===")

# Try extracting specific bits from specific positions
for bit in range(8):
    plane = (grayscale >> bit) & 1
    # Check if this plane has any interesting patterns
    # Look for runs of alternating bits
    flat = plane.flatten()
    runs = []
    current_run = 1
    for i in range(1, len(flat)):
        if flat[i] == flat[i-1]:
            current_run += 1
        else:
            runs.append(current_run)
            current_run = 1
    
    if runs:
        avg_run = sum(runs) / len(runs)
        print(f"Bit {bit}: Average run length: {avg_run:.2f}, Max run: {max(runs)}")

# Let's try to look for data encoded in pairs of pixels
print("\n=== Looking at pixel pairs ===")
flat = grayscale.flatten()
for i in range(0, min(len(flat)-1, 10000), 2):
    pair = (flat[i], flat[i+1])
    # Check if pair could encode a hex character
    # For example, (high_nibble, low_nibble)
    if pair[0] < 16 and pair[1] < 16:
        hex_char = f"{pair[0]:x}{pair[1]:x}"
        if i < 100:
            print(f"Pair at {i}: ({pair[0]}, {pair[1]}) -> {hex_char}")

print("\n=== Summary ===")
print("The cHRM chunk data does NOT match the prize address when used as a private key.")
print("The puzzle likely requires extracting data from the pixel content itself.")
print("The README author noted that PK seems to be on the biggest boat.")
print("Try examining the boat region with different contrast/brightness settings.")
