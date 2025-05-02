# SOP: Release

This document outlines the Standard Operating Procedure (SOP) for running a new release.

## Introduction

Tari follows a structured release cycle. Releases begin on the `development` branch. After a period of development and internal testing, the release is promoted to `nextnet` for further stability testing. Following successful testing, the release is promoted to `mainnet`.

For more information, see: `docs/src/branching_releases.md`.

## Release: Development

1. Run  
   ```bash
   standard-version --infile changelog-development.md --skip.commit --skip.tag --release-as a.b.c-pre.x
   ```  
   where `a.b.c` is the intended mainnet version, and `x` is the sub-version for the development release cycle.
2. Update version numbers in `Cargo.toml`.
3. Run  
   ```bash
   cargo build
   ```
4. Copy relevant changelog entries to `changelog-nextnet.md` and `changelog-mainnet.md`, updating version numbers as appropriate for each network.
5. Update the README to reflect the latest version numbers for each network.
6. After merging the PR with the above changes into the `development` branch, push the tag:  
   ```bash
   git tag va.b.c-pre.x
   git push origin va.b.c-pre.x
   ```  
   to trigger the build process.

## Release: Nextnet

1. Remove branch protection from `nextnet`.
2. Rebase the `nextnet` branch onto the released `development` branch.
3. Re-enable branch protection on `nextnet`.
4. Update version numbers in `Cargo.toml` to `a.b.c-rc.x`.
5. Run  
   ```bash
   cargo build
   ```
6. After merging the PR into `nextnet`, push the tag:  
   ```bash
   git tag va.b.c-rc.x
   git push origin va.b.c-rc.x
   ```  
   to trigger the build process.

## Release: Mainnet

1. Remove branch protection from `mainnet`.
2. Rebase the `mainnet` branch onto the previously released `nextnet` branch.
3. Re-enable branch protection on `mainnet`.
4. Update version numbers in `Cargo.toml` to `a.b.c`.
5. Run  
   ```bash
   cargo build
   ```
6. After merging the PR into `mainnet`, push the tag:  
   ```bash
   git tag va.b.c
   git push origin va.b.c
   ```  
   to trigger the build process.
