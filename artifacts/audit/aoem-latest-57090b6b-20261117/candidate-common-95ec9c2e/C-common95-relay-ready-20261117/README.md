# C original relay snapshot return

All three existing raw snapshots are returned unchanged; returned-file-hashes.json records their SHA-256 and those of the existing result/stdout/stderr. Each snapshot matches Circuit 457c502773ba39993a584d68cbbf1515 and Route digest 82ead291e70fd94915b90e0dc1591617826b32a64c1860e52b25ba7936521c14. C is relay/cursor1, progressing from 8/8 to 9/9 forwarded packets with zero failed writes. See circuit-correlation.json for exact snapshot timestamps and counters.

The relay forwards end-to-end ciphertext. Its null native_session and absent Object ID do not negate forwarding; the prior Object/Session-only search was insufficient to associate relay evidence. Correlation here uses the specified Circuit/Route, while A/D retain endpoint Object/Session and integrity/time evidence.

No new collection, observer, send or node restart was performed. Existing observer exit code remains unknown; original result records terminal_observed=true. The raw snapshots are preserved byte for byte.
