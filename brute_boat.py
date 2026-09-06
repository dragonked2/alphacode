from PIL import Image, ImageFilter
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
    return False

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

# The sailboat is at approximately y:320-700, x:44-360
# Focus exclusively on extracting the key from the boat region

print("=== APPROACH: Try every 32-byte window in the sailboat region ===")
# Read the boat as a flat stream and try every possible 32-byte window
boat = gray[300:750, 0:500]
flat = boat.flatten()
print(f"  Boat region flat size: {len(flat)}")

# Try every 32-byte window
for start in range(0, len(flat) - 32, 1):
    key = bytes(flat[start:start+32])
    check_and_report(key, f"Boat flat offset {start}")

print(f"  Done. Tests: {total_tests}")

print("\n=== APPROACH: Try every 32-byte window reading columns instead of rows ===")
for col in range(boat.shape[1]):
    col_data = boat[:, col].flatten()
    for start in range(0, len(col_data) - 32, 1):
        key = bytes(col_data[start:start+32])
        check_and_report(key, f"Boat col {col} offset {start}")

print(f"  Done. Tests: {total_tests}")

print("\n=== APPROACH: Try every 32-byte window on the ENTIRE image ===")
flat_all = gray.flatten()
# Try every Nth position for speed
step = max(1, len(flat_all) // 5000000)
for start in range(0, len(flat_all) - 32, step):
    key = bytes(flat_all[start:start+32])
    check_and_report(key, f"Full image offset {start}")

print(f"  Done. Tests: {total_tests}")

print("\n=== APPROACH: XOR adjacent pixels in the boat ===")
for start in range(0, len(flat) - 64, 2):
    key = bytes([int(flat[start+i]) ^ int(flat[start+i+1]) for i in range(0, 64, 2)])
    check_and_report(key, f"Boat XOR pairs offset {start}")

print(f"  Done. Tests: {total_tests}")

print(f"\nTotal tests: {total_tests}")
