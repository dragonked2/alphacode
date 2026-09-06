from PIL import Image, ImageEnhance, ImageOps
import numpy as np
import struct
import zlib
from ecdsa import SigningKey, SECP256k1
import hashlib

TARGET = "0xff2142e98e09b5344994f9beb9c56c95506b9f17"
secp256k1_order = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141

def check_key(key_bytes):
    if len(key_bytes) != 32:
        return False
    key_int = int.from_bytes(key_bytes, 'big')
    if not (0 < key_int < secp256k1_order):
        return False
    try:
        sk = SigningKey.from_string(key_bytes, curve=SECP256k1)
        vk = sk.get_verifying_key()
        public_key = b'\x04' + vk.to_string()
        keccak = hashlib.sha3_256(public_key).digest()
        address = '0x' + keccak[-20:].hex()
        return address.lower() == TARGET
    except:
        return False

# Load the image
img = Image.open('puzzle_image.png')
gray = np.array(img.convert('L'), dtype=np.uint8)

print("=== Enhancing specific regions to look for hidden text ===")

# Focus on the boat region - the author felt PK is on the biggest boat
# Based on the open-crypto-puzzles analysis: sailboat bounding box x:44-359, y:320-599
# Let's also look at the area where the watermark should be

regions = {
    "boat_full": (320, 44, 600, 360),
    "boat_center": (400, 80, 550, 300),
    "boat_hull": (500, 44, 600, 360),
    "skyline": (18, 0, 320, 400),
    "watermark": (1050, 500, 1105, 800),  # Where "twitter.com/ArweaveP" should be
}

for name, (y1, x1, y2, x2) in regions.items():
    region = gray[y1:y2, x1:x2]
    
    # Try extreme contrast enhancement
    img_region = Image.fromarray(region)
    enhancer = ImageEnhance.Contrast(img_region)
    high_contrast = enhancer.enhance(20.0)
    
    # Try auto-contrast
    auto = ImageOps.autocontrast(img_region, cutoff=1)
    
    # Save for inspection
    high_contrast.save(f'enhanced_{name}_contrast.png')
    auto.save(f'enhanced_{name}_auto.png')
    
    # Try to find text by looking at very specific value ranges
    # Text would be very dark (near 0) against light background (near 255)
    dark_pixels = (region < 50)
    if np.sum(dark_pixels) > 0:
        print(f"{name}: {np.sum(dark_pixels)} dark pixels (< 50)")

print("\n=== Trying to extract key from dark pixel positions ===")
# If text is written in dark pixels on light background, the positions might encode the key
for threshold in [10, 20, 30, 40, 50, 60, 70, 80, 90, 100]:
    dark_mask = gray < threshold
    dark_positions = np.where(dark_mask)
    
    if len(dark_positions[0]) > 0:
        # Convert positions to bytes
        pos_bytes = bytes([dark_positions[0][i] % 256 for i in range(min(32, len(dark_positions[0])))])
        if check_key(pos_bytes):
            print(f"  MATCH from positions at threshold {threshold}!")
        
        # Try combining row and column
        combined = []
        for i in range(min(32, len(dark_positions[0]))):
            combined.append(dark_positions[0][i] % 256)
            combined.append(dark_positions[1][i] % 256)
        if len(combined) >= 32:
            if check_key(bytes(combined[:32])):
                print(f"  MATCH from combined positions at threshold {threshold}!")

print("\n=== Trying PRNG seeded with image hash ===")
import random

img_bytes = gray.tobytes()
for hash_func in [hashlib.sha256, hashlib.md5, hashlib.sha1]:
    seed = int.from_bytes(hash_func(img_bytes).digest(), 'big')
    
    for seed_variant in [seed, seed ^ 0xFFFFFFFF, seed + 1, seed - 1]:
        rng = random.Random(seed_variant)
        key = bytes([rng.randint(0, 255) for _ in range(32)])
        if check_key(key):
            print(f"  MATCH with {hash_func.__name__} seed variant {seed_variant}!")

print("\n=== Trying to read values in very specific orders ===")

# Read only non-zero, non-255 values
non_std_positions = []
for y in range(1105):
    for x in range(1600):
        v = int(gray[y, x])
        if 0 < v < 255:
            non_std_positions.append((y, x, v))

print(f"Non-standard pixels: {len(non_std_positions)}")

# Try reading these in different orders
for sort_key in ['row', 'col', 'value', 'row_col', 'col_row']:
    if sort_key == 'row':
        sorted_pos = sorted(non_std_positions, key=lambda p: (p[0], p[1]))
    elif sort_key == 'col':
        sorted_pos = sorted(non_std_positions, key=lambda p: (p[1], p[0]))
    elif sort_key == 'value':
        sorted_pos = sorted(non_std_positions, key=lambda p: p[2])
    elif sort_key == 'row_col':
        sorted_pos = sorted(non_std_positions, key=lambda p: (p[0], p[1]))
    elif sort_key == 'col_row':
        sorted_pos = sorted(non_std_positions, key=lambda p: (p[1], p[0]))
    
    vals = [p[2] for p in sorted_pos]
    
    # Try different subsets
    for start in range(0, min(1000, len(vals) - 32), 10):
        key = bytes(vals[start:start + 32])
        if check_key(key):
            print(f"  MATCH from non-std values sorted by {sort_key} at position {start}!")
        
        # Try lower nibbles
        nibbles = [v % 16 for v in vals[start:start + 32]]
        hex_str = ''.join(f'{n:x}' for n in nibbles)
        key = bytes.fromhex(hex_str)
        if check_key(key):
            print(f"  MATCH from non-std nibbles sorted by {sort_key} at position {start}!")

print("\n=== Trying to use only the alpha channel's near-255 values ===")
# The README mentioned alpha values 225-254 near the sailboat
alpha = np.array(img)[:, :, 1]
near_255_mask = (alpha > 225) & (alpha < 255)
near_255_positions = np.where(near_255_mask)
print(f"Alpha values 226-254: {len(near_255_positions[0])} pixels")

# These form the anti-aliasing halo around the sailboat
# Their values could encode data
if len(near_255_positions[0]) > 0:
    alpha_vals = [int(alpha[near_255_positions[0][i], near_255_positions[1][i]]) 
                  for i in range(len(near_255_positions[0]))]
    print(f"Alpha values: {sorted(set(alpha_vals))}")
    
    # Try as key bytes
    for start in range(0, max(1, len(alpha_vals) - 32), 1):
        key = bytes(alpha_vals[start:start + 32])
        if len(key) == 32 and check_key(key):
            print(f"  MATCH from alpha values at position {start}!")

print("\n=== Trying to use only the non-standard alpha values ===")
non_std_alpha_mask = (alpha > 0) & (alpha < 255)
non_std_alpha_positions = np.where(non_std_alpha_mask)
non_std_alpha_vals = [int(alpha[non_std_alpha_positions[0][i], non_std_alpha_positions[1][i]]) 
                      for i in range(len(non_std_alpha_positions[0]))]
print(f"Non-standard alpha values: {len(non_std_alpha_vals)} values")
print(f"Values: {non_std_alpha_vals}")

# Try as key
if len(non_std_alpha_vals) >= 32:
    key = bytes(non_std_alpha_vals[:32])
    if check_key(key):
        print(f"  MATCH from first 32 alpha values!")

    # Try sorted by position
    sorted_alpha = sorted(zip(non_std_alpha_positions[0], non_std_alpha_positions[1], non_std_alpha_vals))
    sorted_vals = [v for _, _, v in sorted_alpha]
    key = bytes(sorted_vals[:32])
    if check_key(key):
        print(f"  MATCH from sorted alpha values!")
    
    # Try different combinations
    for mask_val in range(1, 255):
        masked = [v & mask_val for v in non_std_alpha_vals]
        key = bytes(masked[:32])
        if check_key(key):
            print(f"  MATCH from alpha values ANDed with {mask_val}!")
    
    for xor_val in range(1, 255):
        xored = [v ^ xor_val for v in non_std_alpha_vals]
        key = bytes(xored[:32])
        if check_key(key):
            print(f"  MATCH from alpha values XORed with {xor_val}!")

print("\nNo match found.")
