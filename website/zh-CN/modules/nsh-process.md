---
title: nsh/process
description: "Process：启动桌面子进程、流式接收输出并写入标准输入。"
order: 8.7
---

`nsh/process` 让 Native Shell 应用直接启动桌面命令。运行时管理进程、管道与回调；TSX 应用发送输入并接收增量输出。

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

## 平台与授权

`isAvailable` 在 macOS、Windows 和 Linux 桌面宿主（包括 GPUI）上为 `true`。应用可在所有平台导入模块，并据此决定是否显示命令操作。

在 `nsh.json` 中声明允许执行的命令与工作目录：

```json
{
  "capabilities": {
    "process": { "execute": ["codex"] },
    "fs": { "read": ["${dataDir}"], "write": ["${dataDir}"] }
  }
}
```

命令严格匹配：`codex` 与 `/usr/bin/codex` 是不同授权项。运行时直接执行程序，参数以字符串数组传入。裸命令名使用宿主 `PATH` 中的绝对目录查找，在切换工作目录前解析。子进程继承宿主环境。`cwd` 必须是读取授权范围内的已有目录，默认是应用数据目录。嵌入宿主可进一步限制授权；独立应用也需要显式声明进程和文件权限。

进程授权允许被启动的程序执行其自身的系统操作。文件根目录约束 NSH 文件 API 和启动工作目录；应考虑所授权程序本身的系统权限。

## 流式输出

`spawn` 立即返回子进程；权限、参数和启动错误同步抛出。`stdout` 和 `stderr` 回调收到增量 UTF-8 字符串。逐行协议需保留未完成的行。`exit` 在最终输出之后触发，每个监听器调用一次。

`write` 将 UTF-8 输入加入队列，`closeStdin` 结束输入。`kill` 终止子进程树；VM 销毁也会清理它拥有的进程。Unix 平台先向进程组发送终止信号，250 ms 后升级为强制结束。

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

示例要求宿主已安装 `codex`，并使用上面的能力声明；文件操作平台支持见 [nsh/fs](./nsh-fs.md)。

## 收集结果与限制

`await Process.run("codex", ["--version"])` 返回 stdout、stderr 和退出信息。非零退出码仍返回结果；启动失败、超时或输出超限时 Promise 拒绝。

| 选项或限制 | 行为 |
|---|---|
| `timeoutMs` | 1 至 3,600,000 ms；`run` 默认 10 秒；`spawn` 默认运行至退出或被停止 |
| `maxOutputBytes` | `run` 合计输出字节上限；默认 1 MiB，最多 8 MiB |
| 活跃子进程 | 每个 VM 最多 32 个 |
| 待处理输出 | 最多 8 MiB；溢出后终止进程 |
| 待写入 stdin | 最多 1 MiB；超限时 `write` 抛出错误 |
| 监听器 | 每个进程每类事件最多 64 个 |
