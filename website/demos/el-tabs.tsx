import { mount, signal, Text, View } from "nsh";
import { Tab, Tabs } from "nsh/base";
import { Platform } from "nsh/device";
import { tint, ui } from "./palette";

// What each section shows: the app switches the content by its own value.
const sections = [
  {
    id: "overview",
    label: "Overview",
    rows: [
      ["Visitors", "12,480"],
      ["Sign-ups", "342"],
      ["Conversion", "2.74%"],
    ],
  },
  {
    id: "activity",
    label: "Activity",
    rows: [
      ["Deploy to production", "09:41"],
      ["Invoice paid", "08:15"],
      ["New member joined", "07:02"],
    ],
  },
  {
    id: "settings",
    label: "Settings",
    rows: [
      ["Region", "Frankfurt"],
      ["Plan", "Pro"],
      ["Seats", "8 of 10"],
    ],
  },
];
const folders = [
  { id: "inbox", label: "Inbox", count: 12 },
  { id: "archive", label: "Archive", count: 0 },
  { id: "spam", label: "Spam", count: 3, locked: true },
];

function App() {
  const section = signal("overview");
  const folder = signal("inbox");
  const events = signal<string[]>([]);
  const log = (line: string) => events.set([line, ...events()].slice(0, 3));
  const current = () => sections.find((s) => s.id === section())!;
  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, gap: 14 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>tabs</Text>
      <Text style={{ color: ui().muted }}>
        {Platform.isDesktop ? "Click a tab, or Tab to one and press Space" : "Tap a tab to show its section"}
      </Text>

      <View style={{ maxWidth: 390, gap: 14 }}>
        {/* A segmented tab list: each tab is a segment, `selectedStyle` raises the selected one. */}
        <Tabs style={{ flexDirection: "row", gap: 4, padding: 3, borderRadius: 10, ...ui().inset }}>
          <Each of={sections}>
            {(s) => (
              <Tab
                selected={section() === s().id}
                onPress={() => {
                  section.set(s().id);
                  log(`${s().label} pressed`);
                }}
                style={{ flex: 1, height: 30, borderRadius: 7, alignItems: "center", justifyContent: "center" }}
                selectedStyle={ui().raised}
              >
                <Text style={{ fontSize: 13, fontWeight: "500", color: section() === s().id ? ui().text : ui().muted }}>
                  {s().label}
                </Text>
              </Tab>
            )}
          </Each>
        </Tabs>

        {/* The selected section's content, which the app shows by its value. */}
        <View style={{ ...ui().card, borderRadius: 14, overflow: "hidden" }}>
          <Each of={current().rows}>
            {(row, i) => (
              <>
                {i() > 0 ? <View style={{ height: 1, marginLeft: 16, backgroundColor: ui().line }} /> : null}
                <View style={{ flexDirection: "row", alignItems: "center", paddingHorizontal: 16, paddingVertical: 12 }}>
                  <Text style={{ flex: 1, fontSize: 15, color: ui().text }}>{row()[0]}</Text>
                  <Text style={{ fontSize: 14, fontWeight: "600", fontVariant: ["tabular-nums"], color: ui().muted }}>{row()[1]}</Text>
                </View>
              </>
            )}
          </Each>
        </View>

        {/* Tabs with counts: the selected one's own fill, a disabled one dimmed. */}
        <Tabs style={{ flexDirection: "row", gap: 6 }}>
          <Each of={folders}>
            {(f) => (
              <Tab
                selected={folder() === f().id}
                disabled={f().locked}
                onPress={() => {
                  folder.set(f().id);
                  log(`${f().label} pressed`);
                }}
                class="active:opacity-70 disabled:opacity-40"
                style={{ flexDirection: "row", alignItems: "center", gap: 6, height: 30, paddingHorizontal: 12, borderRadius: 8 }}
                selectedStyle={{ backgroundColor: tint(ui().accent, 0.16) }}
              >
                <Text style={{ fontSize: 13, fontWeight: "500", color: folder() === f().id ? ui().accent : ui().muted }}>
                  {f().label}
                </Text>
                {f().count > 0 ? (
                  <Text style={{ fontSize: 12, fontWeight: "600", fontVariant: ["tabular-nums"], color: ui().faint }}>{f().count}</Text>
                ) : null}
              </Tab>
            )}
          </Each>
        </Tabs>

        <View style={{ ...ui().inset, borderRadius: 12, padding: 12, gap: 4, minHeight: 76 }}>
          <Text style={{ fontSize: 12, color: ui().muted }}>
            onPress · {section()}, {folder()}
          </Text>
          <Each of={events()}>
            {(e, i) => <Text style={{ fontSize: 12, color: i() === 0 ? ui().text : ui().faint }}>{e()}</Text>}
          </Each>
        </View>
      </View>
    </View>
  );
}

mount(App);
