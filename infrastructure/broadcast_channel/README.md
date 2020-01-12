# Bounded Non-Blocking Single-Producer, Multi-Consumer Broadcast Channel

Parts of this code were forked from https://github.com/filipdulic/bus-queue. 

# Examples
## Simple bare usage
```rust
use tari_bus::bare_channel;

fn main() {
    let (tx, rx) = bare_channel(10);
    (1..15).for_each(|x| tx.broadcast(x).unwrap());

    let received: Vec<i32> = rx.map(|x| *x).collect();
    // Test that only the last 10 elements are in the received list.
    let expected: Vec<i32> = (5..15).collect();

    assert_eq!(expected, received);
}
```

```rust
use tari_bus::bounded;
use futures::executor::block_on;
use futures::stream;
use futures::StreamExt;

fn main() {
    let (publisher, subscriber1) = bounded(10);
    let subscriber2 = subscriber1.clone();

    block_on(async move {
        stream::iter(1..15)
            .map(|i| Ok(i))
            .forward(publisher)
            .await
            .unwrap();
    });

    let received1: Vec<u32> = block_on(async { subscriber1.map(|x| *x).collect().await });
    let received2: Vec<u32> = block_on(async { subscriber2.map(|x| *x).collect().await });
    // Test that only the last 10 elements are in the received list.
    let expected = (5..15).collect::<Vec<u32>>();
    assert_eq!(received1, expected);
    assert_eq!(received2, expected);
}
```
