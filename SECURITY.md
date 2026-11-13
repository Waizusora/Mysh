# Security Policy

## Supported Versions

Security fixes are applied to the latest release and the current `main` branch.

## Reporting a Vulnerability

Do not disclose suspected vulnerabilities in public issues.

Use GitHub's private vulnerability reporting feature for this repository when it is enabled. If private reporting is unavailable, contact the repository maintainer through the contact information published on the maintainer's GitHub profile.

Include the affected version, Android version, target ABI, a concise description, and reproducible steps. Do not include real credentials or other sensitive information.

## Security Boundaries

Mysh runs with the privileges of the Android application or process that launches it. It does not grant root access, bypass application sandboxing, or override Android filesystem permissions.

External command execution is constrained by the operating system's executable permissions, process policy, linker compatibility, and the caller's environment. Users should only execute binaries from trusted sources.

## Scope

Security-sensitive areas include command parsing, path handling, file deletion and copying, external process execution, environment handling, and Android-specific process behavior.
