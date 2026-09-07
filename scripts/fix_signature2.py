#!/usr/bin/env python3
import re
p = 'src/alphacode_tui/tui/model_browser_open.rs'
with open(p, encoding='utf-8') as f:
    s = f.read()

# Find the signature_from_routes function and rewrite it.
# The Box::leak call is the smoking gun.
old_pattern = re.compile(
    r'pub fn signature_from_routes\([^}]*?Box::leak[^}]*?\)\n\}\)',
    re.DOTALL,
)

new_func = (
    'pub fn signature_from_routes(\n'
    '    routes: &[crate::alphacode_tui::tui::PickerOption],\n'
    '    current_model: &str,\n'
    '    task_hint: Option<&str>,\n'
    ') -> BrowserCacheSignature {\n'
    '    // Build the keys in a single pass so we never need a Vec<&str>. The\n'
    '    // sorted order is what makes the signature stable under input permutation.\n'
    '    let mut keys: Vec<String> = routes\n'
    '        .iter()\n'
    '        .map(|r| {\n'
    '            // U+001F (unit separator) never appears in any field in practice,\n'
    '            // so it is safe as a delimiter.\n'
    '            format!(\n'
    '                "{}\\u{1f}{}\\u{1f}{}\\u{1f}{}",\n'
    '                r.provider,\n'
    '                r.api_method,\n'
    '                r.detail,\n'
    '                r.detail_severity as u8,\n'
    '            )\n'
    '        })\n'
    '        .collect();\n'
    '    keys.sort_unstable();\n'
    '    let mut h: u64 = 0xcbf29ce484222325;\n'
    '    for s in &keys {\n'
    '        for b in s.as_bytes() {\n'
    '            h ^= *b as u64;\n'
    '            h = h.wrapping_mul(0x100000001b3);\n'
    '        }\n'
    '    }\n'
    '    BrowserCacheSignature::new(h, current_model.to_string(), task_hint.map(str::to_string))\n'
    '}'
)

m = old_pattern.search(s)
if not m:
    print('not found')
    raise SystemExit(1)

s = s[:m.start()] + new_func + s[m.end():]
with open(p, 'w', encoding='utf-8') as f:
    f.write(s)
print('done')