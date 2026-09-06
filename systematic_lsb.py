from PIL import Image
import numpy as np
import struct
import zlib
from ecdsa import SigningKey, SECP256k1
import hashlib
import itertools

TARGET = "0xff2142e98e09b5344994f9beb9c56c95506b9f17"

# Load and reconstruct the actual pixels (undoing PNG filters)
with open('puzzle_image.png', 'rb') as f:
    raw_data = f.read()

pos = 8
idat_data = b''
while pos < len(raw_data):
    length = struct.unpack('>I', raw_data[pos:pos+4])[0]
    chunk_type = raw_data[pos+4:pos+8]
    chunk_data = raw_data[pos+8:pos+8+length]
    if chunk_type == b'IDAT':
        idat_data += chunk_data
    pos += 12 + length

decompressed = zlib.decompress(idat_data)

def undo_filter(filter_type, row_data, prev_row, bpp=2):
    result = bytearray(len(row_data))
    if filter_type == 0:
        result[:] = row_data
    elif filter_type == 1:
        for i in range(len(row_data)):
            left = result[i - bpp] if i >= bpp else 0
            result[i] = (row_data[i] + left) & 0xFF
    elif filter_type == 2:
        for i in range(len(row_data)):
            up = prev_row[i] if prev_row else 0
            result[i] = (row_data[i] + up) & 0xFF
    elif filter_type == 3:
        for i in range(len(row_data)):
            left = result[i - bpp] if i >= bpp else 0
            up = prev_row[i] if prev_row else 0
            result[i] = (row_data[i] + (left + up) // 2) & 0xFF
    elif filter_type == 4:
        for i in range(len(row_data)):
            left = result[i - bpp] if i >= bpp else 0
            up = prev_row[i] if prev_row else 0
            up_left = prev_row[i - bpp] if prev_row and i >= bpp else 0
            p = left + up - up_left
            pa, pb, pc = abs(p - left), abs(p - up), abs(p - up_left)
            if pa <= pb and pa <= pc:
                predictor = left
            elif pb <= pc:
                predictor = up
            else:
                predictor = up_left
            result[i] = (row_data[i] + predictor) & 0xFF
    return bytes(result)

row_size = 1 + 1600 * 2
all_rows = []
prev_row = None
for row_idx in range(1105):
    start = row_idx * row_size
    filter_byte = decompressed[start]
    row_data = decompressed[start+1:start+row_size]
    actual_row = undo_filter(filter_byte, row_data, prev_row)
    all_rows.append(actual_row)
    prev_row = actual_row

# Separate grayscale and alpha channels
gray_bytes = bytearray()
alpha_bytes = bytearray()
for row_data in all_rows:
    for i in range(0, len(row_data), 2):
        gray_bytes.append(row_data[i])
        alpha_bytes.append(row_data[i+1])

gray_flat = np.array(list(gray_bytes), dtype=np.uint8)
alpha_flat = np.array(list(alpha_bytes), dtype=np.uint8)

print(f"Gray pixels: {len(gray_flat)}")
print(f"Alpha pixels: {len(alpha_flat)}")

# Check if private key is valid
def check_key(key_bytes):
    if len(key_bytes) != 32:
        return False
    key_int = int.from_bytes(key_bytes, 'big')
    secp256k1_order = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
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

print("\n=== Systematic LSB scan across multiple channels ===")
print("Testing grayscale, alpha, and interleaved channels")

channels = {
    'gray': gray_flat,
    'alpha': alpha_flat,
    'interleaved': np.array([b for pair in zip(gray_bytes, alpha_bytes) for b in pair], dtype=np.uint8),
}

total_tests = 0

for ch_name, ch_data in channels.items():
    print(f"\nChannel: {ch_name} ({len(ch_data)} values)")
    
    # Test different bit extractions
    for bit_offset in range(8):
        for bit_count in [1, 2, 3, 4, 5, 6, 7, 8]:
            if bit_offset + bit_count > 8:
                continue
            
            mask = ((1 << bit_count) - 1) << bit_offset
            
            # Extract bits
            extracted = (ch_data & mask) >> bit_offset
            
            # Try different byte packing
            for pack_bits in [8, 4, 2, 1]:
                if bit_count != pack_bits:
                    continue
                    
                # Group into bytes
                bits_per_byte = 8 // pack_bits
                n_bytes = len(extracted) // bits_per_byte
                
                if n_bytes < 32:
                    continue
                
                # Try different starting positions
                for start in range(0, min(1000, len(extracted) - 32 * bits_per_byte)):
                    key_bytes = bytearray()
                    valid = True
                    for i in range(32):
                        byte_val = 0
                        for j in range(bits_per_byte):
                            idx = start + i * bits_per_byte + j
                            if idx >= len(extracted):
                                valid = False
                                break
                            byte_val = (byte_val << pack_bits) | int(extracted[idx])
                        if not valid:
                            break
                        key_bytes.append(byte_val)
                    
                    if valid and len(key_bytes) == 32:
                        total_tests += 1
                        if check_key(bytes(key_bytes)):
                            print(f"\n*** MATCH FOUND! ***")
                            print(f"Channel: {ch_name}")
                            print(f"Bit offset: {bit_offset}, bit count: {bit_count}")
                            print(f"Pack bits: {pack_bits}")
                            print(f"Start: {start}")
                            print(f"Key: {bytes(key_bytes).hex()}")
                            print(f"Address: TARGET")
                            import sys
                            sys.exit(0)
    
    if total_tests % 100000 == 0 and total_tests > 0:
        print(f"  Tested {total_tests} candidates so far...")

print(f"\nTotal candidates tested: {total_tests}")
print("No match found in basic LSB scan.")

# Now try a different approach: extract LSBs from ALL channels combined
print("\n=== Trying LSB from all channels combined ===")
combined = np.concatenate([gray_flat, alpha_flat])

for bit_offset in range(8):
    for bit_count in [1, 2, 3, 4]:
        if bit_offset + bit_count > 8:
            continue
        mask = ((1 << bit_count) - 1) << bit_offset
        extracted = (combined & mask) >> bit_offset
        
        for pack_bits in [bit_count]:
            bits_per_byte = 8 // pack_bits
            n_bytes = len(extracted) // bits_per_byte
            
            if n_bytes < 32:
                continue
            
            for start in range(0, min(500, len(extracted) - 32 * bits_per_byte)):
                key_bytes = bytearray()
                valid = True
                for i in range(32):
                    byte_val = 0
                    for j in range(bits_per_byte):
                        idx = start + i * bits_per_byte + j
                        if idx >= len(extracted):
                            valid = False
                            break
                        byte_val = (byte_val << pack_bits) | int(extracted[idx])
                    if not valid:
                        break
                    key_bytes.append(byte_val)
                
                if valid and len(key_bytes) == 32:
                    total_tests += 1
                    if check_key(bytes(key_bytes)):
                        print(f"\n*** MATCH FOUND! ***")
                        print(f"Key: {bytes(key_bytes).hex()}")
                        import sys
                        sys.exit(0)

print(f"Additional candidates tested: {total_tests}")
print("\nNo match found. The puzzle remains unsolved with these approaches.")
