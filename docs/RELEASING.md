# Releasing PSTDR-DSL (Offline Ed25519 Signing Flow)

PSTDR-DSL uses **offline Ed25519 signing** so that neither a compromised GitHub account nor a compromised CI runner can forge a valid PackStudio update.

- Commits to `main` **never** trigger a release.
- Pushing a SemVer tag (`v*.*.*`) runs [`.github/workflows/release-pstdr.yml`](../.github/workflows/release-pstdr.yml), which builds `spec-bundle.tar.gz`, `pstdr-x86_64-linux.tar.gz`, `pstdr-aarch64-linux.tar.gz`, and an unsigned `pstdr-release.json`, then uploads them to a **GitHub Draft Release**.
- The maintainer signs `pstdr-release.json` **offline** with a local private key, uploads `pstdr-release.json.sig`, and publishes the release.

---

## 1. One-Time Keypair Generation (Offline)

Generate your Ed25519 signing keypair in a private directory **outside** any Git repository (for example, `~/.pstdr-keys`):

```sh
pstdr-c release keygen --out ~/.pstdr-keys
```

This creates:
- `~/.pstdr-keys/pstdr-signing.key` (`chmod 0600`) — **KEEP OFFLINE & BACKED UP. NEVER COMMIT TO GIT.**
- `~/.pstdr-keys/pstdr-signing.pub` — 64-char hex public key to hardcode into PackStudio.

The command prints both the **Public Key (hex)** and **Key Fingerprint (SHA-256)** ready to paste into PackStudio's updater verifier.

---

## 2. Trigger a Draft Release

1. Ensure `Cargo.toml` and `editors/vscode/package.json` reflect the release version, and all tests pass (`cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`).
2. Tag and push:

```sh
git tag v0.2.0
git push origin v0.2.0
```

Wait for the **Release PSTDR-DSL (Draft)** GitHub Actions workflow to finish creating the draft release for `v0.2.0`.

---

## 3. Offline Sign & Publish (3-Command Flow)

Download the unsigned `pstdr-release.json` from the draft release, sign it offline with your local key, upload `pstdr-release.json.sig`, and publish:

```sh
# 1. Download unsigned manifest from the Draft Release
gh release download v0.2.0 --pattern 'pstdr-release.json' --dir /tmp/pstdr-sign

# 2. Sign offline & verify locally against your public key
pstdr-c release sign \
  --manifest /tmp/pstdr-sign/pstdr-release.json \
  --key ~/.pstdr-keys/pstdr-signing.key \
  --out /tmp/pstdr-sign/pstdr-release.json.sig
pstdr-c release verify \
  --manifest /tmp/pstdr-sign/pstdr-release.json \
  --sig /tmp/pstdr-sign/pstdr-release.json.sig \
  --pubkey ~/.pstdr-keys/pstdr-signing.pub

# 3. Upload the signature and publish the release
gh release upload v0.2.0 /tmp/pstdr-sign/pstdr-release.json.sig && gh release edit v0.2.0 --draft=false
```

Clean up `/tmp/pstdr-sign` afterwards. PackStudio will only accept the release once both `pstdr-release.json` and a valid `pstdr-release.json.sig` are present on the published GitHub Release.
