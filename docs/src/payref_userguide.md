# Payment Reference (PayRef) User Guide

## What is a Payment Reference (PayRef)?

A Payment Reference (PayRef) is a globally unique identifier for individual transaction outputs on the Tari blockchain. It serves as proof that a specific payment was made and received, similar to a transaction ID in traditional payment systems but with enhanced privacy features.

## Key Features

- **Globally Unique**: Each PayRef is unique across the entire blockchain history
- **Privacy Preserving**: Reveals only that an output exists in a specific block, not amounts or transaction relationships  
- **Verifiable**: Can be independently computed and verified by any party
- **Stable**: Becomes permanently stable after sufficient confirmations (default: 5 blocks)

## How PayRefs Work

PayRefs are generated using the formula:
```
PayRef = Blake2b_256(block_hash || output_hash)
```

This approach ensures:
- **Global uniqueness** - Block hashes are unique across blockchain history
- **Verifiability** - Any party can compute and verify PayRefs with blockchain data
- **Privacy preservation** - No additional information leakage beyond existing blockchain data
- **Stability** - Becomes permanent after sufficient confirmations

## Using PayRefs in Tari Console Wallet

### Viewing PayRefs in Transaction History

1. Open Tari Console Wallet
2. Navigate to the Transactions tab (press `t`)
3. Select any confirmed transaction using Up/Down arrows
4. In the transaction details panel, you'll see:
   - **PayRef**: The 64-character hexadecimal identifier (when available)
   - **Status**: Current confirmation status

#### PayRef Status Indicators

- **Available (X confirmations)** - Green text, PayRef is ready to use
- **Pending X/5 confirmations (Y blocks remaining)** - Yellow text, waiting for more confirmations
- **Not mined** - Gray text, transaction hasn't been included in a block yet

### Searching for Transactions by PayRef

1. In the Transactions tab, press `s` to activate PayRef search
2. Enter the PayRef (partial matches supported)
   - You can enter the full 64-character hex string
   - Or just part of it to search
   - Spaces are ignored
3. Press Enter to search
4. If found, the matching transaction will be selected and displayed
5. Press Escape to cancel the search

### Copying PayRefs for Support

When contacting exchange support or sharing payment proof:

1. Navigate to the specific transaction
2. The PayRef will be displayed in the transaction details
3. Copy the 64-character hex string
4. Share this with the recipient for verification

**Example PayRef**: `a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456`

## Using PayRefs for Payment Verification

### For Users (Sending Payments)

When making a payment to an exchange or business:

1. Send your payment as normal
2. Wait for at least 5 confirmations (appears as "Available" status)
3. Copy the PayRef from your transaction history
4. Provide the PayRef to the recipient for verification
5. The recipient can use the PayRef to confirm they received your specific payment

### For Businesses (Receiving Payments)

When a customer claims they sent a payment:

1. Ask the customer for their PayRef
2. Use your wallet's gRPC API or search functionality to verify the PayRef
3. If the PayRef is found in your received transactions, the payment is confirmed
4. Credit the customer's account accordingly

## PayRef Configuration

### Default Settings

- **Required confirmations**: 5 blocks
- **Display format**: Shortened (8...8 characters)
- **Auto-refresh**: Every 30 seconds

### Customization Options

PayRef behavior can be configured through the wallet's configuration system:

```toml
[wallet.payment_reference]
required_confirmations = 5        # Number of confirmations needed
display_format = "shortened"      # "full", "shortened", or "custom"
auto_copy_on_click = true        # Auto-copy when clicked (future feature)
show_pending_progress = true     # Show confirmation progress
refresh_interval_seconds = 30    # How often to update status
```

## API Integration

### Console Wallet gRPC

For programmatic access to PayRef functionality:

```bash
# Get all available payment references
grpcurl -plaintext localhost:18143 tari.rpc.Wallet/GetAllPaymentReferences

# Get payment details by PayRef
grpcurl -plaintext -d '{"payment_reference_hex": "a1b2c3..."}' \
  localhost:18143 tari.rpc.Wallet/GetPaymentByReference

# Get unspent payment references only
grpcurl -plaintext localhost:18143 tari.rpc.Wallet/GetUnspentPaymentReferences
```

### Base Node gRPC (Public Verification)

For public verification without wallet access:

```bash
# Search for multiple PayRefs
grpcurl -plaintext -d '{"payment_reference_hex": ["a1b2c3...", "def456..."]}' \
  localhost:18142 tari.rpc.BaseNode/SearchPaymentReferences
```

## Best Practices

### For Users

1. **Wait for confirmations**: Don't share PayRefs until they show "Available" status
2. **Keep records**: Save PayRefs for important payments as proof
3. **Share securely**: PayRefs can be shared publicly without privacy concerns
4. **Verify format**: Ensure PayRefs are exactly 64 hexadecimal characters

### For Businesses

1. **Implement verification**: Set up automated PayRef verification in your systems
2. **Require sufficient confirmations**: Use at least 10 confirmations for large amounts
3. **Log verification attempts**: Keep records of PayRef verifications for audit trails
4. **Train support staff**: Ensure customer support knows how to verify PayRefs

### For Exchanges

1. **Higher confirmation requirements**: Use 10+ confirmations for deposits
2. **Automated processing**: Implement PayRef verification in deposit workflows  
3. **Customer education**: Provide clear instructions on finding PayRefs
4. **Integration testing**: Test PayRef verification with your wallet setup

## Troubleshooting

### PayRef Not Showing

**Problem**: Transaction shows "Not mined" or no PayRef
**Solution**: 
- Wait for the transaction to be included in a block
- Check that your wallet is synchronized
- Verify the transaction was actually broadcast

### PayRef Shows "Pending"

**Problem**: PayRef status shows "Pending X/5 confirmations"
**Solution**: 
- Wait for more blocks to be mined
- Each block adds one confirmation
- PayRef becomes available after 5 confirmations

### PayRef Search Not Working

**Problem**: Search doesn't find a known PayRef
**Solution**:
- Ensure you're searching in completed transactions (not pending)
- Check that the PayRef is correctly copied (64 hex characters)
- Verify the transaction has sufficient confirmations

### Verification Fails

**Problem**: Business cannot verify a provided PayRef
**Solution**:
- Confirm the PayRef format (64 hex characters)
- Check that the business's wallet has received the transaction
- Verify sufficient confirmations have passed
- Ensure wallet is synchronized with the blockchain

## Security Considerations

### What PayRefs Reveal

PayRefs are designed to be privacy-preserving:
- ✅ Proves an output exists in a specific block
- ✅ Can be safely shared publicly
- ✅ Enables payment verification

### What PayRefs Do NOT Reveal

- ❌ Payment amounts
- ❌ Sender or receiver identities
- ❌ Links to other transactions
- ❌ Account balances

### Comparison with Other Cryptocurrencies

- **Bitcoin**: Transaction IDs link all inputs/outputs together
- **Monero**: No public transaction verification possible
- **Tari**: PayRefs identify individual outputs only, preserving privacy

## Technical Details

### PayRef Generation Algorithm

```
Input: block_hash (32 bytes), output_hash (32 bytes)
Output: payment_reference (32 bytes)

1. Initialize Blake2b hasher with domain separation
2. hasher.update(block_hash)
3. hasher.update(commitment)  
4. payment_reference = hasher.finalize()
```

### Confirmation Requirements

PayRefs become available after a configurable number of confirmations to prevent issues with blockchain reorganizations:

- **1-4 confirmations**: Status shows "Pending X/5 confirmations"
- **5+ confirmations**: Status shows "Available" with full PayRef
- **Recommended for exchanges**: 10+ confirmations for large amounts

### Storage Requirements

PayRefs require no additional blockchain storage:
- Computed from existing block hash and commitment data
- Generated on-demand by wallets
- Optional indexing by base nodes for public verification

## Advanced Features

### Custom Confirmation Requirements

Different applications may require different confirmation thresholds:

```toml
# Conservative setting for high-value transactions
required_confirmations = 10

# Fast setting for small payments (higher risk)
required_confirmations = 3
```

### Batch Verification

For businesses processing many payments, batch verification APIs are available:

```bash
# Verify multiple PayRefs at once
grpcurl -plaintext -d '{"payment_reference_hex": ["payref1", "payref2", "payref3"]}' \
  localhost:18142 tari.rpc.BaseNode/SearchPaymentReferences
```

### Integration with Business Systems

PayRef verification can be integrated into existing business workflows:

1. **Customer deposits**: Auto-verify PayRefs provided by customers
2. **Audit trails**: Log all PayRef verifications with timestamps
3. **Reconciliation**: Match PayRefs against internal transaction records
4. **Dispute resolution**: Use PayRefs as proof in payment disputes

## Future Enhancements

### Planned Features

- **One-click copying**: Click PayRef to copy to clipboard
- **QR code generation**: Generate QR codes for easy PayRef sharing
- **Mobile wallet support**: PayRef functionality in mobile apps
- **Block explorer integration**: Search PayRefs on public block explorers

### Community Integration

- **Exchange partnerships**: Work with exchanges to support PayRef verification
- **Merchant tools**: Develop plugins for e-commerce platforms
- **Documentation**: Expand integration guides for different use cases

## Getting Help

### Resources

- **Technical Documentation**: [Tari Developer Docs](https://docs.tari.com)
- **Community Support**: [Tari Discord](https://discord.gg/tari)
- **GitHub Issues**: [Report bugs or request features](https://github.com/tari-project/tari/issues)

### Common Questions

**Q: Can I use PayRefs for privacy coins?**
A: PayRefs are designed specifically for Tari's Mimblewimble implementation. Other privacy coins use different approaches.

**Q: Are PayRefs compatible with hardware wallets?**
A: PayRef generation works with any wallet that stores transaction data, including hardware wallets.

**Q: Can PayRefs be used for audit purposes?**
A: Yes, PayRefs provide an excellent audit trail for payment verification without revealing sensitive transaction details.

**Q: What happens if I lose my PayRef?**
A: PayRefs can be regenerated from blockchain data as long as you have access to your wallet and the transaction has been mined.

---

*This guide covers PayRef functionality as implemented in Tari v4.1.0 and later. For the most up-to-date information, please refer to the latest Tari documentation.*
