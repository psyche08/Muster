# ADR 0082: Progressive disclosure for long reference documents

- Date: 2026-11-02
- Status: Accepted
- Related: [ARCHITECTURE](../ARCHITECTURE.md); [docs/README](../README.md);
  [AGENTS.md](../../AGENTS.md)

## Context

`docs/ARCHITECTURE.md` had grown to 1,120 lines. It held the overview and
principles, every subsystem's detail, the 2026-09-24 legacy diagnosis, the
migration table, all decisions and twelve dated status snapshots. Any change,
however narrow, sent a reader (human or agent) through the whole file, while
AGENTS.md asks agents to read only the documentation a change needs.

## Decision

`docs/ARCHITECTURE.md` becomes a 141-line entry page: §0 overview, §1
requirements, §3.1 design principles, a map from every other section to the
part that holds it, and maintenance rules. The other sections move unchanged
into `docs/architecture/`:

| Part | Sections |
|---|---|
| `runtime.md` | §3.2, §3.3, §3.7–§3.9 |
| `identity-and-trust.md` | §3.4 |
| `network.md` | §3.5 |
| `services.md` | §3.6, §3.6.1, §3.6.3 |
| `agent-halo.md` | §3.6.2, §3.6.4 |
| `multica.md` | §3.6.5 |
| `legacy-baseline.md` | §2 and the baseline appendix |
| `migration.md` | §4–§6 |
| `decisions.md` | §7 |
| `status.md` | §8, §9 |

Section numbers and headings are kept, so existing references such as
"ARCHITECTURE.md §3.6.2" in ADRs and code comments still resolve through the
map. Only heading levels and relative link prefixes changed in the moved text.
AGENTS.md, `docs/README.md`, `P0-validation.md` and `src/muster/README.md`
point at the new locations, and `docs/README.md` records progressive disclosure
as a documentation convention.

Three other reference documents mixed their rules with long dated records. The
records move, unchanged, into `docs/validation/`, and each document links to
them:

| Document | Before → after | Moved to |
|---|---|---|
| `DESKTOP_UPDATES.md` | 3,181 → 303 lines | `validation/DESKTOP-RELEASES-2026-09-29-TO-10-26.md`: every dated release record (1.0.15–1.0.94). Later releases already live in their own ADRs, and new ones go there. |
| `AGENT_INTERFACE.md` | 1,378 → 388 lines | 14 dated verification, diagnosis and incident sections into `validation/AGENT-INTERFACE-RECORDS-2026-09-28-TO-10-27.md`, listed by link at the end. The 325-line Desktop Chat mode specification becomes its own `DESKTOP_CHAT_MODE.md`. |
| `LOCKED_USE.md` | 701 → 524 lines | §7, the 2026-09-28 device deployment record, into `validation/LOCKED-USE-DEPLOYMENT-2026-09-28.md`; the §7 heading stays and links to it. |

Interface rules stay in place, including the dated sections that define
behavior (conversation styles, composer attachments, user-confirmed answer
delivery). Incoming anchor links (`CONVERSATION_WORKBENCH.md`, §8.7 in
`architecture/status.md`) and the `native_chat.rs` module comment point at
the new files.

## Alternatives

- **Trim the file in place.** Most of its length is records that later work
  still cites (status, decisions, the migration table); deleting them loses
  evidence.
- **Move status and the baseline to `docs/history/`.** §8 and §9 are still the
  live record of unaccepted work, and the baseline explains rules that remain
  binding; they stay beside the architecture rather than among superseded
  documents.
- **Renumber sections per part.** It would break dozens of existing "§N"
  references in ADRs, documents and code comments.

## Consequences

- A change reads the entry page plus the parts it touches.
- A new section goes into the part that owns its topic, with a row in the map.
- Dated evidence goes into `docs/validation/` or an ADR, not into a reference
  document.
- `LOCKED_USE.md` §1 (246 lines of confirmed direction) is still long, but it is
  the specification itself and was left whole.

## Verification

- Documentation only; no build or runtime change.
- For each split document, every non-blank line of the original is present in
  the new files. Heading levels and `../` link prefixes were normalized before
  comparing. The only exceptions are intended edits: the architecture status
  line, the §3.2/§3.9 pointer and one line of the §3.9 tree, and the retitled
  Desktop Chat mode heading.
- Every relative link and every heading anchor in `docs/` (except
  `docs/history/`), AGENTS.md and `src/muster/README.md` resolves.
- `scripts/check-adr.sh origin/main` and `scripts/check-public-content.py`.
