# Security Policy

## Reporting a vulnerability

If you believe you have found a security vulnerability in open-public, please report it
privately. **Do not open a public issue for security problems.**

- **Contact:** [@markedown](https://github.com/markedown) or `md@open-public.com`
- Preferably use GitHub's [private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability)
  ("Report a vulnerability" under the repository **Security** tab) if it is enabled.

Please include:

- a description of the issue and its impact,
- the steps required to reproduce it,
- any relevant logs, requests, or proof-of-concept.

We aim to acknowledge reports within a few days and will keep you updated on remediation.

## Scope

open-public stores only publicly sourced political data and minimal account data. An account is
keyed on a salted HMAC-SHA256 hash of its email address; the password is stored only as an argon2
hash, and sessions are opaque tokens kept server-side only as a SHA-256 hash. No plaintext email,
and no other PII, is ever stored. Even so, please report anything that could:

- expose the HMAC secret or any environment secret,
- allow poll-vote stuffing or bypass of the one-vote-per-verified-account rule,
- bypass email verification, or forge or hijack a session,
- allow injection into the database or rendered pages,
- leak a plaintext email address from the database or logs (only the salted hash is persisted).

## Handling of secrets

This repository is public and its git history is permanent. A leaked credential is handled
by **rotating the credential**, not by rewriting history. Only `.env.example` is ever
committed; real secrets live outside the repo.
