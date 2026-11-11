# Post editor

A prototype of Longbridge's short post editor on `RichEditor`: text with three kinds of inline tag, written as the
HTML `POST /v2/social/topics` takes and read back from it. The host's search is fake data (`host.ts`).

| | |
|---|---|
| `$` `@` `#` | open a search (stocks, members, topics); arrows, Enter, Tab or a press pick, Escape closes |
| a press on a stock tag | selects it and opens a menu: bullish (`trend="up"`), bearish (`trend="down"`) or none; each is one undoable `setNodeAttribute` |
| `getHTML()` | the page shows it; `setContent(html, "html")` reads it back (the buttons below the output) |
| the count | the weighted length (`PostLength`): 1000 in a Chinese locale, 2000 in another |

```html
<span type="security-tag" counter_id="ST/US/TSLA" trend="up">$Tesla</span>
<span class="mention" member_id="1001">@Alice</span>
<span class="hashtag">#Earnings</span>
```

`native/` is the app's crate (`nsh.json` `"rust": "native"`): the three nodes as atoms (`src/tags.rs`: schema,
`parseDOM`, `toDOM`), the suggestions of the triggers as one high-priority extension (a `createRichEditor` call's own
plugins come after the keymap, which would take Enter first), and the weighted length (`src/weighted.rs`, the rule of
`@web/utils` `content-limit-rules.ts`).

```sh
./target/debug/nsh test examples/post-editor      # the crate's tests, then App.test.tsx
./target/debug/nsh run examples/post-editor --target web
```
