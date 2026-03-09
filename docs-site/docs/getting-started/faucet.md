---
slug: /getting-started/faucet
sidebar_position: 4
title: "Get Testnet PRIM"
---

# Get Testnet PRIM

Use the Prime Chain faucet to receive testnet PRIM for development and testing.

## Web Interface

1. Open the faucet: **http://46.225.30.187:8080**
2. Enter your wallet address (the one you'll use on Prime Chain).
3. Click the request button to receive testnet PRIM.

:::tip
Ensure your wallet is connected to Prime Chain (Chain ID 7919) before requesting. The faucet sends PRIM to the address you provide.
:::

## Programmatic Access

You can request tokens via HTTP for scripts or CI/CD:

```bash
curl -X POST http://46.225.30.187:8080/faucet \
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
| No PRIM received | Verify the transaction on the [block explorer](http://46.225.30.187). Confirm you're on Chain ID 7919. |
| Rate limited | Wait before retrying. Use a different address if needed for testing. |
