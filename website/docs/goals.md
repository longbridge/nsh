---
title: Goals and design principles
description: "Native Shell's goals: native views and interaction, app-owned design, shared components, and TSX and Rust on one engine."
order: 1.5
group: start
---

Native Shell (NSH) gives you native views and interaction on iOS, Android, macOS, Windows, Linux and the web. Your app owns the visual design. The same component model works for whole apps, features embedded in existing apps, and UI delivered at run time.

## Native foundations, app-owned design

NSH uses each platform's views, text and input. It adapts interaction to the device: keyboard navigation, pointer and touch, text editing, scrolling and accessibility ([Introduction](./index.md#what-native-means)).

Your app defines colors, typography, spacing, shapes, state feedback and animation. For example, a settings screen can keep your brand on iOS and Android, while input and navigation follow each device's conventions. Designers adjust the look through styles and component parts.

A native view is the rendering foundation. A system-styled control is one possible look and behavior, not a requirement. NSH's common controls give you shared behavior and customizable parts, so you can build your own design system.

## Native integration in practice

The native foundation covers rendering, frame-driven animation, input and accessibility.

On iOS and Android, [Input](../elements/input.md) uses the platform text field. The platform handles editing, selection menus, input methods and supported autofill services. You describe the field with `autoComplete`, such as `current-password` or `postal-code`. The system then offers suggestions based on its settings and services ([Forms and keyboard](./forms-and-keyboard.md)). You can style the field and still keep this integration.

A custom renderer that draws its own text field must build its own link to system editing and accessibility. NSH uses the platform text control wherever its backend provides one. Each backend's docs list what it integrates, including NSH's own editor on Windows and in the GPUI embedding backend ([Platforms](./platforms.md)).

NSH's layers follow [GPUI Kit](https://gpui-kit.com), which separates component behavior from visual design. NSH adds platform views and input on top of that idea.

## One component model across platforms

A switch means the same thing on a phone, a desktop and the web. [`nsh/base`](../modules/nsh-base.md) provides state, input handling, focus and accessibility. Your app styles the [Switch](../elements/switch.md)'s track, thumb and states. Buttons, checkboxes and radios work the same way.

| Responsibility | Home | Example |
|---|---|---|
| Rendering, layout, reactivity and environment | `nsh-core`, `nsh` and runtime modules | Views, text, signals, styles |
| Shared behavior and accessible structure | `nsh-base`, imported as `nsh/base` in TSX | `Button`, `Checkbox`, `Switch`, `Tabs` |
| Visual design and styled components | The app or a community library | Branded controls, cards, form rows |
| Platform-specific capabilities | `nsh/mobile`, `nsh/desktop` | Mobile navigation bar, desktop window controls |

Common controls live in one place: `nsh/base`. Platform modules hold only what has platform-specific meaning:

- [`TabBar`](../modules/nsh-mobile.md#tabbar) is mobile top-level navigation. [`Tabs`](../elements/tabs.md) switches tabbed content on every platform. They do different jobs.
- `nsh/mobile` takes capabilities whose interaction is specific to mobile.
- If a common control only differs in appearance, size, tint or animation, handle it in the shared component and app styles.
- A notification or toast with a shared role starts as one cross-platform contract. The component adapts its presentation internally.

A shared component can adapt its interaction. A menu opens as a popup on desktop and as a sheet on a phone. Your app uses one API and supplies the look. System-managed presentations, such as navigation headers and alerts, follow their documented platform rules. To expose your own native components and services, use the [embedding interfaces](./native-components.md).

## The unstyled foundation and the component-library layer

NSH owns the primitives and the unstyled foundation: `nsh-core` and `nsh-base`. Apps and third-party libraries own the styled component layer. This is the role `gpui-component` plays in [GPUI Kit](https://gpui-kit.com). Each product controls its look and reuses the hard parts of component behavior.

This split comes from experience with [GPUI Kit](https://gpui-kit.com). Its styled components grew out of Longbridge Pro. As apps needed more varied designs, users started forking components to customize them. Moving the shared foundation into `gpui-base` let different designs reuse the same behavior. Styled components are still useful as a design reference, and for apps whose needs fit their customization options. The foundation gives every other app direct control over presentation.

NSH starts with this split:

- `nsh/base` owns complex behavior, interaction, reactive state, focus, accessibility and adaptive presentation. Each behavior is tested in the layer that owns it. This includes larger structures such as [Dock](../elements/dock.md).
- Apps build their designs on top with styles, states, slots and parts.
- Visual defaults and branded variants belong in app or third-party components, including contributed ones.
- Documented system-managed presentations and `TextView` reading defaults keep their own contracts ([Architecture](./architecture.md#three-layers)).

### What the Gallery demonstrates

The website demos and the Gallery are styled example apps. Their gradients, shadows, rounded corners, transitions and motion show what NSH customization can do, and give you source to copy. These choices belong to the demos, not to the base components. The base components only provide the behavior and parts underneath. You can borrow the examples or build a different look.

A styled library distributed on its own belongs to the app and community layer above. The Gallery shows one design direction. Whether it fits your product is your call.

## Rust and TSX on one engine

TSX apps, TSX apps extended with Rust, and Rust apps share the same primitives, base components, signals and layout.

- **TSX** holds screens, state and logic, with fast iteration. The same modules can be delivered at run time.
- **Rust** adds compiled logic and reusable native components through [nshm](./nshm.md), or runs a whole app ([Building with Rust](./rust.md)).

The engine updates only bindings whose signals changed, and batches view operations. Text layout, native animation and input belong to the engine and its platform backends ([Architecture](./architecture.md)). Performance is measured with reproducible app workloads ([Performance](./performance.md)).

## Embedding, dynamic delivery and AI

An existing native app can place an NSH view where a feature belongs. It can also expose its own native components and host services. Standalone apps use the same runtime and component model ([Platforms](./platforms.md)).

Dynamically delivered TSX modules own their state, logic and layout. The host decides two things:

- **Trust:** signed packages identify which publishers the host trusts ([Signed packages](./signed-packages.md)).
- **Permissions:** sandbox limits and capabilities define what a module may do ([Sandbox and capabilities](./sandbox-and-capabilities.md)).

AI-generated UI follows the same contract:

1. Generate TSX against the declared APIs.
2. Check and test it.
3. Build and sign the module.
4. Verify and run it under host policy.

Generated type declarations, model-readable docs and CLI tests make this workflow inspectable and repeatable ([Testing and tracing](./testing-and-trace.md)).

## How these goals guide additions

1. Define the user behavior, and check whether it is shared across platforms.
2. Put shared behavior in an existing primitive or base component. Expose state and parts so apps can customize it.
3. Put capabilities with platform-specific meaning in their platform module. Document where they apply and how they combine with shared UI.
4. Build styled components in the app or a community library.
5. Verify behavior, accessibility, layout and input on affected platforms. Document the public contract in both languages.

These rules keep the visual design flexible, the shared APIs consistent and the platform integration explicit as NSH grows.
