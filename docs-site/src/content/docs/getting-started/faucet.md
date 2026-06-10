---
title: "Get Testnet MRSN"
---

Use the Mersennet faucet to receive testnet MRSN for development and testing.

## Web Interface

1. Open the faucet: **https://faucet.mersennet.com**
2. Enter your wallet address (the one you'll use on Mersennet).
3. Click the request button to receive testnet MRSN.

:::tip
Ensure your wallet is connected to Mersennet (Chain ID 131071) before requesting. The faucet sends MRSN to the address you provide.
:::

## Programmatic Access

You can request tokens via HTTP for scripts or CI/CD:

```bash
curl -X POST https://faucet.mersennet.com/faucet \
  -H "Content-Type: application/json" \
  -d '{"address": "0xYourWalletAddress"}'
```

Replace `0xYourWalletAddress` with your Ethereum-style address (e.g., `0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb`).

### Example Response

```json
{
  "success": true,
  "tx_hash": "0x..."
}
```

---

## Rate Limits

The faucet enforces rate limits to prevent abuse:

- **Per address** — Limited requests per address per time window.
- **Per IP** — Additional limits may apply for high-volume requests.

If your request is rate-limited, wait a few minutes before trying again. For automated testing, consider using multiple test addresses or caching faucet responses.

---

## Troubleshooting

| Issue | Solution |
|-------|----------|
| Request fails | Check that your address is a valid 0x-prefixed Ethereum address (40 hex chars). |
| No MRSN received | Verify the transaction on the [block explorer](https://explorer.mersennet.com). Confirm you're on Chain ID 131071. |
| Rate limited | Wait before retrying. Use a different address if needed for testing. |
