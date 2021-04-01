# Wallet to Wallet transaction negotiation with TariScript

This write up seeks to describe the detailed interaction between two wallets who are negotiating a Mimblewimble (MW) transaction 
that includes a TariScript. The goal is to present the full set of operations that each party needs to perform, what data is 
shared between the parties and also what data is retained by the receiver in order to spend the received UTXO in the future. 

For a full breakdown of TariScript please see this [RFC](https://rfc.tari.com/RFC-0201_TariScript.html). This document will 
repeat some of the detail contained in the RFC so that it is self-contained.

We will use the same [notation](https://rfc.tari.com/RFC-0201_TariScript.html#notation) described in the RFC.

## Alice sends a transaction to Bob
Alice is going to send a transaction to Bob which spends a commitment \\( C_a \\), with a spending script \\( \alpha_a \\), 
to a commitment \\( C_b \\), with a spending script \\( \alpha_b \\) that Bob will use to spend the new UTXO in the future.

### Alice Round 1
To spend \\( C_a \\) Alice must know the following:
1. Spending key \\( k_a \\) of \\( C_a \\),
2. Value \\( v_a \\) of \\( C_a \\),
3. Features  \\( F_a \\) of \\( C_a \\),
4. Script \\( \alpha_a \\) whose hash \\( \sigma_a = H(\alpha_a) \\),
5. Script input \\( \theta_a \\) for script \\( \alpha_a \\),
6. Script key \\( k_{sa} \\) whose public value (\\( K_{sa} = k_{sa} \cdot G \\)) is the key left over when script \\( \alpha_a \\) is executed with input \\( \theta_a \\)   
7. Height \\( h_a \\) where \\( C_a \\) was mined.

Alice then chooses the following values:
1. Amount \\( v_b \\) being sent to Bob
2. Fee value \\( f \\). The amounts need to balance out so \\( v_a = v_b + f \\)    
3. A random nonce \\( r_a \\) whose public value is \\( R_a = r_a \cdot G \\)
4. An UTXO offset key \\( k_{Ob} \\) for Bob's UTXO, whose public value is \\( K_{Ob} = k_{Ob} \cdot G \\)
5. The script \\( \alpha_b \\) that will become the spending script on \\( C_b \\).
6. Change spending key \\( k_c \\) for the change output \\( C_c \\)
7. A script for the change UTXO \\( \alpha_c \\)
8. A script key \\( k_{sc} \\) for \\( \alpha_c \\)    
9. The UTXO offset key for the change UTXO \\( k_{Oc} \\) whose public value is \\( K_{Oc} = k_{Oc}G \\)
10. The script for Bob's UTXO \\( \alpha_b \\)
 
Firstly Alice prepares the transaction the same way she would for a vanilla MW transaction by calculating the excess:
\\( x_s = k_c - k_a \\)

Alice will also calculate the rangeproof \\( RP_c \\) for her change output for the modified commitment

\\( \hat{C}_c = (k_c + \beta_c) \cdot G + v_c \cdot H \\)

where

\\( \beta_c = H(\sigma_c || F_c || K_{Oc}) \\).

Alice then sends the following values to Bob:

| Symbol  | Description                                      |
|---------|--------------------------------------------------                                   |
| \\( R_a \\)       | Alice's Public Nonce                                                      |
| \\( v_b \\)       | Amount being sent to Bob                                                  |
| \\( f \\)         | fee                                                                       |
| \\( m \\)         | Transaction metadata (currently just lockheight)                          |
| \\( \alpha_b \\)  | Spending script for Bob's UTXO, \\( C_b \\)                               |
| \\( K_{Ob} \\)    | Public UTXO script offset                                                 |
| message           | A unicode string                                                          |

### Bob Replies
Once Bob receives these values he will choose the following:
1. Spending key \\( k_b \\) of \\( C_b \\)
2. A random nonce \\( r_b \\) whose public value is \\( R_b = r_b \cdot G \\)
3. His script key \\( k_s \\) whose public value \\( K_s = k_s \cdot G \\) must be the final value on the stack after script \\( alpha_b \\) executes.
4. Output features \\( F_b \\) for \\( C_b \\) if any.

Bob produces the rangeproof \\( RP_b \\) for the modified commitment

\\( \hat{C}_b = (k_b + \beta_b) \cdot G + v_b \cdot H \\) 

where 

\\( \beta_b = H(\sigma_b || F_b || K_{Ob}) \\)

and

\\( \sigma_b = H(\alpha_b) \\). 

Bob then produces his partial signature of the transaction as per vanilla MW:

\\( s_b = r_b + ek_b \\)

where

\\( e = H(R_a + R_b || f || m) \\)

Bob then sends the following back to Alice:

| Symbol  | Description                                      |
|---------|--------------------------------------------------|
| \\( C_b \\)       | Bob's commitment                      |
| \\( RP_b \\)      | Bob's range proof                     |
| \\( K_b \\)       | Bob's public spending key             |
| \\( F_b \\)       |  The Output Features of \\( C_b \\)   |
| \\( s_b, R_b \\)  | Bob's partial signature               |

### Alice Round 2
Alice now has the information to construct the challenge \\( e \\)to calculate her partial signature:

\\( s_a = r_a + ek_a \\)

and can produce the final kernel signature:

\\( (s = s_a + s_b,  R = R_a + R_b )\\)

Alice then produces a script signature (\\( s_{sa}, R_{sa} \\)) for spending \\( C_a \\) by signing \\( H( R_{sa} || \alpha_a || \theta_a || h) \\) with her private script key \\( k_{sa} \\).

Alice can now calculate the script offset, \\( \gamma \\) for this transaction which will include the script of the input \\( C_a \\) and the two outputs \\( C_b \\) and \\( C_c \\).

\\( \gamma =  k_{sa} - k_{Ob}U_b - k_{Oc}U_c \\)

### Final transaction

Alice can now construct the final transaction:

| Transaction        |              |
|--------------------|-------       |
| offset          | \\( x_s \\)     |
| script_offset   | \\( \gamma \\)  |

| Transaction Input |      |
|--------------------|-------|
| Commitment                |  \\( C_a = k_a \cdot G + v_a \cdot H  \\) |
| Features                  | Fa                                        |
| Script                    | \\( \alpha_a \\)                          |
| Input Data                | \\( \theta_a \\)                          |
| height                    | \\( h \\)                                 |
| Script Signature          | \\( (s_{sa}, R_{sa}) \\)                  |
| Script offset public key  | \\( K_{Oa} \\)                            |

| Transaction Output |      |
|--------------------|-------|
| Commitment    |  \\( C_b = k_b \cdot G + v_b \cdot H  \\)     |
| Features      | Fb                                            |
| Rangeproof    | \\( RP_b \\) for \\( \hat{C}_b \\)                    |
| Script Hash   | \\( \sigma_b = H( \alpha_b) \\)               |
| Script offset public key | \\( K_{Ob} \\)                     |

| Transaction Output |      |
|--------------------|-------|
| Commitment    |  \\( C_c = k_c \cdot G + v_c \cdot H  \\)     |
| Features      | Fc                                            |
| Rangeproof    | \\( RP_c \\) for \\( \hat{C}_c \\)            |
| Script Hash   | \\( \sigma_c = H( \alpha_c) \\)               |
| Script offset public key | \\( K_{Oc} \\)                     |

| Transaction Kernel |       |
|--------------------|-------|
| Public Excess      | \\( X_s + K_b \\)    |
| Signature          | \\( s, R \\)         |
| Fee                | f                    |
| Metadata           | m                    |



Alice will now send this finalized transaction back to Bob so he has a copy of it and the script and both parties can now submit this transaction to the blockchain.

### After transaction broadcast

After the transaction has been broadcast to the mempool both Alice and Bob will have to monitor the blockchain to detect what height the transaction is mined at. Bob will then
record this Height in his database for use when spending the UTXO and Alice will do the same for her Change UTXO.

## Changes to Transaction messages and stored data in Rust
In [RFC](https://rfc.tari.com/RFC-0201_TariScript.html) the changes to the TransactionInput, TransactionOutput and Transaction structs are detailed but there are
two more structs that will need to be updated to support TariScript.

This is the struct that is sent by the Sender to the Recipient and needs to contain 2 new fields
```rust,ignore
pub struct SingleRoundSenderData {
    /// The transaction id for the recipient
    pub tx_id: u64,
    /// The amount, in µT, being sent to the recipient
    pub amount: MicroTari,
    /// The offset public excess for this transaction
    pub public_excess: PublicKey,
    /// The sender's public nonce
    pub public_nonce: PublicKey,
    /// The transaction metadata
    pub metadata: TransactionMetadata,
    /// Plain text message to receiver
    pub message: String,
    
    // NEW FIELDS
    
    /// Hash of the receivers UTXO script, \sigma
    pub script_hash: Vec<u8,
    /// Public script offset key chosen for Recipient, K_o
    pub public_script_offset_key: PublicKey,
```

And the receiver will need to keep track of their spendable UTXO's using the following updated struct

```rust,ignore
pub struct UnblindedOutput {
    pub value: MicroTari,
    pub spending_key: BlindingFactor,
    pub features: OutputFeatures,
    
    // NEW FIELDS
    
    /// The serialised script
    script: Vec<u8>,
    /// The script input data, if any
    input_data: Vec<u8>,
    /// The block height that the UTXO was mined
    height : u64,
    /// Script private key, k_s
    script_private_key: PrivateKey,
    /// Public script offset, K_O
    public_script_offset: PublicKey,
}
```
**NOTE:** The height in the previous struct will need to be updated AFTER the transaction negotiation is complete once the Receiver has detected the UTXO has been mined.

## Questions
1. Does the receiver need to get the script itself earlier than finding it in the Finalized transaction?
   
   1.1 Initial discussion resulted in us decided that Alice sends the full script in Round 1
2. Do parties agree of the Output Features of the received output or does Bob just choose it? Currently it's assumed default
3. Do you need the script offset public key in both the Transaction Input and the Transaction Output?
