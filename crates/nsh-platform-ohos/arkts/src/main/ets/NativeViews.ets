import { NativeMenuItem, NativeMenuCleanup } from "./NativeMenus";
import { TouchCoordinates, PointerPosting, TouchCancel, NativeRetirement } from "./NativePointer";
import {
  BuilderNode,
  ComponentContent,
  NodeContent,
  NodeController,
  NodeRenderType,
  RenderOptions,
  FrameNode,
} from "@ohos.arkui.node";
import { UIContext } from "@ohos.arkui.UIContext";
import deviceInfo from "@ohos.deviceInfo";
import { NshRuntimeContext } from "./NativeEditors";

interface ViewAction {
  name: string;
  label: string;
}
interface ViewMenu {
  generation: number;
  visible: boolean;
  x: number;
  y: number;
  items: NativeMenuItem[];
}
export interface Settings {
  textureSurface?: string | null;
  textureInsets?: TextureInsets | null;
  targetId: number;
  testId?: string | null;
  width: number;
  height: number;
  hue: number;
  semantic: boolean;
  clickable: boolean;
  disabled: boolean;
  accessible?: boolean | null;
  hidden: boolean;
  label: string;
  hint: string;
  value: string;
  role: string;
  checked?: boolean | null;
  selected: boolean;
  api23: boolean;
  api26: boolean;
  actions: ViewAction[];
  menu?: ViewMenu | null;
}
interface TextureInsets {
  left: number;
  top: number;
  right: number;
  bottom: number;
}
interface ViewParameters {
  entry: ViewEntry;
  settings: Settings;
}
interface MenuParameters {
  entry: ViewEntry;
  menu: ViewMenu;
}
interface MenuLease {
  content: ComponentContent<MenuParameters>;
  generation: number;
  opening: Promise<void>;
  retired: boolean;
}
function captureWidth(settings: Settings): number {
  return (
    settings.width + (settings.textureInsets?.left ?? 0) + (settings.textureInsets?.right ?? 0)
  );
}
function captureHeight(settings: Settings): number {
  return (
    settings.height + (settings.textureInsets?.top ?? 0) + (settings.textureInsets?.bottom ?? 0)
  );
}
interface Acknowledgement {
  accepted: boolean;
}
interface CustomAction {
  name: string;
  onAction: () => void;
}
// A structural interface keeps API-26 methods behind both feature and runtime guards.
interface ActionsAttribute {
  accessibilityCustomActions(actions: CustomAction[]): CommonAttribute;
}
interface StateAttribute {
  accessibilityStateDescription(description: string): CommonAttribute;
}
const quarantinedViews: Set<ViewEntry> = new Set();
const roles: Map<string, AccessibilityRoleType> = new Map([
  ["button", AccessibilityRoleType.BUTTON],
  ["checkbox", AccessibilityRoleType.CHECKBOX],
  ["switch", AccessibilityRoleType.SWITCH],
  ["radio", AccessibilityRoleType.RADIO],
  ["slider", AccessibilityRoleType.SLIDER],
  ["image", AccessibilityRoleType.IMAGE],
  ["link", AccessibilityRoleType.HYPERLINK],
  ["list", AccessibilityRoleType.LIST],
  ["grid", AccessibilityRoleType.GRID],
  ["progress", AccessibilityRoleType.PROGRESS],
  ["search", AccessibilityRoleType.SEARCH],
  ["menuitem", AccessibilityRoleType.MENU_ITEM],
  ["text", AccessibilityRoleType.TEXT],
]);
function role(value: string): AccessibilityRoleType | undefined {
  if (value === "") {
    return undefined;
  }
  const result = roles.get(value);
  if (result === undefined) {
    throw new Error("Invalid native wrapper role");
  }
  return result;
}

class ViewModifier implements AttributeModifier<StackAttribute> {
  private entry: ViewEntry;
  private settings: Settings;
  constructor(entry: ViewEntry, settings: Settings) {
    this.entry = entry;
    this.settings = settings;
  }
  applyNormalAttribute(node: StackAttribute): void {
    const s = this.settings;
    node
      .id(s.testId ?? `nsh-view-${s.targetId}`)
      .width(s.width)
      .height(s.height)
      .hueRotate(s.hue)
      .clip(false)
      .hitTestBehavior(HitTestMode.Transparent)
      .accessibilityText(s.label)
      .accessibilityDescription(
        s.api23 ? s.hint : [s.hint, s.value].filter((value) => value.length > 0).join(", "),
      )
      .accessibilityLevel(
        s.hidden ? "no-hide-descendants" : !s.semantic || s.accessible === false ? "no" : "yes",
      )
      .accessibilityGroup(s.accessible === true)
      .accessibilitySelected(s.selected);
    node.accessibilityRole(role(s.role));
    node.accessibilityChecked(s.checked === null ? undefined : s.checked);
    node.enabled(!s.disabled).onClick(s.clickable ? (): void => {} : undefined);
    if (s.api23 && typeof (node as StateAttribute).accessibilityStateDescription === "function") {
      (node as StateAttribute).accessibilityStateDescription(s.value);
    }
    if (s.api26 && typeof (node as ActionsAttribute).accessibilityCustomActions === "function") {
      const actions: CustomAction[] = s.actions.map((action: ViewAction): CustomAction => {
        return {
          name: action.label,
          onAction: (): void => {
            this.entry.action(action.name);
          },
        };
      });
      (node as ActionsAttribute).accessibilityCustomActions(actions);
    } else if (s.api26 && s.actions.length > 0) {
      throw new Error("The runtime has no ArkUI accessibilityCustomActions implementation");
    }
  }
}
@Component
struct ViewMenuContent {
  entry!: ViewEntry;
  @Prop menu!: ViewMenu;
  build() {
    Menu() {
      ForEach(
        this.menu.items,
        (item: NativeMenuItem) => {
          MenuItem({ content: item.label }).onClick((): void => {
            this.entry.action(item.name, "menu", this.menu.generation);
          });
        },
        (item: NativeMenuItem): string => item.name,
      )
    }.padding(8);
  }
}
@Builder
function menuContent(params: MenuParameters) {
  ViewMenuContent({ entry: params.entry, menu: params.menu });
}
@Builder
function nativeView(params: ViewParameters) {
  Stack({ alignContent: Alignment.TopStart }) {
    ContentSlot(params.entry.slot)
  }
  .width(params.settings.width)
  .height(params.settings.height)
  .attributeModifier(new ViewModifier(params.entry, params.settings))
  .onAccessibilityActionIntercept(
    (action: AccessibilityAction): AccessibilityActionInterceptResult => {
      if (action === AccessibilityAction.ACCESSIBILITY_CLICK && params.entry.action("activate")) {
        return AccessibilityActionInterceptResult.ACTION_INTERCEPT;
      }
      return AccessibilityActionInterceptResult.ACTION_CONTINUE;
    },
  );
}
class ViewController extends NodeController {
  private node: BuilderNode<[ViewParameters]>;
  constructor(node: BuilderNode<[ViewParameters]>) {
    super();
    this.node = node;
  }
  makeNode(_context: UIContext): FrameNode | null {
    return this.node.getFrameNode();
  }
}
@Observed
class ViewState {
  settings: Settings;
  constructor(settings: Settings) {
    this.settings = settings;
  }
}
// Custom roots preserve ArkUI's native export boundary. Observed settings refresh
// their attributes without rebuilding the slot or changing its native ownership.
@Component
struct SourceRoot {
  entry!: ViewEntry;
  @ObjectLink state!: ViewState;
  build() {
    Stack({ alignContent: Alignment.TopStart }) {
      nativeView({ entry: this.entry, settings: this.state.settings })
    }
    .width(captureWidth(this.state.settings))
    .height(captureHeight(this.state.settings))
    .padding(this.state.settings.textureInsets ?? { left: 0, top: 0, right: 0, bottom: 0 })
    .clip(false)
    .hitTestBehavior(HitTestMode.Transparent);
  }
}
@Builder
function sourceContent(params: ViewParameters) {
  SourceRoot({ entry: params.entry, state: params.entry.state });
}
@Component
struct TextureRoot {
  entry!: ViewEntry;
  @ObjectLink state!: ViewState;
  build() {
    NodeContainer(this.entry.sourceController!)
      .width(captureWidth(this.state.settings))
      .height(captureHeight(this.state.settings))
      .clip(false)
      .hitTestBehavior(HitTestMode.Transparent);
  }
}
@Builder
function textureContent(params: ViewParameters) {
  TextureRoot({ entry: params.entry, state: params.entry.state });
}
@Builder
function textureContainer(params: ViewParameters) {
  NodeContainer(params.entry.textureController!)
    .width(captureWidth(params.settings))
    .height(captureHeight(params.settings))
    .clip(false)
    .hitTestBehavior(HitTestMode.Transparent)
    .onTouch((event: TouchEvent): void => {
      params.entry.postMouseTouch(event);
    });
}
@Component
struct DirectRoot {
  entry!: ViewEntry;
  @ObjectLink state!: ViewState;
  build() {
    nativeView({ entry: this.entry, settings: this.state.settings });
  }
}
@Builder
function viewContent(params: ViewParameters) {
  if (params.entry.textureNode) {
    textureContainer(params);
  } else {
    DirectRoot({ entry: params.entry, state: params.entry.state });
  }
}
class ViewEntry {
  readonly state: ViewState;
  readonly slot: NodeContent = new NodeContent();
  readonly textureNode?: BuilderNode<[ViewParameters]>;
  readonly sourceNode?: BuilderNode<[ViewParameters]>;
  readonly sourceController?: ViewController;
  readonly textureController?: ViewController;
  private node: BuilderNode<[ViewParameters]>;
  private context: NshRuntimeContext;
  private ui: UIContext;
  private menus: NativeMenuCleanup;
  private closed: boolean = false;
  private sdkBusy: boolean = false;
  private disposePending: boolean = false;
  private retirementFailure?: Error;
  private menuLease?: MenuLease;
  private pointerPosting: PointerPosting;
  private retirement: NativeRetirement = new NativeRetirement();
  constructor(
    settings: Settings,
    context: NshRuntimeContext,
    menus: NativeMenuCleanup,
    posting: PointerPosting,
  ) {
    this.state = new ViewState(settings);
    this.menus = menus;
    this.context = context;
    this.ui = context.getUIContext();
    this.pointerPosting = posting;
    const options: RenderOptions | undefined =
      settings.textureSurface == null
        ? undefined
        : {
            type: NodeRenderType.RENDER_TYPE_TEXTURE,
            surfaceId: settings.textureSurface,
            selfIdealSize: { width: captureWidth(settings), height: captureHeight(settings) },
          };
    if (options !== undefined) {
      this.textureNode = new BuilderNode<[ViewParameters]>(this.ui, options);
      this.sourceNode = new BuilderNode<[ViewParameters]>(this.ui);
      this.sourceController = new ViewController(this.sourceNode);
      this.textureController = new ViewController(this.textureNode);
    }
    this.node = new BuilderNode<[ViewParameters]>(this.ui);
  }
  get settings(): Settings {
    return this.state.settings;
  }
  create(): void {
    this.sdkBusy = true;
    let stage: string = "build";
    try {
      const params: ViewParameters = { entry: this, settings: this.settings };
      if (this.textureNode) {
        this.sourceNode?.build(wrapBuilder(sourceContent), params, {
          nestingBuilderSupported: true,
        });
        if (this.closed) { throw new Error("Native wrapper creation was cancelled"); }
        this.textureNode.build(wrapBuilder(textureContent), params, {
          nestingBuilderSupported: true,
        });
      }
      if (this.closed) { throw new Error("Native wrapper creation was cancelled"); }
      this.node.build(wrapBuilder(viewContent), params, { nestingBuilderSupported: true });
      if (this.closed || !this.context.isActive()) { throw new Error("Native wrapper creation was cancelled"); }
      stage = "attach";
      const accepted = this.context.attachNativeSubtree(
        this.settings.targetId,
        this.node.getFrameNode(),
        this.slot,
      );
      if (!accepted) {
        throw new Error("Native wrapper attachment was rejected");
      }
      this.syncMenu();
    } catch (error) {
      this.dispose();
      throw new Error(`Native wrapper ${this.settings.targetId} ${stage} failed: ${String(error)}`);
    } finally {
      this.sdkBusy = false;
      if (this.disposePending) { this.disposePending = false; this.disposeBuilders(); }
    }
  }
  update(settings: Settings): void {
    if (settings.targetId !== this.settings.targetId) {
      throw new Error("Native wrapper target cannot change");
    }
    this.state.settings = settings;
    const params: ViewParameters = { entry: this, settings: settings };
    this.sdkBusy = true;
    try { this.node.update(params); }
    finally {
      this.sdkBusy = false;
      if (this.disposePending) { this.disposePending = false; this.disposeBuilders(); }
    }
    if (!this.closed) { this.syncMenu(); }
  }
  private syncMenu(): void {
    try {
      this.applyMenu();
    } catch (error) {
      console.error(`[NSH] native menu construction failed: ${String(error)}`);
      const lease = this.menuLease;
      this.menuLease = undefined;
      if (lease) {
        this.retireMenu(lease);
      }
      this.action("", "menu-dismiss", this.settings.menu?.generation);
    }
  }
  private applyMenu(): void {
    const menu = this.settings.menu;
    const previous = this.menuLease;
    if (this.closed || menu?.visible !== true) {
      this.menuLease = undefined;
      if (previous) {
        this.retireMenu(previous);
      }
      return;
    }
    if (previous?.generation === menu.generation) {
      previous.content.update({ entry: this, menu: menu });
      return;
    }
    if (previous) {
      this.retireMenu(previous);
    }
    const ui = this.ui;
    const frame = this.node.getFrameNode();
    if (!frame) {
      return;
    }
    const content = new ComponentContent<MenuParameters>(
      ui,
      wrapBuilder(menuContent),
      { entry: this, menu: menu },
      { nestingBuilderSupported: true },
    );
    const lease: MenuLease = {
      content: content,
      generation: menu.generation,
      opening: Promise.resolve(),
      retired: false,
    };
    this.menuLease = lease;
    lease.opening = ui
      .getPromptAction()
      .openMenu(content, { id: frame.getUniqueId() }, this.menuOptions());
    lease.opening.catch((error: Error): void => {
      console.error(`[NSH] native menu open failed: ${String(error)}`);
      if (this.menuLease === lease) {
        this.menuLease = undefined;
        this.action("", "menu-dismiss", lease.generation);
      }
      this.retireMenu(lease);
    });
  }
  private retireMenu(lease: MenuLease): void {
    if (lease.retired) {
      return;
    }
    lease.retired = true;
    this.menus.retire(
      lease,
      lease.opening,
      (): Promise<void> => this.ui.getPromptAction().closeMenu(lease.content),
      (): void => lease.content.dispose(),
    );
  }

  postMouseTouch(event: TouchEvent): void {
    const pointer = event.changedTouches[0]?.id;
    const down = event.type === TouchType.Down;
    const terminal = event.type === TouchType.Up || event.type === TouchType.Cancel;
    const owner = pointer === undefined ? undefined : this.pointerPosting.owners.get(pointer);
    const target = this.sourceNode;
    if (
      this.closed ||
      this.pointerPosting.busy ||
      !target ||
      !this.context.isActive() ||
      event.source !== SourceType.Mouse ||
      pointer === undefined ||
      (down ? owner !== undefined : owner?.owner !== String(this.settings.targetId))
    ) {
      return;
    }
    const offset = target.getFrameNode()?.getPositionToWindow();
    if (!offset) {
      return;
    }
    const coordinates = new TouchCoordinates(event);
    this.pointerPosting.busy = true;
    try {
      coordinates.apply(this.ui, offset.x, offset.y);
      const accepted = target.postInputEvent(event);
      if (accepted) {
        if (!terminal) {
          const snapshot = new TouchCancel(event);
          if (owner) {
            owner.snapshot = snapshot;
          } else {
            this.pointerPosting.owners.set(pointer, {
              owner: String(this.settings.targetId),
              snapshot: snapshot,
            });
          }
        }
        event.stopPropagation();
      } else if (down && !this.closed && this.context.isActive()) {
        // Even a miss records DOWN in ArkUI. Close that attempt before another export tries.
        coordinates.cancel();
        target.postInputEvent(event);
      }
    } finally {
      if (terminal) {
        this.pointerPosting.owners.delete(pointer);
      }
      this.pointerPosting.finish(coordinates);
    }
  }
  menuOptions(): MenuOptions {
    const menu = this.settings.menu;
    return {
      placement: Placement.TopLeft,
      offset: { x: menu?.x ?? 0, y: -(menu?.y ?? 0) },
      showInSubWindow: false,
      onDisappear: (): void => {
        // Every popup retains its own generation, including during replacement.
        const generation = menu?.generation;
        if (generation !== undefined) {
          this.action("", "menu-dismiss", generation);
        }
      },
    };
  }
  action(name: string, kind?: string, generation?: number): boolean {
    if (
      this.closed ||
      !this.context.isActive() ||
      (kind !== undefined && generation !== this.settings.menu?.generation)
    ) {
      return false;
    }
    return this.context.viewAction({
      targetId: this.settings.targetId,
      action: name,
      kind: kind,
      generation: generation,
    });
  }
  dispose(cleanup: NshRuntimeContext = this.context): void {
    if (this.closed && !this.retirementFailure) { return; }
    this.closed = true;
    this.retirementFailure = undefined;
    try {
      if (this.menuLease) {
        this.retireMenu(this.menuLease);
        this.menuLease = undefined;
      }
      this.retirement.detachThenDispose(
        (): void => {
          if (!cleanup.retireNativeNode(this.settings.targetId)) {
            throw new Error("Native wrapper retirement was rejected");
          }
        },
        (): void => this.disposeBuilders(),
        this.pointerPosting,
      );
    } catch (error) {
      quarantinedViews.add(this);
      this.retirementFailure = new Error(`Native wrapper retirement failed: ${String(error)}`);
      throw this.retirementFailure;
    }
  }
  private disposeBuilders(): void {
    if (this.sdkBusy) { this.disposePending = true; return; }
    try {
      this.pointerPosting.owners.forEach((stream, pointer: number): void => {
        if (stream.owner !== String(this.settings.targetId)) {
          return;
        }
        this.pointerPosting.owners.delete(pointer);
        if (this.sourceNode?.getFrameNode()) {
          this.pointerPosting.busy = true;
          try {
            this.sourceNode.postInputEvent(stream.snapshot as TouchEvent);
          } finally {
            this.pointerPosting.finish();
          }
        }
      });
      this.node.dispose();
      this.textureNode?.dispose();
      this.sourceNode?.dispose();
      quarantinedViews.delete(this);
    } catch (error) {
      quarantinedViews.add(this);
      console.error(`[NSH] wrapper disposal failed: ${String(error)}`);
    }
  }
}
export class NativeViews {
  private entries: Map<number, ViewEntry> = new Map();
  private menus: NativeMenuCleanup = new NativeMenuCleanup();
  private pointerPosting: PointerPosting = new PointerPosting();
  remove(ids: number[], context: NshRuntimeContext): void {
    for (const id of ids) { this.entries.get(id)?.dispose(context); this.entries.delete(id); }
  }
  update(states: Settings[], context: NshRuntimeContext): void {
    for (const state of states) {
      if (!context.isActive()) { throw new Error("NSH SDK session is inactive"); }
      if (state.api26 && deviceInfo.sdkApiVersion < 26 || state.api23 && deviceInfo.sdkApiVersion < 23) {
        throw new Error("NSH semantic feature requires a newer API level");
      }
      let entry = this.entries.get(state.targetId);
      if (entry && entry.settings.textureSurface !== state.textureSurface) {
        throw new Error("Texture replacement requires a fresh native target");
      }
      if (entry) { entry.update(state); }
      else {
        entry = new ViewEntry(state, context, this.menus, this.pointerPosting);
        this.entries.set(state.targetId, entry);
        entry.create();
      }
    }
  }
  async settled(): Promise<void> { await this.menus.settled(); }
  dispose(context: NshRuntimeContext): void {
    let failure: Error | undefined;
    for (const id of Array.from(this.entries.keys())) {
      const entry = this.entries.get(id)!;
      try { entry.dispose(context); this.entries.delete(id); }
      catch (error) { failure ??= new Error(String(error)); }
    }
    if (failure) { throw failure; }
  }
}
