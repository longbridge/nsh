# Post editor

A prototype of Longbridge's short post editor on `RichEditor`: type, insert tags, write the HTML the server stores and
read it back to keep editing. The host's search APIs are fake data (`host.ts`).

| Part | Where |
|---|---|
| The three inline atoms `securityTag`, `mention`, `hashtag`, each with its `parse_dom` and `to_dom` in the server's format, the `$` `@` `#` suggestions and the press that selects a stock tag | `native/src/tags.rs` |
| The weighted length (the rule of `@web/utils`' `content-limit-rules.ts`), as the module `PostLength` | `native/src/weighted.rs` |
| The page: suggestion list, trend menu, `getHTML()` output, HTML read back with `setContent(html, "html")` | `App.tsx` |
| Round trips of every tag, the length rule | `native/tests/post_editor/`, `native/src/weighted.rs` |
| The page's behavior, headless | `App.test.tsx` |

| Node | HTML |
|---|---|
| `securityTag` | `<span type="security-tag" counter_id="ST/US/TSLA" trend="up">$Tesla</span>` (`trend` absent until picked) |
| `mention` | `<span class="mention" member_id="1001">@Alice</span>` |
| `hashtag` | `<span class="hashtag">#Earnings</span>` |

The text inside a tag (its trigger and label) is this prototype's choice: the server's attributes are the contract.

```sh
./target/debug/nsh test examples/post-editor        # the Rust and TSX tests
./target/debug/nsh run examples/post-editor --target web
```

On macOS the web target needs an LLVM `clang` for WebAssembly: `CC_wasm32_unknown_unknown` and
`AR_wasm32_unknown_unknown` at Homebrew's `llvm` (`docs/dev/web.md`).
