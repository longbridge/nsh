import { NshSurface } from "@nsh/ohos";

@Entry
@Component
struct Index {
  private surface: NshSurface = new NshSurface(this.getUIContext());

  aboutToAppear(): void {
    this.surface.loadResource("bundle.js");
  }

  aboutToDisappear(): void {
    this.surface.dispose();
  }

  onBackPress(): boolean {
    return this.surface.onBackPress();
  }

  build() {
    Column() {
      ContentSlot(this.surface.content)
    }
    .width("100%")
    .height("100%")
  }
}
