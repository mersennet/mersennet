# Security policy

Mersennet runs a public testnet with real users, so please treat findings as
you would on a production network.

**Report privately** to **security@mersennet.com** (or ask an admin in the
[Telegram group](https://t.me/Mersennet) for a private channel). Include steps
to reproduce, the affected component (node, precompile, terminal, API,
explorer, faucet, SDK) and, for chain issues, the block height. Every public
hostname publishes the same contact at `/.well-known/security.txt`.

- We acknowledge within **48 hours** and keep you informed until the fix ships.
- Credit in the changelog is yours if you want it.
- Please do not test against other users' funds or run denial-of-service
  traffic against the public infrastructure.
- Coordinated disclosure: we ask for up to 90 days before publication for
  consensus-affecting issues, less for everything else.

## Bug bounty (testnet)

Valid reports earn testnet points by severity: **Critical 50,000 · High 20,000 ·
Medium 5,000 · Low 1,000**. Scope, examples and rules:
[docs.mersennet.com/getting-started/points/#bug-bounty](https://docs.mersennet.com/getting-started/points/#bug-bounty).
The first report of an issue earns the points; demonstrate, do not exploit.
Points have no monetary value.

Supported: the current signed release published at
[mersennet.com/downloads](https://mersennet.com/downloads/). Older builds are
not patched; upgrading is the one-line installer.
