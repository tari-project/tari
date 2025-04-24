# Tari Transaction Creation Process

This document outlines how Tari creates transactions using the Transaction Protocol. More details about how Tari transactions work can be found in [RFC-8002](https://rfc.tari.com/RFC-8002_TransactionProtocol) and [RFC-0201](https://rfc.tari.com/RFC-0201_TariScript).

## Step-by-Step Walkthrough

We'll explain the transaction creation process using this unit test as an example:  
[`async fn single_recipient_with_rewindable_change_and_receiver_outputs_bulletproofs`](https://github.com/tari-project/tari/blob/9361615fa80cfff13ea3d8b84809678b7c0472e4/base_layer/core/src/transactions/transaction_protocol/sender.rs#L1567). We've included the snippet of code below:

<details>
<summary>Single Recipient Transaction with Rewindable Change and Receiver Outputs</summary>

```rust
 async fn single_recipient_with_rewindable_change_and_receiver_outputs_bulletproofs() {
        // Alice's parameters
        let key_manager_alice = create_memory_db_key_manager().unwrap();
        let key_manager_bob = create_memory_db_key_manager().unwrap();
        // Bob's parameters
        let bob_test_params = TestParams::new(&key_manager_bob).await;
        let alice_value = MicroMinotari(25000);
        let input = create_test_input(alice_value, 0, &key_manager_alice, vec![], None).await;
        let script = script!(Nop).unwrap();
        let consensus_constants = create_consensus_constants(0);

        let mut builder = SenderTransactionProtocol::builder(consensus_constants.clone(), key_manager_alice.clone());
        let change_params = TestParams::new(&key_manager_alice).await;
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(20))
            .with_change_data(
                // "colour" this output so that we can find it later
                script!(PushInt(1) Drop Nop).unwrap(),
                inputs!(change_params.script_key_pk),
                change_params.script_key_id.clone(),
                change_params.commitment_mask_key_id.clone(),
                Covenant::default(),
                TariAddress::default(),
            )
            .with_input(input)
            .await
            .unwrap()
            .with_recipient_data(
                script.clone(),
                OutputFeatures::default(),
                Covenant::default(),
                0.into(),
                MicroMinotari(5000),
                TariAddress::default(),
            )
            .await
            .unwrap();
        let mut alice = builder.build().await.unwrap();
        assert!(alice.is_single_round_message_ready());
        let msg = alice.build_single_round_message(&key_manager_alice).await.unwrap();

        let change = alice_value - msg.amount - msg.metadata.fee;

        println!(
            "amount: {}, fee: {},  Public Excess: {}, Nonce: {}, Change: {}",
            msg.amount,
            msg.metadata.fee,
            msg.public_excess.to_hex(),
            msg.public_nonce.to_hex(),
            change
        );

        // Send message down the wire....and wait for response
        assert!(alice.is_collecting_single_signature());

        let bob_public_key = msg.sender_offset_public_key.clone();
        let bob_output = WalletOutput::new_current_version(
            MicroMinotari(5000),
            bob_test_params.commitment_mask_key_id,
            OutputFeatures::default(),
            script.clone(),
            ExecutionStack::default(),
            bob_test_params.script_key_id,
            bob_public_key,
            ComAndPubSignature::default(),
            0,
            Covenant::default(),
            EncryptedData::default(),
            0.into(),
            PaymentId::Empty,
            &key_manager_bob,
        )
        .await
        .unwrap();

        // Receiver gets message, deserializes it etc, and creates his response
        let bob_info =
            SingleReceiverTransactionProtocol::create(&msg, bob_output, &key_manager_bob, &consensus_constants)
                .await
                .unwrap();

        // Alice gets message back, deserializes it, etc
        alice
            .add_single_recipient_info(bob_info, &key_manager_alice)
            .await
            .unwrap();
        // Transaction should be complete
        assert!(alice.is_finalizing());
        match alice.finalize(&key_manager_alice).await {
            Ok(_) => (),
            Err(e) => panic!("{:?}", e),
        };

        assert!(alice.is_finalized());
        let tx = alice.get_transaction().unwrap();
        assert_eq!(tx.body.outputs().len(), 2);

        let output = tx.body.outputs().iter().find(|o| o.script.size() > 1).unwrap();

        let (key, _value, _) = key_manager_alice.try_output_key_recovery(output, None).await.unwrap();
        assert_eq!(key, change_params.commitment_mask_key_id);
    }
```
</details>
<br>

```rust
let key_manager_alice = create_memory_db_key_manager().unwrap();
```

Tari uses a **key manager** to control key derivation. Many keys are derived based on their intended use (spending, coinbase, etc.) and whether the wallet is hardware-based or not. The **key manager** handles all secrets in the Tari codebase, and these secrets never leave the manager.

```rust
let mut builder = SenderTransactionProtocol::builder(consensus_constants.clone(), key_manager_alice.clone());
```

We initiate the transaction protocol as the sender. We pass in the current consensus constants (which define max weight, coinbase lock height and other parameters.) and an `Arc` reference to the key manager.

After customizing all the transaction details, we construct the single-round message:

```rust
let mut alice = builder.build().await.unwrap();
let msg = alice.build_single_round_message(&key_manager_alice).await.unwrap();
```

This message is then sent to Bob.

Bob constructs his output:

```rust
let bob_output = WalletOutput::new_current_version(..);
```

Using this output, he creates the receiver part of the transaction:

```rust
let bob_info = SingleReceiverTransactionProtocol::create(&msg, bob_output, &key_manager_bob, &consensus_constants).await.unwrap();
```

Alice uses the information in `bob_info` to finalize the transaction:

```rust
alice.add_single_recipient_info(bob_info, &key_manager_alice).await.unwrap();
alice.finalize(&key_manager_alice).await;
```

Finally, the completed transaction can be retrieved:

```rust
let tx = alice.get_transaction().unwrap();
```

## One-Sided Transactions

For one-sided transactions, Alice also controls the `SingleReceiverTransactionProtocol`. The only significant difference is the script used. See [RFC-0204](https://rfc.tari.com/RFC-0204_TariScriptExamples#one-sided-payment) for more details.
