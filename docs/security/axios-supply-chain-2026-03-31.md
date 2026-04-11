# Supply Chain Incident Analysis: axios npm Compromise (2026-03-31)

**Date of analysis:** 2026-04-01
**Analyst:** Michael Pursifull (with Claude Code tooling)
**Scope:** ArcavenAE/forestage — exposure assessment against compromised axios npm packages
**Verdict:** Not compromised. No exposure during the attack window.

---

## Incident Summary

On 2026-03-31, an attacker compromised the npm credentials of the lead axios
maintainer (jasonsaayman) and published two malicious package versions:

- `axios@1.14.1` (targeting the 1.x line)
- `axios@0.30.4` (targeting the legacy 0.x line)

Neither version modified axios source code. Instead, both injected a single
new dependency — `plain-crypto-js@4.2.1` — whose `postinstall` hook
downloaded platform-specific remote access trojans (RATs) for macOS, Windows,
and Linux. The malware established persistence, contacted a C2 server, and
erased its own installation traces.

The malicious versions were live on npm for approximately three hours before
removal. Any project with a caret range (e.g., `^1.x.x`) that ran
`npm install` without a lockfile during this window would have resolved to
the compromised version.

axios has over 400 million monthly downloads and 174,000 direct dependents.

### References

- Endor Labs: <https://www.endorlabs.com/learn/npm-axios-compromise>
- Picus Security: <https://www.picussecurity.com/resource/blog/axios-npm-supply-chain-attack-cross-platform-rat-delivery-via-compromised-maintainer-credentials>
- StepSecurity timeline (referenced in above sources)

---

## Attack Timeline (UTC)

| Time (UTC)        | Event                                                    |
|-------------------|----------------------------------------------------------|
| 2026-03-30 05:57  | `plain-crypto-js@4.2.0` published (clean decoy/reputation seed) |
| 2026-03-30 23:59  | `plain-crypto-js@4.2.1` published (malicious `postinstall` RAT dropper) |
| 2026-03-31 00:21  | `axios@1.14.1` published (malicious dependency injection) |
| 2026-03-31 01:00  | `axios@0.30.4` published (same pattern, legacy line)     |
| 2026-03-31 ~03:25 | npm removes both malicious axios versions (~3-hour window) |

The attack was staged over approximately 18 hours. The clean decoy was
published first to establish package history and evade "brand-new package"
heuristics. The malicious payload was injected just before the axios
versions were published.

---

## forestage CI Activity During the Attack Period

Data sourced from GitHub Actions run history (`gh run list --repo ArcavenAE/forestage`).

### Runs proximate to the attack window

| Run ID       | Time (UTC)         | Trigger                    | Result  | Ran `bun install`? |
|--------------|--------------------|----------------------------|---------|---------------------|
| 23726508924  | 2026-03-30 03:21   | PR (rename spectacle)      | success | Yes — 202 packages  |
| 23726514959  | 2026-03-30 03:22   | workflow_run (Release Verify) | success | No (verify only) |
| 23767847220  | 2026-03-30 21:08   | push (kos harvest)         | success | Yes — 200–202 packages |
| 23767987864  | 2026-03-30 21:11   | workflow_run (Release Verify) | success | No (verify only) |
| *(none)*     | 2026-03-31 00:21–03:25 | —                       | —       | —                   |

**No CI runs occurred during the 3-hour attack window.**

The last CI run completed at 2026-03-30 21:12 UTC — 3 hours and 9 minutes
before `axios@1.14.1` was published. No subsequent runs occurred before npm
removed the malicious packages.

### CI install output (run 23767847220, 2026-03-30 21:08 UTC)

Resolved top-level dependencies across all four build jobs (Lint & Test,
darwin-arm64, linux-amd64, linux-arm64):

```
@anthropic-ai/claude-agent-sdk@0.1.77
@anthropic-ai/sdk@0.39.0
commander@13.1.0
smol-toml@1.6.0
yaml@2.8.2
@types/node@22.19.15
eslint@9.39.4
tsx@4.21.0
typescript@5.9.3
typescript-eslint@8.57.1
vitest@3.2.4
```

- **axios does not appear** in the resolved package list of any CI run.
- **`plain-crypto-js` does not appear** in any CI log output.
- All runs installed from lockfile (`[migrated lockfile from package-lock.json]`).
  Note: bun's lockfile migration converts npm's `package-lock.json` format but
  is not equivalent to `--frozen-lockfile`. Migration may resolve differently
  than npm for packages not precisely represented in the lockfile. In this case,
  the protection held because axios was not in the dependency tree at all, not
  because the lockfile pinned it to a safe version.
- Package counts were consistent across runs (200–202 packages).

---

## Dependency Tree Analysis

### Direct dependencies

forestage's `cli/package.json` does not list axios as a direct dependency.
The dependency tree consists of the Anthropic SDKs, commander, smol-toml,
yaml, and dev tooling (eslint, typescript, vitest, tsx).

### Transitive presence of axios

axios appears in the forestage dependency tree in one location:

- **`rollup/package.json`** contains an `overrides` entry: `"axios": "^1.13.5"`

This is a version-pinning directive within rollup's own package.json,
constraining how axios is resolved if any of rollup's transitive dependencies
pull it in. rollup is a transitive dependency of vitest (dev dependency).

Critically, **axios is not actually installed** — no `axios/` directory exists
anywhere in `node_modules`. The override is declarative only; no dependency
in the resolved tree currently requires axios at runtime or build time.

### Range exposure

The rollup override range `^1.13.5` would match `1.14.1` (the malicious
version) per semver. If a future dependency change caused axios to be
resolved through this override path, and if the install occurred without a
lockfile or with a stale lockfile, the malicious version would have been
pulled during the attack window.

This did not happen. The range creates a latent exposure path, not an
actual one.

---

## Local Development Assessment

- `cli/package-lock.json` was last modified on **2026-03-18** — 13 days
  before the attack. No local `npm install` or `bun install` modified
  the lockfile during the attack window.
- No `axios/` directory exists in the local `node_modules`.
- The last forestage commit before the attack window was `0a0dbf1` at
  2026-03-30 21:07 UTC (kos process adoption — documentation only,
  no dependency changes).

---

## CI Pipeline Risk Profile

The forestage CI pipeline (`ci.yml`, `release.yml`, `release-verify.yml`)
performs the following on every push to `main`:

1. `bun install` (with lockfile migration)
2. Lint and test
3. Build binaries for darwin-arm64, linux-amd64, linux-arm64
4. Code sign and notarize (macOS)
5. Create alpha release on GitHub
6. Update Homebrew tap formula

If a CI run **had** occurred during the attack window and axios **had** been
in the resolved dependency tree, the blast radius would have extended to:

- Compiled binaries distributed as GitHub releases
- Homebrew tap formula pointing to compromised artifacts
- Any user who ran `brew install` or `brew upgrade` during that period

This scenario did not occur, but it illustrates the downstream amplification
risk of supply chain attacks against build-time dependencies in projects
that produce signed, distributed binaries.

---

## Protective Factors

| Factor | Status | Notes |
|--------|--------|-------|
| axios not a direct dependency | **Protected** | Not in `package.json` |
| axios not resolved in dep tree | **Protected** | Override-only, not installed |
| Lockfile present and used in CI | **Protected** | `bun install` migrates from `package-lock.json` |
| No CI runs during attack window | **Protected** | Last run 3h09m before malicious publish |
| No local installs during window | **Protected** | Lockfile last modified 2026-03-18 |

---

## Recommendations

1. **Audit rollup override.** The `"axios": "^1.13.5"` override in rollup's
   package.json is a latent exposure path. Monitor rollup releases for
   changes to this override or for transitive deps that begin requiring
   axios.

2. **Pin CI lockfile behavior.** CI currently runs `bun install` which
   migrates from `package-lock.json`. Consider using `--frozen-lockfile`
   (or bun equivalent) to prevent resolution drift in CI.

3. **Add npm audit to CI.** A scheduled or pre-release audit step would
   surface known-compromised transitive dependencies before they reach
   the build-and-release pipeline.

4. **Monitor for IOCs.** Although forestage was not exposed, any developer
   machine that ran `npm install` or `bun install` in any project
   containing axios during 2026-03-31 00:21–03:25 UTC should be checked
   against the published indicators of compromise (C2 domains, RAT
   binaries, modified `package.json` in `node_modules/plain-crypto-js/`).

---

## Conclusion

forestage was not compromised by the axios supply chain attack of 2026-03-31.
The project does not resolve axios as an installed dependency, the lockfile
was not modified during the attack window, and no CI runs occurred during
the 3-hour period when malicious versions were available on npm. Multiple
independent protective factors prevented exposure.

The incident is a useful case study for the forestage project's own supply
chain posture: the CI pipeline produces signed binaries distributed via
Homebrew, which means a compromised build-time dependency would have
downstream amplification well beyond the CI runner itself.
