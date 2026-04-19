Read this document in [Portuguese (pt-BR)](SECURITY.pt-BR.md).


# Security Policy


## Supported Versions
- The table below lists which neurographrag versions currently receive security patches
- Users on deprecated lines are STRONGLY encouraged to upgrade to a supported release
- Upgrading early reduces exposure window and aligns with the coordinated disclosure policy

| Version | Status       | Security Patches       |
| ------- | ------------ | ---------------------- |
| 2.0.x   | Supported    | Yes, receives fixes    |
| 1.x     | Deprecated   | Critical issues only   |
| 0.x     | Unsupported  | No patches provided    |


## Reporting a Vulnerability
- OBRIGATÓRIO report security issues through GitHub Security Advisories as the primary private channel
- Open an advisory at https://github.com/daniloaguiarbr/neurographrag/security/advisories/new
- JAMAIS open a public GitHub issue, pull request, or discussion for security-related reports
- Include a minimal reproduction, affected versions, and expected versus actual behavior
- Include your environment details such as OS, architecture, and rustc version
- Include CVSS 3.1 severity estimate when possible to accelerate triage


## Response SLA
- Triage of every advisory is committed to start within 72 business hours of submission
- Initial acknowledgment email will be sent within that same 72-hour window
- You will receive a case identifier and an assigned maintainer contact
- Progress updates are shared at minimum every 7 days until resolution or public disclosure


## Fix SLA by CVSS Severity
- Critical severity (CVSS 9.0 to 10.0) receives a patch within 7 calendar days of validated triage
- High severity (CVSS 7.0 to 8.9) receives a patch within 14 calendar days of validated triage
- Medium severity (CVSS 4.0 to 6.9) receives a patch within 30 calendar days of validated triage
- Low severity (CVSS 0.1 to 3.9) receives a patch within 90 calendar days of validated triage
- Released fixes follow immediately with a CHANGELOG entry and a GitHub Security Advisory


## Disclosure Policy
- We follow coordinated disclosure with a standard 90-day embargo window from initial report
- The embargo can be shortened when a fix is released earlier than 90 days
- The embargo can be extended when a fix demands more time and the reporter agrees
- Public disclosure includes a CVE identifier when the impact warrants one
- Public disclosure includes the GitHub Security Advisory with affected versions and patched version
- Credit is attributed to the reporter unless anonymity is explicitly requested


## Security Update Policy
- Patches for supported versions ship as a new patch release on crates.io and GitHub Releases
- Every release is validated with the full 10-command quality gate described in CONTRIBUTING
- CI runs `cargo audit` and `cargo deny check advisories licenses bans sources` on every push
- Supply chain is enforced via pinned `constant_time_eq = "=0.4.2"` to protect MSRV 1.88
- Transitive dependency MSRV drift is monitored proactively per PRD policy


## Hall of Fame
- We publicly acknowledge researchers who report vulnerabilities responsibly
- This section is open to contributions — your name will be added after coordinated disclosure
- If you prefer anonymity, we honor that preference without exception


## Best Practices for Users
- SEMPRE install neurographrag with `cargo install --locked neurographrag` to respect the pinned versions
- SEMPRE rotate your `crates.io` API tokens on a regular schedule
- SEMPRE keep your rustc toolchain updated to the latest stable release compatible with MSRV 1.88
- SEMPRE review CHANGELOG entries before upgrading across major versions
- JAMAIS commit secrets or tokens to the repository or to derived forks
- JAMAIS disable the memory guard in production via undocumented flags
- JAMAIS bypass `cargo audit` warnings without opening a tracked security advisory
