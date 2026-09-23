---
name: git-workflow
description: Expert git workflows — branching strategies, rebasing, stashing, conflict resolution, bisect, interactive rebase, cherry-pick, and clean commit hygiene. Turns messy git histories into clean, reviewable, professional output.
---

# Git Workflow — AlphaCode Edition

You are a senior engineer who treats git history as a permanent record of engineering decisions. Every commit tells a clear story. Every branch has a purpose. Every merge is clean.

## Core Principles

1. **One logical change per commit** — if you can't describe it in one sentence, split it
2. **Never commit broken code** — every commit should build and pass tests
3. **Write commit messages that explain WHY, not WHAT** — the diff shows what changed
4. **Keep main/master always deployable** — no experimental commits on main
5. **Rebase before merge** — clean history, not merge commit soup

## 1. Branching Strategy

### Branch Types
```
main              — production-ready, always green
develop           — integration branch (if using gitflow)
feature/TICKET-123 — new functionality
fix/TICKET-456    — bug fixes
hotfix/TICKET-789 — emergency production fixes
refactor/TICKET   — code restructuring, no behavior change
chore/TICKET      — tooling, dependencies, CI config
docs/TICKET       — documentation only
```

### Branch Naming Rules
- Lowercase with hyphens (not underscores or camelCase)
- Include ticket/issue number when available
- Keep it descriptive but short: `fix/login-timeout` not `fix/issue-where-users-cant-login-because-of-timeout-bug`

### Creating Branches
```bash
# Feature branch from main
git checkout main && git pull
git checkout -b feature/user-auth

# Fix branch from current work
git stash
git checkout main && git pull
git checkout -b fix/null-pointer
git stash pop
```

## 2. Commit Messages

### Format
```
<type>(<scope>): <subject line — imperative mood, max 72 chars>

<body — wrap at 72 chars, explain WHY not what>

<footer — references, breaking changes>
```

### Types
| Type | When |
|------|------|
| `feat` | New feature or capability |
| `fix` | Bug fix |
| `refactor` | Code restructure, no behavior change |
| `docs` | Documentation only |
| `test` | Adding or fixing tests |
| `chore` | Tooling, deps, CI, config |
| `perf` | Performance improvement |
| `style` | Formatting, no logic change |
| `ci` | CI/CD pipeline changes |

### Good Examples
```
feat(auth): add JWT refresh token rotation

Refresh tokens now rotate on each use, preventing token replay.
Old tokens are invalidated after 5 minutes grace period.

Closes #123
```

```
fix(api): prevent race condition in user creation

The previous implementation allowed two concurrent requests to
create users with the same email. Added a unique constraint check
before insert and wrapped in a transaction.

Fixes #456
```

### Bad Examples
```
fix stuff                    — vague, tells nothing
update code                  — what code? why?
WIP                          — don't commit WIP, use stash
asdfasdf                     — not a message, a cry for help
```

## 3. Interactive Rebase

### Clean Up Local Commits Before Push
```bash
# Last 3 commits — squash, reorder, reword
git rebase -i HEAD~3
```

### Rebase Editor Commands
```
pick   = keep commit as-is
squash = merge into previous commit (keep message)
fixup  = merge into previous commit (discard message)
reword = keep commit, edit message
edit   = pause to amend the commit
drop   = remove commit entirely
```

### Common Rebase Patterns
```bash
# Squash last 3 commits into one
git rebase -i HEAD~3
# Change "pick" to "squash" for the 2nd and 3rd commits

# Reorder commits
git rebase -i HEAD~5
# Reorder the lines — top runs first

# Split a commit
git rebase -i HEAD~2
# Change "pick" to "edit" for the commit to split
# When paused: git reset HEAD^, then stage files in 2+ commits
```

## 4. Conflict Resolution

### Strategy
```bash
# When merge conflict occurs:
git status                    # see which files conflict
git diff                      # see the conflict markers
# Open the file, find <<<<<<< markers
# Decide which version to keep (or combine both)
# Remove all conflict markers
git add <file>                # mark as resolved
git commit                    # or git rebase --continue
```

### Advanced Conflict Tools
```bash
# Use a merge tool
git mergetool

# Accept theirs (remote version)
git checkout --theirs <file>

# Accept ours (local version)  
git checkout --ours <file>

# Accept all incoming changes for a file
git merge --strategy-option theirs

# During rebase, accept theirs for entire rebase
git rebase -X theirs <branch>
```

### Preventing Conflicts
```bash
# Pull with rebase to avoid merge commits
git pull --rebase origin main

# Keep feature branches short-lived (< 3 days)
# Rebase onto main frequently
git fetch origin
git rebase origin/main
```

## 5. Stashing

```bash
# Quick stash
git stash

# Stash with a message
git stash push -m "WIP: halfway through auth refactor"

# Stash including untracked files
git stash -u

# Stash only specific files
git stash push -m "partial" -- src/auth.rs src/middleware.rs

# List stashes
git stash list

# Apply most recent stash (keep in stash list)
git stash apply

# Pop most recent stash (remove from stash list)
git stash pop

# Apply a specific stash
git stash apply stash@{2}

# Drop a stash
git stash drop stash@{0}

# Clear all stashes
git stash clear
```

## 6. Cherry-Pick & Backport

```bash
# Apply a specific commit to current branch
git cherry-pick <commit-hash>

# Cherry-pick multiple commits
git cherry-pick <hash1> <hash2> <hash3>

# Cherry-pick a range
git cherry-pick A..B

# Cherry-pick without committing (stage changes only)
git cherry-pick --no-commit <hash>

# Abort a cherry-pick in progress
git cherry-pick --abort
```

## 7. Git Bisect (Binary Search for Bugs)

```bash
# Start bisect
git bisect start

# Mark current commit as bad
git bisect bad

# Mark a known-good commit
git bisect good v1.0.0

# Git checks out a middle commit — test it, then mark:
git bisect good    # if bug is NOT present
git bisect bad     # if bug IS present

# Continue until git finds the first bad commit
# When done:
git bisect reset
```

### Automate Bisect
```bash
# Run a test script automatically
git bisect run npm test

# Run a specific command
git bisect run grep -q "fixed_function" src/lib.rs
```

## 8. Undoing Things

### Undo Last Commit (Keep Changes)
```bash
git reset --soft HEAD~1        # unstage, keep changes
git reset HEAD~1               # unstage, unstage files too
git reset --hard HEAD~1        # DELETE all changes (dangerous!)
```

### Undo a Pushed Commit (Safe)
```bash
# Creates a new commit that reverses the target commit
git revert <commit-hash>

# Revert a merge commit
git revert -m 1 <merge-commit-hash>
```

### Amend Last Commit
```bash
# Change the message
git commit --amend -m "new message"

# Add forgotten files
git add forgotten-file.rs
git commit --amend --no-edit

# Undo amend (restore to before amend)
git reflog
git reset HEAD@{1}
```

## 9. Useful Aliases

```bash
git config --global alias.co checkout
git config --global alias.br branch
git config --global alias.ci commit
git config --global alias.st status
git config --global alias.lg "log --oneline --graph --decorate -20"
git config --global alias.last "log -1 --stat"
git config --global alias.unstage "reset HEAD --"
git config --global alias.amend "commit --amend --no-edit"
```

## 10. Pre-commit Hygiene

```bash
# Before pushing, always:
git fetch origin
git rebase origin/main          # rebase on latest main
cargo check                     # or npm test / pytest
git push

# If rebase changed history:
git push --force-with-lease     # safer than --force
```

### .gitignore Best Practices
```gitignore
# Build outputs
/target
/dist
/build

# Dependencies
/node_modules
/vendor

# Environment
.env
.env.local

# IDE
.vscode/
.idea/
*.swp

# OS
.DS_Store
Thumbs.db

# Logs
*.log
```

## 11. Troubleshooting

### Detached HEAD
```bash
# You're on a commit, not a branch
git checkout -b temp-branch     # create branch to save work
git checkout main               # or just go back to main
```

### Accidentally Deleted a Branch
```bash
git reflog                      # find the last commit on that branch
git checkout -b recovered <hash>
```

### Accidentally Committed to Wrong Branch
```bash
git stash                       # save the commits
git checkout correct-branch
git stash pop                   # apply to correct branch
```

### Large Files Accidentally Committed
```bash
# Remove from history
git filter-branch --force --index-filter \
  "git rm --cached --ignore-unmatch large-file.zip" \
  --prune-empty --tag-name-filter cat -- --all

# Better: use git-lfs for large files from the start
```

## 12. Commit Checklist

Before every push:
- [ ] Commit message follows convention (type(scope): description)
- [ ] Code compiles / builds without errors
- [ ] Tests pass
- [ ] No debug prints or console.log left
- [ ] No secrets or credentials committed
- [ ] Branch is rebased on latest main
- [ ] Force push only with `--force-with-lease`
