// Keeps the async resource's states, controls and superseded-load behavior.
// Build with `cargo xtask api --keep api/cases/core-resource.tsx`.
import { Text, View, mount, resource, signal } from "nsh";

function App() {
  const source = signal<number | false>(false);
  const info = signal("none");
  const aborted = signal(false);
  let resolve: (value: string) => void = () => {};
  let reject: (reason: string) => void = () => {};
  let previousSignal: AbortSignal | undefined;
  let previousResolve: (value: string) => void = () => {};
  const value = resource(source, (key, options) => {
    aborted.set(previousSignal?.aborted ?? false);
    previousSignal = options.signal;
    previousResolve = resolve;
    info.set(`${key} refetch ${options.refetching}`);
    return new Promise<string>((yes, no) => { resolve = yes; reject = no; });
  });
  const initialized = resource(() => new Promise<string>(() => {}), { initialValue: "initial" });

  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <Text>Resource: {value() ?? "empty"} state {value.state()} loading {String(value.loading())} error {String(value.error())}</Text>
      <Text>Load: {info()}</Text>
      <Text>Aborted: {String(aborted())}</Text>
      <Text>Initialized: {initialized()} state {initialized.state()}</Text>
      <View testID="start" onPress={() => source.set(1)}><Text>Start</Text></View>
      <View testID="resolve" onPress={() => resolve("loaded")}><Text>Resolve</Text></View>
      <View testID="refresh" onPress={value.refetch}><Text>Refresh</Text></View>
      <View testID="mutate" onPress={() => value.mutate("optimistic")}><Text>Mutate</Text></View>
      <View testID="reject" onPress={() => reject("failed")}><Text>Reject</Text></View>
      <View testID="next" onPress={() => source.set(2)}><Text>Next source</Text></View>
      <View testID="stale" onPress={() => previousResolve("stale")}><Text>Resolve stale</Text></View>
    </View>
  );
}

mount(App);
