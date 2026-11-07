import { BuilderNode } from "@ohos.arkui.node";
import webview from "@ohos.web.webview";
import { NshRuntimeContext } from "./NativeEditors";

export interface WebSettings {
  targetId: number;
  url?: string | null;
  html?: string | null;
  htmlBaseUrl?: string | null;
  loadGeneration: number;
  javaScriptEnabled: boolean;
  scrollEnabled: boolean;
  userAgent?: string | null;
  injectedJavascript?: string | null;
  documentStartScript: string;
}

export interface WebCommand {
  targetId: number;
  kind: string;
  script?: string | null;
}

let webEngineInitialized = false;
let nextWebNativeTagId = 1;

function ensureWebEngineInitialized(): void {
  if (!webEngineInitialized) {
    webview.WebviewController.initializeWebEngine();
    webEngineInitialized = true;
  }
}

@Builder
function webContent(entry: WebEntry) {
  Web({
    src: "",
    controller: entry.controller,
    renderMode: RenderMode.SYNC_RENDER,
  })
    .id(`nsh-web-${entry.settings.targetId}`)
    .width("100%")
    .height("100%")
    .backgroundColor(Color.Transparent)
    .hitTestBehavior(HitTestMode.Block)
    .javaScriptAccess(entry.settings.javaScriptEnabled)
    .domStorageAccess(true)
    .fileAccess(false)
    .onlineImageAccess(true)
    .imageAccess(true)
    .runJavaScriptOnDocumentStart([
      {
        script: entry.settings.documentStartScript,
        scriptRules: ["*", "nsh-asset://"],
      },
    ])
    .onControllerAttached(() => {
      entry.onControllerAttached();
    })
    .onLoadIntercept((event): boolean => {
      return entry.onLoadIntercept(event.data.getRequestUrl());
    })
    .onPageBegin(() => {
      entry.onPageBegin();
    })
    .onPageEnd(() => {
      entry.onPageEnd();
    })
    .onTitleReceive(() => {
      entry.emit("title");
    })
    .onFocus(() => {
      entry.emit("focus");
    })
    .onBlur(() => {
      entry.emit("blur");
    })
    .onErrorReceive((event) => {
      if (event.request.isMainFrame()) {
        entry.emit("error", event.error.getErrorInfo(), event.error.getErrorCode());
      }
    });
}

const quarantinedWebs: Set<WebEntry> = new Set();

class WebEntry {
  readonly nativeTag: string = `nsh.web.${nextWebNativeTagId++}`;
  readonly controller: webview.WebviewController = new webview.WebviewController(this.nativeTag);
  readonly node: BuilderNode<[WebEntry]>;
  settings: WebSettings;
  private context: NshRuntimeContext;
  private attached: boolean = false;
  private controllerObserved: boolean = false;
  private controllerReady: boolean = false;
  private appliedLoadGeneration: number = 0;
  private pendingCommands: WebCommand[] = [];
  private loading: boolean = false;
  private epoch: number = 0;
  private sdkBusy: boolean = false;
  private disposePending: boolean = false;
  private closed: boolean = false;
  private retired: boolean = false;
  private disposed: boolean = false;

  constructor(settings: WebSettings, context: NshRuntimeContext) {
    this.settings = settings;
    this.context = context;
    ensureWebEngineInitialized();
    this.node = new BuilderNode<[WebEntry]>(context.getUIContext());
  }

  create(): void {
    this.sdkBusy = true;
    try {
      this.node.build(wrapBuilder(webContent), this);
      if (this.closed || !this.context.isActive()) {
        throw new Error("WebView creation was cancelled");
      }
      const accepted = this.context.attachNativeNode(
        this.settings.targetId,
        this.node.getFrameNode(),
      );
      if (!accepted || this.closed) {
        throw new Error("Native WebView target is no longer live");
      }
      this.attached = true;
      if (this.controllerObserved && !this.controllerReady) {
        this.finishControllerAttach();
      }
    } catch (error) {
      this.dispose();
      throw new Error(String(error));
    } finally {
      this.sdkBusy = false;
      if (this.disposePending) {
        this.disposePending = false;
        this.release();
      }
    }
  }

  onControllerAttached(): void {
    this.controllerObserved = true;
    if (!this.attached || this.controllerReady || this.closed || !this.context.isActive()) {
      return;
    }
    this.finishControllerAttach();
  }

  private finishControllerAttach(): void {
    if (this.controllerReady || this.closed || !this.context.isActive()) {
      return;
    }
    try {
      this.controller.setScrollable(this.settings.scrollEnabled);
      this.controller.setCustomUserAgent(this.settings.userAgent ?? "");
      if (!this.context.webControllerAttached(this.settings.targetId, this.nativeTag)) {
        throw new Error("Native WebView controller attachment was rejected");
      }
      this.controllerReady = true;
      this.applySource();
      const queued = this.pendingCommands.splice(0);
      for (const cmd of queued) {
        this.command(cmd);
      }
    } catch (error) {
      this.emit("error", String(error));
    }
  }

  update(settings: WebSettings): void {
    if (this.closed) {
      throw new Error("Native WebView is retired");
    }
    if (settings.targetId !== this.settings.targetId) {
      throw new Error("WebView replacement requires a new target");
    }
    this.settings = settings;
    if (!this.controllerReady) {
      return;
    }
    try {
      this.controller.setScrollable(settings.scrollEnabled);
      this.controller.setCustomUserAgent(settings.userAgent ?? "");
      if (settings.loadGeneration !== this.appliedLoadGeneration) {
        this.applySource();
      }
    } catch (error) {
      this.emit("error", String(error));
    }
  }

  private applySource(): void {
    this.appliedLoadGeneration = this.settings.loadGeneration;
    if (this.settings.url != null) {
      this.controller.loadUrl(this.settings.url);
    } else {
      this.controller.loadData(
        this.settings.html ?? "",
        "text/html",
        "UTF-8",
        this.settings.htmlBaseUrl ?? "about:blank",
        " ",
      );
    }
  }

  onLoadIntercept(url: string): boolean {
    if (this.closed || !this.context.isActive()) {
      return true;
    }
    return this.context.webNavigate(this.settings.targetId, url);
  }

  onPageBegin(): void {
    this.loading = true;
    this.epoch++;
    this.emit("start");
  }

  onPageEnd(): void {
    if (this.closed || !this.context.isActive()) {
      return;
    }
    const epoch = this.epoch;
    const script = this.settings.injectedJavascript;
    if (script && this.settings.javaScriptEnabled) {
      this.controller.runJavaScript(script).then(
        () => {
          if (!this.closed && epoch === this.epoch) {
            this.loading = false;
            this.emit("finish");
          }
        },
        (error: Error) => {
          if (!this.closed && epoch === this.epoch) {
            this.loading = false;
            this.emit("error", String(error));
          }
        },
      );
    } else {
      this.loading = false;
      this.emit("finish");
    }
  }

  emit(phase: string, message?: string, code?: number): void {
    if (!this.attached || this.closed || !this.context.isActive()) {
      return;
    }
    if (phase === "error") {
      this.loading = false;
    }
    let url = "";
    let title = "";
    let canGoBack = false;
    let canGoForward = false;
    if (this.controllerReady) {
      try {
        url = this.controller.getUrl();
        title = this.controller.getTitle();
        canGoBack = this.controller.accessBackward();
        canGoForward = this.controller.accessForward();
      } catch {}
    }
    this.context.webEvent({
      targetId: this.settings.targetId,
      phase,
      url,
      title,
      loading: this.loading,
      canGoBack,
      canGoForward,
      message,
      code,
    });
  }

  command(command: WebCommand): void {
    if (this.closed || !this.context.isActive()) {
      return;
    }
    if (!this.controllerReady) {
      this.pendingCommands.push(command);
      return;
    }
    try {
      if (command.kind === "reload") {
        this.controller.refresh();
      } else if (command.kind === "go-back") {
        if (this.controller.accessBackward()) {
          this.controller.backward();
        }
      } else if (command.kind === "go-forward") {
        if (this.controller.accessForward()) {
          this.controller.forward();
        }
      } else if (command.kind === "stop-loading") {
        this.controller.stop();
      } else if (command.kind === "focus") {
        this.context
          .getUIContext()
          .getFocusController()
          .requestFocus(`nsh-web-${this.settings.targetId}`);
      } else if (command.kind === "evaluate") {
        if (command.script != null) {
          this.controller.runJavaScript(command.script).catch((error: Error) => {
            this.emit("error", String(error));
          });
        }
      }
    } catch (error) {
      this.emit("error", String(error));
    }
  }

  private release(): void {
    try {
      try {
        this.controller.stop();
        this.controller.loadUrl("about:blank");
      } catch (_error) {}
      this.node.dispose();
      this.disposed = true;
      quarantinedWebs.delete(this);
    } catch (error) {
      quarantinedWebs.add(this);
      throw new Error(String(error));
    }
  }

  dispose(cleanup: NshRuntimeContext = this.context): void {
    if (this.disposed) {
      return;
    }
    this.closed = true;
    this.epoch++;
    this.pendingCommands = [];
    try {
      if (!this.retired) {
        if (!cleanup.retireNativeNode(this.settings.targetId)) {
          throw new Error("Native WebView retirement was rejected");
        }
        this.retired = true;
      }
      if (this.sdkBusy) {
        this.disposePending = true;
      } else {
        this.release();
      }
    } catch (error) {
      quarantinedWebs.add(this);
      throw new Error(`Native WebView retirement failed: ${String(error)}`);
    }
  }
}

export class NativeWebViews {
  private entries: Map<number, WebEntry> = new Map();

  remove(ids: number[], context: NshRuntimeContext): void {
    for (const id of ids) {
      this.entries.get(id)?.dispose(context);
      this.entries.delete(id);
    }
  }

  update(states: WebSettings[], commands: WebCommand[], context: NshRuntimeContext): void {
    for (const state of states) {
      if (!context.isActive()) {
        throw new Error("NSH SDK session is inactive");
      }
      let entry = this.entries.get(state.targetId);
      if (entry) {
        entry.update(state);
      } else {
        entry = new WebEntry(state, context);
        this.entries.set(state.targetId, entry);
        entry.create();
      }
    }
    for (const command of commands) {
      this.entries.get(command.targetId)?.command(command);
    }
  }

  dispose(context: NshRuntimeContext): void {
    let failure: Error | undefined;
    for (const id of Array.from(this.entries.keys())) {
      const entry = this.entries.get(id)!;
      try {
        entry.dispose(context);
        this.entries.delete(id);
      } catch (error) {
        failure ??= new Error(String(error));
      }
    }
    if (failure) {
      throw failure;
    }
  }
}
