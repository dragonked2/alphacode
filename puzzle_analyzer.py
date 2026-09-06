import struct
import zlib
from PIL import Image

# Read PNG file
with open('puzzle_image.png', 'rb') as f:
    sig = f.read(8)
    print('PNG Signature:', sig.hex())
    
    chunks = []
    while True:
        data = f.read(4)
        if len(data) < 4:
            break
        length = struct.unpack('>I', data)[0]
        chunk_type = f.read(4)
        chunk_data = f.read(length)
        crc = f.read(4)
        
        chunk_name = chunk_type.decode('ascii', errors='ignore')
        chunks.append({
            'type': chunk_name,
            'length': length,
            'data': chunk_data,
            'crc': crc.hex()
        })
        
        print(f'\nChunk: {chunk_name} Length: {length}')
        
        if chunk_name == 'IHDR':
            ihdr = struct.unpack('>IIBBBBB', chunk_data[:13])
            print(f'  Width: {ihdr[0]}, Height: {ihdr[1]}, Bit depth: {ihdr[2]}, Color type: {ihdr[3]}')
            print(f'  Full IHDR data: {chunk_data[:13].hex()}')
        elif chunk_name == 'cHRM':
            print(f'  cHRM data (32 bytes): {chunk_data.hex()}')
            # Try interpreting as private key (32 bytes)
            print(f'  Potential private key: 0x{chunk_data.hex()}')
        elif chunk_name == 'tEXt' or chunk_name == 'iTXt' or chunk_name == 'zTXt':
            try:
                text = chunk_data.decode('utf-8', errors='ignore')
                print(f'  Text data: {text}')
            except:
                print(f'  Raw data: {chunk_data.hex()}')
        elif chunk_name == 'tIME':
            print(f'  Time data: {chunk_data.hex()}')

print(f'\nTotal chunks: {len(chunks)}')