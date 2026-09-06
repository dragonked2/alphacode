from PIL import Image
import numpy as np

# Load the image
img = Image.open('puzzle_image.png')
img_array = np.array(img)

print(f'Image mode: {img.mode}')
print(f'Array shape: {img_array.shape}')

# Extract grayscale and alpha channels
grayscale = img_array[:, :, 0]
alpha = img_array[:, :, 1]

print(f'\nGrayscale stats:')
print(f'  Min: {grayscale.min()}, Max: {grayscale.max()}')
print(f'  Unique values: {len(np.unique(grayscale))}')
print(f'  Non-zero values: {np.count_nonzero(grayscale)}')

print(f'\nAlpha stats:')
print(f'  Min: {alpha.min()}, Max: {alpha.max()}')
print(f'  Unique values: {len(np.unique(alpha))}')
print(f'  Non-zero values: {np.count_nonzero(alpha)}')

# Check different bit planes
print('\n=== Bit plane analysis ===')
for bit in range(8):
    plane = (grayscale >> bit) & 1
    ones = np.sum(plane)
    zeros = plane.size - ones
    print(f'Bit {bit}: {ones} ones, {zeros} zeros, ratio: {ones/plane.size:.4f}')

# Check LSB specifically
lsb = grayscale & 1
print(f'\nLSB stats:')
print(f'  All ones: {np.all(lsb == 1)}')
print(f'  All zeros: {np.all(lsb == 0)}')
print(f'  Ones count: {np.sum(lsb == 1)}')
print(f'  Zeros count: {np.sum(lsb == 0)}')

# Look at alpha channel more closely
print('\n=== Alpha channel analysis ===')
alpha_nonzero = alpha[alpha > 0]
if len(alpha_nonzero) > 0:
    print(f'Non-zero alpha values: {len(alpha_nonzero)}')
    print(f'Unique non-zero values: {np.unique(alpha_nonzero)}')
    
    # Find locations of non-zero alpha
    locations = np.where(alpha > 0)
    print(f'Number of non-zero alpha locations: {len(locations[0])}')
    if len(locations[0]) > 0:
        print(f'First 10 locations: {list(zip(locations[0][:10], locations[1][:10]))}')
        print(f'First 10 alpha values: {alpha[locations[0][:10], locations[1][:10]]}')

# Try to find patterns in specific areas
print('\n=== Looking for hidden patterns ===')
# Check if there's data in specific value ranges
value_ranges = [(0, 50), (50, 100), (100, 150), (150, 200), (200, 255)]
for low, high in value_ranges:
    mask = (grayscale >= low) & (grayscale <= high)
    count = np.sum(mask)
    if count > 0:
        print(f'Range {low}-{high}: {count} pixels ({count/grayscale.size*100:.2f}%)')