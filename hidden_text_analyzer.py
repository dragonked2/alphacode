from PIL import Image
import numpy as np

# Load the image
img = Image.open('puzzle_image.png')
img_array = np.array(img)

# Extract grayscale channel
grayscale = img_array[:, :, 0]

print('=== Looking for hidden text in specific regions ===')
print(f'Image dimensions: {grayscale.shape}')

# Let's look at the first row more carefully
# The README mentioned strange pixels in the first line
print('\n=== First row detailed analysis ===')
first_row = grayscale[0, :]

# Find non-255 values (background is likely white/255)
non_white = np.where(first_row != 255)[0]
print(f'Number of non-255 pixels in first row: {len(non_white)}')
print(f'Positions of non-255 pixels: {non_white[:50]}')
print(f'Values at these positions: {first_row[non_white[:50]]}')

# Try to interpret these values as ASCII
print('\n=== Interpreting first row values as ASCII ===')
for i, pos in enumerate(non_white[:50]):
    val = first_row[pos]
    if 32 <= val <= 126:
        print(f'Position {pos}: Value {val} = ASCII "{chr(val)}"')
    else:
        print(f'Position {pos}: Value {val} (non-printable)')

# Let's also look at specific regions where the boat might be
# Based on the README, the boat is in the center-left area
print('\n=== Looking at potential boat region ===')
# Check a region around y=400-600, x=200-800
boat_region = grayscale[400:600, 200:800]
print(f'Boat region shape: {boat_region.shape}')

# Look for unusual patterns in this region
print(f'Unique values in boat region: {np.unique(boat_region)[:20]}')

# Check if there's any text-like pattern
# Text would typically have values that are either very dark or very light
dark_pixels = np.sum(boat_region < 128)
light_pixels = np.sum(boat_region >= 128)
print(f'Dark pixels (< 128): {dark_pixels}')
print(f'Light pixels (>= 128): {light_pixels}')

# Let's try a different approach - look for the hidden data in the LSBs
# but with a different extraction method
print('\n=== Alternative LSB extraction ===')

# Method: Extract LSBs from specific regions
regions = [
    (0, 0, 100, 100),      # Top-left corner
    (0, 0, 1600, 10),      # First 10 rows
    (550, 800, 600, 850),  # Center region
]

for y1, x1, y2, x2 in regions:
    region = grayscale[y1:y2, x1:x2]
    lsb_region = region.flatten() & 1
    
    # Convert to bytes
    bytes_data = []
    for i in range(0, len(lsb_region), 8):
        if i + 8 <= len(lsb_region):
            byte = 0
            for j in range(8):
                byte = (byte << 1) | lsb_region[i + j]
            bytes_data.append(byte)
    
    # Check if it looks like text
    text_data = bytes(bytes_data[:100])
    printable = all(32 <= b <= 126 or b in [10, 13] for b in text_data)
    print(f'Region ({y1},{x1})-({y2},{x2}): {len(bytes_data)} bytes, printable: {printable}')
    if printable:
        print(f'  Text: {text_data.decode("ascii", errors="replace")}')

# Let's also check the metadata for any hidden information
print('\n=== Checking for hidden data in metadata ===')
# The README mentioned a comment field with the address
# Let's check if there are any other hidden fields

# Read the raw PNG file
with open('puzzle_image.png', 'rb') as f:
    data = f.read()
    
    # Look for the address in the raw data
    address_bytes = b'0xFF2142E98E09b5344994F9bEB9C56C95506B9F17'
    pos = data.find(address_bytes)
    if pos != -1:
        print(f'Found address at position {pos}')
        # Show surrounding data
        start = max(0, pos - 50)
        end = min(len(data), pos + len(address_bytes) + 50)
        print(f'Surrounding data: {data[start:end]}')
    
    # Look for other patterns
    # Check for "private" or "key" strings
    for pattern in [b'private', b'key', b'password', b'secret']:
        pos = data.find(pattern)
        if pos != -1:
            print(f'Found "{pattern.decode()}" at position {pos}')
            start = max(0, pos - 20)
            end = min(len(data), pos + len(pattern) + 20)
            print(f'Surrounding data: {data[start:end]}')