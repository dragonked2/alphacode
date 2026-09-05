---
name: regex
description: Expert regular expressions — pattern building, common patterns, debugging, performance, and language-specific regex for JavaScript, Python, Rust, Go, and command-line tools (grep, sed, awk).
---

# Regex — AlphaCode Edition

You are a pattern-matching expert who writes regex that is correct, readable, and maintainable. You know when regex is the right tool and when it's not.

## Core Principles

1. **Correctness first** — test every regex against edge cases
2. **Readability matters** — use named groups and comments for complex patterns
3. **Know the limits** — regex can't parse HTML/JSON; use proper parsers instead
4. **Test thoroughly** — use regex testers with multiple inputs
5. **Prefer simple** — if a regex needs explanation, consider a simpler approach

## 1. Regex Fundamentals

### Quick Reference
| Pattern | Meaning | Example |
|---------|---------|---------|
| `.` | Any character (except newline) | `a.c` matches `abc`, `a1c` |
| `\d` | Digit [0-9] | `\d+` matches `123` |
| `\w` | Word character [a-zA-Z0-9_] | `\w+` matches `hello_123` |
| `\s` | Whitespace | `\s+` matches spaces, tabs |
| `\b` | Word boundary | `\bcat\b` matches `cat` but not `catch` |
| `^` | Start of string | `^Hello` matches `Hello world` |
| `$` | End of string | `world$` matches `Hello world` |
| `*` | 0 or more | `ab*c` matches `ac`, `abc`, `abbc` |
| `+` | 1 or more | `ab+c` matches `abc`, `abbc` not `ac` |
| `?` | 0 or 1 | `colou?r` matches `color` and `colour` |
| `{n}` | Exactly n | `\d{3}` matches `123` |
| `{n,m}` | Between n and m | `\d{2,4}` matches `12`, `123`, `1234` |
| `[abc]` | Character set | `[aeiou]` matches any vowel |
| `[^abc]` | Negated set | `[^0-9]` matches non-digit |
| `(abc)` | Capture group | `(\\d+)` captures digits |
| `(?:abc)` | Non-capturing group | `(?:ab)+` matches `ab`, `abab` |
| `(?<name>abc)` | Named group | `(?<year>\\d{4})` |
| `a\|b` | Alternation | `cat\|dog` matches `cat` or `dog` |
| `(?!abc)` | Negative lookahead | `(?!\d{3})` not 3 digits |
| `(?=abc)` | Positive lookahead | `(?=@)` followed by `@` |

## 2. Common Patterns

### Email
```regex
^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$
```

### URL
```regex
https?://[a-zA-Z0-9.-]+(?:/[^\s]*)?
```

### IP Address (IPv4)
```regex
\b(?:(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.){3}(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\b
```

### Date (YYYY-MM-DD)
```regex
\d{4}-(?:0[1-9]|1[0-2])-(?:0[1-9]|[12]\d|3[01])
```

### Phone Number (US)
```regex
(?:\+1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}
```

### Hex Color
```regex
#(?:[0-9a-fA-F]{3}){1,2}\b
```

### Strong Password
```regex
^(?=.*[a-z])(?=.*[A-Z])(?=.*\d)(?=.*[@$!%*?&])[A-Za-z\d@$!%*?&]{8,}$
```

### Username
```regex
^[a-zA-Z][a-zA-Z0-9_-]{2,19}$
```

### Semantic Version
```regex
^\d+\.\d+\.\d+(?:-(?:alpha|beta|rc)\.\d+)?$
```

### File Path
```regex
(?:/[a-zA-Z0-9._-]+)+/?$
```

### HTML Tag
```regex
<([a-z][a-z0-9]*)\b[^>]*>(.*?)</\1>
```

### JSON Key-Value
```regex
"([^"]+)"\s*:\s*("(?:[^"\\]|\\.)*"|\d+(?:\.\d+)?|true|false|null)
```

## 3. Language-Specific Regex

### JavaScript
```javascript
// Literal syntax
const pattern = /\d{3}-\d{4}/;
const match = text.match(pattern);

// Constructor (for dynamic patterns)
const pattern = new RegExp(`\\d{${length}}`);

// Named groups
const regex = /(?<year>\d{4})-(?<month>\d{2})-(?<day>\d{2})/;
const { groups } = text.match(regex);
console.log(groups.year, groups.month, groups.day);

// Replace with function
const result = text.replace(/\b(\w+)\b/g, (match, word) => {
    return word.toUpperCase();
});

// Global replace
const result = text.replace(/old/g, 'new');

// Test
const isValid = regex.test(input);
```

### Python
```python
import re

# Basic match
match = re.search(r'\d+', text)
if match:
    print(match.group())

# Named groups
pattern = r'(?P<year>\d{4})-(?P<month>\d{2})-(?P<day>\d{2})'
match = re.search(pattern, text)
if match:
    print(match.group('year'), match.group('month'))

# Find all
emails = re.findall(r'[\w.+-]+@[\w-]+\.[\w.]+', text)

# Split
parts = re.split(r'\s+', text)

# Replace
result = re.sub(r'\b(\w+)\b', lambda m: m.group(1).upper(), text)

# Compile for reuse
pattern = re.compile(r'\d{3}-\d{4}')
matches = pattern.findall(text)

# Flags
re.IGNORECASE  # case-insensitive
re.MULTILINE   # ^ and $ match line boundaries
re.DOTALL      # . matches newlines
re.VERBOSE     # allows comments and whitespace
```

### Rust
```rust
use regex::Regex;

// Basic match
let re = Regex::new(r"\d+").unwrap();
if let Some(mat) = re.find(text) {
    println!("{}", mat.as_str());
}

// Named groups
let re = Regex::new(r"(?P<year>\d{4})-(?P<month>\d{2})").unwrap();
if let Some(caps) = re.captures(text) {
    println!("{}-{}", &caps["year"], &caps["month"]);
}

// Find all
let matches: Vec<&str> = re.find_iter(text)
    .map(|m| m.as_str())
    .collect();

// Replace
let result = re.replace_all(text, "REDACTED");

// Compile once, use many times
lazy_static! {
    static ref EMAIL_RE: Regex = Regex::new(
        r"[\w.+-]+@[\w-]+\.[\w.]+"
    ).unwrap();
}
```

### Command Line
```bash
# grep — search files
grep -rn "pattern" src/             # recursive, line numbers
grep -i "pattern" file.txt          # case-insensitive
grep -E "pattern1|pattern2" file    # extended regex
grep -P "\d{3}-\d{4}" file         # Perl regex (PCRE)

# sed — search and replace
sed 's/old/new/g' file.txt          # replace all occurrences
sed -i 's/old/new/g' file.txt      # in-place edit
sed -n '10,20p' file.txt           # print lines 10-20

# awk — field processing
awk '/pattern/ { print $1, $2 }' file.txt
awk -F: '$3 >= 1000 { print $1 }' /etc/passwd
```

## 4. Debugging Regex

### Test Every Regex
```bash
# Online testers
# https://regex101.com/ (recommended — explains every match)
# https://regexr.com/
# https://debuggex.com/ (visual debugger)
```

### Common Mistakes
```regex
# ❌ Greedy — matches everything between first < and last >
<.*>

# ✅ Lazy — matches first closing tag
<.*?>

# ❌ Unescaped dots
example.com          # matches "exampleXcom"

# ✅ Escape special characters
example\.com         # matches "example.com"

# ❌ Missing anchors
\d{3}-\d{4}          # matches anywhere in string

# ✅ Use anchors for full match
^\d{3}-\d{4}$        # must be exactly 3 digits, dash, 4 digits
```

### Performance Tips
```regex
# ❌ Catastrophic backtracking
(a+)+b               # exponential time on "aaaaaaaaaaaaac"

# ✅ Atomic group or possessive
(?>a+)+b             # no backtracking

# ❌ Overly broad
.*                    # matches everything

# ✅ Be specific
[a-zA-Z0-9]+         # matches alphanumeric
```

## 5. When NOT to Use Regex

### Don't Use Regex For
- ❌ HTML parsing — use an HTML parser
- ❌ JSON parsing — use json.loads() or JSON.parse()
- ❌ XML parsing — use an XML parser
- ❌ Complex nested structures — use a proper parser
- ❌ Anything with balanced brackets — use a parser

### Use Regex For
- ✅ Simple pattern matching in text
- ✅ Validation (email, phone, date formats)
- ✅ Search and replace in text
- ✅ Extracting structured data from unstructured text
- ✅ Log parsing and filtering

## 6. Regex Cheat Sheet

```bash
# Character classes
\d  = [0-9]           # digit
\D  = [^0-9]          # non-digit
\w  = [a-zA-Z0-9_]    # word
\W  = [^a-zA-Z0-9_]   # non-word
\s  = [\t\n\r\f ]     # whitespace
\S  = [^\t\n\r\f ]    # non-whitespace

# Quantifiers
*    = 0 or more       # greedy
+    = 1 or more       # greedy
?    = 0 or 1          # greedy
{n}  = exactly n
{n,m}= n to m
*?   = 0 or more       # lazy
+?   = 1 or more       # lazy

# Lookaround
(?=X)   = positive lookahead
(?!X)   = negative lookahead
(?<=X)  = positive lookbehind
(?<!X)  = negative lookbehind
```
