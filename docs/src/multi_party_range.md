# Multi-party range proving

## Introduction

This technical note describes a method for producing a [Bulletproofs+](https://eprint.iacr.org/2020/735) range proof such that multiple players (who do not trust each other) wish to produce an aggregated range proof on commitments with values known to all players, but with mask components that are secret to each player.

Even in the case of a non-aggregated range proof, this method is not compatible with mask extraction.

We use notation from the original Bulletproofs+ preprint as needed, but modify in a straightforward way to support additive notation.

This construction has not undergone any formal review and may contain errors that make it unsuitable for use in production.


## Method

Suppose we have $p$ players (indexed $1 \leq i \leq p$) who, for $1 \leq k \leq m$ (such that $m$ is a power of two), each hold a share $\gamma\_{i,k}$ of a mask for a commitment
\\[ V\_k = v\_k G + \sum\_{i=1}^p \gamma\_{i,k} H \\]
to a value $v\_k$ known by all players.
The players are not assumed to trust each other.
They wish to collaboratively produce an aggregated Bulletproofs+ range proof on this set of commitments such that no player $i$ learns the mask share $\gamma\_{j,k}$ of another player $j \neq i$ for any $k$.

Each player $1 \leq i \leq p$ does the following:
- Sets $V\_{i,k} = \gamma\_{i,k} H$ for $1 \leq k \leq m$.
- Chooses a scalar $\alpha\_i$ uniformly at random, and sets $\Gamma\_i = \alpha\_i H$.
- For $1 \leq k \leq m$, generates a zero-knowledge proof of knowledge $\Sigma^V\_{i,k}$ of the representation of $V\_{i,k}$ with respect to $H$.
- Generates a zero-knowledge proof of knowledge $\Sigma^\Gamma\_i$ of the representation of $\Gamma\_i$ with respect to $H$.
- Sends the tuple
\\[ \left( \\{V\_{i,k}\\}\_{k=1}^m, \Gamma\_i, \\{\Sigma^V\_{i,k}\\}\_{k=1}^m, \Sigma^\Gamma\_i \right) \\]
to all other players.
- (Abort point 1) On receipt of a tuple
\\[ \left( \\{V\_{j,k}\\}\_{k=1}^m, \Gamma\_j, \\{\Sigma^V\_{j,k}\\}\_{k=1}^m, \Sigma^\Gamma\_j \right) \\]
from another player $j$, verifies each $\Sigma^V\_{j,k}$ and $\Sigma^\Gamma\_j$, and aborts if any verification fails.
- Sets
\\[ V\_k = v\_k G + \sum\_{j=1}^p V\_{j,k} \\]
for $1 \leq k \leq m$ and
\\[ A = \vec{a}\_L \vec{G} + \vec{a}\_R \vec{H} + \sum\_{j=1}^p \Gamma\_j \\]
after receiving and verifying these values from all other players.
- Uses $\\{V\_k\\}\_{k=1}^m$ and $A$ to proceed through the Bulletproofs+ protocol as usual until reaching the definition of $\widehat{\alpha}$.
- Sets
\\[ \widehat{\alpha}\_i = \alpha\_i + y^{mn+1} \sum\_{k=1}^m z^{2k} \gamma\_{i,k} \\]
and sends this value to all other players.
- (Abort point 2) On receipt of a value $\widehat{\alpha}\_j$ from another player $j$, checks that
\\[ \widehat{\alpha}\_j H = \Gamma\_j + y^{mn+1} \sum\_{k=1}^m z^{2k} V\_{j,k} \\]
and aborts otherwise.
- Sets
\\[ \widehat{\alpha} = \sum\_{j=1}^p \widehat{\alpha}\_i \\]
after receiving and verifying these values from all other players.
- Completes the Bulletproofs+ protocol as usual; see remarks on this below.


## Remarks

### Identifiable abort points

The steps described above introduce abort points to the protocol, with the intent that if the abort points do not trigger, the resulting range proof is valid.
Further, it is intended that if an abort point is triggered, each malicious player is identified and can be excluded from future operations.
This property ensures that denial of service by malicious players is mitigated.

_Informal proof._
Suppose that an honest player completes the checks at abort points 1 and 2 for all other players successfully.
Successful verification of $\Sigma^V\_{i,k}$ implies knowledge of $\gamma\_{i,k}$ such that $V\_{i,k} = \gamma\_{i,k} H$, and verification of $\Sigma^\Gamma\_i$ implies knowledge of $\alpha\_i$ such that $\Gamma\_i = \alpha\_i H$.

This means the verifying player computes
\begin{alignat*}{1}
V\_k &= v\_k G + \sum\_{i=1}^p V\_{i,k} \\
&= v\_k G + \sum\_{i=1}^p \gamma\_{i,k} H
\end{alignat*}
and
\begin{alignat*}{1}
A &= \vec{a}\_L \vec{G} + \vec{a}\_R \vec{H} + \sum\_{i=1}^p \Gamma\_i \\
&= \vec{a}\_L \vec{G} + \vec{a}\_R \vec{H} + \sum\_{i=1}^p \alpha\_i H
\end{alignat*}
locally.
Further, since the check
\begin{alignat*}{1}
\widehat{\alpha}\_i H &= \Gamma\_i + y^{mn+1} \sum\_{k=1}^m z^{2k} V\_{i,k} \\
&= \alpha\_i H + y^{mn+1} \sum\_{k=1}^m z^{2k} \gamma\_{i,k} H
\end{alignat*}
passes, it follows with high probability that
\\[ \alpha\_i = \alpha\_i + y^{mn+1} \sum\_{k=1}^m z^{2k} \gamma\_{i,k} \\]
and therefore that
\begin{alignat*}{1}
\widehat{\alpha} &= \sum\_{i=1}^p \widehat{\alpha}\_i \\
&= \sum\_{i=1}^p \alpha\_i + y^{mn+1} \sum\_{k=1}^m z^{2k} \sum\_{i=1}^p \gamma\_{i,k}
\end{alignat*}
as expected, implying the honest player can complete a valid proof. ∎

We note that in the event of an abort, it is important that each player in subsequent proof generation start the entire process over, especially being sure not to reuse previous nonces or other proof values.


### Random nonces and weighted inner product proofi

When completing the range proof, any player can produce the weighted inner product sub-proof required by the Bulletproofs+ protocol.
This sub-proof requires the use of several random nonces that do not need to be separated into secret player shares, so players who produce this proof independently will not produce the same proof.
There are effectively three options for how to proceed:
- Any designated player produces this sub-proof and uses it as needed in the resulting transaction.
In this case, the transaction is not valid unless the sub-proof is valid.
However, there is no guarantee that the player selected the nonces honestly.
- Each player commits to shares (sampled independently and uniformly and random) of all sub-proof nonces during the first round of communication, opens these commitments during the second round of communication, computes the sub-proof nonces as sums of these shares, and verifies that all other players produce the same proof.
- Each player commits to a share (sampled uniformly at random) of a single secret during the first round of communication, opens this commitment during the second round of communication, computes the secret as sums of these shares, uses the secret with a suitable cryptographic hash function to derive all sub-proof nonces, and verifies that all other players produce the same proof.

Which approach to choose depends on the use case.
For example, if an overlying protocol allows any valid proof for a given input commitment set to be used, nothing stops a malicious player from producing a weighted inner product sub-proof using non-random nonces and providing it to the protocol.
If the overlying protocol requires players to agree on a proof using additional cryptographic approaches like signatures, then it is possible to use one of the latter two approaches where each player contributes to nonce generation and can verify the final proof before signing to indicate its approval.
