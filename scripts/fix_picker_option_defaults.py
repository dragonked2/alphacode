#!/usr/bin/env python3
"""Add `..PickerOption::default()` to every `PickerOption { ... }` literal
that does not already have a `..` spread. Handles nested braces and strings.
"""
import re

FILES = [
    'src/alphacode_tui/tui/app/auth_account_picker.rs',
    'src/alphacode_tui/tui/app/inline_interactive/helpers.rs',
    'src/alphacode_tui/tui/app/inline_interactive/openers.rs',
    'src/alphacode_tui/tui/app/inline_interactive.rs',
    'src/alphacode_tui/tui/app/tests/remote_model_picker_hotkeys.rs',
    'src/alphacode_tui/tui/app/tests/state_model_poke_03.rs',
    'src/alphacode_tui/tui/ui_inline_interactive.rs',
    'src/alphacode_tui/tui/ui_tests/inline_picker.rs',
]


def fix_literal(text, start):
    """Given an index pointing at the start of `PickerOption {`, find the
    matching closing brace and return the patched version (insert spread),
    or None if a spread already exists.
    """
    brace_open = text.index('{', start)
    depth = 1
    i = brace_open + 1
    while i < len(text) and depth > 0:
        c = text[i]
        if c == '{':
            depth += 1
        elif c == '}':
            depth -= 1
        elif c == '"':
            i += 1
            while i < len(text) and text[i] != '"':
                if text[i] == '\\':
                    i += 2
                    continue
                i += 1
        i += 1
    brace_close = i - 1
    inner = text[brace_open + 1:brace_close]
    if '..' in inner:
        return None
    new = text[:brace_close] + ', ..PickerOption::default()' + text[brace_close:]
    return new


def main():
    pat = re.compile(r'PickerOption\s*\{')
    total_changes = 0
    changed_files = 0
    for f in FILES:
        src = open(f, encoding='utf-8').read()
        new = src
        matches = list(pat.finditer(new))
        n_changes = 0
        for m in reversed(matches):
            start = m.start()
            result = fix_literal(new, start)
            if result is not None:
                new = result
                n_changes += 1
        if n_changes > 0:
            open(f, 'w', encoding='utf-8').write(new)
            changed_files += 1
            total_changes += n_changes
            print(f'{f}: {n_changes} sites fixed')
    print(f'Total: {changed_files} files, {total_changes} sites fixed')


if __name__ == '__main__':
    main()