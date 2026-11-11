# 0008. Faint text contrast

Status: proposed, 2026-11-10. Replaces the `faint` value in [0003. Visual design](0003-design.md).

`node scripts/check-contrast.mjs` measures every text and background pair the UI uses. The `faint` token, `#5d6069`, reads at 2.90:1 on `bg` and 2.68:1 on `surface`. WCAG AA asks for 4.5:1 for text this size. `faint` colours the sync time, the key hints, the agent mark, and the "usual reply" note.

## Decision

`faint` becomes `#808490`. It keeps the hue of the old value and gets lighter until it reaches 4.5:1 on `surface`, the darker of the two backgrounds that `faint` text sits on. It stays darker than `muted` (`#8a8d96`), so the three text levels stay in order.

| Background | `#5d6069` before | `#808490` after |
| --- | --- | --- |
| `bg` (`#141519`) | 2.90:1 | 4.88:1 |
| `surface` (`#1c1d22`) | 2.68:1 | 4.50:1 |

`design/tokens.json` and `crates/web/style/tokens.css` carry the new value. The wiki page needs `node scripts/tokens-to-wiki.mjs <wiki-checkout>` and a push, which the operator does. CI runs `node scripts/check-contrast.mjs`, so a later token change cannot drop a pair under its minimum.

## Alternatives

| Option | Why not |
| --- | --- |
| Keep `#5d6069` and exempt "decorative" text | The sync time and key hints carry information |
| Use `muted` for everything faint | Loses the third level of text emphasis |

## Consequences

`faint` and `muted` are 1.13:1 apart, so the third text level reads through size and position more than colour. The brand SVGs in `design/brand/` keep `#5d6069`, because the mark is not text.
