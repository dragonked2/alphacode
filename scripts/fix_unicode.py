#!/usr/bin/env python3
import re
p = 'src/alphacode_tui/tui/model_browser_render.rs'
with open(p, encoding='utf-8') as f:
    s = f.read()
# Convert \u25xx style escapes to \u{25xx} (Rust requires braces)
pattern = re.compile(r'\\u([0-9a-fA-F]{4})(?![0-9a-fA-F{])')
s2 = pattern.sub(r'\\u{\1}', s)
with open(p, 'w', encoding='utf-8') as f:
    f.write(s2)
print('done')