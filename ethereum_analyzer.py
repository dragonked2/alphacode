from ecdsa import SigningKey, SECP256k1
import hashlib

# cHRM chunk data (32 bytes)
chrm_data = bytes.fromhex('00007a26000080840000fa00000080e8000075300000ea6000003a9800001770')

print('=== cHRM chunk as potential private key ===')
print(f'Data (hex): {chrm_data.hex()}')
print(f'As integer: {int.from_bytes(chrm_data, byteorder="big")}')

# Check if it's a valid private key
private_key = int.from_bytes(chrm_data, byteorder='big')
secp256k1_order = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
print(f'Is valid private key? {0 < private_key < secp256k1_order}')

# Try to derive Ethereum address
try:
    # Create signing key from private key
    sk = SigningKey.from_string(chrm_data, curve=SECP256k1)
    
    # Get the verifying key (public key)
    vk = sk.get_verifying_key()
    
    # Get the uncompressed public key (65 bytes)
    public_key = b'\x04' + vk.to_string()
    
    print(f'\nPublic key (hex): {public_key.hex()}')
    
    # Hash the public key with Keccak-256
    keccak = hashlib.sha3_256(public_key).digest()
    
    # Take last 20 bytes as Ethereum address
    address = keccak[-20:]
    
    # Format as Ethereum address
    eth_address = '0x' + address.hex()
    print(f'Derived Ethereum address: {eth_address}')
    
    # Compare with prize address
    prize_address = '0xFF2142E98E09b5344994F9bEB9C56C95506B9F17'
    print(f'Prize address: {prize_address}')
    print(f'Match: {eth_address.lower() == prize_address.lower()}')
    
except Exception as e:
    print(f'Error: {e}')

# Also try with little endian
print('\n=== Trying little endian interpretation ===')
chrm_data_le = bytes.fromhex('00007a26000080840000fa00000080e8000075300000ea6000003a9800001770')
chrm_data_le = chrm_data_le[::-1]  # Reverse bytes
private_key_le = int.from_bytes(chrm_data_le, byteorder='big')
print(f'Little endian data (hex): {chrm_data_le.hex()}')
print(f'As integer: {private_key_le}')
print(f'Is valid private key? {0 < private_key_le < secp256k1_order}')

try:
    sk_le = SigningKey.from_string(chrm_data_le, curve=SECP256k1)
    vk_le = sk_le.get_verifying_key()
    public_key_le = b'\x04' + vk_le.to_string()
    
    keccak_le = hashlib.sha3_256(public_key_le).digest()
    address_le = keccak_le[-20:]
    eth_address_le = '0x' + address_le.hex()
    
    print(f'Derived Ethereum address (little endian): {eth_address_le}')
    print(f'Match with prize address: {eth_address_le.lower() == prize_address.lower()}')
    
except Exception as e:
    print(f'Error: {e}')