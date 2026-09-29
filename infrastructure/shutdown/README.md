# A convenient shutdown signal

`tari_shutdown` is a small cancellation primitive. A `Shutdown` is the owner side; each `ShutdownSignal` created from
it is a future that resolves when the owner shuts down. It works with any futures-based runtime.

## Basic usage

Create a `Shutdown`, and hand a `ShutdownSignal` to every task that should stop when it is triggered:

```rust
use tari_shutdown::{Shutdown, ShutdownReason};

# #[tokio::main(flavor = "current_thread")]
# async fn main() {
let shutdown = Shutdown::new();
let signal = shutdown.to_signal();
assert!(!shutdown.is_triggered());

let task = tokio::spawn(async move {
    let mut signal = signal;
    (&mut signal).await;
    // `is_triggered` is already true when a signal wakes up
    assert!(signal.is_triggered());
    println!("Finished");
});

// All signals resolve
shutdown.trigger();
// `trigger` is idempotent
shutdown.trigger();
assert!(shutdown.is_triggered());
assert_eq!(shutdown.reason(), Some(ShutdownReason::Triggered));

task.await.unwrap();
# }
```

## Clones and dropping

`Shutdown` is `Clone`, and every clone controls the same shutdown. Signals resolve when **any** clone calls `trigger`,
or when the **last** clone is dropped. `reason()` tells the two apart:

```rust
use tari_shutdown::{Shutdown, ShutdownReason};

let shutdown = Shutdown::new();
let other = shutdown.clone();
let signal = shutdown.to_signal();

drop(shutdown);
// A clone is still alive, so nothing has happened yet
assert!(!signal.is_triggered());
assert_eq!(signal.reason(), None);

drop(other);
// The last clone is gone: the signal has resolved
assert!(signal.is_triggered());
assert_eq!(signal.reason(), Some(ShutdownReason::Dropped));
```

So hold on to a `Shutdown` for as long as its listeners should keep running.

Dropping a `ShutdownSignal` does **not** trigger anything. It only tells the owner that one listener is gone.

## Waiting for listeners to exit

A signal is a notice: `trigger` returns immediately, before listening tasks have stopped. To wait until they have,
use `wait_for_listeners`, which resolves once every `ShutdownSignal` (including clones) has been dropped. A listener
that is never dropped keeps it pending forever, so bound it with a timeout:

```rust
use std::time::Duration;

use tari_shutdown::Shutdown;

# #[tokio::main(flavor = "current_thread")]
# async fn main() {
let shutdown = Shutdown::new();
let mut signal = shutdown.to_signal();
tokio::spawn(async move {
    (&mut signal).await;
    // ... clean up while still holding the signal ...
    drop(signal);
});

shutdown.trigger();
tokio::time::timeout(Duration::from_secs(30), shutdown.wait_for_listeners())
    .await
    .expect("listeners did not exit in time");
# }
```

A holder that only has a signal can use `ShutdownSignal::drained`, which consumes the signal and waits for all the
others.

## `FusedFuture`

`ShutdownSignal::is_terminated` follows the `FusedFuture` contract: it is true only after that particular handle has
returned `Poll::Ready`. Use `is_triggered` to ask whether the shutdown has happened.
