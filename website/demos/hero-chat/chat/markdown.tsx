import { type JSX, Text, View } from "nsh";
import { TextView } from "nsh/base";
import { colors, Press } from "../ui";
import { Glyph } from "./glyphs";

/**
 * A reply's Markdown, shown by `TextView` at a chat's scale (its default
 * typography otherwise, in the theme's colors), code blocks with a Copy
 * button.
 *
 * While a reply streams (`streaming`), syntax its end leaves open shows as
 * if closed and only the block being written renders again; `caret` is
 * drawn at the end of the last block, inline with its text.
 *
 * Every block (and each list item and table cell) is one selectable text:
 * a long press highlights it and offers Copy. `copied` names the copied
 * code block (`${id}:${code}`).
 */
export function Markdown(props: {
  md: string;
  id: string;
  streaming?: boolean;
  caret?: JSX.Element;
  copied?: string | null;
  onCopy?: (key: string, text: string) => void;
}) {
  return (
    <TextView
      markdown={props.md}
      options={{ streaming: !!props.streaming }}
      selectable
      codeBlockScrollIndicator={false}
      caret={props.caret}
      styles={{
        // A chat's scale: tighter than a document's.
        root: { gap: 12, fontSize: 15, lineHeight: 22, color: colors.text },
        heading1: { fontSize: 20, lineHeight: 26 },
        heading2: { fontSize: 17, lineHeight: 22 },
        heading3: { fontSize: 15, lineHeight: 22 },
        code: { color: colors.code },
        codeBlockCode: { color: colors.code },
        link: { color: colors.accent },
      }}
      renderCodeBlockActions={(code) => <CopyButton copied={props.copied === `${props.id}:${code.code}`} onPress={() => props.onCopy?.(`${props.id}:${code.code}`, code.code)} />}
    />
  );
}

/** A code block's Copy button, "Copied" for a moment after a press. */
function CopyButton(props: { copied: boolean; onPress: () => void }) {
  return (
    <Press onPress={props.onPress} accessibilityLabel="Copy code" hitSlop={4}>
      <View style={{ height: 16, flexDirection: "row", alignItems: "center", gap: 4 }}>
        <Glyph name={props.copied ? "check" : "copy"} size={12} tint={props.copied ? colors.up : colors.muted} />
        <Text style={{ fontSize: 11, color: props.copied ? colors.up : colors.muted }}>{props.copied ? "Copied" : "Copy"}</Text>
      </View>
    </Press>
  );
}
