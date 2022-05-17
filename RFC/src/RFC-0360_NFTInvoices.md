# RFC-0360/NFTInvoices

## NFT sale via Mimblwimble Invoice

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Philip Robinson](https://github.com/philipr-za)

# Licence

[The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2021 The Tari Development Community

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
following conditions are met:

1. Redistributions of this document must retain the above copyright notice, this list of conditions and the following
   disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following
   disclaimer in the documentation and/or other materials provided with the distribution.
3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products
   derived from this software without specific prior written permission.

THIS DOCUMENT IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS", AND ANY EXPRESS OR IMPLIED WARRANTIES,
INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
WHETHER IN CONTRACT, STRICT LIABILITY OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF
THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

## Language

The keywords "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", 
"NOT RECOMMENDED", "MAY" and "OPTIONAL" in this document are to be interpreted as described in 
[BCP 14](https://tools.ietf.org/html/bcp14) (covering RFC2119 and RFC8174) when, and only when, they appear in all capitals, as 
shown here.

## Disclaimer

This document and its content are intended for information purposes only and may be subject to change or update
without notice.

This document may include preliminary concepts that may or may not be in the process of being developed by the Tari
community. The release of this document is intended solely for review and discussion by the community of the
technological merits of the potential system outlined herein.

## Goals

The aim of this Request for Comment (RFC) is to describe a formulation of a [Mimblewimble] [transaction] negotiation process
to allow for the sale of an [Non Fungible Token] (NFT) by a Seller to a Buyer. The Seller will initiate the transaction negotiation by
specifying which [NFT] containing [UTXO] they are offering for sale and how much Tari they expect in return. Having the party
that will receive the funds specify the funds they wish to receive is termed an Invoice. The Buyer will then provide their payment 
input and resulting change output to the transaction and provide a partial signature. The final step will be the Seller 
completing the final signature to produce a complete transaction.

The process must be completely atomic. At no point should the data included in the negotiation be malleable by the other party
or allow for one party to change the elements provided by the counterparty without making the transaction invalid.

## Related Requests for Comment

* [RFC-0201: TariScript](RFC-0201_TariScript.md)
* [RFC-0300: Digital Assets Network](RFCD-0300_DAN.md)

$$
\newcommand{\hash}[1]{\mathrm{H}\bigl({#1}\bigr)}
$$

## Description
The standard interactive [Mimblewimble] transaction is usually initiated by a party, the Sender, that is going to send 
funds to a Receiver. The Sender selects a number of input UTXOs from which the sent funds and fees will come from, they
build a change UTXO to receive any remaining funds and calculate the excess of these inputs and change output. The 
Receiver is then told how many Tari they will receive and provides the aggregated public excess and public nonce of the 
Sender's portion of the transaction signature. The Receiver builds a commitment with the specified value and their 
private spending key and then provides their partial signature for their UTXO and the excess. This gets sent back to the 
Sender who then completes the transaction by providing their partial signature and aggregating it with the Receiver's 
partial signature.

In the standard Mimblewimble negotiation there are two main elements that make the process trustless for the Sender and
Receiver. Firstly, for the Sender, whose inputs are being spent, has the final say in whether to complete the transaction
by producing the final signature. For the Receiver there is little risk because they have not provided any inputs at all
and are just receiving funds, so they have nothing to lose if the process is compromised. Furthermore, their partial signature
means that the Sender cannot change anything about the Receiver's output in the final stage of negotiation.

When it comes to the sale of a token using a [Mimblewimble] style transaction the risk profile for both the Seller and the
Buyer is a bit more complicated.
1. The Seller has to prove that they own a given token UTXO and provide some partial signature committing to the terms of 
the sale without the Buyer being able to use that data to build a new transaction transferring ownership of the token
with different terms to what the Seller specified.
2. The Buyer has to provide inputs into the transaction before the finalization step in such a way that those inputs are 
committed but not at risk of the Seller being able to claim the inputs without completing the agreed upon transaction in full.

So the ultimate goal must be a negotiation process where the Seller and Buyer can interact completely trustlessly to build
this transaction to the agreed terms. This must be done while each party's contributions to the negotiation are not 
malleable by the other party and the entire process must be atomic. It either results in a valid transaction built as both
parties expect or not at all.

## Transaction Negotiation

### Assumptions:
- The UTXOs containing the token data have a value of zero (This has not been decided yet in the RFCs regarding token UTXOs).
- The Buyer is going to pay the fees.
- The Buyer will select a single input for payment purposes whose value will cover the requested payment and fees.
- In the following notation a Transaction Input/Output will be represented by \\( C_x \\) and when used in equations
will represent a Pederson Commitment but when sent to the counterparty will include all the addition metadata as described in
[RFC-0201: TariScript](RFC-0201_TariScript.md)

In the following equations the capital letter subscripts, _S_ and _B_ refer to a token _Seller_ and _Buyer_ respectively.

The transaction that is constructed by this process will need to produce an output [metadata signature] for the Transaction Output
the Buyer will receive the token in and also the Transaction Output the Seller will receive their payment in. These signatures
will be aggregated [Commitment Signature]s that are described in detail in [RFC-0201: TariScript](RFC-0201_TariScript.md).

[RFC-0201: TariScript](RFC-0201_TariScript.md) is written from the perspective of a standard Mimblewimble transaction where
only the Sender has Transaction Inputs and the Receiver only has a single Transaction Output. The case this RFC discusses 
both parties receive a Transaction Output and both parties supply a Transaction Input. This results in the requirement of an
extra round of communication between the parties. However, the negotiation described below allows for a transaction to be 
built that can be validated exactly the same was as is described in [RFC-0201: TariScript](RFC-0201_TariScript.md).

### Negotiation

#### Round 1
The Seller initiates the negotiation by collecting and calculating the following:

| Terms                                                     | Description                                                                                                              |
|--------------------------------------                     |-----------------------------------------------------------------------------------------------------------               |
| \\( C_{St} = 0 \cdot H  + k_{St} \cdot G \\)              | Select the Seller's token Transaction Input to be offered                                                                |
| \\( C_{Sp} = v_p \cdot H + k_{Sp} \cdot G \\)             | Choose a value to be requested from Buyer and select a spending key for the payment Transaction Output to be received    |
| \\( R_{Sk} = r_{Sk} \cdot G \\)                           | Choose a excess signature nonce                                                                                          |
| \\( R_{SMSt} = r_{SMSt_a} \cdot H + r_{SMSt_b} \cdot G \\)| Choose a Seller commitment signature nonce for the Buyer's token output                                                  |
| \\( K_{SO} = k_{SO} \cdot G \\)                           | Choose a Seller Offset to be used in the Buyer's received token output [metadata signature]                              | 
| \\( x_S = k_{Sp} - k_{St} \\)                             | Seller's private Excess                                                                                                  |
| \\( X_S = x_S \cdot G \\)                                 | Seller's public Excess                                                                                                   |
| \\( s_{St} \\)                                            | A [Commitment Signature] using \\( C_{St} \\) signing the message \\( e = (X_S \Vert v_p \Vert R_S) \\)                  |  

The Seller now sends the following to the Buyer

| Items           | Description                                                                                                  |
|-----------------|-----------------------------------------------------------------------------------------------------------   |
| \\( C_{St} \\)  | The Seller's token Transaction Input to be offered                                                           |
| \\( C_{Sp} \\)  | Seller's payment Transaction Output to be received                                                           |
| \\( v_p \\)     | The value the Seller is requesting as part of this offer                                                     |
| \\( R_{Sk} \\)  | Seller's public excess signature nonce                                                                       |
| \\( R_{SMSt}\\) | Seller's public metadata signature nonce for the Buyer's token UTXO                                          |
| \\( K_{SO}\\)   | Seller Offset public key                                                                                     |
| \\( s_{St} \\)  | A [Commitment Signature] using \\( C_{St} \\) signing the message \\( e = (X_S \Vert v_p \Vert R_s) \\)      |


#### Round 2
The commitment signature is provided to the Buyer to show that the Seller does indeed own the token UTXO \\( C_{St} \\). 
The Buyer's wallet should verify on the base layer that this UTXO is for the token they are being offered.

The Buyer will construct the Seller's public Excess, \\( X_s \\) from the provided components:
\\( X_s = C_{Sp} - C_{St} - v_p \cdot H \\)
This operation confirms that the only commitments that form part of the Seller's excess are the token input and a commitment 
with the value of the requested payment. To ensure that there is no malleability in the Seller's payment output the Buyer
will also provide the Seller with an offset public key so that the Seller can produce an output [metadata signature].

The Buyer will now calculate/choose the following:

| Terms                                                                    | Description                                                                                                                          |
|--------------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------                           |
| \\( C_{Bp} = v_{Bp} \cdot H  + k_{Bp} \cdot G \\)                        | Buyer's selected Transaction Input from which the payment is drawn                                                                   |
| \\( C_{Bc} = (v_{Bp} - v_p - \text{fees}) \cdot H + k_{Bc} \cdot G \\)   | Buyer's change Transaction Output accounting for the amount to pay to seller and the fees                                            |
| \\( C_{Bt} = 0 \cdot H + k_{Bt} \cdot G \\)                              | Buyer's token Transaction Output that he will own if the transaction is completed                                                    |
| \\( x_B = k_{Bt} + k_{Bc} - k_{Bp} \\)                                   | Buyers's private Excess                                                                                                              |
| \\( X_B = x_B \cdot G \\)                                                | Buyers's public Excess                                                                                                               |
| \\( R_B = r_B \cdot G \\)                                                | Choose an excess signature nonce                                                                                                     |
| \\( s_{B} = r_B + e_k x_B \\)                                            | A partial excess signature signing the message \\( e_k = (R_S + R_B \Vert X_S + X_B \Vert \text{metadata} \Vert \text{fees} ) \\)    |
| \\( R_{BMSt} = r_{BMSt_a} \cdot H + r_{BMSt_b} \cdot G \\)               | Choose a Buyer commitment signature nonce for the Buyer's token output                                                               |
| \\( e_{Bt} = \hash{ (R_{SMSt} + R_{BMSt}) \Vert \alpha_{Bt} \Vert F_{Bt} \Vert K_{SO} \Vert C_{Bt}} \\) | Buyer's token output metadata signature challenge where \\( \alpha_{Bt} \\) is the Buyer's token UTXO script and \\( F_{Bt} \\) is the Buyer's token UTXO output features |
| \\( s_{BMSt} = (a_{BMSt}, b_{BMSt}, R_{BMSt} ) \\)                       | A partial metadata commitment signature for \\( C_{Bt} \\) signing message \\( e_{Bt} \\)  |  
| \\( R_{BMSp} = r_{BMSp_a} \cdot H + r_{BMSp_b} \cdot G \\)               | Buyer's public metadata signature nonce for use by the Seller to calculate their payment output [metadata signature]                 |
| \\( K_{BO} = k_{BO} \cdot G \\)                                          | Choose a Buyer Offset to be used in the Sellers's received payment output [metadata signature]                                       |

The Buyer will then return the following to the Seller:

| Items                          | Description                                                                                         |
|--------------------------------|--------------------------------------------------------------------------------                     |
| \\( C_{Bp} \\)                 | Buyer's Transaction Input that the payment will come from                                           |
| \\( C_{Bc} \\)                 | Buyer's change Transaction Output                                                                   |
| \\( C_{Bt} \\)                 | Buyer's token Transaction Input that will be received if the sale is completed                      |
| \\( R_B \\)                    | Buyers's public Excess                                                                              |
| \\( \text{fees & metadata} \\) | The fees and Mimblewimble transaction metadata                                                      |
| \\( s_{B} \\)                  | The Buyer's partial excess signature                                                                |
| \\( R_{SMSt}\\)                | Buyer's public metadata signature nonce for use in the Seller's payment output [metadata signature] |
| \\( K_{BO}\\)                  | Buyer Offset to be used in the Sellers's received payment output [metadata signature]               |
| \\( s_{BMSt} \\)               | A partial metadata commitment signature for \\( C_{Bt} \\)                                          |

#### Round 3

The Seller can calculate the Buyer's excess as follows:

\\( X_B = C_{Bt} + C_{Bc} - C_{Bp} + (v_p + \text{fees}) \cdot H  \\)

The Seller will now calculate/choose the following:

| Terms                                                                 | Description                                                                                                                 |
|-----------------------------------------------------------            |-----------------------------------------------------------------------------------------------------------                  |
| \\( s_{S} = r_S + e x_S \\)                                           | Sellers's partial excess signature                                                                                          |
| \\( s = s_S + s_B, R = R_S + R_B \\)                                  | Seller's aggregates the excess signatures to produce the final excess signature                                             |
| \\( b_{SMSt} = r_{SMSt_b} + e_{Bt}(k_SO) \\)                          | Seller's partial metadata signature for the Buyer's token output                                                           |
| \\( s_{MSt} = (a_{BMSt}, b_{SMSt} + b_{BMSt}, R_{SMSt} + R_{BMSt} \\) | Aggregated [metadata signature] for Buyer's token Transaction Output                                                        |
| \\( R_{SMS} = r_{SMSp_a} \cdot H + r_{SMSp_b} \cdot G \\)             | Choose a Seller commitment signature nonce for the Seller's payment output                                                  |
| \\( e_{Sp} = \hash{ (R_{SMSpt} + R_{BMSp}) \Vert \alpha_{Sp} \Vert F_{Sp} \Vert K_{BO} \Vert C_{Sp}} \\) |where \\( \alpha_{Sp} \\) is the Sellers's payment UTXO script and \\( F_{Sp} \\) is the Seller's payment UTXO output features |
| \\( s_{SMSp} = (a_{SMSp}, b_{SMSp}, R_{SMSp} ) \\)                    | A partial metadata commitment signature for \\( C_{Sp} \\) signing message \\( e_{Sp} \\)                                   |  
| \\( \gamma_S = k_Sst - k_SO \\)                                       | The Seller's portion of the Script Offset constructed using the script key from \\(C_St\\), \\( k_Sst \\), and the private Seller Offset    |

The Seller can almost fully complete the transaction construction now. However, while the Seller can complete their portion 
of the final [script offset] they cannot complete the entire offset because the Buyer also has input's in the transaction. 
This means we need one extra round of communication where the Seller returns the almost complete transaction to the Buyer
who will complete there portion of the final [script offset] to complete the transaction.

The Seller sends the following back to the Buyer:

| Items                          | Description                                                                       |
|--------------------------------|--------------------------------------------------------------------------------   |
| \\( C_{St} \\)                 | Seller's token Transaction Input                                                  |
| \\( C_{Sp} \\)                 | Seller's payment Transaction Output                                               |
| \\( C_{Bp} \\)                 | Buyer's Transaction Input that the payment will come from                         |
| \\( C_{Bc} \\)                 | Buyer's change Transaction Output                                                 |
| \\( C_{Bt} \\)                 | Buyer's token Transaction Output that will be received if the sale is completed   |
| \\( X_S + X_B \\)              | Public Excess                                                                     |
| \\( s \\)                      | Aggregated Excess Signature                                                       |
| \\( s_{MSt} \\)                | Aggregated [metadata signature] for Buyer's token Transaction Output              |
| \\( s_{SMSp} \\)               | Partial metadata commitment signature for \\( C_{Sp} \\)                          |
| \\( \gamma_S \\)               | The Seller's portion of the Script Offset                                         |

### Round 4

The final step will be for the Buyer to complete their portion of the [metadata signature] for the Seller's payment Transaction
Output and complete their portion of the Script Offset.

| Terms                                                                 | Description                                                                                                                                |
|-----------------------------------------------------------            |-----------------------------------------------------------------------------------------------------------                                 |
| \\( b_{BMSp} = r_{BMSp_b} + e_{Sp}(k_BO) \\)                          | Buyers's partial metadata signature for the Seller's payment output                                                                        |
| \\( s_{MSp} = (a_{SMSp}, b_{SMSp} + b_{BMSp}, R_{SMSp} + R_{BMSp} \\) | Aggregated [metadata signature] for Seller's payment Transaction Output                                                                    |
| \\( \gamma_B = k_Bsp - k_BO \\)                                       | The Seller's portion of the Script Offset constructed using the script key from \\(C_Bp\\), \\( k_Bsp \\), and the private Buyer Offset    |
| \\( \gamma = \gamma_B + \gamma_S \\)                                  | The final [script offset] for the completed transaction.                                                                                   |

The Buyer's change Transaction Output will be constructed fully by them including the [metadata signature] which will be included in the final \\( \gamma \\) value.

### Validation
A [Base Node] will validate this transaction in the exact same way as described in [RFC-0201: TariScript](RFC-0201_TariScript.md).

In addition to the standard Mimblewimble and TariScript validation operations the consensus rules required for assets and
tokens must also be applied. The most important is to confirm that the `unique_id` of the token being sold in this transaction
exists only once as an input and once as an output and the metadata is correctly transfered.

### Security concerns
For this negotiation to be secure it must not be possible for the following to occur:
1. For the Buyer to use the information provided by the Seller during the first round to construct a valid transaction 
spending the Seller's token UTXO without the requested payment being fulfilled.
2. For the Seller to take the information provided by the Buyer in the second round and construct a valid transaction 
where the ownership of the token is not transferred even if the Mimblewimble arithmetic balances out.

These points should both be taken care of by the aggregated excess signature and the respective output [metadata signatures]. 
These signatures prevent the counterparty from changing anything about the Transaction Outputs without the cooperation of the
other party.

[Mimblewimble]: Glossary.md#mimblewimble
[Transaction]: Glossary.md#transaction
[UTXO]: Glossary.md#unspent-transaction-outputs
[Non Fungible Token]: Glossary.md#nft
[Commitment Signature]: https://eprint.iacr.org/2020/061.pdf
[Base Node]: Glossary.md#base-node
[metadata signature]: Glossary.md#metadata-signature
[script offset]: Glossary.md#script-offset
