from PIL import Image
import numpy as np

# Load the image
img = Image.open('puzzle_image.png')
img_array = np.array(img)

# Extract grayscale channel
grayscale = img_array[:, :, 0]

print('=== Looking for hidden text/data ===')
print(f'Image dimensions: {grayscale.shape}')

# Let's try different LSB extraction methods
# Method 1: Standard LSB
print('\n--- Method 1: Standard LSB extraction ---')
lsb_bits = (grayscale.flatten() & 1)
# Convert to bytes
lsb_bytes = []
for i in range(0, len(lsb_bits), 8):
    if i + 8 <= len(lsb_bits):
        byte = 0
        for j in range(8):
            byte = (byte << 1) | lsb_bits[i + j]
        lsb_bytes.append(byte)

# Check for patterns
print(f'First 50 bytes (hex): {bytes(lsb_bytes[:50]).hex()}')
print(f'First 50 bytes (decimal): {lsb_bytes[:50]}')

# Method 2: Try different bit positions
print('\n--- Method 2: Different bit positions ---')
for bit in range(1, 4):
    bits = (grayscale.flatten() >> bit) & 1
    bytes_data = []
    for i in range(0, len(bits), 8):
        if i + 8 <= len(bits):
            byte = 0
            for j in range(8):
                byte = (byte << 1) | bits[i + j]
            bytes_data.append(byte)
    print(f'Bit {bit} first 50 bytes (hex): {bytes(bytes_data[:50]).hex()}')

# Method 3: Look for specific patterns in the image
print('\n--- Method 3: Looking for hex patterns ---')
# Reshape grayscale to 1D
flat = grayscale.flatten()

# Look for sequences that could be hex characters
# In ASCII, hex chars are: 0-9 (48-57), a-f (97-102), A-F (65-70)
hex_chars = set(range(48, 58)) | set(range(97, 103)) | set(range(65, 71))
print(f'Looking for potential hex strings...')

# Check first few rows
for row in range(min(10, grayscale.shape[0])):
    row_data = grayscale[row, :]
    # Look for sequences of hex-like values
    potential_hex = []
    for val in row_data:
        if val in hex_chars:
            potential_hex.append(chr(val))
        else:
            if len(potential_hex) >= 2:
                print(f'Row {row}: Potential hex: {"".join(potential_hex)}')
            potential_hex = []

# Method 4: Check if the image contains text visually
print('\n--- Method 4: Visual inspection hints ---')
# Check specific regions
# Let's look at the center area where the boat might be
center_y, center_x = grayscale.shape[0] // 2, grayscale.shape[1] // 2
print(f'Center region (around {center_x}, {center_y}):')
print(f'  Grayscale value: {grayscale[center_y, center_x]}')

# Check if there are any unusual patterns
print('\n--- Method 5: Looking for unusual value patterns ---')
# Check for repeated patterns
for pattern_len in [2, 3, 4]:
    patterns = {}
    for i in range(len(flat) - pattern_len):
        pattern = tuple(flat[i:i+pattern_len])
        if pattern in patterns:
            patterns[pattern] += 1
        else:
            patterns[pattern] = 1
    
    # Find most common patterns
    common_patterns = sorted(patterns.items(), key=lambda x: x[1], reverse=True)[:5]
    print(f'Most common {pattern_len}-byte patterns:')
    for pattern, count in common_patterns:
        print(f'  {pattern}: {count} times')