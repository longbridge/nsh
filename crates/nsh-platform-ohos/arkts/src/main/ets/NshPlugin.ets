import { NativeEditors, EditorSettings, EditorCommand, NshRuntimeContext } from "./NativeEditors";
import { NativeViews, Settings } from "./NativeViews";
import { NativeWebViews, WebSettings, WebCommand } from "./NativeWebViews";

export interface Batch {
  editors: EditorSettings[];
  commands: EditorCommand[];
  removeEditors: number[];
  views: Settings[];
  removeViews: number[];
  webs: WebSettings[];
  webCommands: WebCommand[];
  removeWebs: number[];
}

/** Instance-owned SDK fragments (editors, menus/textures, WebViews) for one UIContext attachment. */
export class NshPlugin {
  private editors: NativeEditors = new NativeEditors();
  private views: NativeViews = new NativeViews();
  private webs: NativeWebViews = new NativeWebViews();

  apply(batch: Batch, context: NshRuntimeContext): void {
    if (!context.isActive()) {
      throw new Error("NSH SDK session is inactive");
    }
    context.getUIContext();
    this.editors.remove(batch.removeEditors, context);
    this.views.remove(batch.removeViews, context);
    this.webs.remove(batch.removeWebs, context);
    this.editors.update(batch.editors, batch.commands, context);
    this.views.update(batch.views, context);
    this.webs.update(batch.webs, batch.webCommands, context);
  }

  dispose(context: NshRuntimeContext): void {
    try {
      this.webs.dispose(context);
    } finally {
      try {
        this.views.dispose(context);
      } finally {
        this.editors.dispose(context);
      }
    }
  }

  async settled(): Promise<void> {
    await this.views.settled();
  }
}
