# Feature gating

## Step 1: define features

In build.rs, we define our features.

Features have a
* name - will also be the attribute name you use in code
* description - a short description of the features
* issue tracking number on Github
* The current status.

#### Status

* New: New feature, may not even be working or compiling. Will be present on `testnet`
* Testing: Feature is undergoing testing on `nextnet`. Does not exist on `mainnet` or `stagenet`.
* Active: Feature is live and gate has been removed. Will be running on all networks.
* Removed: Feature has been cancelled. Can not be invoked anywhere.

In `build.rs`, we maintain the list of feature flags. For example:

```rust
const FEATURE_LIST: [Feature; 4] = [
    Feature::new("add_pair", "Allows you to add two numbers together", Some(123), Status::New),
    Feature::new("about_to_launch", "Live in NextNet. If we stabilise, will go to mainnet", Some(123), Status::Testing),
    Feature::new("live_feature", "This feature has been stabilised and is live!", Some(150), Status::Active),
    Feature::new("old_feature", "We decided not to go with this featuree", Some(145), Status::Removed),
];
```

## Step 2: Demarcate feature flag code

In your code, you can now use any of the flags as attributes to mark code belonging to a feature.

Example

```rust
#[cfg(tari_feature_add_pair)]
use add_pair::add_pair;

fn main() {
    println!("Hello world!");
    #[cfg(tari_feature_add_pair)]
    println!("40 + 2 = {}", add_pair(40, 2));
    println!("foo={}", foo());
    println!("Bye, world!");
}

#[cfg(tari_feature_add_pair)]
fn foo() -> usize {
    1
}

#[cfg(not(tari_feature_add_pair))]
fn foo() -> usize {
    2
}
```

## Step 3: Specify the target network when building

This PoC uses the `TARI_NETWORK` envar to specify the target network, but in principle, we can also read the `Cargo.toml`
file, or compiler flags.

`$ TARI_NETWORK=dibbler cargo run -vv`

Filtered output:

```text
Building for Dibbler
These features are ACTIVE on mainnet (no special code handling is done)
live_feature. This feature has been stabilised and is live!. Tracking issue: https://github.com/tari-project/tari/issues/150

These features are DEPRECATED and will never be compiled
old_feature. We decided not to go with this featuree. Tracking issue: https://github.com/tari-project/tari/issues/145

** Activating add_pair. Allows you to add two numbers together. Tracking issue: https://github.com/tari-project/tari/issues/123 **
** Activating about_to_launch. Live in NextNet. If we stabilise, will go to mainnet. Tracking issue: https://github.com/tari-project/tari/issues/123 **

Finished dev [unoptimized + debuginfo] target(s) in 7.44s
     Running `target/debug/feature_gates`
Hello world!
40 + 2 = 42
foo=1
Bye, world!
```

Or compiling for MainNet:

`$ TARI_NETWORK=mainnet cargo run -vv`

Filtered output:

```text
Building for MainNet

These features are ACTIVE on mainnet (no special code handling is done)
live_feature. This feature has been stabilised and is live!. Tracking issue: https://github.com/tari-project/tari/issues/150

These features are DEPRECATED and will never be compiled
old_feature. We decided not to go with this featuree. Tracking issue: https://github.com/tari-project/tari/issues/145
    Finished dev [unoptimized + debuginfo] target(s) in 6.15s
     Running `target/debug/feature_gates`
Hello world!
foo=2
Bye, world!
```
