from PIL import Image
import numpy as np
from ecdsa import SigningKey, SECP256k1
import hashlib

# Load the image
img = Image.open('puzzle_image.png')
img_array = np.array(img)

print("=== Focused Alpha Channel Analysis ===")
print(f"Image shape: {img_array.shape}")
print(f"Image mode: {img.mode}")

# The README says alpha channel is "mostly zero" but has non-zero values around the boat
# But our analysis shows alpha values 225-255 everywhere
# This is suspicious - maybe the image was processed/re-encoded

# Let's look at the raw PNG data more carefully
with open('puzzle_image.png', 'rb') as f:
    raw_data = f.read()

print(f"\nRaw file size: {len(raw_data)} bytes")

# Extract IDAT chunks (compressed image data)
import struct
import zlib

# Parse PNG chunks
pos = 8  # Skip PNG signature
idat_data = b''
while pos < len(raw_data):
    length = struct.unpack('>I', raw_data[pos:pos+4])[0]
    chunk_type = raw_data[pos+4:pos+8]
    chunk_data = raw_data[pos+8:pos+8+length]
    
    if chunk_type == b'IDAT':
        idat_data += chunk_data
    
    pos += 12 + length  # length + type + data + crc

print(f"Total IDAT data: {len(idat_data)} bytes")

# Decompress
decompressed = zlib.decompress(idat_data)
print(f"Decompressed data: {len(decompressed)} bytes")

# The decompressed data contains the raw pixel data
# For a Grayscale+Alpha image, each pixel is 2 bytes
# Plus a filter byte at the start of each row
# So for 1600x1105 image: 1105 * (1 + 1600 * 2) = 1105 * 3201 = 3,537,105 bytes
expected_size = 1105 * (1 + 1600 * 2)
print(f"Expected decompressed size: {expected_size} bytes")

# Check if the size matches
if len(decompressed) == expected_size:
    print("Size matches!")
else:
    print(f"Size mismatch! Got {len(decompressed)} bytes")

# Parse the raw pixel data
print("\n=== Parsing raw pixel data ===")
row_size = 1 + 1600 * 2  # filter byte + 1600 pixels * 2 bytes each
rows = []
for row_idx in range(1105):
    start = row_idx * row_size
    filter_byte = decompressed[start]
    row_data = decompressed[start+1:start+row_size]
    rows.append((filter_byte, row_data))

# Show first few rows
print("\nFirst 5 rows filter bytes:")
for i, (filter_byte, _) in enumerate(rows[:5]):
    print(f"Row {i}: filter={filter_byte}")

# The filter byte indicates the prediction method used:
# 0 = None, 1 = Sub, 2 = Up, 3 = Average, 4 = Paeth

# Let's look at the raw pixel values
print("\n=== Raw pixel values (first row) ===")
filter_byte, row_data = rows[0]
print(f"Filter byte: {filter_byte}")

# Parse pixels (each pixel is 2 bytes: grayscale + alpha)
pixels = []
for i in range(0, len(row_data), 2):
    gray = row_data[i]
    alpha = row_data[i+1]
    pixels.append((gray, alpha))

print(f"First 10 pixels: {pixels[:10]}")
print(f"Number of pixels in row: {len(pixels)}")

# Check if alpha is really 0 as README claims
alpha_values = [alpha for _, alpha in pixels]
print(f"Alpha values in first row: {set(alpha_values)}")

# Check a few more rows
print("\n=== Alpha values in different rows ===")
for row_idx in [0, 100, 500, 1000, 1104]:
    filter_byte, row_data = rows[row_idx]
    alpha_vals = set()
    for i in range(1, len(row_data), 2):
        alpha_vals.add(row_data[i])
    print(f"Row {row_idx}: alpha values = {alpha_vals}")

# Now let's check if there are any rows with alpha=0
print("\n=== Looking for rows with alpha=0 ===")
for row_idx in range(1105):
    filter_byte, row_data = rows[row_idx]
    has_zero_alpha = False
    for i in range(1, len(row_data), 2):
        if row_data[i] == 0:
            has_zero_alpha = True
            break
    if has_zero_alpha:
        print(f"Row {row_idx} has alpha=0 pixels")
        if row_idx > 10:
            break
