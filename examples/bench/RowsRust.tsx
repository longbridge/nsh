// `nsh test examples/bench/RowsRust.tsx --trace`: the Rows screen with the Rust Button.
import { mount } from "nsh";
import { Button } from "nsh/ui/mobile";
import { Rows } from "./buttons";

mount(() => <Rows Button={Button} />);
