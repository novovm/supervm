# Source synchronization / full candidate CI split

User-directed scheduling correction; no product code, runtime package, node,
database, key, resource limit or timeout changed.

Ordinary verified main pushes select `sync`. Full candidate validation uses the
original explicit full entry; PR and scheduled scopes retain their rules.
The original native-auth host job is reused by full CI, not removed. The new
`Full candidate result` is separate from lightweight `CI scope result`.
See [routing contract](../../../docs/NOVOVM_CI_CHECK_TIERS.md).

## Local evidence

- Windows routing regressions: 27/27 passed, including invalid syntax/JSON,
  damaged ZIP CRC, missing history, failed selected jobs and full-scope coverage.
- Ubuntu 24.04 WSL routing regressions: 27/27 passed.
- actionlint 1.7.12: exit 0 for both changed workflows; shellcheck and pyflakes
  integrations disabled. This is YAML/Actions validation, not shell execution.
- Real previous push range `1ef31893..5c6a6b48` replay: `sync`, all heavy-job
  flags false; actual audit JSON and evidence ZIP checked.
- Original product job blocks and native-auth host job are byte-identical to
  `5c6a6b48db817f7ef2f9dd6c759698a7083423c6` (comparison script and output retained).
- Git diff whitespace check passed.

The first Linux launcher failed before Python because an expanded Windows PATH
was passed unquoted to Bash. The corrected explicit Linux PATH ran the original
tests; the initial launcher failure is retained, not counted as a test pass.

Raw local results: `original-evidence.zip`, SHA256
`a976a110f5ce3f0309f3023f582d638a277a610d87b5a77fb03a03e2162ab771`.
The archive contains logs and the replay/comparison inputs, not private material.

## Acceptance boundary

No new full CI was dispatched or existing run cancelled by this work. The
previous program run `42717131364` for `1ef31893` ended cancelled, not success;
it does not qualify that candidate. Local scheduling checks do not replace
product tests, full candidate qualification or real-network evidence.
The next ordinary push must demonstrate the lightweight route. Node deployment
and the unresolved public Carrier connection remain outside this work item.
