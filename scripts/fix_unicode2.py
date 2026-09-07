#!/usr/bin/env python3
import re
p = 'src/alphacode_tui/tui/model_browser_render.rs'
with open(p, encoding='utf-8') as f:
    s = f.read()
# Match \u followed by exactly 4 hex digits not in braces
s2 = re.sub(r'\\u([0-9a-fA-F]{4})(?!\})', r'\\u{\1}', s)
with open(p, 'w', encoding='utf-8') as f:
    f.write(s2)
print('done')