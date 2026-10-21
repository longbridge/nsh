import { mount, signal, Text, View } from "nsh";
import { Checkbox } from "nsh/base";
import { Platform } from "nsh/device";
import { ui } from "./palette";

// The parts, styled with classes: `checked:`, `indeterminate:` and
// `disabled:` follow the checkbox, `group-checked:` / `group-indeterminate:`
// pick the mark inside the indicator, `dark:` follows the page's theme.
const row =
  "flex-row items-center gap-3 px-4 py-3 active:opacity-70 disabled:opacity-40 checked:bg-[rgba(15,138,85,0.07)] dark:checked:bg-[rgba(69,212,154,0.07)]";
const indicator =
  "group size-5 items-center justify-center rounded-md border-2 border-[#8c9097] transition-colors dark:border-[#5a5e65] " +
  "checked:border-[#0f8a55] checked:bg-[#0f8a55] indeterminate:border-[#0f8a55] indeterminate:bg-[#0f8a55] " +
  "dark:checked:border-[#45d49a] dark:checked:bg-[#45d49a] dark:indeterminate:border-[#45d49a] dark:indeterminate:bg-[#45d49a]";

/** A check while checked, a dash while indeterminate. */
const mark = () => (
  <>
    <Text class="hidden text-xs font-bold text-white group-checked:flex dark:text-[#0c0d0f]">✓</Text>
    <View class="hidden h-0.5 w-2.5 rounded-full bg-white group-indeterminate:flex dark:bg-[#0c0d0f]" />
  </>
);

const items = [
  { id: "photos", label: "Photos", detail: "2,418 items · 6.1 GB" },
  { id: "documents", label: "Documents", detail: "312 files · 840 MB" },
  { id: "messages", label: "Messages", detail: "Attachments included · 1.2 GB" },
];

function Label(props: { label: string; detail: string; strong?: boolean }) {
  return (
    <View style={{ flex: 1, gap: 2 }}>
      <Text style={{ fontSize: 15, fontWeight: props.strong ? "600" : "500", color: ui().text }}>{props.label}</Text>
      <Text style={{ fontSize: 12, color: ui().muted }}>{props.detail}</Text>
    </View>
  );
}

function App() {
  const selected = signal<string[]>(["photos"]);
  const last = signal("–");
  const all = () => selected().length === items.length;
  const some = () => selected().length > 0 && !all();
  const line = () => <View style={{ height: 1, marginLeft: 48, backgroundColor: ui().line }} />;
  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, gap: 12 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>checkbox</Text>
      <Text style={{ color: ui().muted }}>
        {Platform.isDesktop ? "Click a row, or Tab to a checkbox and press Space" : "Tap a row: the label toggles it too"}
      </Text>

      <View style={{ maxWidth: 390, gap: 12 }}>
        <View style={{ ...ui().card, borderRadius: 14, overflow: "hidden" }}>
          <Checkbox
            checked={all()}
            indeterminate={some()}
            onChange={(next) => {
              selected.set(next ? items.map((i) => i.id) : []);
              last.set(`Select all → ${next}`);
            }}
            class={row}
            indicatorClass={indicator}
            indicator={mark()}
          >
            <Label label="Back up everything" detail={`${selected().length} of ${items.length} selected`} strong />
          </Checkbox>
          <Each of={items}>
            {(item) => (
              <>
                {line()}
                <Checkbox
                  checked={selected().includes(item().id)}
                  onChange={(next) => {
                    selected.set(next ? [...selected(), item().id] : selected().filter((id) => id !== item().id));
                    last.set(`${item().label} → ${next}`);
                  }}
                  class={row}
                  indicatorClass={indicator}
                  indicator={mark()}
                >
                  <Label label={item().label} detail={item().detail} />
                </Checkbox>
              </>
            )}
          </Each>
        </View>

        <View style={{ ...ui().card, borderRadius: 14, overflow: "hidden" }}>
          <Checkbox checked disabled class={row} indicatorClass={indicator} indicator={mark()}>
            <Label label="System settings" detail="Always backed up" />
          </Checkbox>
        </View>

        <Text style={{ color: ui().muted, fontSize: 13 }}>
          select all: {all() ? "checked" : some() ? "indeterminate" : "unchecked"} · [{selected().join(", ")}]
        </Text>
        <Text style={{ color: ui().up }}>last onChange: {last()}</Text>
      </View>
    </View>
  );
}

mount(App);
