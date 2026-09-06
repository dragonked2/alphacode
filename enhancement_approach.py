from PIL import Image, ImageEnhance, ImageOps
import numpy as np
from ecdsa import SigningKey, SECP256k1
import hashlib

# Load the image
img = Image.open('puzzle_image.png')
img_array = np.array(img)
grayscale = img_array[:, :, 0]

print("=== Trying image enhancement approaches ===")

# 1. Try to enhance contrast to reveal hidden text
print("\n--- Method 1: Extreme contrast enhancement ---")
enhanced = Image.fromarray(grayscale)
enhancer = ImageEnhance.Contrast(enhanced)
high_contrast = enhancer.enhance(10.0)
high_contrast.save('high_contrast.png')

# 2. Try thresholding
print("\n--- Method 2: Thresholding ---")
thresholds = [128, 100, 150, 50, 200]
for thresh in thresholds:
    binary = ((grayscale > thresh) * 255).astype(np.uint8)
    img_binary = Image.fromarray(binary)
    img_binary.save(f'threshold_{thresh}.png')
    print(f"Saved threshold_{thresh}.png")

# 3. Try to extract data from specific bit planes
print("\n--- Method 3: Bit plane extraction ---")
for bit in range(8):
    plane = ((grayscale >> bit) & 1) * 255
    img_plane = Image.fromarray(plane.astype(np.uint8))
    img_plane.save(f'bitplane_{bit}.png')
    print(f"Saved bitplane_{bit}.png")

# 4. Try to find text by looking at rows with specific patterns
print("\n--- Method 4: Looking for text patterns ---")
# Text would have specific pixel patterns - let's look for rows that could contain text
for row in range(grayscale.shape[0]):
    row_data = grayscale[row, :]
    # Check if this row has alternating dark/light patterns (like text)
    diffs = np.abs(np.diff(row_data.astype(int)))
    if np.sum(diffs > 50) > 10:  # Many large jumps
        # This row might contain text
        # Try to extract text by thresholding
        text_row = ((row_data > 128) * 255).astype(np.uint8)
        # Check if this looks like text (has both black and white)
        if np.sum(text_row == 0) > 10 and np.sum(text_row == 255) > 10:
            print(f"Row {row} might contain text")

# 5. Try to use the image as a key derivation source
print("\n--- Method 5: Using image as key derivation source ---")
# Maybe the key is derived from the image content somehow
# Let's try SHA256 of the image data
img_hash = hashlib.sha256(grayscale.tobytes()).digest()
print(f"Image SHA256 hash: {img_hash.hex()}")

# Try using the hash as a key
key_int = int.from_bytes(img_hash, 'big')
secp256k1_order = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
if 0 < key_int < secp256k1_order:
    print(f"Hash as key: Valid!")
    sk = SigningKey.from_string(img_hash, curve=SECP256k1)
    vk = sk.get_verifying_key()
    public_key = b'\x04' + vk.to_string()
    keccak = hashlib.sha3_256(public_key).digest()
    address = '0x' + keccak[-20:].hex()
    print(f"Derived address: {address}")
    print(f"Match: {address.lower() == '0xff2142e98e09b5344994f9eb9c56c95506b9f17'.lower()}")

# 6. Try to use the image data in a different way
print("\n--- Method 6: XOR with specific patterns ---")
# Try XORing with common patterns
patterns = [
    bytes(32),  # All zeros
    bytes([0xff] * 32),  # All ones
    bytes(range(32)),  # 0,1,2,...,31
    bytes([i * 8 for i in range(32)]),  # Multiples of 8
]

for pattern in patterns:
    # Take first 32 bytes of image data
    img_data = grayscale.tobytes()[:32]
    # XOR
    xored = bytes(a ^ b for a, b in zip(img_data, pattern))
    key_int = int.from_bytes(xored, 'big')
    if 0 < key_int < secp256k1_order:
        print(f"XOR with {pattern[:8].hex()}: Valid key!")
        sk = SigningKey.from_string(xored, curve=SECP256k1)
        vk = sk.get_verifying_key()
        public_key = b'\x04' + vk.to_string()
        keccak = hashlib.sha3_256(public_key).digest()
        address = '0x' + keccak[-20:].hex()
        print(f"Derived address: {address}")
        print(f"Match: {address.lower() == '0xff2142e98e09b5344994f9eb9c56c95506b9f17'.lower()}")

# 7. Try to use specific regions of the image
print("\n--- Method 7: Using specific image regions ---")
regions = [
    (0, 0, 32, 32),      # Top-left corner
    (0, 0, 1600, 32),    # First 32 rows
    (1073, 0, 1105, 32), # Last 32 rows
]

for y1, x1, y2, x2 in regions:
    region = grayscale[y1:y2, x1:x2]
    region_bytes = region.tobytes()[:32]
    key_int = int.from_bytes(region_bytes, 'big')
    if 0 < key_int < secp256k1_order:
        print(f"Region ({y1},{x1})-({y2},{x2}): Valid key!")
        sk = SigningKey.from_string(region_bytes, curve=SECP256k1)
        vk = sk.get_verifying_key()
        public_key = b'\x04' + vk.to_string()
        keccak = hashlib.sha3_256(public_key).digest()
        address = '0x' + keccak[-20:].hex()
        print(f"Derived address: {address}")
        print(f"Match: {address.lower() == '0xff2142e98e09b5344994f9eb9c56c95506b9f17'.lower()}")

print("\n=== Analysis complete ===")
print("Generated enhanced images for visual inspection:")
print("- high_contrast.png")
print("- threshold_*.png")
print("- bitplane_*.png")
