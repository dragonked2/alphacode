import struct, zlib
import numpy as np
import time
from coincurve import PrivateKey
import hashlib

TARGET = bytes.fromhex("ff2142e98e09b5344994f9beb9c56c95506b9f17")
ORDER = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
total_checks = 0

def check_key(kb):
    global total_checks
    ki = int.from_bytes(kb, 'big')
    if ki == 0 or ki >= ORDER: return False
    total_checks += 1
    try:
        sk = PrivateKey(kb)
        pub = sk.public_key.format(compressed=False)
        return hashlib.new('sha3_256', pub).digest()[12:] == TARGET
    except: return False

print("Loading...")
with open('puzzle_image.png', 'rb') as f:
    raw = f.read()
pos = 8; idat = b''
while pos < len(raw):
    l = struct.unpack('>I', raw[pos:pos+4])[0]
    ct = raw[pos+4:pos+8]
    if ct == b'IDAT': idat += raw[pos+8:pos+8+l]
    pos += 12 + l
dec = zlib.decompress(idat)

rows = []
pr = None
for ri in range(1105):
    s = ri * 3201; ft = dec[s]
    rd = bytearray(dec[s+1:s+3201])
    if ft == 1:
        for i in range(2, len(rd)): rd[i] = (rd[i] + rd[i-2]) & 0xFF
    elif ft == 2:
        if pr:
            for i in range(len(rd)): rd[i] = (rd[i] + pr[i]) & 0xFF
    elif ft == 4:
        for i in range(len(rd)):
            l2 = rd[i-2] if i >= 2 else 0
            u = pr[i] if pr else 0
            ul = pr[i-2] if pr and i >= 2 else 0
            p = l2+u-ul; pa,pb,pc = abs(p-l2),abs(p-u),abs(p-ul)
            pred = l2 if (pa<=pb and pa<=pc) else (u if pb<=pc else ul)
            rd[i] = (rd[i] + pred) & 0xFF
    rows.append(bytes(rd)); pr = rows[-1]

ab = b''.join(rows)
gf = np.frombuffer(ab, dtype=np.uint8)[0::2].copy()
af = np.frombuffer(ab, dtype=np.uint8)[1::2].copy()
del ab, rows, dec
gc = gf.reshape(1105,1600).T.flatten()
ac = af.reshape(1105,1600).T.flatten()
print(f"OK. {len(gf)} px. Starting scans...")

t0 = time.time()

def scan_bytes(data, name):
    global total_checks
    n = len(data); t = time.time()
    for o in range(n-32):
        if check_key(bytes(data[o:o+32])):
            print(f"*** FOUND {name} off={o} key={data[o:o+32].hex()}"); return True
        if (o+1) % 500000 == 0: print(f"  {name}: {o+1} ({time.time()-t:.0f}s)")
    print(f"  {name}: done {n-32} in {time.time()-t:.0f}s"); return False

def scan_1b(flat, bit, name, rev=False, col=False):
    global total_checks
    if col: flat = flat.reshape(1105,1600).T.flatten()
    if rev: flat = flat[::-1]
    packed = np.packbits(((flat >> bit) & 1).astype(np.uint8))
    return scan_bytes(packed, name)

def scan_nb(flat, bpp, shift, name, rev=False, col=False, lsf=False):
    global total_checks
    if col: flat = flat.reshape(1105,1600).T.flatten()
    if rev: flat = flat[::-1]
    ext = ((flat & (((1<<bpp)-1)<<shift)) >> shift).astype(np.uint8)
    ppb = 8//bpp; nb = len(ext)//ppb
    packed = np.zeros(nb, dtype=np.uint8)
    for i in range(ppb):
        if lsf: packed |= (ext[i::ppb] << (bpp*i))
        else: packed |= (ext[i::ppb] << (8-bpp*(i+1)))
    return scan_bytes(packed, name)

# === 8-BIT GRAY ===
print("\n=== GRAY 8-BIT ===")
for n, d in [("g8r", gf), ("g8r_", gf[::-1]), ("g8c", gc), ("g8c_", gc[::-1])]:
    if scan_bytes(d, n): break

# === 8-BIT ALPHA ===
print("\n=== ALPHA 8-BIT ===")
for n, d in [("a8r", af), ("a8r_", af[::-1]), ("a8c", ac), ("a8c_", ac[::-1])]:
    if scan_bytes(d, n): break

# === 1-BIT GRAY ===
print("\n=== GRAY 1-BIT ===")
for b in range(8):
    for tag, rv, cl in [("r",False,False), ("r_",True,False), ("c",False,True), ("c_",True,True)]:
        if scan_1b(gf, b, f"g1b{b}{tag}", rev=rv, col=cl): break

# === 1-BIT ALPHA ===
print("\n=== ALPHA 1-BIT ===")
for b in range(8):
    for tag, rv, cl in [("r",False,False), ("r_",True,False), ("c",False,True), ("c_",True,True)]:
        if scan_1b(af, b, f"a1b{b}{tag}", rev=rv, col=cl): break

# === 2-BIT GRAY ===
print("\n=== GRAY 2-BIT ===")
for s in [0,2,4,6]:
    for tag, rv, cl in [("r",False,False), ("r_",True,False), ("c",False,True), ("c_",True,True)]:
        if scan_nb(gf, 2, s, f"g2b{s}{tag}", rev=rv, col=cl): break

# === 2-BIT ALPHA ===
print("\n=== ALPHA 2-BIT ===")
for s in [0,2,4,6]:
    for tag, rv, cl in [("r",False,False), ("r_",True,False), ("c",False,True), ("c_",True,True)]:
        if scan_nb(af, 2, s, f"a2b{s}{tag}", rev=rv, col=cl): break

# === 4-BIT ===
print("\n=== 4-BIT ===")
for ch, fl in [("g",gf),("a",af)]:
    for s in [0,4]:
        for tag, rv, cl in [("r",False,False), ("r_",True,False), ("c",False,True), ("c_",True,True)]:
            if scan_nb(fl, 4, s, f"{ch}4b{s}{tag}", rev=rv, col=cl): break

# === COMBINED ===
print("\n=== COMBINED ===")
xored = (gf ^ af).astype(np.uint8)
print("xor"); 
if scan_bytes(xored, "xor"): pass
elif scan_bytes(xored[::-1], "xor_"): pass
elif scan_bytes(xored.reshape(1105,1600).T.flatten(), "xor_c"): pass

print("and")
if scan_bytes((gf & af).astype(np.uint8), "and"): pass

print("or")
if scan_bytes((gf | af).astype(np.uint8), "or"): pass

inter = np.empty(len(gf)*2, dtype=np.uint8)
inter[0::2] = gf; inter[1::2] = af
print("interleave")
if scan_bytes(inter, "il"): pass
elif scan_bytes(inter[::-1], "il_"): pass

# Difference
diff = np.abs(gf.astype(np.int16) - af.astype(np.int16)).astype(np.uint8)
print("diff")
if scan_bytes(diff, "diff"): pass
elif scan_bytes(diff[::-1], "diff_"): pass

# Sum (mod 256)
summ = ((gf.astype(np.uint16) + af.astype(np.uint16)) % 256).astype(np.uint8)
print("sum")
if scan_bytes(summ, "sum"): pass

# Binary threshold
print("thresh127")
if scan_bytes(((gf > 127).astype(np.uint8) * 255), "th127"): pass
print("thresh64")
if scan_bytes(((gf > 64).astype(np.uint8) * 255), "th64"): pass

# Inverted gray
print("inv_gray")
if scan_bytes(255 - gf, "inv"): pass
elif scan_bytes((255 - gf)[::-1], "inv_"): pass

print(f"\n=== DONE. {total_checks} checks in {time.time()-t0:.0f}s ===")
