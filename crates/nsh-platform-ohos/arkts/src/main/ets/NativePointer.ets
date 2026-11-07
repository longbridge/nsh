import type { UIContext } from "@ohos.arkui.UIContext";

class PointCoordinates {
  private point: TouchObject;
  private windowX: number;
  private windowY: number;
  private displayX: number;
  private displayY: number;
  private globalX?: number;
  private globalY?: number;
  private type: TouchType;
  constructor(point: TouchObject) {
    this.point = point;
    this.windowX = point.windowX;
    this.windowY = point.windowY;
    this.displayX = point.displayX;
    this.displayY = point.displayY;
    this.globalX = point.globalDisplayX;
    this.globalY = point.globalDisplayY;
    this.type = point.type;
  }
  apply(ui: UIContext, left: number, top: number): void {
    this.point.windowX = ui.vp2px(left + this.point.x);
    this.point.windowY = ui.vp2px(top + this.point.y);
    this.point.displayX = ui.vp2px(this.displayX);
    this.point.displayY = ui.vp2px(this.displayY);
    this.point.globalDisplayX = this.globalX === undefined ? undefined : ui.vp2px(this.globalX);
    this.point.globalDisplayY = this.globalY === undefined ? undefined : ui.vp2px(this.globalY);
  }
  cancel(): void {
    this.point.type = TouchType.Cancel;
  }
  restore(): void {
    this.point.windowX = this.windowX;
    this.point.windowY = this.windowY;
    this.point.displayX = this.displayX;
    this.point.displayY = this.displayY;
    this.point.globalDisplayX = this.globalX;
    this.point.globalDisplayY = this.globalY;
    this.point.type = this.type;
  }
}

// Keep the SDK object wrapped during synchronous dispatch so its native payload
// retains modifier keys. Restore this callback's conversion before bubbling.
export class TouchCoordinates {
  private points: PointCoordinates[];
  private event: TouchEvent;
  private type: TouchType;
  constructor(event: TouchEvent) {
    this.event = event;
    this.type = event.type;
    // A point can appear in both arrays. Capture every original value before applying any conversion.
    this.points = Array.from(new Set([...event.touches, ...event.changedTouches])).map(
      (point: TouchObject): PointCoordinates => new PointCoordinates(point),
    );
  }
  apply(ui: UIContext, left: number, top: number): void {
    for (const point of this.points) {
      point.apply(ui, left, top);
    }
  }
  cancel(): void {
    this.event.type = TouchType.Cancel;
    for (const point of this.points) {
      point.cancel();
    }
  }
  restore(): void {
    for (const point of this.points) {
      point.restore();
    }
    this.event.type = this.type;
  }
}

function cancelPoint(point: TouchObject): TouchObject {
  return {
    type: TouchType.Cancel,
    id: point.id,
    x: point.x,
    y: point.y,
    windowX: point.windowX,
    windowY: point.windowY,
    displayX: point.displayX,
    displayY: point.displayY,
    globalDisplayX: point.globalDisplayX,
    globalDisplayY: point.globalDisplayY,
    screenX: point.screenX,
    screenY: point.screenY,
    hand: point.hand,
    pressedTime: point.pressedTime,
    pressure: point.pressure,
    width: point.width,
    height: point.height,
  };
}

// Terminal cleanup owns plain data, never the callback's wrapped SDK event.
export class TouchCancel {
  type: TouchType = TouchType.Cancel;
  timestamp: number;
  source: SourceType;
  sourceTool: SourceTool;
  deviceId?: number;
  targetDisplayId?: number;
  touches: TouchObject[];
  changedTouches: TouchObject[];
  constructor(event: TouchEvent) {
    this.timestamp = event.timestamp;
    this.source = event.source;
    this.sourceTool = event.sourceTool;
    this.deviceId = event.deviceId;
    this.targetDisplayId = event.targetDisplayId;
    this.touches = event.touches.map(cancelPoint);
    this.changedTouches = event.changedTouches.map(cancelPoint);
  }
}

export interface TouchStream {
  owner: string;
  snapshot: TouchCancel;
}
export class PointerPosting {
  busy: boolean = false;
  owners: Map<number, TouchStream> = new Map();
  retirements: (() => void)[] = [];
  finish(coordinates?: TouchCoordinates): void {
    try {
      coordinates?.restore();
    } finally {
      this.busy = false;
      while (this.retirements.length > 0) {
        for (const retire of this.retirements.splice(0)) {
          retire();
        }
      }
    }
  }
}

/** Detach within the live cleanup scope; only builder disposal may outlive posting. */
export class NativeRetirement {
  private state: "live" | "retiring" | "retired" = "live";
  detachThenDispose(detach: () => void, dispose: () => void, posting: PointerPosting): void {
    if (this.state === "retired") {
      return;
    }
    if (this.state === "retiring") {
      throw new Error("Native retirement is already in progress");
    }
    this.state = "retiring";
    try {
      detach();
    } catch (error) {
      this.state = "live";
      throw error as Error;
    }
    this.state = "retired";
    if (posting.busy) {
      posting.retirements.push(dispose);
    } else {
      dispose();
    }
  }
}
