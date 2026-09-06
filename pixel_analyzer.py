from PIL import Image
import numpy as np

# Load the image
img = Image.open('puzzle_image.png')
img_array = np.array(img)

# Extract grayscale channel
grayscale = img_array[:, :, 0]

print('=== Detailed pixel analysis ===')
print(f'Image shape: {grayscale.shape}')

# Check if all pixels are either 0 or 255
unique_values = np.unique(grayscale)
print(f'Number of unique values: {len(unique_values)}')
print(f'Unique values: {unique_values}')

# Check the distribution
print(f'\nValue distribution:')
for val in unique_values[:20]:  # First 20 values
    count = np.sum(grayscale == val)
    print(f'  Value {val}: {count} pixels')

# Let's check specific regions
print('\n=== Region analysis ===')
# Top-left corner
top_left = grayscale[:100, :100]
print(f'Top-left 100x100: {np.unique(top_left)}')

# Center
center_y, center_x = grayscale.shape[0] // 2, grayscale.shape[1] // 2
center_region = grayscale[center_y-50:center_y+50, center_x-50:center_x+50]
print(f'Center region: {np.unique(center_region)}')

# Check if there's any pattern in the LSB
print('\n=== LSB pattern analysis ===')
lsb = grayscale & 1
print(f'LSB unique values: {np.unique(lsb)}')

# Let's look at the image more carefully
# Maybe the hidden data is in specific pixel values
print('\n=== Looking for hidden data in specific values ===')

# Check if there's data hidden in the alpha channel
alpha = img_array[:, :, 1]
print(f'Alpha channel unique values: {np.unique(alpha)}')

# The README mentioned alpha channel has non-zero values around the biggest boat
# Let's find where alpha is non-zero but not 255
alpha_nonstandard = (alpha > 0) & (alpha < 255)
if np.any(alpha_nonstandard):
    locations = np.where(alpha_nonstandard)
    print(f'Non-standard alpha locations: {len(locations[0])}')
    if len(locations[0]) > 0:
        print(f'First 5 locations: {list(zip(locations[0][:5], locations[1][:5]))}')
        print(f'First 5 alpha values: {alpha[locations[0][:5], locations[1][:5]]}')

# Let's try a different approach - check if the image contains text visually
# by looking at regions with unusual pixel patterns
print('\n=== Looking for unusual patterns ===')

# Check for rows or columns with unusual patterns
for row in range(min(20, grayscale.shape[0])):
    row_values = grayscale[row, :]
    if len(np.unique(row_values)) > 1:
        print(f'Row {row} has {len(np.unique(row_values))} unique values')
        print(f'  Unique values: {np.unique(row_values)[:10]}')

# Check the very first row more carefully
print('\n=== First row analysis ===')
first_row = grayscale[0, :]
print(f'First row unique values: {np.unique(first_row)}')
print(f'First row value counts:')
for val in np.unique(first_row):
    count = np.sum(first_row == val)
    print(f'  Value {val}: {count} pixels')