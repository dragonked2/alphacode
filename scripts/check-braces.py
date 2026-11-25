"""Report brace/paren balance per file, ignoring strings and comments.

Used to tell a genuinely truncated file from a misread one.
"""
import sys

BACKSLASH = chr(92)


def strip(src):
    out = []
    i = 0
    n = len(src)
    while i < n:
        c = src[i]
        if c == "/" and i + 1 < n and src[i + 1] == "/":
            j = src.find("\n", i)
            i = n if j < 0 else j
            continue
        if c == '"':
            i += 1
            while i < n and src[i] != '"':
                i += 2 if src[i] == BACKSLASH else 1
            i += 1
            continue
        out.append(c)
        i += 1
    return "".join(out)


for path in sys.argv[1:]:
    src = open(path, encoding="utf-8").read()
    text = strip(src)
    print(
        "%-58s open=%4d close=%4d delta=%+d"
        % (path, text.count("{"), text.count("}"), text.count("{") - text.count("}"))
    )