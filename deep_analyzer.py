import struct
from PIL import Image
import numpy as np

# Load the image
img = Image.open('puzzle_image.png')
print(f'Image mode: {img.mode}')
print(f'Image size: {img.size}')

# Convert to numpy array
img_array = np.array(img)
print(f'Array shape: {img_array.shape}')
print(f'Array dtype: {img_array.dtype}')

# Analyze the cHRM chunk data
chrm_data = bytes.fromhex('00007a26000080840000fa00000080e8000075300000ea6000003a9800001770')
print(f'\ncHRM chunk data (32 bytes): {chrm_data.hex()}')

# Try different interpretations
print('\n=== Potential private key interpretations ===')
print(f'As hex: 0x{chrm_data.hex()}')

# Try as 32-byte private key
private_key = int.from_bytes(chrm_data, byteorder='big')
print(f'As integer (big endian): {private_key}')
print(f'As hex string: {private_key:064x}')

# Try as 32-byte private key (little endian)
private_key_le = int.from_bytes(chrm_data, byteorder='little')
print(f'As integer (little endian): {private_key_le}')
print(f'As hex string (little endian): {private_key_le:064x}')

# Check if it's a valid Ethereum private key (should be less than secp256k1 curve order)
secp256k1_order = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
print(f'\nIs valid private key (big endian)? {0 < private_key < secp256k1_order}')
print(f'Is valid private key (little endian)? {0 < private_key_le < secp256k1_order}')

# Extract LSB from all channels
print('\n=== LSB Analysis ===')
# Get all pixel values
pixels = img_array.flatten()

# Extract LSBs
lsbs = pixels & 1
print(f'Number of LSB bits: {len(lsbs)}')

# Try to interpret as ASCII text
lsb_bits = ''.join(map(str, lsbs[:1000]))  # First 1000 bits
print(f'First 1000 LSB bits: {lsb_bits[:100]}...')

# Try to interpret as bytes
lsb_bytes = []
for i in range(0, min(1000, len(lsbs)), 8):
    byte = 0
    for j in range(8):
        if i + j < len(lsbs):
            byte = (byte << 1) | lsbs[i + j]
    lsb_bytes.append(byte)

# Try ASCII interpretation
try:
    ascii_text = bytes(lsb_bytes).decode('ascii', errors='ignore')
    print(f'First 125 bytes as ASCII: {ascii_text}')
except:
    print('Could not decode as ASCII')

# Check first row for anomalies
print('\n=== First row analysis ===')
first_row = img_array[0, :]
print(f'First row values (first 50): {first_row[:50]}')
print(f'Non-zero values in first row: {np.count_nonzero(first_row)}')
print(f'Unique values in first row: {np.unique(first_row)}')

# Check alpha channel if present
if img_array.shape[2] == 4:
    print('\n=== Alpha channel analysis ===')
    alpha = img_array[:, :, 3]
    print(f'Alpha channel non-zero pixels: {np.count_nonzero(alpha)}')
    print(f'Alpha channel unique values: {np.unique(alpha)}')