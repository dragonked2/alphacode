from PIL import Image
import numpy as np
from hashlib import sha256
import hashlib

# Load the image
img = Image.open('puzzle_image.png')
img_array = np.array(img)

# Extract LSBs from the grayscale channel
grayscale = img_array[:, :, 0]
lsbs = grayscale.flatten() & 1

# Convert LSBs to bytes
lsb_bytes = []
for i in range(0, len(lsbs), 8):
    if i + 8 <= len(lsbs):
        byte = 0
        for j in range(8):
            byte = (byte << 1) | lsbs[i + j]
        lsb_bytes.append(byte)

# Convert to bytes
lsb_data = bytes(lsb_bytes)

print(f'Total LSB bytes: {len(lsb_data)}')
print(f'First 100 bytes (hex): {lsb_data[:100].hex()}')
print(f'First 100 bytes (ASCII): {lsb_data[:100].decode("ascii", errors="replace")}')

# Look for Ethereum private key patterns
# Private keys are 32 bytes (64 hex chars)
print('\n=== Looking for 32-byte patterns ===')
for i in range(len(lsb_data) - 32):
    chunk = lsb_data[i:i+32]
    # Check if it looks like a private key (not all zeros, not all ones)
    if any(chunk) and not all(chunk):
        # Try to convert to integer
        key_int = int.from_bytes(chunk, byteorder='big')
        # Check if it's in valid range for secp256k1
        secp256k1_order = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
        if 0 < key_int < secp256k1_order:
            print(f'Potential key at offset {i}: {chunk.hex()}')
            # Try to derive Ethereum address
            try:
                # This would require ecdsa library to derive address
                # For now, just print the potential key
                print(f'  As hex: 0x{chunk.hex()}')
            except:
                pass

# Also check for text patterns
print('\n=== Looking for text patterns ===')
# Look for "0x" patterns (Ethereum addresses)
for i in range(len(lsb_data) - 42):
    if lsb_data[i:i+2] == b'0x':
        potential_addr = lsb_data[i:i+42]
        if all(c in b'0123456789abcdefABCDEF' for c in potential_addr):
            print(f'Potential address at offset {i}: {potential_addr.decode("ascii")}')