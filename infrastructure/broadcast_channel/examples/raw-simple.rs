use tari_broadcast_channel::raw_bounded;

fn main() {
    let (tx, rx) = raw_bounded(10);
    (1..15).for_each(|x| tx.broadcast(x).unwrap());

    let received: Vec<i32> = rx.map(|x| *x).collect();
    // Test that only the last 10 elements are in the received list.
    let expected: Vec<i32> = (5..15).collect();

    assert_eq!(expected, received);
}
