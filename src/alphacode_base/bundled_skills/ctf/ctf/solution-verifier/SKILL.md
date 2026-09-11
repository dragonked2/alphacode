---
name: ctf-solution-verifier
description: Pre-submission verification engine for CTF flags — validates flag format, checks for honeypots, cross-references with challenge metadata, and ensures solution confidence before submission. Use BEFORE submitting any flag to avoid point penalties.
---

# CTF Solution Verifier Skill

## Core Principle: Never Submit Without Verification

Every flag submission is irreversible. Wrong flags cost points, waste time, and reveal your approach to other teams. **Verification is not optional — it's part of solving.**

**Rule: The time you spend verifying is always less than the time you spend recovering from a wrong submission.**

---

## Phase 1: Flag Format Validation

### Step 1: Identify the CTF's Flag Format

```python
#!/usr/bin/env python3
"""
Validate flag format against CTF's established pattern.
Run this on EVERY flag before submission.
"""
import re
import sys

# Common flag formats (update as you learn the CTF's format)
KNOWN_FORMATS = {
    'ctfd_default': r'^flag\{[a-zA-Z0-9_!@#$%^&*()\-+=\[\]{}|;:\'",.<>?/\\~` ]+\}$',
    'picoctf': r'^picoCTF\{[a-zA-Z0-9_]+\}$',
    'hackthebox': r'^HTB\{[a-zA-Z0-9_!@#$%^&*()\-+=]+\}$',
    'tryhackme': r'^THM\{[a-zA-Z0-9_]+\}$',
    'hitcon': r'^hitcon\{[a-zA-Z0-9_]+\}$',
    'generic': r'^(flag|ctf|FLAG|CTF|Flag)\{[a-zA-Z0-9_!@#$%^&*()\-+=]+\}$',
}

def validate_flag_format(flag, ctf_format=None, known_flags=None):
    """
    Validate flag format against expected patterns.
    
    Args:
        flag: The flag string to validate
        ctf_format: Regex pattern for this CTF's format (if known)
        known_flags: List of previously valid flags from this CTF
    
    Returns:
        dict with validation results
    """
    results = {
        'flag': flag,
        'valid_format': False,
        'format_match': None,
        'warnings': [],
        'confidence': 0
    }
    
    # 1. Basic sanity checks
    if not flag or len(flag) < 5:
        results['warnings'].append("Flag too short")
        return results
    
    if len(flag) > 200:
        results['warnings'].append("Flag unusually long")
    
    # 2. Check against known CTF format
    if ctf_format:
        if re.match(ctf_format, flag):
            results['valid_format'] = True
            results['format_match'] = 'custom'
            results['confidence'] += 30
        else:
            results['warnings'].append(f"Does not match expected format: {ctf_format}")
    
    # 3. Check against common formats
    for name, pattern in KNOWN_FORMATS.items():
        if re.match(pattern, flag):
            results['valid_format'] = True
            results['format_match'] = name
            results['confidence'] += 20
            break
    
    # 4. Check against known valid flags from this CTF
    if known_flags:
        # Check if format matches other valid flags
        if known_flags:
            sample = known_flags[0]
            prefix = sample.split('{')[0] + '{' if '{' in sample else ''
            if flag.startswith(prefix):
                results['confidence'] += 20
                results['warnings'].append(f"Format matches CTF prefix: {prefix}")
    
    # 5. Check for suspicious patterns
    suspicious_patterns = [
        (r'^(test|example|placeholder|dummy|fake|decoy)', "Looks like a placeholder"),
        (r'\{0+\}', "Contains all zeros"),
        (r'\{a+\}', "Contains all 'a's"),
        (r'(.)\1{5,}', "Contains 5+ repeated characters"),
        (r'^flag\{\}$', "Empty flag body"),
        (r'flag\{test', "Contains 'test'"),
        (r'flag\{admin', "Contains 'admin'"),
        (r'flag\{password', "Contains 'password'"),
    ]
    
    for pattern, warning in suspicious_patterns:
        if re.search(pattern, flag, re.IGNORECASE):
            results['warnings'].append(warning)
            results['confidence'] -= 20
    
    # 6. Calculate final confidence
    if results['valid_format'] and not results['warnings']:
        results['confidence'] += 30
    elif results['valid_format']:
        results['confidence'] += 10
    
    return results

if __name__ == "__main__":
    flag = sys.argv[1] if len(sys.argv) > 1 else input("Enter flag: ")
    result = validate_flag_format(flag)
    
    print(f"Flag: {result['flag']}")
    print(f"Valid format: {result['valid_format']}")
    print(f"Format match: {result['format_match']}")
    print(f"Confidence: {result['confidence']}%")
    if result['warnings']:
        print("Warnings:")
        for w in result['warnings']:
            print(f"  - {w}")
```

### Step 2: Cross-Reference with Challenge Metadata

```
CHALLENGE METADATA CHECKS:

1. POINTS vs COMPLEXITY
   - Challenge worth 500+ points → solution should require significant effort
   - Challenge worth 100-200 points → solution should be straightforward
   - Challenge worth <100 points → solution should be quick
   
   RED FLAG: High points + easy solution = probably a trap

2. SOLVE COUNT ANALYSIS
   - 0 solves → either very hard or newly posted (investigate)
   - 1-5 solves → probably non-obvious approach needed
   - 5-20 solves → pattern is known but not trivial
   - 20+ solves → pattern is probably simple
   
   RED FLAG: Your solution is different from what 20+ people likely used

3. HINT ANALYSIS
   - If hints were purchased → check what they reveal
   - If hints are available but unused → consider buying one
   - If hints are expensive → challenge is probably hard
   
   RED FLAG: Hint contradicts your solution approach

4. TAG ANALYSIS
   - Challenge tagged "easy" + complex solution = trap
   - Challenge tagged "hard" + simple solution = gift (submit confidently)
   - Challenge tagged with specific technique + your solution uses that technique = good sign
```

---

## Phase 2: Honeypot Detection

### Honeypot Identification Checklist

```
HONEYPOT RED FLAGS:

□ FLAG FOUND TOO QUICKLY (<2 minutes of work)
  - Real flags usually require some effort
  - Quick flags are often decoys

□ FLAG FOUND IN OBVIOUS LOCATION
  - strings output → probably decoy
  - source code comments → probably decoy
  - first thing you checked → probably decoy
  - README or documentation → probably decoy

□ FLAG FORMAT SLIGHTLY WRONG
  - Extra characters (flag{test} vs flag{test123})
  - Missing characters (flag{tes} vs flag{test})
  - Wrong capitalization (Flag{} vs flag{})
  - Wrong delimiter (flag[test] vs flag{test})

□ MULTIPLE FLAGS FOUND
  - If you found 2+ flags, at least one is a honeypot
  - The LEAST obvious one is probably real

□ SOLUTION FELT "TOO EASY"
  - If the challenge seemed straightforward, it probably isn't
  - Easy solutions to hard challenges are traps

□ CHALLENGE SEEMS "DONE" BUT YOU HAVEN'T USED KEY TECHNIQUE
  - If challenge is tagged "crypto" and you solved it with "web" → wrong approach
  - If challenge requires specific tool and you didn't use it → probably wrong
```

### Honeypot Verification Commands

```bash
# 1. Check if flag appears in multiple locations
echo "=== Checking all files for flag ==="
grep -rnEi 'flag\{[^}]+\}' . 2>/dev/null | head -20

# 2. Check if flag is in obvious location
echo "=== Checking strings output ==="
strings * | grep -iE 'flag\{[^}]+\}' | head -10

# 3. Check if flag matches CTF format
echo "=== Checking format ==="
echo "YOUR_FLAG" | grep -E '^flag\{[a-zA-Z0-9_!@#$%^&*()\-+=]+\}$'

# 4. Check if flag is in source code
echo "=== Checking source code ==="
find . -name "*.py" -o -name "*.js" -o -name "*.php" -o -name "*.c" | xargs grep -l "flag{" 2>/dev/null

# 5. Check if flag is in metadata
echo "=== Checking metadata ==="
exiftool * 2>/dev/null | grep -i flag
```

---

## Phase 3: Solution Logic Verification

### Logical Consistency Check

```
SOLUTION LOGIC VERIFICATION:

1. CAUSAL CHAIN VALIDITY
   - Does each step logically follow from the previous?
   - Are there gaps in reasoning?
   - Did you make any assumptions that aren't verified?
   
   RED FLAG: "I tried X and it worked" without understanding WHY

2. TECHNIQUE APPROPRIATENESS
   - Does the technique match the challenge category?
   - Is the technique known to work for this type of challenge?
   - Did you use the intended technique or a shortcut?
   
   RED FLAG: Using web technique on a crypto challenge

3. RESULT CONSISTENCY
   - Does the decoded/decrypted result look meaningful?
   - Is the flag content contextually appropriate?
   - Does the flag relate to the challenge theme?
   
   RED FLAG: Flag contains random characters or doesn't make sense

4. REPRODUCIBILITY
   - Can you reproduce the solution step-by-step?
   - Are there any steps that relied on luck?
   - Would this solution work on a similar challenge?
   
   RED FLAG: "I just ran a bunch of commands and it worked"
```

### Automated Logic Check

```python
#!/usr/bin/env python3
"""
Verify solution logic before submission.
"""
import sys

def verify_solution_logic(solution_steps, challenge_info):
    """
    Verify that solution logic is sound.
    
    Args:
        solution_steps: List of (action, result) tuples
        challenge_info: Dict with challenge metadata
    
    Returns:
        dict with verification results
    """
    results = {
        'logic_valid': True,
        'warnings': [],
        'confidence_adjustments': []
    }
    
    # 1. Check for logical gaps
    for i, (action, result) in enumerate(solution_steps):
        if not result or result.strip() == '':
            results['warnings'].append(f"Step {i+1} has no observable result")
            results['logic_valid'] = False
    
    # 2. Check for technique-category mismatch
    category = challenge_info.get('category', '').lower()
    techniques_used = [action.lower() for action, _ in solution_steps]
    
    category_technique_map = {
        'web': ['curl', 'sqlmap', 'ffuf', 'nikto', 'burp', 'request'],
        'crypto': ['decrypt', 'decode', 'rsa', 'aes', 'xor', 'brute'],
        'pwn': ['exploit', 'overflow', 'rop', 'shellcode', 'pwntools'],
        'rev': ['decompile', 'disassemble', 'ghidra', 'ida', 'radare'],
        'forensics': ['extract', 'analyze', 'pcap', 'steghide', 'volatility'],
    }
    
    if category in category_technique_map:
        expected_techniques = category_technique_map[category]
        used_expected = any(
            any(t in action for t in expected_techniques)
            for action, _ in solution_steps
        )
        if not used_expected:
            results['warnings'].append(
                f"Solution doesn't use typical {category} techniques"
            )
    
    # 3. Check for solution completeness
    if len(solution_steps) < 2:
        results['warnings'].append("Solution has very few steps (might be too simple)")
    
    # 4. Check for reliance on specific values
    hardcoded_values = []
    for action, result in solution_steps:
        if any(str(v) in action for v in range(10)):
            hardcoded_values.append(action)
    if hardcoded_values:
        results['warnings'].append(
            f"Solution uses hardcoded values: {hardcoded_values[:3]}"
        )
    
    return results

if __name__ == "__main__":
    print("=== Solution Logic Verifier ===")
    print("Enter solution steps (one per line, format: 'action → result')")
    print("Enter empty line when done")
    
    steps = []
    while True:
        line = input("Step: ").strip()
        if not line:
            break
        if '→' in line:
            action, result = line.split('→', 1)
            steps.append((action.strip(), result.strip()))
        else:
            steps.append((line, "unknown"))
    
    challenge_info = {
        'category': input("Challenge category: ").strip(),
        'points': int(input("Challenge points: ").strip() or "0"),
    }
    
    results = verify_solution_logic(steps, challenge_info)
    
    print(f"\nLogic valid: {results['logic_valid']}")
    if results['warnings']:
        print("Warnings:")
        for w in results['warnings']:
            print(f"  - {w}")
```

---

## Phase 4: Final Submission Checklist

### Pre-Submission Gate (MANDATORY)

```
BEFORE SUBMITTING ANY FLAG, COMPLETE THIS CHECKLIST:

FORMAT VALIDATION:
□ Flag matches the CTF's established format exactly
□ No trailing/leading whitespace
□ Correct capitalization (flag vs FLAG vs Flag)
□ No extra characters or missing characters
□ Flag body is not empty
□ Flag body is not obviously a placeholder

HONEYPOT CHECKS:
□ Flag was NOT found in an obvious location (strings, comments, README)
□ Flag was NOT found in <2 minutes of work
□ No other flags were found in the challenge (if multiple, submit least obvious first)
□ Flag content is contextually appropriate for the challenge
□ Solution difficulty matches challenge points

LOGIC CHECKS:
□ Solution has a clear, reproducible logical chain
□ Technique matches the challenge category
□ Each step produced observable results
□ No steps relied on luck or guessing
□ Solution would work on a similar challenge

CONFIDENCE CHECK:
□ Confidence level is HIGH or MEDIUM
□ No warnings from verification tools
□ Challenge metadata is consistent with solution
□ The "squint test" passes (flag looks like it belongs)

IF ANY BOX IS UNCHECKED:
- Do NOT submit
- Re-examine the solution
- Consider that you may have fallen for a trap
- Try a different approach
```

### Confidence Scoring

```
FINAL CONFIDENCE CALCULATION:

START: 50 points (neutral)

ADD:
+20 if flag format matches exactly
+15 if solution was logical and reproducible
+15 if technique matches challenge category
+10 if no honeypot red flags detected
+10 if challenge difficulty matches solution complexity

SUBTRACT:
-20 if flag found in obvious location
-20 if flag found in <2 minutes
-15 if multiple flags found in challenge
-15 if solution doesn't use expected technique for category
-10 if flag content is contextually inappropriate
-10 if solution relied on luck/guessing

RESULT:
80-100 → HIGH confidence → Submit
60-79 → MEDIUM confidence → Verify once more, then submit
40-59 → LOW confidence → Do NOT submit; re-examine
<40 → VERY LOW confidence → Rethink entire approach
```

---

## Phase 5: Post-Submission Analysis

### After Every Submission (Correct or Wrong)

```
POST-SUBMISSION CHECKLIST:

IF CORRECT:
1. What made this solution correct?
2. What was the intended vulnerability?
3. What trap did the author set (that you avoided)?
4. What pattern does this teach for future challenges?
5. Add this pattern to your knowledge base

IF WRONG:
1. What made this solution incorrect?
2. What trap did you fall for?
3. What was the correct approach?
4. How could you have detected the trap earlier?
5. Add this trap pattern to your detection database
6. Share the lesson with teammates
```

### Learning Database Update

```python
#!/usr/bin/env python3
"""
Update learning database with solution verification results.
"""
import json
from datetime import datetime

def update_learning_db(challenge_name, category, solution, correct, trap_type=None):
    """
    Add solution verification results to learning database.
    """
    db_file = "ctf_learning_db.json"
    
    try:
        with open(db_file, 'r') as f:
            db = json.load(f)
    except:
        db = {"challenges": [], "patterns": [], "traps": []}
    
    entry = {
        "name": challenge_name,
        "category": category,
        "solution_summary": solution[:100],
        "correct": correct,
        "timestamp": datetime.now().isoformat(),
        "trap_type": trap_type
    }
    
    db["challenges"].append(entry)
    
    if trap_type:
        if trap_type not in db["traps"]:
            db["traps"].append(trap_type)
    
    with open(db_file, 'w') as f:
        json.dump(db, f, indent=2)
    
    print(f"Learning database updated: {challenge_name} ({'correct' if correct else 'wrong'})")
```

---

## Integration with Solving Workflow

```
ENHANCED SOLVING WORKFLOW:

1. TRIAGE → Categorize challenge
2. META-ANALYSIS → Detect traps (advanced-reasoning skill)
3. ADVERSARIAL ANALYSIS → Detect hidden instructions (adversarial-thinking skill)
4. HYPOTHESIS → Generate 3+ approaches
5. SOLVE → Execute best approach
6. VERIFY → Run solution-verifier checks (THIS SKILL)
7. SUBMIT → Only if confidence is HIGH/MEDIUM
8. POST-MORTEM → Update learning database
```

---

## Speed Metrics

```
Flag format validation: <10 seconds
Honeypot detection: <30 seconds
Logic verification: <1 minute
Confidence scoring: <10 seconds
Total verification time: <2 minutes per flag
Time saved by avoiding wrong submissions: 5-15 minutes per trap avoided
```
