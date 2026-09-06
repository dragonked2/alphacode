from PIL import Image
import numpy as np

# Load the image
img = Image.open('puzzle_image.png')
img_array = np.array(img)

# Extract grayscale channel
grayscale = img_array[:, :, 0]

print('=== Looking for hidden data using different methods ===')

# Method 1: Check for data in specific bit planes
print('\n--- Method 1: Bit plane analysis ---')
for bit in range(8):
    plane = (grayscale >> bit) & 1
    # Check if this bit plane contains structured data
    # by looking at the entropy or pattern
    unique_bits = len(np.unique(plane))
    ones_ratio = np.sum(plane) / plane.size
    print(f'Bit {bit}: {unique_bits} unique values, ones ratio: {ones_ratio:.4f}')

# Method 2: Check for data in specific value ranges
print('\n--- Method 2: Value range analysis ---')
# Focus on values that could encode text
# ASCII printable characters: 32-126
# But in grayscale, values 0-255 could map to different things

# Let's look for sequences of values that could be text
print('Looking for potential ASCII sequences...')
for row in range(grayscale.shape[0]):
    row_data = grayscale[row, :]
    # Look for runs of values in ASCII range
    ascii_run = []
    for val in row_data:
        if 32 <= val <= 126:
            ascii_run.append(chr(val))
        else:
            if len(ascii_run) >= 3:
                text = ''.join(ascii_run)
                if any(word in text.lower() for word in ['key', 'private', 'wallet', '0x', 'eth']):
                    print(f'Row {row}: Potential text: {text}')
            ascii_run = []

# Method 3: Check for patterns in the image
print('\n--- Method 3: Pattern analysis ---')
# Look for repeating patterns
print('Looking for repeating patterns...')

# Check 8x8 blocks
block_size = 8
patterns = {}
for y in range(0, grayscale.shape[0] - block_size, block_size):
    for x in range(0, grayscale.shape[1] - block_size, block_size):
        block = grayscale[y:y+block_size, x:x+block_size]
        pattern = tuple(block.flatten())
        if pattern in patterns:
            patterns[pattern] += 1
        else:
            patterns[pattern] = 1

# Find most common patterns
common_patterns = sorted(patterns.items(), key=lambda x: x[1], reverse=True)[:10]
print(f'Most common {block_size}x{block_size} patterns:')
for pattern, count in common_patterns:
    print(f'  Count {count}: First 8 values: {pattern[:8]}')

# Method 4: Check for hidden data in the image structure
print('\n--- Method 4: Image structure analysis ---')
# Look for unusual structures in the image
# Check for horizontal or vertical lines with unusual patterns

# Check horizontal lines
print('Checking horizontal lines...')
for y in range(grayscale.shape[0]):
    row = grayscale[y, :]
    # Check if this row has any unusual pattern
    # (e.g., alternating values, specific sequences)
    if len(np.unique(row)) > 100:  # Many different values
        # Check for specific patterns
        diffs = np.diff(row.astype(int))
        if np.any(np.abs(diffs) > 100):  # Large jumps
            print(f'Row {y}: Large value jumps detected')
            # Find where jumps occur
            jump_positions = np.where(np.abs(diffs) > 100)[0]
            for pos in jump_positions[:5]:
                print(f'  Jump at column {pos}: {row[pos]} -> {row[pos+1]}')

# Method 5: Check if the image contains hidden data using LSB with different offsets
print('\n--- Method 5: LSB with different offsets ---')
for offset in range(8):
    # Extract bits with specific offset
    bits = (grayscale.flatten() >> offset) & 1
    
    # Convert to bytes
    bytes_data = []
    for i in range(0, len(bits), 8):
        if i + 8 <= len(bits):
            byte = 0
            for j in range(8):
                byte = (byte << 1) | bits[i + j]
            bytes_data.append(byte)
    
    # Check if it looks like text
    text_data = bytes(bytes_data[:100])
    printable = all(32 <= b <= 126 or b in [10, 13] for b in text_data)
    print(f'Offset {offset}: {len(bytes_data)} bytes, printable: {printable}')
    if printable:
        print(f'  Text: {text_data.decode("ascii", errors="replace")}')