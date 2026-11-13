# Tool argument preflight

The registry checks advertised required fields before running policy hooks,
acquiring network permits, or executing tools. It reports all missing fields at
the first failing object, including paths into nested objects and array items.
`allOf` required contracts are checked together. Rejections enter the existing
malformed-call repeat guard; corrected arguments have their own retry history.
Error messages contain schema field names and paths, without argument values.

This is a bounded first step toward the performance report's schema middleware,
not a complete JSON Schema implementation. Scalar types, enum values, ranges,
references, conditional schemas, and alternative branches remain the tool's
responsibility. Recursion stops at 64 levels. Required fields inside optional
objects are checked only when those objects are supplied.

Native tools that already recover aliases or truncated payloads retain their
own argument validation. The compatibility list lives in
`src/alphacode_app_core/tool/schema_preflight.rs`. Removing an exemption requires
sharing that tool's canonicalization with the preflight rather than rejecting
previously accepted calls. External tools do not inherit exemptions from similar
names.

Regression coverage includes nested batch requirements, missing-field feedback,
secret-value omission, optional objects, numeric-string compatibility, external
tool isolation, and registry rejection before a permissive fixture can execute.

The report's persistent hypothesis ledger, coverage dashboard, browser escalation,
and engagement checklist are separate workstreams. This change makes no claim
that those features or the report's success metrics are complete.
