---
title: nsh/fs
description: "FileSystem: asynchronously read and write UTF-8 files inside granted native directories."
order: 8.8
---

`nsh/fs` gives Native Shell apps capability-checked native text files. Disk operations run on worker threads and return promises.

```ts
import { FileSystem, isAvailable } from "nsh/fs";

const FileSystem: {
  appDataDir(): Promise<string>;
  readText(path: string): Promise<string>;
  writeText(path: string, text: string, options?: { exclusive?: boolean }): Promise<void>;
  mkdir(path: string, options?: { recursive?: boolean }): Promise<void>;
  exists(path: string): Promise<boolean>;
  rename(from: string, to: string): Promise<void>;
  remove(path: string): Promise<void>;
};
```

## Platforms and capabilities

`isAvailable` reports whether the host provides native file support. macOS and Linux desktop hosts, including GPUI hosts on those platforms, provide it; check this value before using file operations.

Declare read and write roots separately in `nsh.json`:

```json
{
  "capabilities": {
    "fs": {
      "read": ["${dataDir}"],
      "write": ["${dataDir}"]
    }
  }
}
```

`${dataDir}` comes from the host's data directory for this app. `appDataDir` also requires a read grant for that directory. Roots are absolute paths or paths under `${dataDir}`. An embedding host may narrow the grants further. Every operation checks both the declaration and host grant, rejecting traversal and symlink escapes. Process and file capabilities require explicit declarations in standalone apps too.

## Operations

| Method | Behavior and required grant |
|---|---|
| `appDataDir` | Returns this app's data directory; read grant |
| `readText` | Reads UTF-8 text; read grant |
| `writeText` | Creates or replaces a file and syncs it to disk; write grant; `exclusive: true` refuses to overwrite an existing file |
| `mkdir` | Creates a directory; `recursive: true` creates missing parents; write grant |
| `exists` | Resolves `true` for an existing path, `false` for a missing path; read grant |
| `rename` | Renames a file, atomically replacing the destination when the filesystem permits; write grants for source and destination |
| `remove` | Deletes one file; write grant |

`writeText` requires an existing parent directory. `remove` accepts file paths; create directories with `mkdir`. Reads and writes accept up to 16 MiB of UTF-8 data per operation, with at most 64 pending operations per VM. Use `await` to order dependent operations.

Permission, I/O and quota failures reject the promise. Reading a missing file includes `ENOENT` in the error message; invalid UTF-8 includes `EILSEQ`; the size limit includes `EFBIG`. `exists` converts only missing paths to `false`, and rejects other failures.

## Save a draft from TSX

Use the capability declaration above with this app:

```tsx
import { mount, signal, Text, View } from "nsh";
import { FileSystem, isAvailable } from "nsh/fs";

const status = signal("Ready");
async function save() {
  try {
    if (!isAvailable) {
      status.set("Native files require a supported host");
      return;
    }
    const dir = await FileSystem.appDataDir();
    await FileSystem.mkdir(dir, { recursive: true });
    await FileSystem.writeText(`${dir}/draft.txt`, "Hello from TSX");
    status.set(await FileSystem.readText(`${dir}/draft.txt`));
  } catch (error) {
    status.set(String(error));
  }
}
mount(() => <View onPress={save}><Text>{status()}</Text></View>);
```

For settings by key, use [nsh/storage](./nsh-storage.md).
