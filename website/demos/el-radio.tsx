import { mount, signal, Text, View } from "nsh";
import { Radio, RadioGroup } from "nsh/base";
import { Platform } from "nsh/device";
import { ui } from "./palette";

const plans = [
  { id: "starter", label: "Starter", detail: "1 project · 5 GB", price: "Free" },
  { id: "pro", label: "Pro", detail: "Unlimited projects · 100 GB", price: "$12" },
  { id: "team", label: "Team", detail: "Shared workspace · 1 TB", price: "$40", locked: true },
];
const cycles = ["Monthly", "Yearly"];

function App() {
  const plan = signal("pro");
  const cycle = signal("Monthly");
  const events = signal<string[]>([]);
  const log = (line: string) => events.set([line, ...events()].slice(0, 3));
  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, gap: 14 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>radio</Text>
      <Text style={{ color: ui().muted }}>
        {Platform.isDesktop ? "Click an option, or Tab to one and press Space" : "Tap an option: its label checks it too"}
      </Text>

      <View style={{ maxWidth: 390, gap: 14 }}>
        {/* A segmented choice: each radio is a segment, `checkedStyle` raises the checked one. */}
        <RadioGroup style={{ flexDirection: "row", gap: 4, padding: 3, borderRadius: 10, ...ui().inset }}>
          <Each of={cycles}>
            {(c) => (
              <Radio
                checked={cycle() === c()}
                onChange={(next) => {
                  cycle.set(c());
                  log(`${c()} → ${next}`);
                }}
                style={{ flex: 1, height: 30, borderRadius: 7, alignItems: "center", justifyContent: "center" }}
                checkedStyle={ui().raised}
              >
                <Text style={{ fontSize: 13, fontWeight: "500", color: cycle() === c() ? ui().text : ui().muted }}>{c()}</Text>
              </Radio>
            )}
          </Each>
        </RadioGroup>

        {/* A list of options: a ring with a dot, which `group-checked:` shows. */}
        <RadioGroup style={{ ...ui().card, borderRadius: 14, overflow: "hidden" }}>
          <Each of={plans}>
            {(item, i) => (
              <>
                {i() > 0 ? <View style={{ height: 1, marginLeft: 48, backgroundColor: ui().line }} /> : null}
                <Radio
                  checked={plan() === item().id}
                  disabled={item().locked}
                  onChange={(next) => {
                    plan.set(item().id);
                    log(`${item().label} → ${next}`);
                  }}
                  class="group flex-row items-center gap-3 px-4 py-3 active:opacity-70 disabled:opacity-40"
                >
                  <View
                    style={{
                      width: 20,
                      height: 20,
                      borderRadius: 10,
                      borderWidth: 2,
                      borderColor: plan() === item().id ? ui().accent : ui().faint,
                      alignItems: "center",
                      justifyContent: "center",
                    }}
                  >
                    <View class="opacity-0 group-checked:opacity-100" style={{ width: 10, height: 10, borderRadius: 5, backgroundColor: ui().accent }} />
                  </View>
                  <View style={{ flex: 1, gap: 2 }}>
                    <Text style={{ fontSize: 15, fontWeight: "500", color: ui().text }}>{item().label}</Text>
                    <Text style={{ fontSize: 12, color: ui().muted }}>{item().detail}</Text>
                  </View>
                  <Text style={{ fontSize: 14, fontWeight: "600", color: ui().text }}>{item().price}</Text>
                </Radio>
              </>
            )}
          </Each>
        </RadioGroup>

        <View style={{ ...ui().inset, borderRadius: 12, padding: 12, gap: 4, minHeight: 76 }}>
          <Text style={{ fontSize: 12, color: ui().muted }}>
            onChange(checked) · {plan()}, {cycle()}
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
