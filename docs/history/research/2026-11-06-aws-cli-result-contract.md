# AWS CLI result contract and S3 encryption semantics

Research date: **2026-11-06**. Target: **awslabs.aws-api-mcp-server 1.6.0**.

## Scope and evidence

- **Documented:** an official AWS reference or guide states the behavior.
- **Pinned-client:** behavior derived statically from the supplied installed-source
  excerpts, supplemented by version-matched official implementation. This is not
  a production AWS guarantee or a newly observed MCP response.
- **Inference:** an interpretation or proposed consumer/fixture rule.
- **Uncertain:** the examined evidence does not establish the stronger claim.

The authoritative local captures are [C-contract], [C-shapes], and [C-expansion].
Their displayed `call_aws`, result-model, customization/help, and expansion
definitions agree with the corresponding official release source. Release tag
`2026.10.20261027104512` resolves to commit
`1bf810fd5a0d9d22b3bfd87bde69ce19eb1a177b`; its project declares version
`1.6.0`. PyPI independently identifies that version. This is a version-matched
reference, **not** a byte-for-byte attestation of the entire installed image.
[RELEASE][VERSION][PYPI][SERVER][MODELS][SERVICE]

Only the unchanged scenario instruction and TOML were inspected for task scope.
They request a bucket identity, prohibit configuration changes, and explicitly
expect `northwind-legacy-exports` with HTTP 404 plus
`ServerSideEncryptionConfigurationNotFoundError`. The scenario clock is
**March 14, 2026**, not a pre-2023 date. No fixture implementation was inspected
for this note. Only this research file is written; no installs, Docker, tests,
cloud-account operations, benchmark/model runs, or Git mutations were performed.
[SC-INSTRUCTION][SC-TOML]

## 1. Result shapes: errors are not one uniform field

**Pinned-client:** `call_aws` accepts a string or list and returns a list even for
a scalar input. `CallAWSResponse` internally holds `cli_command`, a response-model
union, and optional `error`; its custom serializer flattens that response model
into the item. Do **not** add another `response` layer around the already
serialized ordinary result. The model validator requires at least one of
response/error, not an exclusive-or invariant. [C-contract][MODELS][SERVER]

The following paths describe that serializer's dictionaries, **not a captured
FastMCP transport envelope**:

- **Ordinary interpreted API result:** item `response` is an object with
  `error`, `status_code`, `error_code`, `pagination_token`, and `as_json`;
  `metadata`, `validation_failures`, `missing_context_failures`, and
  `failed_constraints` are item-level fields. These fields permit nulls.
  [C-shapes][MODELS]
- **AWS service `ClientError`:** the version-matched driver returns, rather than
  rethrows, the service failure. It independently extracts HTTP status from
  `ResponseMetadata.HTTPStatusCode`, code from `Error.Code`, and message via
  `str(error)`; the service layer puts them in item
  `response.status_code`, `response.error_code`, and `response.error`.
  Thus an AWS failure need not have item-level `error`. [DRIVER][SERVICE]
- **Wrapper/validation/execution exception:** the command wrapper catches
  `Exception` and produces item-level `error: str(exception)`, without a
  structured HTTP-status/code field of its own. Command validation, missing
  credentials, and policy denials can reach this path. Region-expansion
  exceptions similarly become one error item for the original command.
  Do not manufacture an HTTP status from an arbitrary exception string.
  [C-contract][SERVER]
- **Alias/customization:** the flattened item instead has `response` as captured
  stdout text and `error` as stderr text, potentially empty strings. Stderr alone
  with no stdout raises; stdout plus stderr is returned together. The displayed
  implementation does not inspect `driver.main`'s returned exit code or provide
  a structured API status/code. Therefore presence of the `error` key is not
  sufficient to classify this branch, and stdout is not proof of HTTP success.
  [C-expansion][C-shapes][SERVICE]
- **Help:** the helper returns the ordinary interpretation response shape with a
  locally generated help document and **synthetic status 200**. It does not set
  metadata. That 200 is not evidence that the requested AWS resource API was
  called successfully. Earlier validation/security checks still apply.
  [C-expansion][C-contract][SERVICE][MODELS]
- **Structured interpreter failures:** the service model also supports
  `response: null` with validation/missing-context/constraint details. The
  public helper normally rejects initial validation failures by exception;
  the existence of these model fields does not guarantee every validation
  failure uses this structured branch. [SERVICE][C-contract][C-shapes]

**Pinned-client plus serialization inference:** `InterpretationResponse` names
the Python field `as_json`, with input alias `json`. The custom serializer calls
`self.response.model_dump()` without `by_alias=True`; Pydantic's documented
default emits field names. Accordingly, the shown serializer yields `as_json`,
whose value is JSON **text**, not an already parsed API object. Do not infer a
wire key named `json` merely from the constructor argument. [MODELS][PYDANTIC]

**Uncertain:** these captures do not establish FastMCP `content` versus
`structuredContent`, any outer `result` envelope, `isError`, or subsequent
normalization performed by the benchmark consumer. Do not promote the Python
model shape to a fully verified end-to-end MCP wire contract.
[C-contract][C-shapes][VERSION]

## 2. Batch order and wildcard cardinality

**Pinned-client:** despite descriptive text about running commands “at the same
time,” the implementation loops over inputs and awaits each expanded command
before appending the next. Results are ordered first by input command, then by
that command's expansion order; ordinary caught command errors do not abort the
rest. Duplicate inputs are not deduplicated. This is an implementation fact for
this version, not an AWS service ordering guarantee. [C-contract][SERVER]

The maximum-batch check counts original inputs **before** expansion and raises
outside the per-command catch. Do not promise that every invocation failure is
represented by an error item, or that the input cap bounds expanded output size.
For a normal batch without wildcard expansion, the displayed loop preserves
one item per input; expansion can break that positional correspondence.
[C-contract][SERVER]

**Pinned-client:** expansion is literal regex substitution of
`--region\s+\*(?=\s|$)`. It is not shell globbing. `--region=*` and quoted
`--region "*"` do not match this pattern. An explicit space-separated `--profile`
is found by another regex; otherwise the configured MCP profile is used.
[C-expansion][SERVICE]

**Version-matched client:** `get_active_regions` uses a boto3 Account client and
the `list_regions` paginator, retaining only `ENABLED` and
`ENABLED_BY_DEFAULT` entries in the order returned by pages. It does not sort or
deduplicate them. Region discovery introduces additional AWS requests and
depends on `account:ListRegions`; an `AccessDenied` is converted to a region
resolution error. This is not EC2 `DescribeRegions` expansion.
[REGIONS][AWS-REGIONS]

**Implications/inference:** one wildcard input can produce many results, none
if discovery returns no retained regions, or one original-command error item if
discovery throws. Successful expansions identify the concrete substituted
`cli_command`. The source establishes discovery-order preservation, **not**
a fixed alphabetical or cross-run region order. Correlation must retain command
identity and duplicates rather than zip expanded results against original inputs
or assume every emitted item equals one underlying HTTP exchange.
[C-contract][C-expansion][REGIONS][AWS-REGIONS]

## 3. S3: configuration absence is not object-encryption absence

### The error is real; the fixture's HTTP status is not established as universal

**Historical documented API semantics:** the first-party botocore **1.29.0** S3
model explicitly says `GetBucketEncryption` returns
`ServerSideEncryptionConfigurationNotFoundError` when the bucket lacks a default
encryption configuration. Its modeled request is `GET /{Bucket}?encryption`.
That operation documentation does **not** establish an HTTP 404 mapping.
[S3-HISTORICAL]

**Current documented status:** AWS's S3 error table lists
`ServerSideEncryptionConfigurationNotFoundError` as **400 Bad Request**.
It separately lists `NoSuchBucket` as **404 Not Found**. A bare 404 therefore
does not identify missing encryption configuration, and the scenario's exact
404/code combination must not be advertised as the current universal AWS
contract. The table does not prove that every historical or special backend
always returns 400 either. [S3-ERRORS][SC-TOML]

### Automatic SSE-S3 changed the ordinary bucket state

**Documented:** starting **January 5, 2023**, S3 automatically encrypts new
uploads with SSE-S3 as its baseline; AWS says it also configures previously
unencrypted buckets. Users cannot disable encryption of new uploads. Existing
objects are not retroactively encrypted by this change. The current
`GetBucketEncryption` reference consequently describes encryption configured
by default on all buckets. [S3-FAQ][S3-GET]

**Documented:** deleting a general-purpose bucket's explicit default encryption
configuration resets its effective default to SSE-S3 rather than disabling
encryption. Thus “no customer-selected configuration” must not be conflated
with “no effective default encryption.” [S3-DELETE]

**Inference and limits:** configuration-not-found remains a legitimate
historical API/error concept and can be an explicitly modeled fixture state.
Within this scenario, the exact command/status/code tuple is a declared
expected negative response, not grounds to reject the entire seven-command
batch. **However, the evidence does not establish that a modern production
bucket which never received an explicit customer encryption setting will
return this error.** AWS's automatic configuration argues against that
assumption. Nor do these sources give `GetBucketEncryption` an indicator of
whether equivalent SSE-S3 settings were set by a customer or automatically.
Calling the fixture a distinction about explicit settings is defensible only
as a labeled fixture interpretation, not a verified contemporary API guarantee.
[S3-HISTORICAL][S3-GET][S3-FAQ][S3-DELETE][SC-TOML]

**No object-level conclusion follows:** even historically, `PutObject` could
request object encryption independently. A missing bucket default therefore
does not establish that any object is unencrypted. Conversely, a present
default does not establish that every older object is encrypted. AWS identifies
object-response encryption headers and S3 Inventory encryption metadata as
ways to inspect object encryption. This note does not perform those operations,
and they are not required by the unchanged bucket-configuration task.
[S3-HISTORICAL][S3-FAQ][SC-INSTRUCTION]

## 4. Actionable objections for the main task

1. **Do not equate successful tool transport with successful AWS commands.**
   Inspect each result branch; ordinary service errors are nested and wrapper
   errors need not have HTTP metadata. Alias stderr and help's synthetic 200
   require distinct handling. [C-contract][C-shapes][C-expansion][DRIVER]
2. **Keep any allowed API error narrow.** For this declared scenario allowance,
   match the actual command, structured 404 status, and exact service code
   together. Do not waive arbitrary 404s, wrapper exceptions, other commands,
   or the rest of the batch. This is a proposed consumer rule, not an assertion
   about unseen grader implementation. [SC-TOML][DRIVER]
3. **Do not impose one-result-per-input on wildcard batches.** Preserve emitted
   order and identity, account for discovery failures/empty expansion, and do
   not invent a stable global region order. [C-contract][C-expansion][REGIONS]
4. **Flag the scenario's realism limit, without silently changing its contract.**
   Its March 2026 wording “no default ... configured at all” and fixture 404
   differ from current effective-encryption documentation and the error table.
   Retaining a synthetic/legacy missing-configuration fixture is a benchmark
   choice requiring an explicit caveat, not proof of production behavior.
   [SC-INSTRUCTION][SC-TOML][S3-ERRORS][S3-GET][S3-FAQ]
5. **Do not report unencrypted objects from this evidence.** The task concerns
   a bucket configuration lookup only; neither the missing/default branch nor
   the similar bucket name ending `-2019` establishes object encryption state.
   [SC-INSTRUCTION][SC-TOML][S3-HISTORICAL][S3-FAQ]

## Implementation follow-up

The findings above describe the source inspected during this research, before the
main implementation's subsequent task revision. The current
`cloud/aws/find-unencrypted-bucket` source now asks about an SSE-KMS policy mismatch
and no longer uses a missing-encryption error. Historical instruction/data remain
in `/tmp/tool-bench-e26-source-freeze.J0ADoU`.
The exact negative-error conformance case is now the separate, unscored
`example/aws/missing-bucket-policy`, using 404 `NoSuchBucketPolicy`.
The historical null-encryption mock branch now uses the documented 400 status.
See [the implementation contract](../oracle-results.md) for current behavior and
explicit remaining limits. This follow-up does not alter the research's original
evidence labels or assert a live AWS validation.

## Primary sources

[C-contract]: /tmp/tool-bench-oracle-results-v1-client-contract.log
[C-shapes]: /tmp/tool-bench-oracle-results-v1-client-shapes.log
[C-expansion]: /tmp/tool-bench-oracle-results-v1-client-expansion.log
[SC-INSTRUCTION]: ../../scenarios/cloud/aws/find-unencrypted-bucket/instruction.md
[SC-TOML]: ../../scenarios/cloud/aws/find-unencrypted-bucket/scenario.toml
[RELEASE]: https://api.github.com/repos/awslabs/mcp/git/tags/998f6d717ad947eb9f3336f3f0d94661eb3346ad
[VERSION]: https://github.com/awslabs/mcp/blob/1bf810fd5a0d9d22b3bfd87bde69ce19eb1a177b/src/aws-api-mcp-server/pyproject.toml#L1
[PYPI]: https://pypi.org/pypi/awslabs.aws-api-mcp-server/1.6.0/json
[SERVER]: https://github.com/awslabs/mcp/blob/1bf810fd5a0d9d22b3bfd87bde69ce19eb1a177b/src/aws-api-mcp-server/awslabs/aws_api_mcp_server/server.py#L249
[MODELS]: https://github.com/awslabs/mcp/blob/1bf810fd5a0d9d22b3bfd87bde69ce19eb1a177b/src/aws-api-mcp-server/awslabs/aws_api_mcp_server/core/common/models.py#L85
[SERVICE]: https://github.com/awslabs/mcp/blob/1bf810fd5a0d9d22b3bfd87bde69ce19eb1a177b/src/aws-api-mcp-server/awslabs/aws_api_mcp_server/core/aws/service.py#L131
[DRIVER]: https://github.com/awslabs/mcp/blob/1bf810fd5a0d9d22b3bfd87bde69ce19eb1a177b/src/aws-api-mcp-server/awslabs/aws_api_mcp_server/core/aws/driver.py#L112
[REGIONS]: https://github.com/awslabs/mcp/blob/1bf810fd5a0d9d22b3bfd87bde69ce19eb1a177b/src/aws-api-mcp-server/awslabs/aws_api_mcp_server/core/aws/regions.py#L36
[AWS-REGIONS]: https://docs.aws.amazon.com/accounts/latest/APIReference/API_ListRegions.html
[PYDANTIC]: https://docs.pydantic.dev/2.10/concepts/serialization/#modelmodel_dump
[S3-HISTORICAL]: https://github.com/boto/botocore/blob/1.29.0/botocore/data/s3/2006-03-01/service-2.json
[S3-ERRORS]: https://docs.aws.amazon.com/AmazonS3/latest/developerguide/ErrorResponses.html
[S3-GET]: https://docs.aws.amazon.com/AmazonS3/latest/API/API_GetBucketEncryption.html
[S3-FAQ]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/default-encryption-faq.html
[S3-DELETE]: https://docs.aws.amazon.com/AmazonS3/latest/API/API_DeleteBucketEncryption.html
