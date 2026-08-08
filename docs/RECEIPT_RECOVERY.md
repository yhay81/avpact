# Receipt recovery after publication

AVPact verifies an output before publishing it. Receipt persistence happens
after that no-clobber publication. A storage failure at this final step is
therefore a reconciliation event, not permission to repeat the media
operation.

## Structured recovery signal

When the requested receipt cannot be written, AVPact keeps the verified output
and attempts to persist the same receipt beside it as:

```text
.avpact-recovery-<receipt-id>.json
```

New recovery receipts use the content-addressed
`avpact.receipt/v0.2` format and retain the same identifier as the receipt that
could not be written at the requested path. See
[receipt integrity](RECEIPT_INTEGRITY.md).

The fallback uses create-new semantics and never overwrites an existing path.
The CLI exits unsuccessfully with either `receipt_recovery_required` or
`receipt_recovery_failed`. Its versioned error JSON contains a `recovery`
object with:

- `action: "do_not_retry_apply"`;
- the retained output path and SHA-256;
- the originally requested receipt path;
- the recovery receipt path;
- whether that recovery receipt was successfully persisted.

AVPact does not delete the published output or a partially written receipt.
Deleting after a separate identity check would allow another process to
replace the path between the check and deletion.

## Reconciliation procedure

1. Do not rerun `avpact apply`. The transformation has already completed.
2. Compare the retained output's SHA-256 with
   `error.recovery.output_sha256`.
3. Run `avpact verify <output> --against <plan> --format json`.
4. If `recovery_receipt_persisted` is `true`, read the recovery JSON and
   confirm its `publication.output` and
   `verification.output.source.sha256` match the error.
5. Repair the requested receipt destination. Copy the recovery receipt to a
   new path with no-clobber semantics, or retain the recovery path as the
   durable receipt. Never overwrite an existing receipt.
6. Preserve the error JSON and any partial requested receipt for diagnosis.

If recovery persistence also failed, reconstructing a receipt is deliberately
unsupported. Preserve the output, plan, structured error, and storage state,
then investigate capacity, permissions, path length, and filesystem errors.
