from PIL import Image
import numpy as np

# Load the image
img = Image.open('puzzle_image.png')
img_array = np.array(img)

# Extract grayscale channel
grayscale = img_array[:, :, 0]

print('=== Trying matrix reshape approach ===')
print(f'Original shape: {grayscale.shape}')
print(f'Total pixels: {grayscale.size}')

# Try different reshape configurations
# The idea is that the original image with hidden text was reshaped
# Let's try different dimensions that multiply to the same total
total_pixels = grayscale.size

# Find factors of total_pixels
factors = []
for i in range(1, int(np.sqrt(total_pixels)) + 1):
    if total_pixels % i == 0:
        factors.append((i, total_pixels // i))

print(f'\nPossible reshape dimensions (factors of {total_pixels}):')
for h, w in factors[:20]:
    print(f'  {h} x {w}')

# Try reshaping with different dimensions
print('\n=== Trying different reshapes ===')
for h, w in factors[:10]:
    if h != grayscale.shape[0] and w != grayscale.shape[1]:
        try:
            reshaped = grayscale.reshape(h, w)
            # Save as image to inspect
            img_reshaped = Image.fromarray(reshaped)
            img_reshaped.save(f'reshaped_{h}x{w}.png')
            print(f'Reshaped to {h}x{w} and saved as reshaped_{h}x{w}.png')
        except Exception as e:
            print(f'Error reshaping to {h}x{w}: {e}')

# Let's also try transposing
print('\n=== Transpose and other transformations ===')
transposed = grayscale.T
print(f'Transposed shape: {transposed.shape}')
img_transposed = Image.fromarray(transposed)
img_transposed.save('transposed.png')

# Try rotating
for angle in [90, 180, 270]:
    rotated = np.rot90(grayscale, angle // 90)
    img_rotated = Image.fromarray(rotated)
    img_rotated.save(f'rotated_{angle}.png')
    print(f'Rotated {angle} degrees and saved as rotated_{angle}.png')

# Let's look for patterns in specific value ranges
print('\n=== Analyzing specific value ranges ===')
# Check if there's data hidden in specific bit ranges
for low, high in [(0, 1), (2, 3), (4, 7), (8, 15), (16, 31), (32, 63), (64, 127), (128, 255)]:
    mask = (grayscale >= low) & (grayscale <= high)
    count = np.sum(mask)
    if count > 0:
        # Get the positions
        positions = np.where(mask)
        print(f'Range {low}-{high}: {count} pixels')
        if count < 100:
            print(f'  Positions: {list(zip(positions[0][:20], positions[1][:20]))}')
            print(f'  Values: {grayscale[positions[0][:20], positions[1][:20]]}')