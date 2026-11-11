# 0003. Visual design

Status: accepted, 2026-11-05. [0008. Faint text contrast](0008-faint-contrast.md) proposes to replace the `faint` value. Builds on [0002. Architecture](0002-architecture.md).

Radar is a focused inbox. You open it, deal with what is waiting on you, and leave. The design borrows from two directions explored in [prototypes/design-directions.html](../../prototypes/design-directions.html). The list comes from "Triage": one row per item, oldest first, driven from the keyboard. The opening answer comes from "Your move": a big count. The working demo is [prototypes/direction-ac.html](../../prototypes/direction-ac.html).

Radar is dark only. It has no light theme.

## Tokens

The table records the values chosen on 2026-11-05. The current values live in [`design/tokens.json`](../../design/tokens.json). The wiki's [Design tokens](https://github.com/Karthik-beta/radar/wiki/Design-tokens) page is generated from that file.

| Token | Value | Use |
| --- | --- | --- |
| `bg` | `#141519` | Page background (graphite) |
| `surface` | `#1c1d22` | Selected row, active tab, cards |
| `line` | `#25272d` | Dividers |
| `text` | `#e8e9ec` | Primary text |
| `muted` | `#8a8d96` | Secondary text: repo, actor, reason |
| `faint` | `#5d6069` | Tertiary text: sync time, agent mark, key hints |
| `warm` | `#e0b062` | Age past the first threshold |
| `hot` | `#f07a5a` | Age past the second threshold |
| `red` | `#ff5a4e` | Failing CI, and nothing else |

## Colour has two jobs

- **Time waited.** An age turns `warm`, then `hot`, as an item waits. On Your turn the thresholds are 1 day and 3 days. On Their turn they are 7 and 14 days (F4, configurable).
- **Broken CI.** `red` marks failing CI only.

Nothing else gets colour. The reason an item is waiting appears as a grey word, not a coloured dot.

## Type

- **Schibsted Grotesk** (400, 500, 600) for all interface text. Body size is 13.5 px on laptops.
- **Funnel Display** (600, 700) only for the count and its heading on Your turn. That count is the one bold element on the page.
- Numbers use tabular figures so ages line up.

## Layout

- The header has the wordmark, the tabs, and the sync time. The tabs double as the counters (F5): Your turn, Their turn, Pipeline, Merged.
- Your turn opens with the count and the age of the oldest item, then the list.
- Laptop rows are one line: reason, repo and number, title, who did what, age.
- At 600 px and below, each row becomes three lines: the title with its age, who did what, then the repo. The count grows to 104 px, because a phone visit is a glance.
- Items opened by an agent carry a faint "agent" mark after the title.

## Keyboard

`j` and `k` move the selection. `enter` opens the item on GitHub. `e` dismisses it and `s` snoozes it (F9). A dismissed row slides out. This is the only motion on the page apart from responses to input.

## Empty states

An empty list says what is still true, for example "Nothing is waiting on you. 7 threads are waiting on others."
