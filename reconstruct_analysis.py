from PIL import Image
import numpy as np
import struct
import zlib
from ecdsa import SigningKey, SECP256k1
import hashlib

# Load and decompress raw PNG data
with open('puzzle_image.png', 'rb') as f:
    raw_data = f.read()

pos = 8
idat_data = b''
while pos < len(raw_data):
    length = struct.unpack('>I', raw_data[pos:pos+4])[0]
    chunk_type = raw_data[pos+4:pos+8]
    chunk_data = raw_data[pos+8:pos+8+length]
    if chunk_type == b'IDAT':
        idat_data += chunk_data
    pos += 12 + length

decompressed = zlib.decompress(idat_data)

# Parse raw pixel data
row_size = 1 + 1600 * 2
raw_pixels = []
for row_idx in range(1105):
    start = row_idx * row_size
    filter_byte = decompressed[start]
    row_data = decompressed[start+1:start+row_size]
    raw_pixels.append((filter_byte, row_data))

# Reconstruct the actual pixel values by reversing the PNG filters
def undo_filter(filter_type, row_data, prev_row, bpp=2):
    """Reverse PNG filter to get actual pixel values"""
    result = bytearray(len(row_data))
    
    if filter_type == 0:  # None
        result[:] = row_data
    elif filter_type == 1:  # Sub
        for i in range(len(row_data)):
            left = result[i - bpp] if i >= bpp else 0
            result[i] = (row_data[i] + left) & 0xFF
    elif filter_type == 2:  # Up
        for i in range(len(row_data)):
            up = prev_row[i] if prev_row else 0
            result[i] = (row_data[i] + up) & 0xFF
    elif filter_type == 3:  # Average
        for i in range(len(row_data)):
            left = result[i - bpp] if i >= bpp else 0
            up = prev_row[i] if prev_row else 0
            result[i] = (row_data[i] + (left + up) // 2) & 0xFF
    elif filter_type == 4:  # Paeth
        for i in range(len(row_data)):
            left = result[i - bpp] if i >= bpp else 0
            up = prev_row[i] if prev_row else 0
            up_left = prev_row[i - bpp] if prev_row and i >= bpp else 0
            p = left + up - up_left
            pa, pb, pc = abs(p - left), abs(p - up), abs(p - up_left)
            if pa <= pb and pa <= pc:
                predictor = left
            elif pb <= pc:
                predictor = up
            else:
                predictor = up_left
            result[i] = (row_data[i] + predictor) & 0xFF
    
    return bytes(result)

# Reconstruct all pixels
print("=== Reconstructing pixel data ===")
all_rows = []
prev_row = None
for row_idx in range(1105):
    filter_byte, row_data = raw_pixels[row_idx]
    actual_row = undo_filter(filter_byte, row_data, prev_row)
    all_rows.append(actual_row)
    prev_row = actual_row

# Parse pixels from reconstructed data
print("\n=== Parsing reconstructed pixels ===")
pixels = []  # List of (grayscale, alpha) tuples
for row_idx, row_data in enumerate(all_rows):
    row_pixels = []
    for i in range(0, len(row_data), 2):
        gray = row_data[i]
        alpha = row_data[i+1]
        row_pixels.append((gray, alpha))
    pixels.append(row_pixels)

# Now analyze the actual alpha values
print("\n=== Alpha channel analysis (reconstructed) ===")
alpha_map = np.zeros((1105, 1600), dtype=np.uint8)
gray_map = np.zeros((1105, 1600), dtype=np.uint8)

for row_idx in range(1105):
    for col_idx in range(1600):
        gray, alpha = pixels[row_idx][col_idx]
        gray_map[row_idx, col_idx] = gray
        alpha_map[row_idx, col_idx] = alpha

# Find non-zero alpha values
non_zero_alpha = alpha_map[alpha_map > 0]
print(f"Total non-zero alpha pixels: {len(non_zero_alpha)}")
print(f"Unique non-zero alpha values: {np.unique(non_zero_alpha)}")

# Find locations of non-zero alpha
locations = np.where(alpha_map > 0)
print(f"Number of non-zero alpha locations: {len(locations[0])}")

# Focus on the area around the boat (based on README hints)
print("\n=== Alpha in boat region (rows 400-800, cols 0-600) ===")
boat_alpha = alpha_map[400:800, 0:600]
boat_nonzero = boat_alpha[boat_alpha > 0]
print(f"Non-zero alpha in boat region: {len(boat_nonzero)}")
print(f"Unique values: {np.unique(boat_nonzero)}")

if len(boat_nonzero) > 0:
    boat_locations = np.where(boat_alpha > 0)
    print(f"Locations (first 20): {list(zip(boat_locations[0][:20] + 400, boat_locations[1][:20]))}")

# Check what the alpha values could encode
print("\n=== Alpha value analysis ===")
# Get all non-zero alpha values with their positions
alpha_data = []
for row_idx in range(1105):
    for col_idx in range(1600):
        if alpha_map[row_idx, col_idx] > 0:
            alpha_data.append((row_idx, col_idx, alpha_map[row_idx, col_idx]))

print(f"Total alpha data points: {len(alpha_data)}")

# Print first 50 alpha data points
print("\nFirst 50 alpha data points:")
for i, (row, col, val) in enumerate(alpha_data[:50]):
    print(f"  Row {row}, Col {col}: {val} (0x{val:02x})")

# Check if alpha values form a pattern
print("\n=== Checking alpha patterns ===")
# Group by row
alpha_by_row = {}
for row, col, val in alpha_data:
    if row not in alpha_by_row:
        alpha_by_row[row] = []
    alpha_by_row[row].append((col, val))

print(f"Rows with non-zero alpha: {sorted(alpha_by_row.keys())}")

# Check specific rows
for row_idx in [1000, 1001, 1002, 1003]:
    if row_idx in alpha_by_row:
        print(f"\nRow {row_idx}:")
        for col, val in alpha_by_row[row_idx]:
            print(f"  Col {col}: {val} (0x{val:02x})")
