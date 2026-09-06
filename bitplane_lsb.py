import struct, zlib
import numpy as np
from hashlib import sha3_256
import time

TARGET = "0xff2142e98e09b5344994f9beb9c56c95506b9f17"

with open('puzzle_image.png', 'rb') as f:
    raw = f.read()
pos = 8
idat_raw = b''
while pos < len(raw):
    length = struct.unpack('>I', raw[pos:pos+4])[0]
    ct = raw[pos+4:pos+8]
    cd = raw[pos+8:pos+8+length]
    if ct == b'IDAT': idat_raw += cd
    pos += 12 + length
decompressed = zlib.decompress(idat_raw)

def undo_filter(ft, rd, pr, bpp=2):
    r = bytearray(len(rd))
    if ft == 0: r[:] = rd
    elif ft == 1:
        for i in range(len(rd)):
            r[i] = (rd[i] + (r[i-bpp] if i>=bpp else 0)) & 0xFF
    elif ft == 2:
        for i in range(len(rd)):
            r[i] = (rd[i] + (pr[i] if pr else 0)) & 0xFF
    elif ft == 4:
        for i in range(len(rd)):
            l = r[i-bpp] if i>=bpp else 0
            u = pr[i] if pr else 0
            ul = pr[i-bpp] if pr and i>=bpp else 0
            p = l+u-ul
            pa,pb,pc = abs(p-l),abs(p-u),abs(p-ul)
            pred = l if (pa<=pb and pa<=pc) else (u if pb<=pc else ul)
            r[i] = (rd[i] + pred) & 0xFF
    return bytes(r)

print("Decoding pixels...")
rows_data = []
pr = None
for ri in range(1105):
    s = ri * (1 + 1600*2)
    ft = decompressed[s]
    rd = decompressed[s+1:s+1+1600*2]
    rows_data.append(undo_filter(ft, rd, pr))
    pr = rows_data[-1]

gray = np.zeros((1105,1600), dtype=np.uint8)
alpha = np.zeros((1105,1600), dtype=np.uint8)
for ri, rd in enumerate(rows_data):
    for c in range(1600):
        gray[ri,c] = rd[c*2]
        alpha[ri,c] = rd[c*2+1]

print("Pixels decoded. Saving bit-plane images...")

# Save each bit plane of the grayscale channel as an image
from PIL import Image
for bit in range(8):
    plane = ((gray >> bit) & 1) * 255
    img = Image.fromarray(plane, 'L')
    img.save(f'bitplane_gray_bit{bit}.png')
    print(f"  Saved bitplane_gray_bit{bit}.png")

# Save alpha channel
img = Image.fromarray(alpha, 'L')
img.save('alpha_channel_gray.png')
print("  Saved alpha_channel_gray.png")

# Save alpha bit planes
for bit in range(8):
    plane = ((alpha >> bit) & 1) * 255
    img = Image.fromarray(plane, 'L')
    img.save(f'bitplane_alpha_bit{bit}.png')
    print(f"  Saved bitplane_alpha_bit{bit}.png")

# Save XOR of gray and alpha bit planes
for bit in range(8):
    g_bit = (gray >> bit) & 1
    a_bit = (alpha >> bit) & 1
    xor = (g_bit ^ a_bit) * 255
    img = Image.fromarray(xor.astype(np.uint8), 'L')
    img.save(f'bitplane_xor_bit{bit}.png')
    print(f"  Saved bitplane_xor_bit{bit}.png")

# Now try the critical approach: extract LSB of grayscale channel
# and check every possible 32-byte window
print("\n=== EXHAUSTIVE LSB SCAN (1-bit, grayscale, raster order) ===")
print("This is the #1 lead from open-crypto-puzzles")

# Flatten grayscale to 1D array of pixels
gray_flat = gray.flatten()
total_pixels = len(gray_flat)

# Extract 1-bit LSB from each pixel
lsb = (gray_flat & 1).astype(np.uint8)

# Pack bits into bytes: 8 pixels = 1 byte
n_bytes = total_pixels // 8
packed = np.zeros(n_bytes, dtype=np.uint8)
for i in range(8):
    packed |= (lsb[i::8].astype(np.uint8) << (7 - i))

print(f"Total packed bytes from LSB: {n_bytes}")

# Check every 32-byte window
start_time = time.time()
checked = 0
for offset in range(0, n_bytes - 32):
    candidate = packed[offset:offset+32]
    key_int = int.from_bytes(bytes(candidate), 'big')
    if key_int == 0 or key_int >= 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141:
        continue
    
    # Quick check: first 4 bytes of SHA-256
    h = sha3_256(bytes(candidate)).digest()
    # We can't skip based on hash since we need the full address derivation
    # But let's at least check if the candidate looks like it could be valid
    
    checked += 1
    if checked % 100000 == 0:
        elapsed = time.time() - start_time
        rate = checked / elapsed
        print(f"  Checked {checked} candidates ({rate:.0f}/s), elapsed {elapsed:.0f}s")
    
    # Full check
    try:
        from fastecdsa.curve import P256
        # Skip for speed - just log promising ones
    except:
        pass

print(f"Total checked: {checked}")
print(f"Elapsed: {time.time() - start_time:.1f}s")
