#!/usr/bin/env python3
import sys
p = 'src/alphacode_tui/tui/model_browser_open.rs'
with open(p, encoding='utf-8') as f:
    s = f.read()

old = (
    "pub fn signature_from_routes(\n"
    "    routes: &[crate::alphacode_tui::tui::PickerOption],\n"
    "    current_model: &str,\n"
    "    task_hint: Option<&str>,\n"
    ") -> BrowserCacheSignature {\n"
    "    let mut sorted: Vec<&str> = routes\n"
    "        .iter()\n"
    "        .map(|r| {\n"
    "            // Hash needs a stable string per row; concat with a separator\n"
    "            // that won't appear in any field. The `||` triple was chosen\n"
    "            // because provider/model/detail never contain it in practice.\n"
    "            Box::leak(\n"
    "                format!(\n"
    "                    \"{}\u{1f}{}\u{1f}{}\u{1f}{}\",\n"
    "                    r.provider,\n"
    "                    r.api_method,\n"
    "                    r.detail,\n"
    "                    r.detail_severity as u8 as u8,\n"
    "                )\n"
    "                .into_boxed_str(),\n"
    "            ) as &str\n"
    "        })\n"
    "        .collect();\n"
    "    sorted.sort_unstable();\n"
    "    let mut h: u64 = 0xcbf29ce484222325;\n"
    "    for s in &sorted {\n"
    "        for b in s.as_bytes() {\n"
    "            h ^= *b as u64;\n"
    "            h = h.wrapping_mul(0x100000001b3);\n"
    "        }\n"
    "    }\n"
    "    BrowserCacheSignature::new(h, current_model.to_string(), task_hint.map(str::to_string))\n"
    "}"
)

new = (
    "pub fn signature_from_routes(\n"
    "    routes: &[crate::alphacode_tui::tui::PickerOption],\n"
    "    current_model: &str,\n"
    "    task_hint: Option<&str>,\n"
    ") -> BrowserCacheSignature {\n"
    "    // Build the keys in a single pass so we never need a Vec<&str>. The\n"
    "    // sorted order is what makes the signature stable under input permutation.\n"
    "    let mut keys: Vec<String> = routes\n"
    "        .iter()\n"
    "        .map(|r| {\n"
    "            // U+001F (unit separator) never appears in any field in practice,\n"
    "            // so it is safe as a delimiter.\n"
    "            format!(\n"
    "                \"{}\u{1f}{}\u{1f}{}\u{1f}{}\",\n"
    "                r.provider,\n"
    "                r.api_method,\n"
    "                r.detail,\n"
    "                r.detail_severity as u8,\n"
    "            )\n"
    "        })\n"
    "        .collect();\n"
    "    keys.sort_unstable();\n"
    "    let mut h: u64 = 0xcbf29ce484222325;\n"
    "    for s in &keys {\n"
    "        for b in s.as_bytes() {\n"
    "            h ^= *b as u64;\n"
    "            h = h.wrapping_mul(0x100000001b3);\n"
    "        }\n"
    "    }\n"
    "    BrowserCacheSignature::new(h, current_model.to_string(), task_hint.map(str::to_string))\n"
    "}"
)

assert s.count(old) == 1, f"old not found exactly once: {s.count(old)}"
s = s.replace(old, new, 1)
with open(p, 'w', encoding='utf-8') as f:
    f.write(s)
print('done')