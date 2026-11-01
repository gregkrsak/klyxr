# Sentinel 0 implementation ledger

Candidate under Issue #89. Implementation authorization is Greg's supplied
2026-10-21 directive; Sarah's independent implementation review and Greg's
acceptance are pending. No merge or ready transition is authorized.

Baseline: `4bd4f8588c0435a018050a64b3f154a076f075ee`, tree
`198cb0b5b52d806d1d8b6bf45987d9b92ebb12eb`. Reconciled against feasibility baseline
`0f3b81224bbd0aa823c60d960fc19c295d681b95`: PR #90 changed only documentation policy,
README and canonical/generated documentation; no compiler/runtime/CLI changes.

Branch: `sentinel-0-reference-application`; target `prod`; PR remains draft.
No local Rust was installed, repaired, formatted, compiled, linked or executed.
GitHub Actions is the sole executable evidence environment.

## Publication provenance

The local preparation checkpoint remains preserved and clean at commit
`72497d9bf0ee68ee9e8172933ad4d5575967e771`, tree
`dd338cfcfb4014134eff012f55317d952cc433ae`. Git HTTPS credentials and SSH transport
were unavailable. Greg explicitly authorized replacement commit identities through
the authenticated ChatGPT GitHub connector. The single initial published commit
`572cff0b4113a7a00c4f246ae023d04d1a8925fc` has that exact same tree, verified by
GitHub readback; no initial byte, path, mode or documentation substitution occurred.
It is a direct child of the stated baseline. [PR #91](https://github.com/gregkrsak/klyxr/pull/91)
is draft; subsequent repairs are normal descendants, without history rewriting.

The first Actions gate compiled but rejected Rust formatting and the private
combined-failure control's admission envelope. Its default million-step execution
ceiling exceeded the existing replay controller's conservative work bound. Commit
`68e8607c45a341a69b9f777788d7e6e15d0ba462`, tree
`f993392f7299525064d201bd50906202b0e9e3b5`, applies the Actions-produced rustfmt diff
and uses 512 steps, eight call frames and at most 1,000,000,000 replay-admission
work for that private test. The original production controller, public execution
defaults, schedules and admission rejection remain unchanged. This is test setup,
not a new language rule or relaxation of controlled-input validation.

## Mechanism and classified evidence

The source is the reviewed flat-report algorithm with explicit public aliases and
ordinary // comments under DP-016. The existing checked per-session next_sensor
identity maps the two immutable fixtures. No runtime field, event shape, schedule,
operation row, grant, signature or error subset is changed. The original catalog
selection/approval correspondence authenticates the scenario. Released table slots
are reusable; logical successful-acquisition identities are not.

`sentinel-0-evidence.json` classifies every added identity independently of retained
KED-035 evidence. All 2,112 predecessor identities and numeric assertions are
retained with zero migrations. Typed Result assertions, actual runtime event and
identity assertions, public CLI rendering and private addressed response evidence
are distinct. The combined-failure control admits only existing legal operation
responses, retains actual dispatch/custody, and checks release before delivery.
No third public scenario is added. Threshold receipt controls are private tests.

## Gate status

The complete exact-head workflow run/job identities, debug/release reconciliation,
scenario results and limitations belong to the identical readback-verified
implementation reports on PR #91 and Issue #89. A prior green commit or this
ledger alone is not evidence that a later head passed. The manifest requires
2,112 unchanged predecessors plus 14 separately classified Sentinel identities
in each complete profile, with no ignored, filtered or failed tests. Compilation,
rustfmt, Clippy, Rustdoc, proof regressions, docs and generated-context checks must
all pass for push and pull_request at the reported final head.

Sarah's independent review and Greg's acceptance remain pending after those gates.
Stage N and KED-036 remain out of scope; the application adds no language rule,
dependency, general mock, source filesystem or persisted authority.
