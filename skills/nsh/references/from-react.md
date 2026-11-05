# Coming from React

NSH's TSX looks like React and React Native. What runs when is different: NSH follows
Solid's model. Each row below is a habit that goes wrong, and how the toolchain reports
it. "Nothing" means the code builds and runs, and only behaves wrongly.

| Habit | In NSH | Reported by |
| --- | --- | --- |
| `const [n, setN] = useState(0)` | `const n = signal(0)`; read `n()`, write `n.set(1)` or `n.update((v) => v + 1)` | `nsh build`: `` `nsh` has no export `useState` `` |
| `import React from "react"`, npm UI kits | Nothing to import; compose `nsh` and `nsh/base` | `nsh build` |
| `<div>`, `<span>`, `<button>` | `View`, `Text`, `Button` from `nsh/base` | `nsh build`: lowercase tag |
| `<View>hello</View>` | `<View><Text>hello</Text></View>` | `nsh build` |
| `function Row({ title })` | `function Row(props)`, read `props.title` where it is used | `nsh build`: a warning, "takes its props apart" |
| `const { a, ...rest } = props` | `const [picked, rest] = split(props, ["a"])` | Nothing |
| `const label = props.title.toUpperCase()` in the body, expected to update | `const label = () => props.title.toUpperCase()`, or `computed` | Nothing: read once |
| `const v = count();` in the body, `<Text>{v}</Text>` | `<Text>{count()}</Text>` | Nothing: read once |
| `useEffect(fn, [deps])` | `effect(fn)`: it follows what it reads, with no list; `cleanup(stop)` or return the stop | `nsh build` if imported |
| `useMemo`, `useCallback`, `memo` | Not needed: components do not run again. `computed` for a derived value | `nsh build` if imported |
| `useRef`, `useContext` | `let field: InputRef | undefined; <Input ref={field} />`, `context()` | `nsh build` if imported |
| An effect that copies one signal to another | `computed`, or a closure | Nothing |
| `cond && <A />`, `items.map(…)` | `<If when={cond()}>`, `<Each of={items()} key={…}>`: they keep what stays. A row's item is a signal: `item().id` | Nothing: the branch or rows are built again |
| `className=` | `class=`, with `classList` for conditions | `nsh check` only |
| A class that does not exist | [Tailwind v4's classes](https://nsh.dev/modules/tailwind-reference.md) and the theme's tokens | `nsh build`: a warning, and the class does nothing |
| `window`, `document`, `localStorage` | `windowSize()`, `Storage` (async), refs | `nsh check` only |
| `import fs from "node:fs"` | `fs/promises`, one of the standard modules | `nsh build` |
| `onClick`, `TouchableOpacity`, `Pressable` | `onPress`; `Button` from `nsh/base`; `active:` classes for the pressed look | `nsh check` |
| `FlatList` with `data` and `renderItem` | `<List each={rows()}>{(row) => …}</List>`, where `row` is a signal and the cell is reused | `nsh check` |
| `useState` inside a list row | State in data keyed by id: a signal in a row travels to the next item | Nothing |
| `onChangeText={setText}` | `onChange={({ value }) => text.set(value)}`: events take one object | `nsh check` |
| `StyleSheet.create` | `class`, or `style={{ … }}` where numbers are points | `nsh check` |
| `onBeforeLeave={() => false}` | `popOnBack={!dirty()}` with `onBackPress`: the main thread never waits for app code | `nsh check` |
| A hand-made dropdown from `View` and `Overlay` | `Select`, `Menu`, `Popover` | Nothing |
| Raw `#hex` colors | `bg-primary`, `text-muted-foreground`, or a role in the theme file | Nothing |

The last column is why the loop runs both commands: `nsh build` compiles without type
checking, so run `nsh check` as well, and write a test for what neither sees.
