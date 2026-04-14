import os
import re
import sys

FORBIDDEN_PATTERNS = [
    r"ST[0-9A-Z]{38}",
    r"\"SP\.\.\.\"",
    r"\"ST\.\.\.\""
]

EXCLUDE_DIRS = [
    "node_modules", "target", ".git", "tests", "scripts", "docs"
]

EXCLUDE_FILES = [
    "Cargo.lock", "CHANGELOG.md", "README.md", "verify_contamination_guard.py", "check_production_boundary.sh"
]

def check_file(file_path):
    violations = []
    try:
        with open(file_path, 'r', encoding='utf-8') as f:
            content = f.read()
            for pattern in FORBIDDEN_PATTERNS:
                if re.search(pattern, content):
                    violations.append(pattern)
    except Exception:
        pass
    return violations

def main():
    print("Running Contamination Guard...")
    total_violations = 0
    for root, dirs, files in os.walk("."):
        dirs[:] = [d for d in dirs if d not in EXCLUDE_DIRS]
        for file in files:
            if file in EXCLUDE_FILES:
                continue
            file_path = os.path.join(root, file)
            violations = check_file(file_path)
            if violations:
                print(f"FAIL: {file_path} contains forbidden patterns: {', '.join(violations)}")
                total_violations += len(violations)

    if total_violations == 0:
        print("PASS: Contamination guard cleared.")
        sys.exit(0)
    else:
        print(f"FAILED: Found {total_violations} violations.")
        sys.exit(1)

if __name__ == "__main__":
    main()
