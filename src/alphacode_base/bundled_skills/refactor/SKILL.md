---
name: refactor
description: Systematic code refactoring: detect code smells, extract functions, simplify conditionals, remove duplication, and improve readability while preserving behavior.
---

# Refactoring Skill

Systematic code improvement while preserving behavior.

## Process

1. **Identify** — find code smells (long functions, deep nesting, duplication, god classes)
2. **Verify** — ensure existing tests pass before changing anything
3. **Refactor** — apply one refactoring pattern at a time
4. **Test** — run tests after each change
5. **Commit** — one commit per refactoring step

## Code Smell Detection

- Functions > 50 lines → extract sub-functions
- Duplicate code (3+ copies) → extract to shared function
- Nested conditionals > 3 levels → early returns or extract condition
- Long parameter lists → use options struct
- God classes → split by responsibility
- Magic numbers/strings → named constants
- Dead code → remove
- Complex expressions → extract to named variables

## Refactoring Patterns

- Extract Function / Method
- Extract Variable
- Inline Function
- Replace Temp with Query
- Introduce Parameter Object
- Replace Conditional with Polymorphism
- Move Method / Field
- Extract Class
- Replace Inheritance with Delegation

## Rules

- Never change behavior during refactoring
- One pattern at a time
- Run tests after every change
- If no tests exist, write characterization tests first
