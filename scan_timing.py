import struct, zlib
import numpy as np
import time
from coincurve import PrivateKey
import hashlib

TARGET = bytes.fromhex("ff2142e98e09b5344994f9beb9c56c95506b9f17")
ORDER = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141

def check_key(key_bytes):
    key_int = int.from_bytes(key_bytes, 'big')
    if key_int == 0 or key_int >= ORDER: return False
    try:
        sk = PrivateKey(key_bytes)
        pub = sk.public_key.format(compressed=False)
        addr = hashlib.new('sha3_256', pub).digest()[12:]
        return addr == TARGET
    except: return False

print("Loading image...")
with open('puzzle_image.png', 'rb') as f:
    raw = f.read()
pos = 8; idat_raw = b''
while pos < len(raw):
    length = struct.unpack('>I', raw[pos:pos+4])[0]
    ct = raw[pos+4:pos+8]
    cd = raw[pos+8:pos+8+length]
    if ct == b'IDAT': idat_raw += cd
    pos += 12 + length
decompressed = zlib.decompress(idat_raw)

rows = []
pr = None
for ri in range(1105):
    s = ri * 3201
    ft = decompressed[s]
    rd = bytearray(decompressed[s+1:s+3201])
    if ft == 1:
        for i in range(2, len(rd)):
            rd[i] = (rd[i] + rd[i-2]) & 0xFF
    elif ft == 2:
        if pr:
            for i in range(len(rd)):
                rd[i] = (rd[i] + pr[i]) & 0xFF
    elif ft == 4:
        for i in range(len(rd)):
            l = rd[i-2] if i >= 2 else 0
            u = pr[i] if pr else 0
            ul = pr[i-2] if pr and i >= 2 else 0
            p = l + u - ul
            pa, pb, pc = abs(p - l), abs(p - u), abs(p - ul)
            if pa <= pb and pa <= pc: pred = l
            elif pb <= pc: pred = u
            else: pred = ul
            rd[i] = (rd[i] + pred) & 0xFF
    rows.append(bytes(rd))
    pr = rows[-1]

all_bytes = b''.join(rows)
gf = np.frombuffer(all_bytes, dtype=np.uint8)[0::2].copy()
af = np.frombuffer(all_bytes, dtype=np.uint8)[1::2].copy()
del all_bytes, rows
print(f"Loaded. Pixels: {len(gf)}")

# Timing test
print("\nTiming: 1-bit LSB gray bit0 raster")
bits = ((gf >> 0) & 1).astype(np.uint8)
packed = np.packbits(bits)
n = len(packed)
print(f"  Packed bytes: {n}, checking 32-byte windows: {n-32}")

start = time.time()
checked = 0
for off in range(min(10000, n-32)):
    if check_key(bytes(packed[off:off+32])):
        print(f"  FOUND at {off}!")
        break
    checked += 1
elapsed = time.time() - start
rate = checked / elapsed
print(f"  {checked} checks in {elapsed:.1f}s = {rate:.0f} checks/sec")
print(f"  Estimated time for full scan: {(n-32)/rate:.0f}s = {(n-32)/rate/60:.1f}min")
