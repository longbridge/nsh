// A case of the API's test suite (docs/dev/compatibility.md): `nsh/canvas`.
// A headless run paints nothing, so the script checks what JS sees: the
// size `draw` gets, the canvas's layout, and what the kept context answers
// natively from what it recorded (hit tests on `Path2D` paths, its line
// dash, text measures, gradients made). Its script checks
// each.
import { Text, View, signal } from "nsh";
import { Canvas, CanvasGradient, CanvasRenderingContext2D, Path2D } from "nsh/canvas";

function App() {
  let kept: CanvasRenderingContext2D | undefined;
  let size = "";
  let draws = 0;
  // Above the diagonal from the top left to the bottom right.
  const triangle = new Path2D("M0 0 L100 0 L100 100 Z");
  const both = new Path2D();
  both.rect(0, 0, 10, 10);
  both.addPath(new Path2D("M50 50 h10 v10 h-10 Z"));
  let gradient: CanvasGradient | undefined;
  const report = signal("Not checked");
  const check = () => {
    const ctx = kept!;
    report.set(
      [
        `size ${size}`,
        `canvas ${ctx.canvas.width}x${ctx.canvas.height}`,
        `drawn ${draws > 0}`,
        `context ${ctx instanceof CanvasRenderingContext2D}`,
        `gradient ${gradient instanceof CanvasGradient}`,
        `in ${ctx.isPointInPath(triangle, 75, 25)}`,
        `out ${ctx.isPointInPath(triangle, 25, 75)}`,
        `added ${ctx.isPointInPath(both, 55, 55)}`,
        `current ${ctx.isPointInPath(150, 30)}`,
        `stroke ${ctx.isPointInStroke(triangle, 50, 1)}`,
        `dash ${ctx.getLineDash().join(",")}`,
        `measured ${ctx.measureText("Hello").width > 0}`,
      ].join(" "),
    );
  };
  return (
    <View testID="root" style={{ flex: 1, paddingTop: 64 }}>
      <Canvas
        testID="canvas"
        style={{ width: 200, height: 100 }}
        draw={(ctx, s) => {
          kept = ctx;
          size = `${s.width}x${s.height}`;
          draws++;
          gradient = ctx.createLinearGradient(0, 0, 200, 0);
          gradient.addColorStop(0, "red");
          gradient.addColorStop(1, "blue");
          ctx.fillStyle = gradient;
          ctx.fill(triangle);
          ctx.lineWidth = 4;
          ctx.setLineDash([4, 2]);
          ctx.strokeStyle = "black";
          ctx.stroke(triangle);
          ctx.setLineDash([]);
          ctx.font = "16px sans-serif";
          ctx.fillText("Hi", 120, 50);
          ctx.beginPath();
          ctx.rect(120, 10, 60, 40);
          ctx.fill();
          ctx.setLineDash([3, 1]);
        }}
      />
      <View testID="check" onPress={check}>
        <Text>Check</Text>
      </View>
      <Text>{report()}</Text>
    </View>
  );
}

export default App;
