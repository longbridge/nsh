# Runtime capability plugins (proposal)

Status: proposed, for discussion in [#1576](https://github.com/longbridge/nsh/issues/1576).
This draft includes a startup-composition prototype and a working user-plugin example. The broader service
contract below remains a proposal.

## Purpose

An app should be able to add a runtime capability through the same extension contract that an official NSH
capability uses. Theme switching is the first candidate: the official theme provider would be installed by
default, and an app could register a workspace-theme plugin that uses it. A third party should be able to build
that plugin without changing NSH's module tables or maintaining separate QuickJS and browser implementations.

Here, a plugin provides a capability inside an engine. The engine, reactive graph, renderer and layout keep
their current responsibilities. The host selects the plugins when it creates the engine. Each engine owns
their state and releases their work when it stops.

This proposal builds on the existing registry model. It covers composition and ownership of capabilities;
[#1258](https://github.com/longbridge/nsh/issues/1258) covers the vocabulary and types of app-owned themes.
The theme service must accommodate that work rather than define another theme format.

## Try the prototype

[`examples/rust-exports/Workspace.tsx`](../../examples/rust-exports/Workspace.tsx) selects an Ocean or Orchid
workspace palette and restores the theme in `nsh.json`. The app's
[`WorkspaceTheme`](../../examples/rust-exports/native/src/workspace.rs) is a Rust plugin with its own per-engine
selection state. It declares `nsh.theme` as a dependency, receives that engine's official `ThemePlugin` during
startup, and releases its dependency on stop. Its generated `Workspace` module exposes the selection to TSX.

```rust
registry.plugin::<WorkspaceTheme>().module::<Workspace>();
```

```tsx
import { Workspace } from "rust-exports/native";
import { token } from "nsh";

Workspace.select("ocean");
const accent = token("workspace-accent"); // the new value in this call
Workspace.select("built");
```

Build the CLI and run the example or its test from the checkout:

```sh
cargo build -p nsh-cli
./target/debug/nsh run examples/rust-exports/Workspace.tsx
./target/debug/nsh run examples/rust-exports/Workspace.tsx --target web
./target/debug/nsh test examples/rust-exports/Workspace.test.tsx --trace
cargo test -p rust-exports-native --test rust_exports workspace::
```

The prototype adds `Plugin`, `PluginDependency`, `Registry::plugin` and `System::plugin`. Registries carry
immutable definitions; the VM creates instances in dependency order, rejects invalid compositions, rolls back
failed initialization and stops instances in reverse order. The default `nsh` registry registers the official
theme plugin. Its generated `setTheme` export and the user plugin use the same theme implementation and runtime
publication hook on QuickJS and the browser.

This first slice binds dependencies to plugin IDs, exact contract versions and concrete Rust types. The
independent service/provider selection described below, automatic platform and permission requirement metadata,
and the JS/TS lifecycle helper still need design. Composition is fixed at startup; changing it requires a new
engine. The existing app and host checks remain in force at their service boundaries.

The example's tests check immediate reads, rendered colors, both appearances and reset, and exercise two
engines with independent selections. Core tests cover dependency errors and rollback; native and actual-Wasm
tests exercise the shared theme service, notification counts and disposal.

## Existing extension points

NSH already has the parts that should remain the basis of a plugin:

| Existing part | Role in this proposal |
|---|---|
| `Registry`, `#[native_module]` and the app's `nsh.json` `"rust"` entry | Register an app crate's exports and describe them to the toolchain. Plugin metadata attaches to this composition. |
| `#[module]` functions and generated declarations | Keep one typed definition for native and browser calls. Plugins use the existing module loader. |
| `Registry::setup` | Initialize a set in each engine. The plugin contract adds dependencies, fallible activation and owned cleanup. |
| `System` and `Locals` | Supply the engine's access policy and per-VM module state. Plugin instances need the same engine scope. |
| `Registry::tokens_from` and `ThemeTokens` | Read an app's theme and prepare effective tokens, including host overrides. |

See [Rust modules](rust-modules.md), [module calls and state](rust-classes.md),
[app-crate registration](tooling.md#apps-with-rust-components-of-their-own-nshjson-rust) and
[the runtime module map](js-modules.md). Relevant code is in `nsh-core`'s
[`element/component.rs`](../../crates/nsh-core/src/element/component.rs),
[`element/system.rs`](../../crates/nsh-core/src/element/system.rs) and
[`runtime/mod.rs`](../../crates/nsh-core/src/runtime/mod.rs).

Some runtime-sensitive exports still have separate adapter implementations. For example,
[`setTheme` (#1216)](https://github.com/longbridge/nsh/pull/1216) prepares immediate token state and queues UI
work in [`nsh-js/src/env.rs`](../../crates/nsh-js/src/env.rs) and
[`nsh-platform-web/src/browser/mod.rs`](../../crates/nsh-platform-web/src/browser/mod.rs). The missing extension
point is a shared contract for that combination of work, alongside explicit plugin lifecycle ownership.

## Composition contract

Official and user plugins follow the same rules:

| Concern | Proposed rule |
|---|---|
| Identity | A stable, namespaced plugin ID identifies the definition. Exported module specifiers keep their current namespaces. |
| Services | A service has a stable key and a contract version, independent of its provider's crate version. |
| Dependencies | A plugin declares the services and contract versions it requires. The host selects one provider per service. |
| State | A factory makes one instance per engine. Definitions may be shared; mutable instances may not. |
| Calls | Exports use generated module bindings and their existing synchronous, queued or async semantics. |
| Ownership | Each instance owns its subscriptions, timers, tasks and cleanup handles. |
| Availability | Mandatory initialization requirements are checked before activation. Service calls retain platform, access and quota checks. |

For the first version, service versions can be exact contract versions. Cargo and the JS toolchain keep resolving
packages; the runtime does not become another package manager.

The host validates the whole composition before activating it. Duplicate plugin IDs, conflicting exports,
ambiguous service providers, missing or incompatible dependencies, and dependency cycles fail with the involved
IDs in the diagnostic. Registry order never silently overrides a provider. The default composition installs
today's official capabilities and keeps their exports available to existing apps.

This is an illustrative composition, with names still open for discussion:

```text
Engine
  official theme provider       provides: theme, contract 1
  acme/workspace-theme          requires: theme, contract 1
  acme/theme-audit               requires: theme, contract 1

Existing app import             { setTheme, token } from "nsh"
App plugin import               { applyWorkspace } from "acme/theme"
```

The workspace plugin selects or translates app-owned palettes and uses the theme service. The audit plugin
observes accepted changes and owns the subscription. Both use public extension points. Official plugin status
affects which composition NSH ships by default; it does not grant a different registration or lifecycle API.

## Activation and disposal

An engine first receives its host services and access policy, then validates and starts plugins in dependency
order, before running the app bundle. A plugin can use its ready dependencies during initialization. Calls to
an instance that is starting, failed or disposed must report that state instead of creating another instance.

If initialization fails, the failing instance releases any partial work and the engine disposes the instances
already started, in reverse dependency order. The bundle does not run with a partially activated composition.
Cleanup continues if one plugin's cleanup fails; the engine reports the failure.

On engine shutdown, pending work is cancelled and subscriptions are detached before their callbacks and VM
state are released. Disposal runs once, in reverse dependency order. A completion arriving after disposal
cannot call back into the old instance. Full app reload creates new instances; a JS module hot update keeps
engine-scoped providers and releases module-owned subscriptions through the existing owner cleanup path.

Startup composition is the first milestone. It makes capabilities selectable without requiring live unloading.
Installing or replacing providers in a running engine needs a separate contract for dependents and pending work.

## Shared runtime services

Most plugin exports can use existing module call paths. A capability such as theme switching also needs to
publish state synchronously and queue UI work in the same JS turn. A queued call alone cannot make the next
`token()` read see the new value.

The proposal adds a narrow runtime service hook for this operation. A plugin prepares a typed update;
the adapter accepts the queued work and publishes the corresponding runtime snapshot before the call returns.
If validation or queue acceptance fails, it publishes neither part. Reactive notification follows a successful
publication. Reentrant readers must see a consistent snapshot, and a later core echo must not notify them again
for the same effective value.

The shared contract belongs below both JS adapters. QuickJS and the browser implement the hook once; a plugin
uses typed services and owned handles. QuickJS pointers and browser globals stay inside those adapters. Ordinary
exports continue to use their current call paths, with no extra service lookup per node or layout operation.
UI work still joins the existing operation batch described in [Architecture](architecture.md).

The exact Rust types and the way they attach to `Registry` need review before implementation. The first change
should extend the existing descriptions and generated bindings, rather than add a second module loader or
configuration format.

## Theme as the first official plugin

The official provider owns the built-theme reset state, prepares effective tokens through the registered
theme reader, and publishes accepted changes through the runtime service hook. The engine retains rendering
and appearance handling. The existing `setTheme(theme: Theme | null): void` export delegates to this provider.

Its observable behavior stays the same:

1. Validate the complete candidate before changing runtime state or queuing UI work. Unreadable input throws
   the documented `TypeError` and leaves the current theme in force.
2. `token()` reads the new effective values before `setTheme` returns. Token classes, Rust components and the
   platform follow through the next commit in both appearances.
3. A supplied theme replaces the app's built theme over NSH's defaults. App-level partial changes are composed
   by the caller; they do not silently accumulate in the provider.
4. Embedding-host overrides keep their precedence. `null` restores the built theme, or the default when the app
   has none, with those overrides still applied.
5. Reactive readers are notified once per effective change. An invalid input or a repeated effective value
   does not produce another notification.

The existing validation follow-up [#1571](https://github.com/longbridge/nsh/pull/1571) remains a focused bug fix.
Extracting a plugin should preserve its regression coverage. App-owned theme types and readers proposed in
[#1258](https://github.com/longbridge/nsh/issues/1258) must be usable through the same service.

An app plugin may consume this service to implement brand selection or workspace palettes. A future explicit
replacement provider must satisfy the same conformance tests before it can occupy the theme service slot.

## User plugins and authority

Rust plugins use the existing app-crate registration path and compile into the app's native or WebAssembly host.
They can add exports and use installed services through the same contract as official plugins. A JS/TS lifecycle
helper can later let app extensions compose installed services and own subscriptions; it cannot install native
code that the host did not include. The first milestone needs a working user Rust plugin, and should leave the
JS/TS helper for a follow-up with its own example and cleanup tests.

Plugin requirements describe needs; the app declaration and host grants still decide authority. Native service
boundaries enforce access checks and quotas on every call, including calls made through another plugin.
An unavailable service or a permission declared mandatory for initialization fails activation with a diagnostic.
Hosts can omit optional plugins when choosing the composition.

Official providers that expose permission-gated APIs remain installed by default, even when the app has no
grant. Their calls retain today's denial or unavailability behavior. Merely registering such a provider must
not make an existing app fail startup.

Plugins in one app's JS realm share that app's trust boundary. Linked Rust plugins have the host's native-code
trust. A context object does not isolate untrusted extensions; sandboxing those would need a separate design.
The existing [sandbox and capability rules](sandbox.md) remain authoritative.

## Rollout and acceptance

The default composition must keep existing bundles, `nsh` imports, declarations and behavior working without
migration. Roll out the design in focused changes:

| Stage | Deliverable | Evidence needed before proceeding |
|---|---|---|
| Composition | Registry metadata, validation and per-engine ownership, with an official provider and a user Rust example | Dependency failures occur before activation; two engines have separate instances; failed starts roll back; teardown releases work once in reverse order. |
| Theme | Shared service hook and official theme provider, reached through the existing facade on both adapters | The same conformance scenarios pass on QuickJS and the actual browser WebAssembly runtime. |
| Further composition | A JS/TS helper, then consideration of another official capability | Examples use public extension points; subscriptions and pending work are released on reload and stop. |

Theme conformance includes immediate token reads, rendered classes and Rust components, light and dark,
embedding overrides, invalid input, reset and notification counts. A custom plugin must pass through the same
checks without changes to module tables or adapter-specific plugin code. Permission-denial tests must verify
that refused calls cause no native side effects.

Run existing compatibility cases and add coverage for any new app-facing API. Compare `nsh test --trace` before
and after extraction, including commit and mount-item counts, to verify operation batching. Browser tests must
exercise the real runtime as well as JS stand-ins. Regenerate the browser bootstrap when its source changes.

Live installation/unloading, provider replacement during execution, native dynamic-library loading and an
untrusted extension sandbox remain separate follow-ups. Each needs a working ownership contract first.

## Review decisions

1. Should metadata and ownership extend `Registry`, or should a small composition layer hold existing registries?
2. What is the smallest typed runtime hook that preserves the theme guarantees on both adapters?
3. Is startup composition with engine-owned disposal the right first milestone for official and user plugins?
