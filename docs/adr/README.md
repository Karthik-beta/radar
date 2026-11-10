# Architecture decision records

Each record captures one decision: the context, what we chose, and what we rejected. Records are numbered in the order they were accepted.

| Number | Decision | Status |
| --- | --- | --- |
| [0001](0001-tech-stack.md) | Tech stack | Superseded in part by 0005 |
| [0002](0002-architecture.md) | Architecture | Accepted |
| [0003](0003-design.md) | Visual design | Superseded in part by 0008 |
| [0004](0004-brand-assets.md) | Brand assets and self-hosted fonts | Accepted |
| [0005](0005-rust.md) | Rust everywhere | Superseded in part by 0006 |
| [0006](0006-ui-components.md) | UI components and Tailwind | Accepted |
| [0007](0007-client-routing.md) | Client routing without leptos_router | Proposed |
| [0008](0008-faint-contrast.md) | Faint text contrast | Proposed |

## Add a record

1. Copy [`template.md`](template.md) to the next free number, for example `0004-short-name.md`.
2. Fill it in with status `Proposed`.
3. Add a row to the table above.
4. When the decision is agreed, change the status to `Accepted` and add the date.

## Change a decision

Do not rewrite an accepted record. Write a new record that replaces it, then set the old record's status to `Superseded by 00NN` and update its row. An accepted record can still get small fixes, such as typos, broken links, or a pointer to where the current values live.

The [PRD](../prd.md) describes what Radar must do. When a record changes something the PRD says, the record lists the affected PRD sections at the end.
