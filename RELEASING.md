# Releasing

`@aws/dogwood-wasm` is published to npm only by
[`.github/workflows/release.yml`](.github/workflows/release.yml), when a
version tag is pushed. Nobody publishes from a laptop, and no npm token exists:
the workflow authenticates with [npm trusted
publishing](https://docs.npmjs.com/trusted-publishers) (GitHub OIDC), and npm
attaches a provenance attestation linking each version to the workflow run and
commit that built it.

## One-time setup

You need admin access to the GitHub repository. The `@aws` npm organization is
managed centrally, so the npm steps are done by its administrators, through
their onboarding process, not by this project.

**The repository must be public before the first release.** npm generates
provenance only for packages published from a public repository, and the
workflow publishes with `--provenance`, so a release from a private repository
fails. Changing the visibility is restricted by an enterprise ruleset and goes
through the open source program office.

### GitHub

Releases are controlled by the `dogwood-policy/dogwood-dev` team: only
its members can create a `v*` tag, and a release needs the approval of a member
other than the one who pushed the tag. Every maintainer of this repository is
an admin, so admin bypass is turned off wherever GitHub offers it.

1. **Team access.** Settings → Collaborators and teams → give `dogwood-dev`
   at least the **Read** role. GitHub accepts a team as a required reviewer
   only if it has access to the repository.
2. **Environment.** Settings → Environments → New environment `npm-publish`.
   - **Required reviewers:** the `dogwood-dev` team.
   - Turn on **Prevent self-review**, so whoever pushed the tag cannot approve
     the release.
   - Turn off **Allow administrators to bypass configured protection rules**.
     Otherwise any admin could publish without a second approver.
   - **Deployment branches and tags:** Selected branches and tags → add the tag
     pattern `v*`.
3. **Tag rulesets.** Settings → Rules → Rulesets → New tag ruleset. GitHub
   applies a bypass list to a whole ruleset, not to one rule, so this takes two
   rulesets, both targeting the tag pattern `v*` with enforcement Active:
   - `v* tags: immutable`: Restrict updates, Restrict deletions, Block force
     pushes. No bypass for anyone, so a published version's tag can never move.
   - `v* tags: creation restricted to dogwood-dev`: Restrict creations,
     with the `dogwood-dev` team as the only bypass actor (always).
4. **Branch ruleset for `main`** requiring the `build-test`, `cargo-deny`,
   `npm-audit` and `pinned` checks. The release workflow only publishes commits
   already on `main`, so this is what makes "reviewed" mean something.

### npm (`@aws` organization administrators)

Ask the `@aws` npm organization administrators to:

1. Create the package `@aws/dogwood-wasm` in the `@aws` organization, or
   reserve the name for it.
2. Add a **trusted publisher** for it (Package → Settings → Trusted publishing
   → GitHub Actions):
   - Organization or user: `dogwood-policy` (the GitHub organization, not the
     npm one)
   - Repository: `dogwood-wasm`
   - Workflow filename: `release.yml`
   - Environment: `npm-publish`
3. Set Package → Settings → **Publishing access** to *Require two-factor
   authentication and disallow tokens*. This does not affect the trusted
   publisher.
4. Confirm that no npm token held by this project can publish the package.

A new trusted-publisher configuration expires if the first publish through it
does not happen **within 2 days**. Set it up just before the first release, or
recreate it if it expires.

**First publish.** If npm will not accept a trusted publisher until the
package exists, agree the bootstrap with the `@aws` administrators. They either
create the package themselves, or issue a granular token scoped to this one
package with a 1-day expiry, used once from a clean checkout of the release tag
(`npm publish ./pkg --provenance`, after `scripts/prepare-release.sh` passes)
and revoked immediately afterwards. Then do steps 2–4. Every later version goes
through the workflow.

## Cutting a release

1. On a branch, set the version in `Cargo.toml` (`version = "X.Y.Z"`). Update
   `Cargo.lock` with `cargo update -p dogwood-wasm`, then open a PR and merge it
   once CI is green.
2. A member of `dogwood-dev` tags the merge commit on `main` and pushes
   the tag. Nobody else can create a `v*` tag, and once pushed it cannot be
   moved or deleted.
   ```sh
   git checkout main && git pull
   git tag -s vX.Y.Z -m "vX.Y.Z"
   git push origin vX.Y.Z
   ```
   A prerelease tag (`vX.Y.Z-rc.1`) publishes under the `next` dist-tag, not
   `latest`.
3. The `release` workflow runs CI and then waits for approval on the
   `npm-publish` environment. **Another** member of `dogwood-dev`, not
   the one who pushed the tag, approves it: Actions → the `release` run →
   **Review deployments** → `npm-publish` → Approve and deploy. Before
   approving, the reviewer checks that the tag points at the intended commit on
   `main` and that the `ci` jobs passed.
4. The publish job builds a release build (no `dev` feature, so no panic hook),
   runs `npm run verify` against that exact `pkg/`, and runs
   `scripts/prepare-release.sh` (name, version, repository and panic-hook
   checks, plus adding `NOTICE`). Only then does it publish.
5. Check the result: the npm page shows a **Provenance** badge linking to the
   workflow run, and `npm audit signatures` in a project that installs the
   package reports a verified attestation.

If any check fails, nothing is published. Fix it on `main` and release a new
patch version. A version number cannot be reused on npm.
