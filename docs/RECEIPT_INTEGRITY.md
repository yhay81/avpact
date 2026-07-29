# Receipt integrity

AVPact receipts record what plan ran, which backend ran it, what was measured,
and how the verified output was published. The current
`avpact.receipt/v0.2` contract makes that complete evidence content-addressed.

## v0.2 identifier

A v0.2 identifier has the form `rcpt_<64 lowercase hexadecimal characters>`.
AVPact computes the SHA-256 digest over the compact JSON serialization of this
ordered identity material:

1. `schema_version`;
2. `plan_id` and `plan_digest`;
3. `started_unix_ms`, `completed_unix_ms`, and `elapsed_ms`;
4. the complete backend identity and argument vector;
5. every warning;
6. the complete verification report, including output properties and checks;
7. the complete publication evidence.

The `id` field itself is excluded. Struct field order and map ordering are
fixed by the typed serializer and pinned by the versioned golden corpus.

`read_receipt` recomputes the identifier and rejects a mismatch.
`receipt show <receipt-id>` additionally requires the requested store key to
equal the embedded identifier, so moving a different valid receipt under an
existing key also fails closed.

## v0.1 compatibility

AVPact continues to read and exactly round-trip
`avpact.receipt/v0.1` documents. Their 32-hex identifier binds only the plan
identifier, verified output SHA-256, and completion time. It does not bind all
other evidence fields. Existing v0.1 evidence cannot be strengthened
retroactively without changing what was originally recorded, so no automatic
migration is provided.

Treat v0.1 receipts as legacy evidence. New `apply` operations emit v0.2.

## Trust boundary

The receipt identifier detects corruption or mutation when the expected
identifier is retained independently, as it is for a `receipt show` lookup. It
is an unkeyed digest, not a signature, and does not prove who created the
receipt. An actor able to replace both a receipt and every trusted copy of its
identifier can compute a new internally consistent identifier.

For evidence crossing a hostile storage or organizational boundary, preserve
the expected receipt identifier in an independently protected log or sign the
receipt with the organization's established signing system. To re-check the
current output itself, run:

```text
avpact verify <output> --against <plan> --format json
```
