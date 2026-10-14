# Local simple-dns 0.12.0 patch

Source: crates.io simple-dns 0.12.0, MIT; upstream repository https://github.com/balliegojr/simple-dns.
Original crates.io checksum: 2b6f884fa9a8d48101774bfbd3aeb81e968dd22cffd19a372da69f183db22c1a. Runtime change is limited to OPT TTL bit positions. The README BADVERS wire example is corrected to match the standard.

RFC 6891 section 6.1.3 places extended RCODE in bits 31..24, version in 23..16,
and flags in 15..0. Upstream 0.12.0 used bits 7..0 for extended RCODE and 15..8
for version, causing a successful real DNS answer with reserved flag 0x0001 to
become BADVERS. This blocked Iroh relay selection on a fresh L3 start.

Correct both read and write positions. Do not ignore real extended errors, alter
DNS identity/question checks, bypass TLS, change trusted resolvers or replace
NOVONET identity. Wire tests cover flags, real BADVERS, version and captured
response shape. Local diagnostics have no user identity or message payload.
Reference: https://www.rfc-editor.org/rfc/rfc6891#section-6.1.3
