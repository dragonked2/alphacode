import struct, zlib
import numpy as np
import time
from coincurve import PrivateKey
import hashlib

TARGET = bytes.fromhex("ff2142e98e09b5344994f9beb9c56c95506b9f17")
ORDER = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
total_checks = 0
found = False

def check_key(key_bytes):
    global total_checks
    key_int = int.from_bytes(key_bytes, 'big')
    if key_int == 0 or key_int >= ORDER: return False
    total_checks += 1
    try:
        sk = PrivateKey(key_bytes)
        pub = sk.public_key.format(compressed=False)
        addr = hashlib.new('sha3_256', pub).digest()[12:]
        return addr == TARGET
    except: return False

# Load image
with open('puzzle_image.png', 'rb') as f:
    raw = f.read()
pos = 8; idat_raw = b''
while pos < len(raw):
    length = struct.unpack('>I', raw[pos:pos+4])[0]
    ct = raw[pos+4:pos+8]; cd = raw[pos+8:pos+8+length]
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
del all_bytes, rows, decompressed

gf_col = gf.reshape(1105, 1600).T.flatten()
af_col = af.reshape(1105, 1600).T.flatten()

gf_snake = np.concatenate([gf.reshape(1105,1600)[i::2].flatten() if i%2==0 else gf.reshape(1105,1600)[i::-1].flatten() for i in range(1105)])
af_snake = np.concatenate([af.reshape(1105,1600)[i::2].flatten() if i%2==0 else af.reshape(1105,1600)[i::-1].flatten() for i in range(1105)])

print(f"Image loaded. {len(gf)} pixels")
total_start = time.time()

def scan(data, name):
    global total_checks, found
    if found: return
    n = len(data)
    t0 = time.time()
    for off in range(0, n - 32):
        if check_key(bytes(data[off:off+32])):
            print(f"\n*** SOLVED! {name} off={off} key={data[off:off+32].hex()}")
            found = True; return
    print(f"  {name}: {n-32} checked in {time.time()-t0:.1f}s")

def scan_1bit(flat, bit, name, reverse=False, transpose=False):
    global total_checks, found
    if found: return
    if transpose:
        flat = flat.reshape(1105, 1600).T.flatten()
    if reverse:
        flat = flat[::-1]
    bits = ((flat >> bit) & 1).astype(np.uint8)
    packed = np.packbits(bits)
    scan(packed, name)

def scan_nbit(flat, bpp, shift, name, reverse=False, transpose=False, lsb_first=False):
    global total_checks, found
    if found: return
    if transpose:
        flat = flat.reshape(1105, 1600).T.flatten()
    if reverse:
        flat = flat[::-1]
    mask = ((1 << bpp) - 1) << shift
    extracted = ((flat & mask) >> shift).astype(np.uint8)
    ppb = 8 // bpp
    n_bytes = len(extracted) // ppb
    packed = np.zeros(n_bytes, dtype=np.uint8)
    if lsb_first:
        for i in range(ppb):
            packed |= (extracted[i::ppb].astype(np.uint8) << (bpp * i))
    else:
        for i in range(ppb):
            packed |= (extracted[i::ppb].astype(np.uint8) << (8 - bpp * (i + 1)))
    scan(packed, name)

# === 8-BIT (full byte) scans ===
print("\n=== 8-BIT FULL BYTE ===")
for name, data in [
    ("gray_8_raster", gf), ("gray_8_rev", gf[::-1]),
    ("gray_8_col", gf_col), ("gray_8_col_rev", gf_col[::-1]),
    ("gray_8_snake", gf_snake), ("gray_8_snake_rev", gf_snake[::-1]),
    ("alpha_8_raster", af), ("alpha_8_rev", af[::-1]),
    ("alpha_8_col", af_col), ("alpha_8_col_rev", af_col[::-1]),
    ("alpha_8_snake", af_snake), ("alpha_8_snake_rev", af_snake[::-1]),
]:
    print(f"\n{name}")
    scan(data, name)

# === 1-BIT scans ===
print("\n=== 1-BIT LSB ===")
for ch_name, flat in [("gray", gf), ("alpha", af)]:
    for bit in range(8):
        for rev in [False, True]:
            for col in [False, True]:
                for lsf in [False, True]:
                    tag = f"{ch_name}_1b{bit}{'_rev' if rev else ''}{'_col' if col else ''}{'_lsbf' if lsf else ''}"
                    print(f"\n{tag}")
                    if lsf:
                        scan_nbit(flat, 1, bit, tag, reverse=rev, transpose=col, lsb_first=True)
                    else:
                        scan_1bit(flat, bit, tag, reverse=rev, transpose=col)

# === 2-BIT scans ===
print("\n=== 2-BIT ===")
for ch_name, flat in [("gray", gf), ("alpha", af)]:
    for shift in [0, 2, 4, 6]:
        for rev in [False, True]:
            for col in [False, True]:
                for lsf in [False, True]:
                    tag = f"{ch_name}_2b{shift}{'_rev' if rev else ''}{'_col' if col else ''}{'_lsbf' if lsf else ''}"
                    print(f"\n{tag}")
                    scan_nbit(flat, 2, shift, tag, reverse=rev, transpose=col, lsb_first=lsf)

# === 4-BIT scans ===
print("\n=== 4-BIT ===")
for ch_name, flat in [("gray", gf), ("alpha", af)]:
    for shift in [0, 4]:
        for rev in [False, True]:
            for col in [False, True]:
                for lsf in [False, True]:
                    tag = f"{ch_name}_4b{shift}{'_rev' if rev else ''}{'_col' if col else ''}{'_lsbf' if lsf else ''}"
                    print(f"\n{tag}")
                    scan_nbit(flat, 4, shift, tag, reverse=rev, transpose=col, lsb_first=lsf)

# === COMBINED CHANNELS ===
print("\n=== COMBINED CHANNELS ===")

# XOR
xored = (gf ^ af)
for name, data in [
    ("xor_raster", xored), ("xor_rev", xored[::-1]),
    ("xor_col", xored.reshape(1105,1600).T.flatten()),
    ("xor_snake", np.concatenate([xored.reshape(1105,1600)[i::2].flatten() if i%2==0 else xored.reshape(1105,1600)[i::-1].flatten() for i in range(1105)])),
]:
    print(f"\n{name}")
    scan(data, name)

# AND
anded = (gf & af)
print("\nAND raster")
scan(anded, "and_raster")
print("AND rev")
scan(anded[::-1], "and_rev")

# OR
ored = (gf | af)
print("\nOR raster")
scan(ored, "or_raster")
print("OR rev")
scan(ored[::-1], "or_rev")

# Interleave
interleaved = np.empty(len(gf)*2, dtype=np.uint8)
interleaved[0::2] = gf; interleaved[1::2] = af
print("\ninterleave raster")
scan(interleaved, "interleave_raster")
print("interleave rev")
scan(interleaved[::-1], "interleave_rev")

# Gray first half + alpha second half
half1 = gf[:len(gf)//2]
half2 = af[len(af)//2:]
concat1 = np.concatenate([half1, half2])
print("\nconcat halves")
scan(concat1, "concat_halves")

# Flip channels
flip = np.empty_like(gf)
flip[0::2] = af[0::2]; flip[1::2] = gf[1::2]
print("\nchannel flip")
scan(flip, "channel_flip")

# 128-threshold binary
binary = ((gf > 127).astype(np.uint8) * 255)
print("\nbinary threshold")
scan(binary, "binary_127")

binary2 = ((gf > 0).astype(np.uint8) * 255)
print("\nbinary threshold >0")
scan(binary2, "binary_0")

# Non-zero alpha positions
print("\n=== ALPHA POSITION-BASED ===")
nz_positions = np.where(af.flatten() != 0)[0]
print(f"Non-zero alpha positions: {len(nz_positions)}")

# Try using alpha!=0 positions as bit indices
if len(nz_positions) >= 256:
    bits_from_alpha_pos = np.zeros(256, dtype=np.uint8)
    for i, pos_val in enumerate(nz_positions[:256]):
        byte_idx = pos_val // 8
        bit_idx = pos_val % 8
        if byte_idx < len(gf) // 8:
            gf_packed = np.packbits(((gf >> 0) & 1).astype(np.uint8))
            bits_from_alpha_pos[i] = gf_packed[byte_idx]
    scan(bits_from_alpha_pos, "alpha_pos_extract")

print(f"\n=== DONE. Total checks: {total_checks}, time: {time.time()-total_start:.1f}s, found: {found} ===")
