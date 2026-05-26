# Shielded SDK Examples

## `view-notes-end-to-end.js`

Runs a local mock JSON-RPC server that serves `prime_viewNotes`, then
uses `PrimeProvider` + `scanGrantedNotes` with a concrete decryptor to
recover a sample note.

Run from `sdk/` after `pnpm build`:

```bash
node ./examples/view-notes-end-to-end.js
```

The decryptor is intentionally example-scoped: it uses a deterministic
shared-secret + XOR scheme so the end-to-end scanner flow is runnable
today without waiting for a cross-language production viewing-key
decrypt primitive. The example now reuses the shared
`createMockNoteDecryptor(...)` helper exported by the SDK.