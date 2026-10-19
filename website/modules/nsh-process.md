---
title: nsh/process
description: "Process: launch desktop child processes, stream output and write standard input."
order: 8.7
---

`nsh/process` lets Native Shell apps launch desktop commands directly. The runtime owns processes, pipes and callback delivery; TSX code sends input and receives incremental output.

```ts
import { Process, isAvailable } from "nsh/process";

export interface ProcessOptions {
  /** Existing directory within a filesystem read grant; defaults to the app data directory. */
  cwd?: string;
  /** Children inherit the host environment, including its PATH. No shell is started. */
  stdin?: "pipe";
  /** Terminates the process tree after this duration; run defaults to 10 seconds. */
  timeoutMs?: number;
  /** run's combined stdout/stderr limit, default 1 MB, maximum 8 MB. */
  maxOutputBytes?: number;
}
export interface ProcessExit { code: number | null; signal?: number; error?: string; }
export interface ProcessResult extends ProcessExit { stdout: string; stderr: string; }
export interface ChildProcess {
  readonly pid: number;
  /** Output is incremental UTF-8; exit follows the final output and fires once per listener. */
  on(event: "stdout" | "stderr", listener: (text: string) => void): void;
  on(event: "exit", listener: (result: ProcessExit) => void): void;
  /** Enqueues UTF-8 stdin; throws if the outstanding queue exceeds 1 MB. */
  write(text: string): void;
  closeStdin(): void;
  /** Terminates the complete child process group, escalating after 250 ms. */
  kill(): void;
}
export interface ProcessModule {
  /** Direct execution; a launch failure throws synchronously. VM shutdown terminates its children. */
  spawn(command: string, args?: readonly string[], options?: ProcessOptions): ChildProcess;
  /** Captures bounded output; launch failures, timeouts and output limits reject. */
  run(command: string, args?: readonly string[], options?: ProcessOptions): Promise<ProcessResult>;
}
```

## Platforms and capabilities

`isAvailable` is `true` on macOS, Windows and Linux desktop hosts, including GPUI. Apps can import the module on every platform and use this value to decide whether to show command actions.

Declare executable commands and working directories in `nsh.json`:

```json
{
  "capabilities": {
    "process": { "execute": ["codex"] },
    "fs": { "read": ["${dataDir}"], "write": ["${dataDir}"] }
  }
}
```

Commands match exactly: `codex` and `/usr/bin/codex` are separate grants. The runtime executes the program directly with an argument array. Bare commands resolve through absolute directories in the host's `PATH`, before changing the working directory. Children inherit the host environment. `cwd` must be an existing directory within a read grant, and defaults to the app's data directory. An embedding host can narrow the grants; standalone apps also require explicit process and file declarations.

A process grant allows the launched program to perform its own system operations. File roots constrain NSH file APIs and the launch working directory; consider the program's own system permissions when granting it.

## Stream output

`spawn` returns a child immediately; capability, argument and launch errors throw synchronously. `stdout` and `stderr` listeners receive incremental UTF-8 strings. Keep incomplete lines between callbacks when decoding line-oriented protocols. `exit` follows final output and fires once per listener.

`write` queues UTF-8 input; `closeStdin` finishes input. `kill` terminates the child process tree; destroying the VM also cleans up its processes. On Unix the runtime first signals the process group to terminate, escalating to a forced stop after 250 ms.

```tsx
import { mount, signal, Text, View } from "nsh";
import { FileSystem } from "nsh/fs";
import { Process, isAvailable } from "nsh/process";

const output = signal("Run Codex version");
async function run() {
  try {
    if (!isAvailable) {
      output.set("This command requires a desktop host");
      return;
    }
    const cwd = await FileSystem.appDataDir();
    await FileSystem.mkdir(cwd, { recursive: true });
    const child = Process.spawn("codex", ["--version"], { cwd });
    output.set("");
    child.on("stdout", (text) => output.update((value) => value + text));
    child.on("stderr", (text) => output.update((value) => value + text));
    child.on("exit", (result) => {
      if (result.error) output.update((value) => value + result.error);
    });
  } catch (error) {
    output.set(String(error));
  }
}
mount(() => <View onPress={run}><Text>{output()}</Text></View>);
```

This example requires `codex` installed for the host and the capability declaration above; see [nsh/fs](./nsh-fs.md) for file operation platform support.

## Capture results and limits

`await Process.run("codex", ["--version"])` returns stdout, stderr and exit information. A nonzero exit status still resolves with that status; launch failures, timeouts and output limits reject the promise.

| Option or limit | Behavior |
|---|---|
| `timeoutMs` | 1–3,600,000 ms; `run` defaults to 10 seconds; `spawn` runs until exit or stop by default |
| `maxOutputBytes` | `run`'s combined output limit; default 1 MiB, maximum 8 MiB |
| Active children | At most 32 per VM |
| Pending output | At most 8 MiB; overflow terminates the process |
| Pending stdin | At most 1 MiB; `write` throws on overflow |
| Listeners | At most 64 per event per child |
