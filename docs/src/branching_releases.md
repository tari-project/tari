# Release Ideology

## Introduction

Tari's releasing philosophy is heavily influenced by Rust's own release process.¹

With our own process we hope to achieve a structured release cycle that is consistent, and dependable. For both users of the protocol, and developers of the protocol to have understandable expectations about when and how releases will happen. And what can be expected within each release.

Predominantly the team at Tari uses the Gitflow² branching model for work on the protocol. We won't linger long over the description of this flow unless necessary, but you should consider yourself acquainted with the process before continuing on. 

## Channels

We run a series of networks for different purposes but each network will have come from one of these three channels.

- TestNet
- NextNet
- StageNet/MainNet

### Networks

- MainNet: The big shebang, the head honcho, the real deal. This is the Tari protocol at work.
- StageNet: A stable network as similar as possible to MainNet available for testing.
- NextNet: The next release of the network. This should become available 8 weeks prior to going live on MainNet.
- TestNet(s): Our named test networks used for development. Where we can play a little fast and loose with features where, although we try to avoid it- the networks may see breakages and resets more frequently.
  - Example networks (non-exhaustive):
    - `Esme`: Has been our stable-er testnet prior to StageNet. It sees new features and enhancements as they come out, and supports our mobile wallets Tari Aurora on iOS and Android.
    - `Igor`: Is used in the development of the DAN Layer and sees frequent resets to help support the rapid pace of the development team.

## Releases

Let's define some facts for our discussion about releasing.

- StageNet/MainNet is at v1.53.0
- NextNet is at v1.54.0-rc.0
- TestNet is at v1.55.0-pre.4

The naming practice here helps to differentiate where any particular version is meant to be deployed too. Any version containing `pre` is destined for a TestNet and will have all features enabled. A version containing `rc` is destined for NextNet as it is the release candidate for the future StageNet/MainNet release. Any singular version with no pre-release version appended such as v1.53.0 means this is a StageNet/MainNet build. None of the in-development features should be enabled when this is compiled.

### Version numbers

Our version numbers can be broken down into 5 segments.

Example: `v1.55.2-rc.7`

- MAJOR: `1` - Used to anytime the network the _MainNet_ will see a hard-fork. TestNets may be reset, or hard-fork on a regular basis without a major version bump. The major version bump will be reserved for only a MainNet hard-fork.
- MINOR: `55` - For upgrades that may contain breaking changes. These changes won't cause the network to fork, but may require a change in configuration, a restart to the node, or other circumstances. This is only changed once on an 8-week release.
- PATCH: `2` - All other changes including HotFixes will be reflected in PATCH version increments.
- PRE-RELEASE TAG: `rc.7` - in the form of `rc` for NextNet and the form `pre` for development networks. In the case of development networks the number following the pre-release tag is incremented anytime a developer wishes to produce a new binary for the testnet. In the case of NextNet it would be used to produce a change to the release candidate.

### Development releases

A developer created a new feature which was added behind a feature gate. They merged it into the `development` environment and want to test it on the `Esme` test network. To do this, they created a temporary branch named `testnet-1.55-pre.5` and made any necessary changes such as upgrading the version. Once complete, they tagged the version as `v1.55.0-pre.5`. The branch can be merged back into development, and then deleted.
By tagging this new version, it will prompt the release CI to compile Tari applications with the `TARI_NETWORK=esme` flag enabled, which allows all development feature gates to be utilized.

Developers can perform this process as often and as many times as they want.

### 8 Week ritual release

Every 8 weeks the development team will trigger both a NextNet, and StageNet/MainNet release. To be specific the current NextNet version will become the next StageNet/MainNet version. And a new NextNet version will be tagged for the NextNet network, and for use 8 week in the future in StageNet/Mainnet. To do this a developer will checkout the current NextNet tag v1.54.0-rc.0. They can then re-tag this version as v1.54.0. This will trigger a new compilation of the StageNet/MainNet binaries with the flag `TARI_NETWORK=mainnet` which will ensure any existing feature gate is closed.  
Next the developer will checkout the development branch. From here create a new release branch `nextnet-1.55.0-rc.0`. Make any version bumps or changes necessary. Tag this new branch as v1.55.0-rc.0 triggering a CI compilation of the NextNet binary with the flag `TARI_NETWORK=nextnet` which will allow feature gates in the testing status to stay open. The release branch can be merged back into development and then deleted.
Lastly the development branch should be reset to the _next_ version v1.56.0-pre.0. This makes it simple for the next testnet release and leaves no questions as to what version comes next.

After all the above tags have occurred it would leave us with the version changes as follows:

- StageNet/MainNet from v1.53.0 -> v1.54.0
- NextNet from v1.54.0-rc.0 -> v1.55.0-rc.0
- TestNet from v1.55.0-pre.4 -> v1.55.0-pre.5 -> v1.56.0-pre.0

A visual aid for a sequence of events and lifetimes of channels:
```
development:      * - - * - - * - - * - - * - - * - - * - - * -- * -- *
                        |           |     |           |          |
testnet:                *           *     |           |          *
                                          |           |
nextnet:                                  * - - - *   * - - - *
                                                  |           |
stagenet/mainnet:                                 *           *
```

### Notable side-effects

There's lots of PR's changing the version. _Yes_. This happens.

## Hotfixes

Note: This follows the Gitflow process for hotfixes³.

If an important bug fix is required on MainNet. A developer will create a new branch *from the last MainNet tag*. The fix will be applied. This branch will have *3* PR's. 
- The hotfix branch back into development
- The hotfix branch into a NextNet branch
- The hotfix branch into a StageNet branch

The oddity here is that this is the _only_ time code from the MainNet tag moves backwards into the development branch.

```
hotfix:                             *
                                    |
stagenet/mainnet:           *       *           *
                            |       |           |
nextnet:              * - - *       *     * - - *
                      |             |     |
development:      * - - * - - * - - * - - * - - *
                        |           |           |
testnet:                *           *           *
```

## References

1. [How Rust is Made](https://doc.rust-lang.org/book/appendix-07-nightly-rust.html) https://doc.rust-lang.org/book/appendix-07-nightly-rust.html
2. [A successful git branching model](https://nvie.com/posts/a-successful-git-branching-model/) https://nvie.com/posts/a-successful-git-branching-model/
3. [Hotfix branches](https://nvie.com/posts/a-successful-git-branching-model/#hotfix-branches) https://nvie.com/posts/a-successful-git-branching-model/#hotfix-branches