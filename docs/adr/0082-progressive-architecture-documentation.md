# ADR 0082: Progressive disclosure for the architecture document

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
- Other long documents have the same problem and are not changed here:
  `DESKTOP_UPDATES.md` (3,181 lines, 2,888 of them a release log under
  "Acceptance boundaries"), `AGENT_INTERFACE.md` (1,378 lines, mostly dated
  verification records among the interface rules) and `LOCKED_USE.md` (701
  lines, with a 182-line deployment record).

## Verification

- Documentation only; no build or runtime change.
- The moved text was compared with the original: apart from the heading levels,
  the `../` link prefixes and one line in the §3.9 tree, it is identical.
- Every relative link in `docs/ARCHITECTURE.md` and `docs/architecture/` resolves
  to an existing file.
- `scripts/check-adr.sh origin/main` and `scripts/check-public-content.py`.
