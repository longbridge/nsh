import { ComponentContent, UIContext } from "@kit.ArkUI";
import { NshNativeProperty, NshSurface } from "@nsh/ohos";

class BadgeParams {
  text: string;
  color: string;
  onTap: () => void;
  opacity: number;
  constructor(text: string, color: string, onTap: () => void, opacity: number = 1) {
    this.text = text;
    this.color = color;
    this.onTap = onTap;
    this.opacity = opacity;
  }
}

class TapData {
  count: number;
  constructor(count: number) {
    this.count = count;
  }
}

@Builder
function badge(params: BadgeParams) {
  Text(params.text)
    .fontSize(16)
    .fontColor("#ffffff")
    .backgroundColor(params.color)
    .padding(10)
    .width("100%")
    .height("100%")
    .opacity(params.opacity)
    .id("host-badge")
    .onClick(params.onTap);
}

/** A host-owned ArkUI widget implementing the same badge contract as iOS/Android. */
export class HostBadge {
  node: ComponentContent<BadgeParams>;
  private text: string = "Native badge";
  private color: string = "#8250df";
  private taps: number = 0;
  private onTap: () => void;
  private readonly context: UIContext;

  constructor(context: UIContext, handle: number, surface: NshSurface) {
    this.context = context;
    this.onTap = () => {
      this.taps++;
      surface.emitComponentEvent(handle, "tap", new TapData(this.taps));
    };
    this.node = new ComponentContent<BadgeParams>(
      context,
      wrapBuilder(badge),
      new BadgeParams(this.text, this.color, this.onTap),
    );
    surface.setComponentSize(handle, 240, 44);
  }

  updateProps(changes: NshNativeProperty[]): void {
    for (const change of changes) {
      if (change.name === "text") {
        this.text = change.removed ? "" : (change.value as string);
      }
      if (change.name === "color") {
        this.color = change.removed ? "#8250df" : (change.value as string);
      }
    }
    this.node.update(new BadgeParams(this.text, this.color, this.onTap));
  }

  command(name: string): void {
    if (name === "pulse" && !this.node.isDisposed()) {
      this.node.update(new BadgeParams(this.text, this.color, this.onTap, 0.3));
      this.context.animateTo({ duration: 600 }, () => {
        this.node.update(new BadgeParams(this.text, this.color, this.onTap));
      });
    }
  }

  dispose(): void {
    if (!this.node.isDisposed()) {
      this.node.dispose();
    }
  }
}
