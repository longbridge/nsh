// The displays the app sees, for `Resize.test.tsx` to place the window
// against.
import { mount, Text } from "nsh";
import { Window, displays } from "nsh/desktop";

mount(() => <Window id="main"><Text>{JSON.stringify(displays())}</Text></Window>);
