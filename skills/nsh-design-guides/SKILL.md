---
name: nsh-design-guides
description: 'The normative Design Guides for Native Shell (NSH) apps. Read before designing or changing any screen, layout, spacing, visual hierarchy, color, density, component choice, interaction state, overlay, motion, data-heavy view or interface copy in an NSH app, in TSX or Rust, and before reviewing UI work for design quality. Also use when asked what the design rules are, or whether a UI decision follows them.'
---

# NSH Design Guides

Resolve the bundled guide relative to this skill’s directory, not the app directory.
The guide is [references/design-guides.md](references/design-guides.md). It is a
requirement, not inspiration. Read the guide file itself before doing UI work. Do
not answer from this page, from an existing screen in the codebase, or from training
data.

If the file is missing, fetch `https://nsh.dev/docs/design-guides.md`. The guide is a
verbatim copy of that page, so a link in it such as `./coding-guides.md` means
`https://nsh.dev/docs/coding-guides.md` (also in the `nsh` skill), and
`../elements/index.md` means `https://nsh.dev/elements/index.md`.

Apply the guide within the user’s requested scope and the app’s established visual
language. For a narrow change, preserve surrounding screens and components. Use
semantic NSH base components, style their visible parts, and inspect the actual
result in the requested appearance and viewport when available. Behavior tests
alone do not verify layout or styling; state when visual verification is unavailable.

For NSH APIs and implementation, use the `nsh` skill when installed, or the relevant
page from [the documentation index](https://nsh.dev/llms.txt).

## How to read it

Read the whole guide for a new screen or a redesign. For a narrow change, read "Design
thesis" and "Start from the task" first, then the section for the change. Sections, in
order (`grep -n '^## ' references/design-guides.md`):

| Section                               | Read when                                                              |
| ------------------------------------- | ---------------------------------------------------------------------- |
| Design thesis                         | Always                                                                 |
| Learning from Shadcn                  | Choosing what to borrow from web component libraries                   |
| Start from the task                   | Always; task hierarchy, what to leave out                              |
| Visual language                       | Color, typography, spacing, radius, borders, elevation, density, icons |
| Layout patterns                       | Window and page structure, sidebars, toolbars, panels, forms           |
| Components and composition            | Picking a component, composing parts, when to build a new one          |
| Interaction states                    | Hover, focus, pressed, selected, disabled, loading, validation, danger |
| Feedback and overlays                 | Dialog, sheet, popover, menu, notification, tooltip, dismissal, focus  |
| Motion                                | Any animation or transition                                            |
| Designing data-heavy interfaces       | Tables, lists, trees, dashboards, dense inspectors                     |
| Interface language                    | Any user-facing text: labels, buttons, titles, errors, empty states    |
| Internationalization and platform fit | Multi-locale copy, Chinese terminology, platform conventions           |
| Guidance for AI-generated interfaces  | Always when an agent produces UI                                       |
| Accessibility checklist               | Before finishing                                                       |
| Design review checklist               | Before finishing; run every item against the work                      |
