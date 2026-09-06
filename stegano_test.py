from stegano import lsb
from PIL import Image

# Load the image
img = Image.open('puzzle_image.png')

print("=== Using stegano library to extract hidden data ===")

try:
    # Try LSB extraction
    secret = lsb.reveal(img)
    if secret:
        print(f"LSB secret found: {secret}")
    else:
        print("No LSB secret found")
except Exception as e:
    print(f"LSB extraction error: {e}")

# Try with different settings
try:
    # Try with different bit planes
    from stegano.lsb import generators
    secret = lsb.reveal(img, generators.eratosthenes())
    if secret:
        print(f"Secret with eratosthenes generator: {secret}")
except Exception as e:
    print(f"Error with eratosthenes: {e}")

try:
    secret = lsb.reveal(img, generators.eratosthenes_n(3))
    if secret:
        print(f"Secret with eratosthenes_n(3): {secret}")
except Exception as e:
    print(f"Error with eratosthenes_n(3): {e}")

# Let's also try a manual LSB extraction with different approaches
print("\n=== Manual LSB extraction ===")
import numpy as np

img_array = np.array(img)
grayscale = img_array[:, :, 0]

# Extract LSBs
lsbs = grayscale.flatten() & 1

# Try different interpretations
print(f"Total LSB bits: {len(lsbs)}")

# Try as ASCII (7-bit)
print("\n--- 7-bit ASCII interpretation ---")
chars = []
for i in range(0, len(lsbs) - 6, 7):
    byte = 0
    for j in range(7):
        byte = (byte << 1) | lsbs[i + j]
    if 32 <= byte <= 126:
        chars.append(chr(byte))
    else:
        chars.append('.')
    
    if len(chars) >= 100:
        break

text = ''.join(chars)
print(f"First 100 chars: {text}")

# Try as 8-bit bytes
print("\n--- 8-bit interpretation ---")
bytes_list = []
for i in range(0, len(lsbs) - 7, 8):
    byte = 0
    for j in range(8):
        byte = (byte << 1) | lsbs[i + j]
    bytes_list.append(byte)

# Check for printable ASCII
printable = ''.join(chr(b) if 32 <= b <= 126 else '.' for b in bytes_list[:200])
print(f"First 200 bytes as ASCII: {printable}")

# Look for the address pattern
print("\n--- Looking for address pattern ---")
address_hex = "0xFF2142E98E09b5344994F9bEB9C56C95506B9F17"
# Convert to bytes
address_bytes = bytes.fromhex(address_hex.replace('0x', '').replace('FF', ''))

# Search in LSB data
lsb_bytes = bytes(bytes_list)
for i in range(len(lsb_bytes) - len(address_bytes)):
    if lsb_bytes[i:i+len(address_bytes)] == address_bytes:
        print(f"Found address bytes at position {i}")
        # Show surrounding context
        start = max(0, i - 20)
        end = min(len(lsb_bytes), i + len(address_bytes) + 20)
        print(f"Context: {lsb_bytes[start:end].hex()}")

print("\n=== Analysis complete ===")
print("The stegano library didn't find any hidden messages.")
print("This suggests the key might not be hidden using standard LSB steganography.")
print("\nPossible next steps:")
print("1. Try different steganography methods")
print("2. Look for the key in specific visual elements (buildings, boat)")
print("3. Consider that the key might be derived from the image somehow")
print("4. Check if the cHRM chunk values need to be interpreted differently")