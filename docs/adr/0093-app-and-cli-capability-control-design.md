# ADR 0093: Separate App and CLI sessions in a capability-based control design

- Date: 2026-11-02
- Status: Proposed; design only, no runtime change
- Related: [Architecture §3.6.2](../architecture/agent-halo.md),
  [technical design](../AGENT_CONTROL_DESIGN.md),
  [CLI adapter](../ORCA_ADAPTER.md), ADR 0045, 0046, 0062, 0068, 0074, 0091

## Context

The owner reports frequent send failures across applications and requests a
technical redesign for discussion, including a plugin route, coverage of all
existing agent features, and an explicit distinction between App and CLI.
This turn reviews source and recorded evidence; it does not collect a new
failure sample or establish a single cause for the current incidents.

The current controller contract and profiles contain useful identity checks,
provider queues, exact draft checks, single-submit boundaries and receipt
reconciliation. DeepSeek Harness already has an in-app plugin, while the
Orca Rust adapter owns independent CLI children. These are different session
ownership models. A provider name or a matching transcript is not authority
to substitute one for the other.

Profiles currently make AX controls and plugins mutually exclusive, and tie
some capabilities to AX tables. That prevents assigning different operations
of one verified App session to different implementations. Historical fixes,
most recently ADR 0091, also demonstrate that application-specific layout
facts must not become universal AX assumptions.

## Decision

Propose the design in [AGENT_CONTROL_DESIGN.md](../AGENT_CONTROL_DESIGN.md):

1. Distinguish App, CLI and existing cloud session domains. Keep desktop
   Code/Chat mode separate from session domain and from provider identity.
   App/CLI identity, lifecycle, authorization and recovery remain separate.
2. Introduce typed capabilities and one Rust-owned route per operation.
   App capabilities may use a verified in-app plugin, a supported application
   API, AX, or a read source; CLI capabilities use the owned CLI runtime.
   Never switch an uncertain App mutation to CLI or another App route.
3. Keep authorization, scheduling, persistence, retry and result semantics in
   Shared Rust. Add durable preparation stages to the existing ledger rather
   than a parallel authoritative operation database.
4. Define a versioned local plugin contract, with identity binding, capability
   negotiation, operation lookup, ordered events and explicit evidence levels.
   A transport acknowledgement is not application acceptance. The first
   implementation uses reviewed adapters, not arbitrary executable manifests.
5. Move AX workflow decisions into a shared execution state machine, with
   bounded observation, action-specific verification and desktop coordination.
   Preserve Locked Use policy, input protection and lock-before-uncover cleanup.
6. Preserve existing feature semantics during migration. File references,
   native attachment cards and structured image input are separate capabilities.
   Unsupported capabilities remain explicit, with a reason and no fabricated
   success or silent substitution.

This document does **not** supersede accepted runtime rules yet. Implementation
ADRs must activate and cross-link the relevant replacements: ADR 0074's
whole-app AX/plugin exclusivity, ADR 0068's outcome presentation where changed,
and the affected profile, controller and scheduling contracts. Existing Locked
Use authorization, cloud scopes and `agents:orca` remain binding. Publishing or
merging this proposal does not authorize an unimplemented App plugin, an
automatic replay, or a change in locked-console policy.

## Alternatives

- Continue isolated selector and timeout changes: useful incident repairs,
  but they do not unify operation ownership, capability gaps and recovery.
- Wrap AX in plugins only: modular packaging does not remove UI dependence.
- Replace desktop operations with CLI: changes the session and ownership,
  contrary to the required App/CLI distinction.
- Require every provider to implement the same features: cannot create an
  absent host API and would misrepresent reduced semantics as feature parity.
- Inject code into unsupported desktop internals: fragile across updates;
  not selected as the general integration mechanism.
- Build a new networking or authorization stack for plugins: duplicates
  existing Rust foundations without solving application acceptance.

## Consequences

The design provides a feature matrix, protocol outline, identity and routing
rules, state transitions, failure handling, staged cutover and acceptance gates.
It adds no new service process and no platform-owned business state.

Full coverage means preserving each existing feature in its applicable domain,
not promising every App and CLI supports every capability. Supported plugin
entry points for Codex/Claude desktop control remain an investigation item;
this source review makes no claim about current external SDK availability.
DeepSeek is the reference integration, not evidence of complete plugin parity.

Implementation requires protocol/storage changes and physical acceptance per
application. AX cannot provide exactly-once delivery through a UI; unsupported
provider idempotency and ambiguous outcomes remain explicit constraints.

## Verification

- Documentation: check relative links in changed documents, `git diff --check`,
  `python3 scripts/check-public-content.py`, and
  `scripts/check-adr.sh origin/main` after committing.
- Source/tests: read-only review of current profiles, controller, AX sending,
  operation guard, receipts, plugin and CLI integration. No runtime source or
  test behavior is changed; application builds are not required for this PR.
- Hosted CI: do not re-enable or dispatch Actions under ADR 0016. Report the
  PR's actual check state; absent checks are not a pass.
- Builds, signing/notarization, publication, installation, runtime and physical
  end-to-end acceptance: not performed by this documentation change.
- Future implementation tests and device acceptance are specified in the
  design; they are requirements, not results.

### Documentation evidence

The six changed documents pass local target/heading checks for 125 relative
links and balanced code fences. The staged diff passes whitespace checks.
The public-content checker scanned 811 tracked text files with zero findings.
These checks establish document consistency only, not implementation behavior.
