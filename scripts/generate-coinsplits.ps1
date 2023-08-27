
$i=1
Do {
    $i

    $splits = get-random -max 5 -min 1
    $taijiAmount = get-random -max 1000 -min 200
    #& cargo run --release --bin taiji_console_wallet -- -b .  --password mike --command-mode-auto-exit -p "wallet.base_node.base_node_rpc_pool_size=1" -p "wallet.command_send_wait_timeout=0" coin-split $taijiAmount $splits 
    & ../../target/release/taiji_console_wallet -b .  --password mike --command-mode-auto-exit -p "wallet.base_node.base_node_rpc_pool_size=1" -p "wallet.command_send_wait_timeout=0" coin-split $taijiAmount $splits 


    $i++
    }
While ($i -le 1000)


