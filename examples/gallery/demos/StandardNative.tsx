import { cleanup, signal, Text } from "nsh";
import console from "console";
import fs from "fs/promises";
import net from "net";
import os from "os";
import process from "process";
import { Button, Card, colors, DemoPage, mono } from "../ui";

/** Keep native-only imports deferred even in the development host's single bundle. */
async function nativeModules() {
  const [{ Buffer }, { createHash, randomUUID }, { default: path }, { fileURLToPath, pathToFileURL }, { WebSocket: SocketClient }, { gzipSync, gunzipSync }] = await Promise.all([
    import("buffer"), import("crypto"), import("path"), import("url"), import("websocket"), import("zlib"),
  ]);
  return { Buffer, createHash, randomUUID, path, fileURLToPath, pathToFileURL, SocketClient, gzipSync, gunzipSync };
}

/** Load the native API demo after its host has been selected. */
export async function loadStandardNative() {
  const modules = await nativeModules();
  return () => nativeDemo(modules);
}

/** Each action calls the native APIs and displays its result or error. */
function nativeDemo({ Buffer, createHash, randomUUID, path, fileURLToPath, pathToFileURL, SocketClient, gzipSync, gunzipSync }: Awaited<ReturnType<typeof nativeModules>>) {
  const results = signal<Record<string, string>>({});
  const busy = signal<Record<string, boolean>>({});
  let disposed = false;
  const release = new Set<() => void>();
  cleanup(() => { disposed = true; for (const close of release) close(); });
  const show = (key: string, text: string) => {
    if (!disposed) results.set({ ...results(), [key]: text });
  };
  const run = async (key: string, action: () => Promise<string>) => {
    if (disposed || busy()[key]) return;
    busy.set({ ...busy(), [key]: true });
    try { show(key, await action()); }
    catch (error) { show(key, String(error)); }
    finally { if (!disposed) busy.set({ ...busy(), [key]: false }); }
  };
  function Action(props: { name: string; detail: string; action: () => Promise<string> }) {
    return <Card icon="system" title={props.name} subtitle={props.detail}>
      <Button label={`Run ${props.name}`} disabled={busy()[props.name] ?? false} onPress={() => { void run(props.name, props.action); }} />
      <Text selectable style={{ color: colors.text, fontSize: 13, fontFamily: mono }}>{results()[props.name] ?? "Ready"}</Text>
    </Card>;
  }
  const dataDirectory = () => {
    if (!process.dataDir) throw new Error("The host has no data directory configured");
    return process.dataDir;
  };
  function Utilities() {
    return <Action name="Utilities" detail="buffer · crypto · path · url · zlib" action={async () => {
      const bytes = Buffer.from("Native Shell", "utf8");
      const packed = gzipSync(bytes);
      const filename = path.join(dataDirectory(), "gallery.txt");
      const url = pathToFileURL(filename);
      const hash = createHash("sha256").update(bytes).digest("hex");
      console.info("Gallery utilities", hash);
      return `${bytes.toString("base64")}\nSHA-256: ${hash}\nUUID: ${randomUUID()}\nGlobal UUID: ${crypto.randomUUID()}\nGlobal Buffer: ${globalThis.Buffer.from("NSH").toString("hex")}\nGzip: ${packed.length} bytes → ${gunzipSync(packed).toString()}\n${url.href}\n${fileURLToPath(url)}\n${path.basename(filename)}`;
    }} />;
  }
  function Environment() {
    return <Action name="Environment" detail="os · process.nextTick · console · import.meta.url" action={async () => {
      await new Promise<void>((resolve) => process.nextTick(resolve));
      return `${os.platform()} / ${os.arch()}\nProcess: ${process.platform} / ${process.arch}\nHome: ${os.homedir()}\nData: ${process.dataDir}\nEOL: ${JSON.stringify(os.EOL)}\nEnv: ${JSON.stringify(process.env)}\nModule: ${import.meta.url}`;
    }} />;
  }
  function Files() {
    return <Action name="Files" detail="fs/promises: create, write, read, list, inspect, rename, remove" action={async () => {
      const dir = path.join(dataDirectory(), `gallery-${randomUUID()}`);
      const file = path.join(dir, "note.txt");
      const moved = path.join(dir, "renamed.txt");
      await fs.mkdir(dir, { recursive: true });
      try {
        await fs.writeFile(file, "Hello from NSH");
        const text = await fs.readFile(file, "utf8");
        const bytes = await fs.readFile(file);
        const stats = await fs.stat(file);
        const link = await fs.lstat(file);
        const entries = await fs.readdir(dir, { withFileTypes: true });
        const names = await fs.readdir(dir);
        let linkResult: string;
        try { linkResult = await fs.readlink(file); } catch (error) { linkResult = String(error); }
        await fs.rename(file, moved);
        const exists = await fs.exists(moved);
        await fs.unlink(moved);
        return `${text}\n${bytes.length} bytes, stat: ${stats.size}, file: ${link.isFile()}\nMode: ${stats.mode.toString(8)}, directory: ${stats.isDirectory()}, symlink: ${link.isSymbolicLink()}\nModified: ${stats.mtime.toISOString()} (${stats.mtimeMs})\nAccessed: ${stats.atime.toISOString()} (${stats.atimeMs})\nCreated: ${stats.birthtime.toISOString()} (${stats.birthtimeMs})\n${names.join(", ")}\n${entries.map((entry) => `${entry.name}: directory=${entry.isDirectory()}`).join("\n")}\nRenamed: ${exists}; removed: ${!(await fs.exists(moved))}\nreadlink on regular file: ${linkResult}`;
      } finally {
        for (const name of await fs.readdir(dir)) await fs.unlink(path.join(dir, name));
        await fs.rmdir(dir);
      }
    }} />;
  }
  function Processes() {
    return <Action name="Processes" detail="process.run · spawn · stdout/stderr/exit · stdin · kill" action={async () => {
      const windows = os.platform() === "windows";
      const command = windows ? "cmd.exe" : "cat";
      const args = windows ? ["/d", "/c", "more"] : [];
      const output = await process.run(windows ? "cmd.exe" : "echo", windows ? ["/d", "/c", "echo", "Native Shell"] : ["Native Shell"], { timeoutMs: 3000 });
      if (disposed) return "Closed";
      const streamed = await new Promise<string>((resolve, reject) => {
        const child = process.spawn(command, args, { stdin: "pipe", lines: true, timeoutMs: 3000 });
        const stop = () => child.kill();
        release.add(stop);
        let text = `pid: ${child.pid}\n`;
        child.on("stdout", (line) => { text += line; });
        child.on("stderr", (line) => { text += line; });
        child.on("exit", (exit) => { release.delete(stop); exit.code === 0 ? resolve(text) : reject(new Error(JSON.stringify(exit))); });
        child.write("Hello through stdin\n");
        child.end();
      });
      return `run: ${output.code} ${output.stdout}${output.stderr}\nspawn: ${streamed}`;
    }} />;
  }
  function Exit() {
    return <Action name="Request host exit" detail="process.exit(0) · enable capabilities.process.exit to let the host handle this request" action={async () => {
      process.exit(0);
      return "Exit requested from the host";
    }} />;
  }
  function Tcp() {
    return <Action name="TCP" detail="net.connect · Socket.write/read/close · local echo on 127.0.0.1:9876" action={async () => {
      const socket = await net.connect("127.0.0.1", 9876);
      const close = () => socket.close();
      if (disposed) { close(); return "Closed"; }
      release.add(close);
      const timer = setTimeout(close, 5000);
      try { await socket.write("Hello TCP\n"); const bytes = await socket.read(1024); return bytes ? Buffer.from(bytes).toString() : "Peer closed"; }
      finally { clearTimeout(timer); close(); release.delete(close); }
    }} />;
  }
  function WebSockets() {
    return <Action name="WebSockets" detail="websocket module and global WebSocket · local echo on 127.0.0.1:9877" action={async () => {
      const url = "ws://127.0.0.1:9877/";
      const socket = await SocketClient.connect(url);
      const close = () => { void socket.close().catch(() => {}); };
      if (disposed) { close(); return "Closed"; }
      release.add(close);
      const timer = setTimeout(close, 5000);
      let reply: string;
      try { await socket.write("Hello module"); reply = String(await socket.read()); }
      finally { clearTimeout(timer); release.delete(close); await socket.close(); }
      if (disposed) return "Closed";
      const eventReply = await new Promise<string>((resolve, reject) => {
        const ws = new WebSocket(url);
        const stop = () => ws.close(); release.add(stop);
        const finish = (error?: Error, value?: string) => {
          clearTimeout(timer); release.delete(stop);
          ws.onopen = null; ws.onmessage = null; ws.onerror = null; ws.onclose = null;
          ws.close(1000, "Demo complete");
          error ? reject(error) : resolve(value ?? "");
        };
        const timer = setTimeout(() => finish(new Error("WebSocket echo timed out")), 5000);
        ws.onopen = () => ws.send("Hello global");
        ws.onmessage = (event) => finish(undefined, String(event.data));
        ws.onerror = () => finish(new Error("WebSocket connection failed"));
        ws.onclose = () => finish(new Error("Peer closed before echo"));
      });
      return `Module: ${reply}\nGlobal: ${eventReply}`;
    }} />;
  }
  return <DemoPage cards={[Utilities, Environment, Files, Processes, Tcp, WebSockets, Exit]} />;
}
