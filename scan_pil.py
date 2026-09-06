from PIL import Image
import numpy as np
import time
from coincurve import PrivateKey
import hashlib

TARGET = bytes.fromhex("ff2142e98e09b5344994f9beb9c56c95506b9f17")
ORDER = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
tc = 0

def check(kb):
    global tc
    ki = int.from_bytes(kb, 'big')
    if ki == 0 or ki >= ORDER: return False
    tc += 1
    try:
        sk = PrivateKey(kb)
        pub = sk.public_key.format(compressed=False)
        return hashlib.new('sha3_256', pub).digest()[12:] == TARGET
    except: return False

print("Loading with PIL...")
img = Image.open('puzzle_image.png')
arr = np.array(img)
print(f"Shape: {arr.shape}, dtype: {arr.dtype}")
H, W = arr.shape[0], arr.shape[1]

if arr.ndim == 3:
    gf = arr[:,:,0].flatten()
    af = arr[:,:,1].flatten() if arr.shape[2] > 1 else np.ones_like(gf) * 255
else:
    gf = arr.flatten()
    af = np.ones_like(gf) * 255
del arr
print(f"Pixels: {len(gf)}")

gc = gf.reshape(H,W).T.flatten()
ac = af.reshape(H,W).T.flatten()
print(f"Ready. {time.time():.1f}")

def s8(data, name):
    global tc
    n = len(data); t = time.time()
    for o in range(n-32):
        if check(bytes(data[o:o+32])):
            print(f"*** FOUND {name} off={o}"); return True
        if (o+1) % 1000000 == 0: print(f"  {name}: {o+1} ({time.time()-t:.0f}s)")
    print(f"  {name}: {n-32} in {time.time()-t:.0f}s"); return False

def s1b(flat, bit, name, rev=False, col=False):
    if col: flat = flat.reshape(H,W).T.flatten()
    if rev: flat = flat[::-1]
    packed = np.packbits(((flat >> bit) & 1).astype(np.uint8))
    return s8(packed, name)

def snb(flat, bpp, shift, name, rev=False, col=False):
    if col: flat = flat.reshape(H,W).T.flatten()
    if rev: flat = flat[::-1]
    mask = ((1 << bpp) - 1) << shift
    ext = ((flat & mask) >> shift).astype(np.uint8)
    ppb = 8 // bpp; nb = len(ext) // ppb
    packed = np.zeros(nb, dtype=np.uint8)
    for i in range(ppb):
        packed |= (ext[i::ppb].astype(np.uint8) << (8 - bpp*(i+1)))
    return s8(packed, name)

print("\n=== 8-BIT GRAY ===")
for n, d in [("g8r",gf),("g8r_",gf[::-1]),("g8c",gc),("g8c_",gc[::-1])]:
    if s8(d, n): break

print("\n=== 8-BIT ALPHA ===")
for n, d in [("a8r",af),("a8r_",af[::-1]),("a8c",ac),("a8c_",ac[::-1])]:
    if s8(d, n): break

print("\n=== 1-BIT GRAY ===")
for b in range(8):
    for tag,rv,cl in [("r",False,False),("r_",True,False),("c",False,True),("c_",True,True)]:
        if s1b(gf, b, f"g1b{b}{tag}", rev=rv, col=cl): break

print("\n=== 1-BIT ALPHA ===")
for b in range(8):
    for tag,rv,cl in [("r",False,False),("r_",True,False),("c",False,True),("c_",True,True)]:
        if s1b(af, b, f"a1b{b}{tag}", rev=rv, col=cl): break

print("\n=== 2-BIT GRAY ===")
for s in [0,2,4,6]:
    for tag,rv,cl in [("r",False,False),("r_",True,False),("c",False,True),("c_",True,True)]:
        if snb(gf, 2, s, f"g2b{s}{tag}", rev=rv, col=cl): break

print("\n=== 2-BIT ALPHA ===")
for s in [0,2,4,6]:
    for tag,rv,cl in [("r",False,False),("r_",True,False),("c",False,True),("c_",True,True)]:
        if snb(af, 2, s, f"a2b{s}{tag}", rev=rv, col=cl): break

print("\n=== 4-BIT ===")
for ch,fl in [("g",gf),("a",af)]:
    for s in [0,4]:
        for tag,rv,cl in [("r",False,False),("r_",True,False),("c",False,True),("c_",True,True)]:
            if snb(fl, 4, s, f"{ch}4b{s}{tag}", rev=rv, col=cl): break

print("\n=== COMBINED ===")
xored = (gf ^ af).astype(np.uint8)
for n,d in [("xor",xored),("xor_",xored[::-1]),("xor_c",xored.reshape(H,W).T.flatten())]:
    if s8(d, n): break

for n,d in [("and",(gf&af).astype(np.uint8)),("or",(gf|af).astype(np.uint8))]:
    if s8(d, n): break

inter = np.empty(len(gf)*2, dtype=np.uint8)
inter[0::2]=gf; inter[1::2]=af
for n,d in [("il",inter),("il_",inter[::-1])]:
    if s8(d, n): break

diff = np.abs(gf.astype(np.int16)-af.astype(np.int16)).astype(np.uint8)
for n,d in [("diff",diff),("diff_",diff[::-1])]:
    if s8(d, n): break

for n,d in [("inv",(255-gf)),("inv_",((255-gf)[::-1]))]:
    if s8(d, n): break

for n,d in [("th127",((gf>127).astype(np.uint8)*255)),("th0",((gf>0).astype(np.uint8)*255))]:
    if s8(d, n): break

print(f"\nDONE. {tc} checks")
