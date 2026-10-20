# Gallery

Run `cargo run` from the repository root to explore the native UI demos. The Standard APIs page exercises NSH's file, process, TCP, WebSocket, environment and job APIs. Its utilities card demonstrates the standard library's Buffer, crypto, path, URL and compression integration.

For the TCP and WebSocket echo actions, start the local peers in another terminal:

```sh
python3 examples/gallery/tools/echo.py
```

The helper uses Python's standard library and binds loopback ports 9876 and 9877. Run the gallery on the same machine to reach these peers. File actions use a temporary directory inside the host's configured data directory and remove it afterwards. Process actions use `echo` and `cat` on Unix or `cmd.exe` on Windows; the host checks the executable grants in `nsh.json`.

The **Request host exit** action calls `process.exit(0)` explicitly. To let the embedding host receive this request, opt in with `"exit": true` inside `capabilities.process` in `nsh.json` and provide the host's exit handler. The page displays capability and host errors from actions so the same gallery can run under different host policies.
