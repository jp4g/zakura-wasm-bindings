# Private native signer ownership

The worker-local signer table owns the upstream derived USK independently of its wallet database. Mnemonic create/import uses the existing normalization and native account transaction; it reserves a non-recycling token slot before the transaction and publishes authority only after commit. Legacy internal import callers retain their account-record result and drop the returned USK.

A native instance admits at most 1024 signer tokens over its lifetime and 1024 live wallet-account bindings per signer. Tokens never identify another instance. The enclosing verified worker owns instance checks and lifetime leases. Closing a database detaches its bindings; explicit signer release drops its authority and all bindings. Ordinary upstream USK drop is not a RAM-erasure guarantee. No spending-key export or persistence exists.

Attachment checks exact network registration and native `UnifiedFullViewingKey::subsumes_ufvk`, including outgoing authority and unknown components. A signer may therefore bind an account restricted to a matching subset of its components. View-only tracking remains recovery-required. Account identifiers alone never establish correspondence.

This private slice is not a completed MemorySigner or fused execution path. Same-owner multiwallet/VFS routing and SDK owner/session lifetime composition are required before public integration; no secret migration or foreign-memory pointer is permitted.
