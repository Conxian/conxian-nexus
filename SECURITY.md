# Security Policy

Conxian Nexus is security-sensitive infrastructure. Please report vulnerabilities privately.

## Supported Versions

| Version | Supported |
| ------- | --------- |
| v0.4.x | ✅ |
| < v0.4 | ❌ |

## Reporting a Vulnerability

Do **not** report vulnerabilities in public issues.

Use one of these private channels:

1. GitHub private vulnerability reporting for this repository.
2. Email [security@conxian-labs.com](mailto:security@conxian-labs.com).

Please include:

- the type of issue
- affected files or components if known
- a concise description
- reproduction steps or proof of concept
- expected impact

## What to expect

- acknowledgement target: 24 to 48 hours
- coordinated remediation and disclosure
- public credit after remediation unless anonymity is requested

## Security expectations

- keep secrets, API keys, private SSH/TLS keys, and credentials out of source control
- enforce strict repository ignore boundaries (`.gitignore` and `.dockerignore`) against generated runtime artifacts (`node_modules/`, `test-results/`, `playwright-report/`), web build outputs (`.next/`, `dist/`, `build/`), local database dumps (`*.db`, `*.sqlite`), and temporary files
- redact sensitive values from logs and debug output
- use protected channels for incident handling
- enforce production storage boundary controls (remote authenticated Redis and PostgreSQL) across both eager and lazy storage initializations (`Storage::new` and `Storage::new_lazy`) in release builds
