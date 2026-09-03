import re
with open(r'C:\Users\tlbbe\OneDrive\Desktop\acode\src\alphacode_app_core\tool\communicate.rs', 'rb') as f:
    data = f.read()
# Find all RateLimited occurrences
for m in re.finditer(b'RateLimited', data):
    pos = m.start()
    line_num = data[:pos].count(b'\n') + 1
    print(f'Line {line_num}:', data[max(0,pos-100):pos+100].decode('utf-8', errors='replace'))
    print()
