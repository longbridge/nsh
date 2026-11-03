// A case of the API's test suite (docs/dev/compatibility.md): `nsh/shared`,
// the state surfaces share, here with the surface's own store: a key and a
// prefix followed as accessors, a transaction that changes several keys as
// one version, a transaction that throws and commits nothing, an event heard
// by its listener, and a value that cannot be shared refused. Its script
// checks each.
import { Text, View, signal } from "nsh";
import { signal as shared, prefix, get, set, remove, transaction, version, emit, on } from "nsh/shared";

function App() {
  const count = shared<number>("count");
  const rows = prefix<number>("row/");
  const heard = signal("none");
  const state = signal("idle");
  on<string>("ping", (value) => heard.set(`ping ${value}`));
  const listed = () => {
    const all = rows();
    return (
      Object.keys(all)
        .sort()
        .map((key) => `${key}=${all[key]}`)
        .join(",") || "none"
    );
  };
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <Text>{`Count: ${count() ?? "none"}`}</Text>
      <Text>{`Rows: ${listed()}`}</Text>
      <Text>{`Heard: ${heard()}`}</Text>
      <Text>{`State: ${state()}`}</Text>
      <View
        testID="set"
        onPress={() => {
          const after = set("count", 1);
          state.set(`set ${get("count")} ${after > 0}`);
        }}
      >
        <Text>Set</Text>
      </View>
      <View
        testID="transaction"
        onPress={() => {
          const before = version();
          const after = transaction((tx) => {
            tx.set("row/a", 1);
            tx.set("row/b", 2);
            tx.set("count", 2);
          });
          state.set(`transaction ${after === before + 1}`);
        }}
      >
        <Text>Transaction</Text>
      </View>
      <View testID="remove" onPress={() => remove("row/a")}>
        <Text>Remove</Text>
      </View>
      <View testID="ping" onPress={() => emit("ping", "hi")}>
        <Text>Ping</Text>
      </View>
      <View
        testID="throws"
        onPress={() => {
          try {
            transaction((tx) => {
              tx.set("row/z", 9);
              throw new Error("no");
            });
          } catch (e) {
            state.set(`threw ${(e as Error).message} ${get("row/z")}`);
          }
        }}
      >
        <Text>Throws</Text>
      </View>
      <View
        testID="bad"
        onPress={() => {
          try {
            set("fn", () => 1);
            state.set("bad accepted");
          } catch (e) {
            state.set(`bad ${(e as Error).name}`);
          }
        }}
      >
        <Text>Bad</Text>
      </View>
    </View>
  );
}

export default App;
