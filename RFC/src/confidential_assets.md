# Confidential assets

The commitment is vH + rG + tF + sE

- v - amount
- r - blinding factor
- t - token id
- s - asset id

## Getting tokens:

---

- minting

```
  message MintRequest {
    bytes asset_id = 1;
    bytes token_id = 2;
    bytes signature = 3;
    bytes commitment = 4;
    bytes proofs = 5;
  }
```

- asset_id - has to be clear for constitution check
- token_id - same as above
- signature - Schnorr signature for the key associated with the asset_id, we don't need to send the public key, VN should now it.
- commitment
- proofs:
  - amount is in the range defined in the constitution (especially for NFTs the equality to one)

## Sending FT

---

```
  message TransferRequest {
    bytes from = 1;
    bytes to = 2;
    bytes input_commitment = 3;
    bytes output_commitment = 4;
    repeated bytes excess_commitment = 5;
    bytes proofs = 6;
  }
```

- from - schnorr signature for the input commitment (we don't need to send the public_key, do we?)
- to - destination public_key
- input_commitment
- output_commitment
- excess_commitment - the change from the transaction
- proofs :
  - v > 0 in commitment
  - v > 0 in the excess_commitment, in case the excess_commitment is there

## Sending NFT

---

The commitment is the same as FT, but with fixed value of **v**.

# Concerns

- Should VN know which token_id are being minted? For NFT the check as to be there, but for FT they should probably not care
- VN should know which asset is being transferred (they need to check the constitution)
