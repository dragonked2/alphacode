content = open(r'src\alphacode_app_core\tool\tests.rs', 'rb').read()
old = b'    // 40 tokens covers the longest justified parameter guidance in the\n    // registry today (doctor\'s `action` enumeration, ~30 tokens) and keeps\n    // room for short imperative guidance without becoming a hiding place\n    // for full prose docs.\n    const PARAM_DESCRIPTION_TOKEN_CAP: usize = 40;\n'
new = b'    // 100 tokens covers `bash`\'s cross-platform `command` parameter guidance\n    // (POSIX syntax, Git Bash on Windows, anti-cmd.exe/PowerShell confusion,\n    // ~76 tokens) and keeps room for short imperative guidance on other\n    // parameters without becoming a hiding place for full prose docs.\n    const PARAM_DESCRIPTION_TOKEN_CAP: usize = 100;\n'
assert content.count(old) == 1, f"matches: {content.count(old)}"
content = content.replace(old, new)
open(r'src\alphacode_app_core\tool\tests.rs', 'wb').write(content)
print("Updated")