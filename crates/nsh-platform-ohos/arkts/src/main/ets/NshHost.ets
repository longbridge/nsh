import {
  AbilityConstant,
  AbilityLifecycleCallback,
  ApplicationStateChangeCallback,
  Configuration,
  EnvironmentCallback,
  UIAbility,
  common,
} from "@kit.AbilityKit";
import { accessibility } from "@kit.AccessibilityKit";
import { NodeContent, window } from "@kit.ArkUI";
import { fileIo as fs } from "@kit.CoreFileKit";
import { photoAccessHelper } from "@kit.MediaLibraryKit";
import { FrameNode } from "@ohos.arkui.node";
import { UIContext } from "@ohos.arkui.UIContext";
import deviceInfo from "@ohos.deviceInfo";
import {
  EditorEventPayload,
  EditorReceipt,
  MenuActionPayload,
  NshRuntimeContext,
  WebEventPayload,
} from "./NativeEditors";
import { Batch, NshPlugin } from "./NshPlugin";

interface AvoidRectPayload {
  left: number;
  top: number;
  width: number;
  height: number;
}

interface AvoidAreaPayload {
  visible: boolean;
  leftRect: AvoidRectPayload;
  topRect: AvoidRectPayload;
  rightRect: AvoidRectPayload;
  bottomRect: AvoidRectPayload;
}

interface HostInitPayload {
  sdkApiVersion: number;
  deviceType: string;
  cacheDir: string;
  colorMode: number;
  fontSizeScale: number;
  language: string;
  screenReader: boolean;
  reduceMotion?: boolean;
}

interface HostCommandPayload {
  kind: string;
  enabled?: boolean;
  lightContent?: boolean;
  animated?: boolean;
  text?: string;
  assertive?: boolean;
  announcement?: string;
  elementId?: number;
  openId?: number;
  limit?: number;
  url?: string;
}

interface NshNativeModule {
  nshRegisterSessionBinder(sdkApiVersion: number, binder: (uiContext: UIContext) => number): void;
  nshFindSession(windowName: string): number;
  nshCreateSession(
    init: HostInitPayload,
    windowName: string,
    platformWindowId: number,
    resourceManager: object,
    applyBatch: (batch: Batch) => void,
    onCommand: (command: HostCommandPayload) => void,
  ): number;
  nshCloseSession(sessionId: number): void;
  nshSetForeground(sessionId: number, foreground: boolean): void;
  nshUpdateConfig(
    sessionId: number,
    colorMode: number,
    fontSizeScale: number,
    language: string,
    deviceType?: string,
  ): void;
  nshUpdateAccessibility(sessionId: number, screenReader?: boolean, reduceMotion?: boolean): void;
  nshUpdateWindow(
    sessionId: number,
    platformWindowId: number,
    visual: AvoidAreaPayload[],
    keyboard: AvoidAreaPayload,
  ): void;
  nshOpenUrlResult(sessionId: number, openId: number, opened: boolean): void;
  nshPickImagesResult(sessionId: number, pickId: number, paths: string[]): void;
  nshReportHostError(sessionId: number, message: string): void;
  nshAttachNativeNode(targetId: number, content: FrameNode): void;
  nshAttachNativeSubtree(targetId: number, content: FrameNode, slot: NodeContent): void;
  nshRetireNativeNode(targetId: number): void;
  nshEditorEvent(targetId: number, event: EditorEventPayload): EditorReceipt;
  nshViewAction(action: MenuActionPayload): boolean;
  nshWebControllerAttached(targetId: number, nativeTag: string): boolean;
  nshWebNavigate(targetId: number, url: string): boolean;
  nshWebEvent(event: WebEventPayload): boolean;
  onBackPressIntercept(uiContext?: UIContext): boolean;
}

let cachedNativeModule: NshNativeModule | undefined = undefined;

function getNativeModule(): NshNativeModule {
  if (cachedNativeModule !== undefined) {
    return cachedNativeModule;
  }
  const libraryName = "libnsh_platform_ohos.so";
  let loaded: ESObject;
  try {
    loaded = loadNativeModule(libraryName);
  } catch (error) {
    throw new Error(`${libraryName} failed to load: ${String(error)}`);
  }
  const candidate: ESObject =
    typeof loaded?.nshRegisterSessionBinder === "function" ? loaded : loaded?.default;
  const mod = candidate as NshNativeModule;
  if (!mod || typeof mod.nshRegisterSessionBinder !== "function") {
    throw new Error(`${libraryName} does not export the Native Shell OHOS host ABI`);
  }
  cachedNativeModule = mod;
  return mod;
}

function toAvoidRect(rect: window.Rect): AvoidRectPayload {
  return {
    left: rect.left,
    top: rect.top,
    width: rect.width,
    height: rect.height,
  };
}

function toAvoidArea(area: window.AvoidArea): AvoidAreaPayload {
  return {
    visible: area.visible,
    leftRect: toAvoidRect(area.leftRect),
    topRect: toAvoidRect(area.topRect),
    rightRect: toAvoidRect(area.rightRect),
    bottomRect: toAvoidRect(area.bottomRect),
  };
}

function emptyAvoidArea(): AvoidAreaPayload {
  const zero: AvoidRectPayload = { left: 0, top: 0, width: 0, height: 0 };
  return {
    visible: false,
    leftRect: zero,
    topRect: zero,
    rightRect: zero,
    bottomRect: zero,
  };
}

function safeAvoidArea(win: window.Window, type: window.AvoidAreaType): AvoidAreaPayload {
  try {
    return toAvoidArea(win.getWindowAvoidArea(type));
  } catch {
    return emptyAvoidArea();
  }
}

class ComponentRuntimeContext implements NshRuntimeContext {
  private readonly ui: UIContext;
  private readonly mod: NshNativeModule;
  private active: boolean = true;

  constructor(ui: UIContext, mod: NshNativeModule) {
    this.ui = ui;
    this.mod = mod;
  }

  deactivate(): void {
    this.active = false;
  }

  isActive(): boolean {
    return this.active;
  }

  getUIContext(): UIContext {
    return this.ui;
  }

  attachNativeNode(targetId: number, content: FrameNode | null): boolean {
    if (!this.active || content === null) {
      return false;
    }
    try {
      this.mod.nshAttachNativeNode(targetId, content);
      return true;
    } catch (error) {
      console.error(`[NSH] attachNativeNode(${targetId}) failed: ${String(error)}`);
      return false;
    }
  }

  attachNativeSubtree(targetId: number, content: FrameNode | null, slot: NodeContent): boolean {
    if (!this.active || content === null) {
      return false;
    }
    try {
      this.mod.nshAttachNativeSubtree(targetId, content, slot);
      return true;
    } catch (error) {
      console.error(`[NSH] attachNativeSubtree(${targetId}) failed: ${String(error)}`);
      return false;
    }
  }

  retireNativeNode(targetId: number): boolean {
    try {
      this.mod.nshRetireNativeNode(targetId);
      return true;
    } catch (error) {
      console.error(`[NSH] retireNativeNode(${targetId}) failed: ${String(error)}`);
      return false;
    }
  }

  editorEvent(targetId: number, event: EditorEventPayload): EditorReceipt {
    if (!this.active) {
      return { accepted: false, count: 0 };
    }
    try {
      return this.mod.nshEditorEvent(targetId, event);
    } catch {
      return { accepted: false, count: 0 };
    }
  }

  viewAction(action: MenuActionPayload): boolean {
    if (!this.active) {
      return false;
    }
    try {
      return this.mod.nshViewAction(action);
    } catch {
      return false;
    }
  }

  webControllerAttached(targetId: number, nativeTag: string): boolean {
    if (!this.active) {
      return false;
    }
    try {
      return this.mod.nshWebControllerAttached(targetId, nativeTag);
    } catch {
      return false;
    }
  }

  webNavigate(targetId: number, url: string): boolean {
    if (!this.active) {
      return true;
    }
    try {
      return this.mod.nshWebNavigate(targetId, url);
    } catch {
      return true;
    }
  }

  webEvent(event: WebEventPayload): boolean {
    if (!this.active) {
      return false;
    }
    try {
      return this.mod.nshWebEvent(event);
    } catch {
      return false;
    }
  }
}

class HostEnvironmentListener extends EnvironmentCallback {
  private readonly owner: HostSession;
  constructor(owner: HostSession) {
    super();
    this.owner = owner;
  }
  onConfigurationUpdated(config: Configuration): void {
    this.owner.onConfigurationUpdated(config);
  }
  onMemoryLevel(_level: AbilityConstant.MemoryLevel): void {}
}

class HostStateChangeListener extends ApplicationStateChangeCallback {
  private readonly owner: HostSession;
  constructor(owner: HostSession) {
    super();
    this.owner = owner;
  }
  onApplicationForeground(): void {
    this.owner.onForegroundChanged(true);
  }
  onApplicationBackground(): void {
    this.owner.onForegroundChanged(false);
  }
}

class HostLifecycleListener extends AbilityLifecycleCallback {
  private readonly owner: HostSession;
  constructor(owner: HostSession) {
    super();
    this.owner = owner;
  }
  onAbilityCreate(_ability: UIAbility): void {}
  onWindowStageCreate(_ability: UIAbility, _windowStage: window.WindowStage): void {}
  onWindowStageActive(ability: UIAbility, windowStage: window.WindowStage): void {
    if (this.owner.matchesWindowStage(ability.context, windowStage)) {
      this.owner.onForegroundChanged(true);
    }
  }
  onWindowStageInactive(_ability: UIAbility, _windowStage: window.WindowStage): void {}
  onWindowStageDestroy(ability: UIAbility, windowStage: window.WindowStage): void {
    if (this.owner.matchesWindowStage(ability.context, windowStage)) {
      this.owner.close();
    }
  }
  onAbilityDestroy(ability: UIAbility): void {
    if (this.owner.matchesAbility(ability.context)) {
      this.owner.close();
    }
  }
  onAbilityForeground(ability: UIAbility): void {
    if (this.owner.matchesAbility(ability.context)) {
      this.owner.onForegroundChanged(true);
    }
  }
  onAbilityBackground(ability: UIAbility): void {
    if (this.owner.matchesAbility(ability.context)) {
      this.owner.onForegroundChanged(false);
    }
  }
  onAbilityContinue(_ability: UIAbility): void {}
}

class HostSession {
  readonly sessionId: number;
  private readonly mod: NshNativeModule;
  private readonly abilityContext: common.UIAbilityContext;
  private readonly appContext?: common.ApplicationContext;
  private readonly windowStage?: window.WindowStage;
  private readonly mainWindow: window.Window;
  private readonly windowId: number;
  private readonly runtimeContext: ComponentRuntimeContext;
  private readonly plugin: NshPlugin = new NshPlugin();
  private closed: boolean = false;
  private envCallbackId?: number;
  private lifecycleCallbackId?: number;
  private stateListener?: HostStateChangeListener;
  private screenReaderHandler?: (enabled: boolean) => void;
  private reduceMotionHandler?: (enabled: boolean) => void;

  private readonly onWindowStageEvent = (event: window.WindowStageEventType): void => {
    if (this.closed) {
      return;
    }
    if (
      event === window.WindowStageEventType.SHOWN ||
      event === window.WindowStageEventType.ACTIVE ||
      event === window.WindowStageEventType.RESUMED
    ) {
      this.onForegroundChanged(true);
    } else if (
      event === window.WindowStageEventType.INACTIVE ||
      event === window.WindowStageEventType.PAUSED ||
      event === window.WindowStageEventType.HIDDEN
    ) {
      this.onForegroundChanged(false);
    }
  };

  private readonly onAvoidAreaChange = (_data: window.AvoidAreaOptions): void => {
    this.syncWindowMetrics();
  };

  private readonly onWindowSizeChange = (_size: window.Size): void => {
    this.syncWindowMetrics();
  };

  private readonly onKeyboardHeightChange = (_height: number): void => {
    this.syncWindowMetrics();
  };

  private constructor(
    mod: NshNativeModule,
    uiContext: UIContext,
    abilityContext: common.UIAbilityContext,
    mainWindow: window.Window,
    windowName: string,
  ) {
    this.mod = mod;
    this.abilityContext = abilityContext;
    this.windowStage = abilityContext.windowStage;
    this.mainWindow = mainWindow;
    try {
      this.appContext = abilityContext.getApplicationContext();
    } catch {}

    let windowId = 0;
    try {
      windowId = mainWindow.getWindowProperties().id;
    } catch {}
    this.windowId = windowId;

    this.runtimeContext = new ComponentRuntimeContext(uiContext, mod);

    let screenReader = false;
    try {
      screenReader = accessibility.isScreenReaderOpenSync();
    } catch {}

    let reduceMotion: boolean | undefined = undefined;
    if (deviceInfo.sdkApiVersion >= 23) {
      try {
        reduceMotion = accessibility.isAnimationReduceEnabledSync();
      } catch {}
    }

    const config = abilityContext.config;
    this.sessionId = mod.nshCreateSession(
      {
        sdkApiVersion: deviceInfo.sdkApiVersion,
        deviceType: deviceInfo.deviceType ?? "phone",
        cacheDir: abilityContext.cacheDir ?? "",
        colorMode: config?.colorMode ?? -1,
        fontSizeScale: config?.fontSizeScale ?? 1.0,
        language: config?.language ?? "",
        screenReader,
        reduceMotion,
      },
      windowName,
      windowId,
      abilityContext.resourceManager,
      (batch: Batch): void => {
        if (!this.closed) {
          this.plugin.apply(batch, this.runtimeContext);
        }
      },
      (command: HostCommandPayload): void => {
        this.handleHostCommand(command);
      },
    );

    this.bindWindowListeners();
    this.subscribeLifecycleListeners();
    this.subscribeAccessibility();
    this.syncWindowMetrics();
  }

  static bind(uiContext: UIContext): number {
    const mod = getNativeModule();
    if (!uiContext || typeof uiContext.isAvailable !== "function" || !uiContext.isAvailable()) {
      throw new Error("NshSurface requires an available UIContext");
    }
    const windowName = uiContext.getWindowName() ?? "";
    if (!windowName) {
      throw new Error("OHOS UIContext does not have an associated window name");
    }
    const existing = mod.nshFindSession(windowName);
    if (existing > 0) {
      return existing;
    }
    const abilityContext = uiContext.getHostContext() as common.UIAbilityContext | undefined;
    if (
      !abilityContext ||
      typeof abilityContext.cacheDir !== "string" ||
      !abilityContext.resourceManager
    ) {
      throw new Error("OHOS UIContext is not bound to a UIAbilityContext");
    }
    let mainWindow: window.Window | undefined = undefined;
    try {
      mainWindow = window.findWindow(windowName);
    } catch {}
    if (!mainWindow && abilityContext.windowStage !== undefined) {
      try {
        const candidate = abilityContext.windowStage.getMainWindowSync();
        if (candidate.getUIContext().getWindowName() === windowName) {
          mainWindow = candidate;
        }
      } catch {}
    }
    if (!mainWindow) {
      throw new Error(`OHOS window '${windowName}' could not be resolved from UIContext`);
    }
    const session = new HostSession(mod, uiContext, abilityContext, mainWindow, windowName);
    return session.sessionId;
  }

  matchesAbility(context: common.UIAbilityContext): boolean {
    return this.abilityContext === context;
  }

  matchesWindowStage(context: common.UIAbilityContext, stage: window.WindowStage): boolean {
    return (
      this.abilityContext === context &&
      (this.windowStage === undefined || this.windowStage === stage)
    );
  }

  onForegroundChanged(foreground: boolean): void {
    if (this.closed) {
      return;
    }
    try {
      this.mod.nshSetForeground(this.sessionId, foreground);
    } catch {}
  }

  onConfigurationUpdated(newConfig: Configuration): void {
    if (this.closed) {
      return;
    }
    try {
      this.mod.nshUpdateConfig(
        this.sessionId,
        newConfig.colorMode ?? -1,
        newConfig.fontSizeScale ?? 1.0,
        newConfig.language ?? "",
        deviceInfo.deviceType,
      );
    } catch {}
  }

  close(): void {
    if (this.closed) {
      return;
    }
    this.closed = true;

    if (this.windowStage !== undefined) {
      try {
        this.windowStage.off("windowStageEvent", this.onWindowStageEvent);
      } catch {}
    }

    try {
      this.mainWindow.off("avoidAreaChange", this.onAvoidAreaChange);
    } catch {}
    try {
      this.mainWindow.off("windowSizeChange", this.onWindowSizeChange);
    } catch {}
    try {
      this.mainWindow.off("keyboardHeightChange", this.onKeyboardHeightChange);
    } catch {}

    if (this.appContext !== undefined) {
      if (this.envCallbackId !== undefined) {
        try {
          this.appContext.off("environment", this.envCallbackId);
        } catch {}
        this.envCallbackId = undefined;
      }
      if (this.lifecycleCallbackId !== undefined) {
        try {
          this.appContext.off("abilityLifecycle", this.lifecycleCallbackId);
        } catch {}
        this.lifecycleCallbackId = undefined;
      }
      if (this.stateListener !== undefined) {
        try {
          this.appContext.off("applicationStateChange", this.stateListener);
        } catch {}
        this.stateListener = undefined;
      }
    }

    if (this.screenReaderHandler !== undefined) {
      try {
        accessibility.off("screenReaderStateChange", this.screenReaderHandler);
      } catch {}
      this.screenReaderHandler = undefined;
    }
    if (deviceInfo.sdkApiVersion >= 23 && this.reduceMotionHandler !== undefined) {
      try {
        accessibility.offAnimationReduceStateChange(this.reduceMotionHandler);
      } catch {}
      this.reduceMotionHandler = undefined;
    }

    try {
      this.plugin.dispose(this.runtimeContext);
    } catch {}
    this.runtimeContext.deactivate();
    void this.plugin.settled();

    try {
      this.mod.nshCloseSession(this.sessionId);
    } catch {}
  }

  private bindWindowListeners(): void {
    if (this.windowStage !== undefined) {
      try {
        this.windowStage.on("windowStageEvent", this.onWindowStageEvent);
      } catch {}
    }
    try {
      this.mainWindow.on("avoidAreaChange", this.onAvoidAreaChange);
    } catch {}
    try {
      this.mainWindow.on("windowSizeChange", this.onWindowSizeChange);
    } catch {}
    try {
      this.mainWindow.on("keyboardHeightChange", this.onKeyboardHeightChange);
    } catch {}
  }

  private subscribeLifecycleListeners(): void {
    const appCtx = this.appContext;
    if (!appCtx) {
      return;
    }
    try {
      this.envCallbackId = appCtx.on("environment", new HostEnvironmentListener(this));
    } catch {}
    try {
      this.lifecycleCallbackId = appCtx.on("abilityLifecycle", new HostLifecycleListener(this));
    } catch {}
    try {
      const listener = new HostStateChangeListener(this);
      appCtx.on("applicationStateChange", listener);
      this.stateListener = listener;
    } catch {}
  }

  private subscribeAccessibility(): void {
    const srHandler = (enabled: boolean): void => {
      if (!this.closed) {
        try {
          this.mod.nshUpdateAccessibility(this.sessionId, enabled, undefined);
        } catch {}
      }
    };
    try {
      accessibility.on("screenReaderStateChange", srHandler);
      this.screenReaderHandler = srHandler;
    } catch {}

    if (deviceInfo.sdkApiVersion >= 23) {
      const rmHandler = (enabled: boolean): void => {
        if (!this.closed) {
          try {
            this.mod.nshUpdateAccessibility(this.sessionId, undefined, enabled);
          } catch {}
        }
      };
      try {
        accessibility.onAnimationReduceStateChange(rmHandler);
        this.reduceMotionHandler = rmHandler;
      } catch {}
    }
  }

  private syncWindowMetrics(): void {
    if (this.closed) {
      return;
    }
    const visual: AvoidAreaPayload[] = [
      safeAvoidArea(this.mainWindow, window.AvoidAreaType.TYPE_SYSTEM),
      safeAvoidArea(this.mainWindow, window.AvoidAreaType.TYPE_CUTOUT),
      safeAvoidArea(this.mainWindow, window.AvoidAreaType.TYPE_NAVIGATION_INDICATOR),
    ];
    const keyboard = safeAvoidArea(this.mainWindow, window.AvoidAreaType.TYPE_KEYBOARD);
    try {
      this.mod.nshUpdateWindow(this.sessionId, this.windowId, visual, keyboard);
    } catch {}
  }

  private bundleName(): string {
    return (
      this.abilityContext.abilityInfo?.bundleName ??
      this.appContext?.applicationInfo?.name ??
      ""
    );
  }

  private handleHostCommand(command: HostCommandPayload): void {
    if (command.kind === "close") {
      this.close();
      return;
    }
    if (this.closed) {
      return;
    }

    if (command.kind === "video-fullscreen") {
      const enabled = command.enabled ?? false;
      for (const bar of ["status", "navigationIndicator"] as window.SpecificSystemBar[]) {
        this.mainWindow
          .setSpecificSystemBarEnabled(bar, !enabled)
          .catch((error: Error): void => {
            this.mod.nshReportHostError(
              this.sessionId,
              `OHOS video fullscreen bar update failed: ${String(error)}`,
            );
          });
      }
      return;
    }

    if (command.kind === "status-bar") {
      const enabled = command.enabled ?? true;
      const animated = command.animated ?? false;
      const lightContent = command.lightContent ?? false;
      this.mainWindow
        .setSpecificSystemBarEnabled("status", enabled, animated)
        .then((): Promise<void> => {
          return this.mainWindow.setWindowSystemBarProperties({
            statusBarContentColor: lightContent ? "#FFFFFFFF" : "#FF000000",
          });
        })
        .catch((error: Error): void => {
          this.mod.nshReportHostError(
            this.sessionId,
            `OHOS status bar update failed: ${String(error)}`,
          );
        });
      return;
    }

    if (command.kind === "announce" || command.kind === "accessibility-live") {
      const text = command.text ?? "";
      if (!text) {
        return;
      }
      accessibility
        .sendAccessibilityEvent({
          type: "announceForAccessibility",
          bundleName: this.bundleName(),
          triggerAction: "common",
          textAnnouncedForAccessibility: text,
        })
        .catch((): void => {});
      return;
    }

    if (command.kind === "accessibility-event") {
      const announcement = command.announcement ?? undefined;
      const elementId = command.elementId;
      accessibility
        .sendAccessibilityEvent({
          type: announcement ? "announceForAccessibility" : "requestFocusForAccessibility",
          bundleName: this.bundleName(),
          triggerAction: "common",
          textAnnouncedForAccessibility: announcement,
          customId: elementId !== undefined && elementId !== null ? String(elementId) : undefined,
        })
        .catch((): void => {});
      return;
    }

    if (command.kind === "open-url") {
      const openId = command.openId ?? 0;
      const url = command.url ?? "";
      if (!url || !/^[A-Za-z][A-Za-z0-9+.-]*:\S+$/.test(url)) {
        this.mod.nshOpenUrlResult(this.sessionId, openId, false);
        return;
      }
      this.abilityContext
        .openLink(url)
        .then((): void => {
          this.mod.nshOpenUrlResult(this.sessionId, openId, true);
        })
        .catch((): void => {
          this.abilityContext
            .startAbility({
              action: "ohos.want.action.viewData",
              uri: url,
            })
            .then((): void => {
              this.mod.nshOpenUrlResult(this.sessionId, openId, true);
            })
            .catch((): void => {
              this.mod.nshOpenUrlResult(this.sessionId, openId, false);
            });
        });
      return;
    }

    if (command.kind === "pick-images") {
      const pickId = command.openId ?? 0;
      const limit = Math.max(1, Math.min(100, command.limit ?? 1));
      try {
        const picker = new photoAccessHelper.PhotoViewPicker();
        const options = new photoAccessHelper.PhotoSelectOptions();
        options.MIMEType = photoAccessHelper.PhotoViewMIMETypes.IMAGE_TYPE;
        options.maxSelectNumber = limit;
        picker
          .select(options)
          .then((result: photoAccessHelper.PhotoSelectResult): void => {
            const uris = result?.photoUris ?? [];
            const paths: string[] = [];
            const cacheBase = this.abilityContext.cacheDir ?? "";
            for (let i = 0; i < uris.length && i < limit; i++) {
              const uri = uris[i];
              if (!uri) {
                continue;
              }
              let srcFile: fs.File | undefined = undefined;
              let dstFile: fs.File | undefined = undefined;
              try {
                srcFile = fs.openSync(uri, fs.OpenMode.READ_ONLY);
                const rawName = decodeURIComponent(uri.split("/").pop() ?? `image_${i}`);
                const safeName = rawName.replace(/[^A-Za-z0-9._-]/g, "_") || `image_${i}`;
                const dstPath = `${cacheBase}/nsh_pick_${this.sessionId}_${pickId}_${i}__${safeName}`;
                dstFile = fs.openSync(
                  dstPath,
                  fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC,
                );
                fs.copyFileSync(srcFile.fd, dstFile.fd);
                paths.push(dstPath);
              } catch {
              } finally {
                if (srcFile !== undefined) {
                  try {
                    fs.closeSync(srcFile);
                  } catch {}
                }
                if (dstFile !== undefined) {
                  try {
                    fs.closeSync(dstFile);
                  } catch {}
                }
              }
            }
            this.mod.nshPickImagesResult(this.sessionId, pickId, paths);
          })
          .catch((): void => {
            this.mod.nshPickImagesResult(this.sessionId, pickId, []);
          });
      } catch {
        this.mod.nshPickImagesResult(this.sessionId, pickId, []);
      }
    }
  }
}

getNativeModule().nshRegisterSessionBinder(
  deviceInfo.sdkApiVersion,
  (uiContext: UIContext): number => HostSession.bind(uiContext),
);
