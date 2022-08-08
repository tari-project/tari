
$i=1
Do {
    $i

    $splits = get-random -max 10 -min 1
    $tariAmount = get-random -max 50000 -min 10000
    & cargo run --release --bin tari_console_wallet -- -b . --network esmeralda --password mike --command-mode-auto-exit  coin-split $tariAmount $splits


    $i++
    }
While ($i -le 1000)


