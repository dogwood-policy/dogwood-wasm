# Releasing

`@dogwood-policy/dogwood-wasm` is published to npm only by
[`.github/workflows/release.yml`](.github/workflows/release.yml), when a
version tag is pushed. Nobody publishes from a laptop, and no npm token exists:
the workflow authenticates with [npm trusted
publishing](https://docs.npmjs.com/trusted-publishers) (GitHub OIDC), and npm
attaches a provenance attestation linking each version to the workflow run and
commit that built it.

## One-time setup

You need admin access to the GitHub repository and to the `dogwood-policy` npm
organization.

### GitHub

1. **Environment.** Settings → Environments → New environment `npm-publish`.
   - **Required reviewers:** at least one maintainer other than whoever pushes tags.
   - Turn on **Prevent self-review**.
   - **Deployment branches and tags:** Selected branches and tags → add the tag
     pattern `v*`.
2. **Tag ruleset.** Settings → Rules → Rulesets → New tag ruleset.
   - Target: tag pattern `v*`; Enforcement: Active; no bypass.
   - Rules: Restrict creations (bypass: maintainers only), Restrict updates,
     Restrict deletions, Block force pushes.
3. **Branch ruleset for `main`** requiring the `build-test`, `cargo-deny`,
   `npm-audit` and `pinned` checks. The release workflow only publishes commits
   already on `main`, so this is what makes "reviewed" mean something.

### npm

1. In the `dogwood-policy` organization, create the package
   `@dogwood-policy/dogwood-wasm`, or check that it exists and the organization
   owns it.
2. Package → Settings → **Trusted publishing** → GitHub Actions:
   - Organization or user: `dogwood-policy`
   - Repository: `dogwood-wasm`
   - Workflow filename: `release.yml`
   - Environment: `npm-publish`
3. Package → Settings → **Publishing access** → *Require two-factor
   authentication and disallow tokens*. This does not affect the trusted
   publisher.
4. Revoke any npm automation or granular tokens that can publish to the
   organization and are not needed.

A new trusted-publisher configuration expires if the first publish through it
does not happen **within 2 days**. Set it up just before the first release, or
recreate it if it expires.

**First publish.** If npm will not let you configure trusted publishing until
the package exists, publish the first version once with a granular token: scope
it to this package only, give it a 1-day expiry, and publish from a clean
checkout of the release tag with `npm publish ./pkg --provenance`, after
`scripts/prepare-release.sh` passes. Revoke the token immediately afterwards,
then do steps 2–4. Every later version goes through the workflow.

## Cutting a release

1. On a branch, set the version in `Cargo.toml` (`version = "X.Y.Z"`). Update
   `Cargo.lock` with `cargo update -p dogwood-wasm`, then open a PR and merge it
   once CI is green.
2. Tag the merge commit on `main` and push the tag:
   ```sh
   git checkout main && git pull
   git tag -s vX.Y.Z -m "vX.Y.Z"
   git push origin vX.Y.Z
   ```
   A prerelease tag (`vX.Y.Z-rc.1`) publishes under the `next` dist-tag, not
   `latest`.
3. The `release` workflow runs CI and then waits for approval on the
   `npm-publish` environment. Before approving, the reviewer checks that the tag
   points at the intended commit on `main` and that the `ci` jobs passed.
4. The publish job builds a release build (no `dev` feature, so no panic hook),
   runs `npm run verify` against that exact `pkg/`, and runs
   `scripts/prepare-release.sh` (name, version, repository and panic-hook
   checks, plus adding `NOTICE`). Only then does it publish.
5. Check the result: the npm page shows a **Provenance** badge linking to the
   workflow run, and `npm audit signatures` in a project that installs the
   package reports a verified attestation.

If any check fails, nothing is published. Fix it on `main` and release a new
patch version. A version number cannot be reused on npm.
