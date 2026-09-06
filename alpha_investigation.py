from PIL import Image
import numpy as np
import struct, zlib
from ecdsa import SigningKey, SECP256k1
import hashlib

TARGET = "0xff2142e98e09b5344994f9beb9c56c95506b9f17"
secp256k1_order = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141

def check_key(key_bytes):
    if len(key_bytes) != 32: return False, None
    key_int = int.from_bytes(key_bytes, 'big')
    if not (0 < key_int < secp256k1_order): return False, None
    try:
        sk = SigningKey.from_string(key_bytes, curve=SECP256k1)
        vk = sk.get_verifying_key()
        public_key = b'\x04' + vk.to_string()
        keccak = hashlib.sha3_256(public_key).digest()
        address = '0x' + keccak[-20:].hex()
        return address.lower() == TARGET, address
    except: return False, None

def check_and_report(key, label):
    global total_tests
    total_tests += 1
    match, addr = check_key(key)
    if match:
        print(f"\n*** SOLVED! ***")
        print(f"  Key: {key.hex()}")
        print(f"  Address: {addr}")
        print(f"  Method: {label}")
        exit(0)

total_tests = 0

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

rows = []
pr = None
for ri in range(1105):
    s = ri * (1 + 1600*2)
    ft = decompressed[s]
    rd = decompressed[s+1:s+1+1600*2]
    rows.append(undo_filter(ft, rd, pr))
    pr = rows[-1]

gray = np.zeros((1105,1600), dtype=np.uint8)
alpha = np.zeros((1105,1600), dtype=np.uint8)
for ri, rd in enumerate(rows):
    for c in range(1600):
        gray[ri,c] = rd[c*2]
        alpha[ri,c] = rd[c*2+1]

# Save alpha channel as image for visual inspection
alpha_img = Image.fromarray(alpha)
alpha_img.save('alpha_channel.png')

# Save alpha enhanced for visual inspection
alpha_enhanced = (alpha.astype(float) * 255 / 255).astype(np.uint8)
Image.fromarray(alpha_enhanced).save('alpha_enhanced.png')

# Invert alpha to see low values better
alpha_inverted = 255 - alpha
Image.fromarray(alpha_inverted).save('alpha_inverted.png')

# Create image showing only non-255 alpha pixels
alpha_masked = np.zeros_like(alpha)
alpha_masked[alpha < 255] = 255 - alpha[alpha < 255]
Image.fromarray(alpha_masked).save('alpha_non255_only.png')

print("Saved alpha visualizations")

# Look at alpha values in the critical rows (449-462) 
# where low alpha values concentrate
print("\n=== Alpha pattern in rows 449-465 ===")
for row in range(449, 466):
    alpha_row = alpha[row, :]
    unique = sorted(set(alpha_row))
    counts = {v: int(np.sum(alpha_row == v)) for v in unique}
    nonzero_counts = {k: v for k, v in counts.items() if v > 0}
    print(f"  Row {row}: {nonzero_counts}")

# The rows with alpha=6 and alpha=15 at 1288 pixels each
# Let's see what these rows look like
print("\n=== Detailed row 461 and 462 ===")
for row in [461, 462]:
    alpha_row = alpha[row, :]
    # Find transitions
    transitions = []
    prev = alpha_row[0]
    for i in range(1, len(alpha_row)):
        curr = alpha_row[i]
        if curr != prev:
            transitions.append((i, prev, curr))
            prev = curr
        else:
            prev = curr
    print(f"  Row {row} transitions: {transitions[:20]}")

# Now: what if the KEY is encoded in the ALPHA CHANNEL using bit manipulation?
# Let's try reading alpha values at specific columns for each row
# and interpreting them as key bytes

print("\n=== Try: Read alpha at column 0 for first 32 rows ===")
for start_row in range(0, 1105 - 32):
    key = bytes([int(alpha[start_row + c, c]) for c in range(32)])
    check_and_report(key, f"Alpha diagonal from row {start_row}")

print(f"\n=== Try: Alpha values at columns matching grayscale non-zero positions ===")
# For each row, find where gray != 0, read alpha there
for row in range(1105):
    nonzero_cols = np.where(gray[row, :] != 0)[0]
    if len(nonzero_cols) >= 32:
        alpha_at_nonzero = [int(alpha[row, c]) for c in nonzero_cols[:32]]
        key = bytes(alpha_at_nonzero)
        check_and_report(key, f"Alpha at gray!=0 in row {row}")
        
        # Try first 64 as nibble pairs
        if len(nonzero_cols) >= 64:
            alpha_64 = [int(alpha[row, c]) % 16 for c in nonzero_cols[:64]]
            hex_str = ''.join(f'{n:x}' for n in alpha_64)
            try:
                key = bytes.fromhex(hex_str[:64])
                check_and_report(key, f"Alpha nibbles at gray!=0 in row {row}")
            except:
                pass

print(f"\n=== Try: Combine alpha XOR gray as key source ===")
combined = np.bitwise_xor(gray, alpha)
combined_flat = combined.flatten()
for start in range(0, len(combined_flat) - 32, max(1, len(combined_flat)//500000)):
    key = bytes(combined_flat[start:start+32])
    check_and_report(key, f"Gray XOR Alpha at offset {start}")

print(f"\n=== Try: Raw byte sequences in the filter types ===")
# 1105 filter bytes, each 0-4
# Map to ASCII or try as binary
filter_str = ''
ftypes = []
s = 0
for ri in range(1105):
    s_pos = ri * (1 + 1600*2)
    ft = decompressed[s_pos]
    ftypes.append(ft)

# Try: map 0->'0', 1->'1', etc. and look for ASCII patterns
filter_ascii = bytes([48 + f for f in ftypes])
print(f"  Filter as ASCII: {filter_ascii[:100]}")

# Try: interpret filter bytes as pairs of bits
bits = ''
for f in ftypes:
    bits += f'{f:03b}'

print(f"  Filter bits length: {len(bits)}")
# Extract 256 bits
for start in range(0, len(bits) - 256, 1):
    key = bytearray()
    for i in range(32):
        byte_val = 0
        for j in range(8):
            byte_val = (byte_val << 1) | int(bits[start + i*8 + j])
        key.append(byte_val)
    check_and_report(bytes(key), f"Filter bits at offset {start}")

print(f"\n=== Try: The EXIF/ICC profile data ===")
# Check if there's an iCCP or other profile chunk
pos = 8
while pos < len(raw):
    length = struct.unpack('>I', raw[pos:pos+4])[0]
    ct = raw[pos+4:pos+8]
    cd = raw[pos+8:pos+8+length]
    if ct == b'iCCP':
        print(f"  Found iCCP: {length} bytes")
        # Decompress the profile data
        profile_name_end = cd.find(b'\x00')
        profile_name = cd[:profile_name_end]
        compression = cd[profile_name_end+1]
        compressed_data = cd[profile_name_end+2:]
        print(f"  Profile name: {profile_name}")
        print(f"  Compression: {compression}")
        try:
            if compression == 0:
                profile_data = zlib.decompress(compressed_data)
            else:
                profile_data = compressed_data
            print(f"  Profile data: {len(profile_data)} bytes")
            print(f"  First 64 bytes: {profile_data[:64].hex()}")
            # Check for key in profile data
            for offset in range(0, len(profile_data) - 32):
                check_and_report(profile_data[offset:offset+32], f"iCCP profile offset {offset}")
        except:
            pass
    pos += 12 + length

print(f"\nTotal tests: {total_tests}")
