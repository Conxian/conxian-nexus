# Release Runbook

## Versioning and tag format

- Follow Semantic Versioning: `MAJOR.MINOR.PATCH`.
- Create release tags as `vMAJOR.MINOR.PATCH`.

## Changelog requirement

- Update `../CHANGELOG.md` before tagging.
- Add an entry for the version being released with the date and notable changes.

## Release steps

1. Ensure `main` is up to date.
2. Confirm `../CHANGELOG.md` includes the release version.
3. Create an annotated tag:
   - `git tag -a vX.Y.Z -m "Release vX.Y.Z"`
4. Push the tag:
   - `git push origin vX.Y.Z`
5. Publish the GitHub Release using the matching version and release notes.

## Verification checklist

- [ ] Tag `vX.Y.Z` exists on GitHub.
- [ ] Required CI checks passed for the release commit.
- [ ] A GitHub release was published for `vX.Y.Z`.
- [ ] The release notes reflect the corresponding `../CHANGELOG.md` section.

## Control sign-offs before tagging

- [ ] State-proof and synchronization changes have been reviewed for correctness.
- [ ] Security-sensitive service or persistence changes have been reviewed.
- [ ] No private operational data or credentials were introduced.
- [ ] Authority boundaries remain unchanged and no custody behavior is introduced.
