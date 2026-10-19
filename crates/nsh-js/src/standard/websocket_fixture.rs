//! Real loopback protocol regressions; no external network service or temp files.
use std::{
    net::TcpListener,
    time::{Duration, Instant},
};

use nsh_core::access::{Access, Capabilities};

use crate::Host;

fn host(allowed: bool) -> Host {
    let host = Host::new().unwrap();
    let capabilities = if allowed {
        serde_json::json!({"network": {"hosts": ["127.0.0.1"]}})
    } else {
        serde_json::json!({})
    };
    host.set_system_access(
        Access {
            declared: Capabilities::from_json(&capabilities).unwrap(),
            granted: None,
        },
        None,
    );
    host
}
fn finish(host: &Host) {
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        host.run_jobs();
        if host
            .eval_source("if (!globalThis.wsDone) throw new Error('pending');")
            .is_ok()
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("WebSocket event loop failed to complete");
}
#[test]
fn websocket_denies_network_before_connecting() {
    let host = host(false);
    host.eval_source("let denied = false; try { new WebSocket('ws://127.0.0.1:9/'); } catch (e) { denied = /capability denied/.test(String(e)); } if (!denied) throw Error('network access escaped');").unwrap();
}
#[test]
// Tungstenite fixes the callback error type to an HTTP response.
#[allow(clippy::result_large_err)]
fn websocket_echo_binary_protocol_and_close() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (tcp, _) = listener.accept().unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut ws = tokio_tungstenite::tungstenite::accept_hdr(tcp, |request: &tokio_tungstenite::tungstenite::handshake::server::Request, mut response: tokio_tungstenite::tungstenite::handshake::server::Response| {
            assert_eq!(request.headers().get("Sec-WebSocket-Protocol").unwrap(), "chat");
            response.headers_mut().insert("Sec-WebSocket-Protocol", "chat".parse().unwrap());
            Ok(response)
        }).unwrap();
        for _ in 0..3 {
            let message = ws.read().unwrap();
            ws.send(message).unwrap();
        }
        let close = ws.read().unwrap();
        assert!(close.is_close());
        let _ = ws.flush();
    });
    let host = host(true);
    host.eval_source(&format!(r#"
        globalThis.wsEvents = [];
        const ws = new WebSocket('ws://{address}/', ['chat']);
        if (!(ws instanceof EventTarget) || ws.readyState !== WebSocket.CONNECTING) throw Error('identity');
        ws.binaryType = 'arraybuffer';
        ws.onopen = () => {{ if (ws.protocol !== 'chat') throw Error('protocol'); wsEvents.push('open'); ws.send('hello'); ws.send(new Uint8Array([1,2,3])); ws.send(new Blob([new Uint8Array([4,5])])); }};
        ws.addEventListener('message', event => {{
            if (!(event instanceof Event)) throw Error('event identity');
            wsEvents.push(typeof event.data === 'string' ? event.data : Array.from(new Uint8Array(event.data)).join(','));
            if (wsEvents.length === 4) ws.close(1000, 'done');
        }});
        ws.onerror = event => {{ globalThis.wsFailure = event.message; }};
        ws.onclose = event => {{ globalThis.wsClosed = [event.code, event.reason, event.wasClean, ws.readyState]; globalThis.wsDone = true; }};
    "#)).unwrap();
    finish(&host);
    host.eval_source("if (globalThis.wsFailure) throw Error(wsFailure); if (JSON.stringify(wsEvents) !== '[\"open\",\"hello\",\"1,2,3\",\"4,5\"]') throw Error(JSON.stringify(wsEvents)); if (JSON.stringify(wsClosed) !== '[1000,\"done\",true,3]') throw Error(JSON.stringify(wsClosed));").unwrap();
    server.join().unwrap();
}
#[test]
fn websocket_rejects_invalid_protocols_and_close_codes() {
    let host = host(true);
    host.eval_source(r#"
        for (const protocols of [['chat','chat'], ['bad protocol'], ['']]) {
            let threw = false;
            try { new WebSocket('ws://127.0.0.1:9/', protocols); } catch (e) { threw = e instanceof SyntaxError; }
            if (!threw) throw Error('invalid protocol accepted');
        }
        for (const url of ['http://127.0.0.1/', 'ws://user:pass@127.0.0.1/', 'ws://127.0.0.1/#fragment']) {
            let threw = false;
            try { new WebSocket(url); } catch (_) { threw = true; }
            if (!threw) throw Error('invalid URL accepted');
        }
        const socket = new WebSocket('ws://127.0.0.1:9/');
        for (const code of [0, 1001, 2999, 5000, 3000.5]) {
            let threw = false;
            try { socket.close(code); } catch (error) { threw = error.name === 'InvalidAccessError'; }
            if (!threw) throw Error('invalid close code accepted');
        }
        let sendThrew = false;
        try { socket.send('premature'); } catch (error) { sendThrew = error.name === 'InvalidStateError'; }
        if (!sendThrew) throw Error('send while connecting accepted');
        let reasonThrew = false;
        try { socket.close(3000, 'x'.repeat(124)); } catch (error) { reasonThrew = error instanceof SyntaxError; }
        if (!reasonThrew) throw Error('long close reason accepted');
        socket.onclose = () => { globalThis.wsDone = true; };
        socket.close();
    "#).unwrap();
    finish(&host);
}

#[test]
fn websocket_owner_disposal_aborts_transport() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (ended_tx, ended_rx) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let (tcp, _) = listener.accept().unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut ws = tokio_tungstenite::tungstenite::accept(tcp).unwrap();
        // Disposal aborts transport promptly rather than waiting for peer data.
        let _ = ws.read();
        ended_tx.send(()).unwrap();
    });
    let host = host(true);
    host.with_raw(|raw| {
        // SAFETY: Host owns the live context for this closure.
        let ctx = unsafe { rquickjs::Ctx::from_raw(std::ptr::NonNull::new_unchecked(raw)) };
        crate::reactive::install_globals(&ctx);
    });
    host.eval_source(&format!(
        r#"
        __nsh_reactive.createRoot(dispose => {{
            globalThis.disposeSocketOwner = dispose;
            const socket = new WebSocket('ws://{address}/');
            socket.onopen = () => {{ globalThis.wsDone = true; }};
        }});
    "#
    ))
    .unwrap();
    finish(&host);
    host.eval_source("disposeSocketOwner();").unwrap();
    for _ in 0..10 {
        host.run_jobs();
    }
    ended_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    drop(host);
    server.join().unwrap();
}

#[test]
fn websocket_live_transport_is_released_with_vm() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (ended_tx, ended_rx) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let (tcp, _) = listener.accept().unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut ws = tokio_tungstenite::tungstenite::accept(tcp).unwrap();
        let _ = ws.read();
        ended_tx.send(()).unwrap();
    });
    let host = host(true);
    host.eval_source(&format!("globalThis.liveSocket = new WebSocket('ws://{address}/'); liveSocket.onopen = () => {{ globalThis.wsDone = true; }};")).unwrap();
    finish(&host);
    drop(host);
    ended_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    server.join().unwrap();
}

#[test]
fn websocket_handshake_redirect_fails_without_contacting_target() {
    use std::io::{Read, Write};
    let redirect = TcpListener::bind("127.0.0.1:0").unwrap();
    let forbidden_target = TcpListener::bind("127.0.0.1:0").unwrap();
    forbidden_target.set_nonblocking(true).unwrap();
    let address = redirect.local_addr().unwrap();
    let target = forbidden_target.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = redirect.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut request = [0u8; 4096];
        assert!(stream.read(&mut request).unwrap() > 0);
        write!(stream, "HTTP/1.1 302 Found\r\nLocation: ws://{target}/secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    let host = host(true);
    host.eval_source(&format!(
        r#"
        import {{ WebSocket as ShellWebSocket }} from 'websocket';
        globalThis.wsOpened = false;
        globalThis.wsErrored = false;
        ShellWebSocket.connect('ws://{address}/redirect')
          .then(() => {{ globalThis.wsOpened = true; }}, () => {{ globalThis.wsErrored = true; }})
          .finally(() => {{ globalThis.wsDone = true; }});
    "#
    ))
    .unwrap();
    finish(&host);
    host.eval_source("if (wsOpened || !wsErrored) throw Error('redirect accepted');")
        .unwrap();
    server.join().unwrap();
    assert_eq!(
        forbidden_target.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "redirect target must never be contacted"
    );
}

#[test]
// Tungstenite's handshake callback fixes the error to an HTTP response.
#[allow(clippy::result_large_err)]
fn websocket_module_matches_shell_connect_socket_contract() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (tcp, _) = listener.accept().unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut socket = tokio_tungstenite::tungstenite::accept_hdr(
            tcp,
            |request: &tokio_tungstenite::tungstenite::handshake::server::Request,
             response: tokio_tungstenite::tungstenite::handshake::server::Response| {
                assert_eq!(request.headers().get("x-shell-test").unwrap(), "yes");
                Ok(response)
            },
        )
        .unwrap();
        for _ in 0..2 {
            let message = socket.read().unwrap();
            socket.send(message).unwrap();
        }
        assert!(socket.read().unwrap().is_close());
        let _ = socket.flush();
    });
    let host = host(true);
    host.eval_source(&format!(r#"
        import * as module from 'websocket';
        if (Object.keys(module).join(',') !== 'WebSocket' || typeof module.WebSocket.connect !== 'function') throw Error('module exports');
        globalThis.moduleFailure = null;
        (async () => {{
            const socket = await module.WebSocket.connect('ws://{address}/', {{headers: {{'X-Shell-Test': 'yes'}}}});
            if (Object.keys(socket).sort().join(',') !== 'close,read,write') throw Error('socket shape');
            const pending = socket.read();
            let duplicate = false;
            try {{ socket.read(); }} catch (error) {{ duplicate = error instanceof TypeError && error.message === 'WebSocket.read() already has an outstanding read'; }}
            if (!duplicate) throw Error('concurrent read did not synchronously throw');
            await socket.write('shell');
            if (await pending !== 'shell') throw Error('text roundtrip');
            await socket.write(new Uint8Array([3,7]));
            const bytes = await socket.read();
            if (!(bytes instanceof Uint8Array) || bytes.join(',') !== '3,7') throw Error('binary roundtrip');
            let invalid = false;
            try {{ socket.write(new ArrayBuffer(1)); }} catch (error) {{ invalid = error instanceof TypeError; }}
            if (!invalid) throw Error('ArrayBuffer should be rejected synchronously');
            await socket.close();
            globalThis.wsDone = true;
        }})().catch(error => {{ moduleFailure = String(error); globalThis.wsDone = true; }});
    "#)).unwrap();
    finish(&host);
    host.eval_source("if (moduleFailure) throw Error(moduleFailure);")
        .unwrap();
    server.join().unwrap();
}

#[test]
fn websocket_module_options_fail_synchronously_like_shell() {
    let host = host(true);
    host.eval_source(r#"
        import {WebSocket as ShellWebSocket} from 'websocket';
        for (const options of [42, {protocols: ['chat']}, {headers: 42}, {headers: {Authorization: 'secret'}}, {headers: {Cookie: 'secret'}}, {headers: {'Sec-WebSocket-Key': 'evil'}}, {headers: {'x-test': 1}}]) {
            let threw = false;
            try { ShellWebSocket.connect('ws://127.0.0.1:9/', options); } catch (error) { threw = error instanceof TypeError; }
            if (!threw) throw Error('invalid options did not synchronously throw: ' + JSON.stringify(options));
        }
        let invalidScheme = false;
        try { ShellWebSocket.connect('https://127.0.0.1/'); } catch (error) { invalidScheme = error instanceof TypeError; }
        if (!invalidScheme) throw Error('invalid scheme did not synchronously throw');
    "#).unwrap();
}

#[test]
fn websocket_module_pending_read_is_released_with_vm() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (ended_tx, ended_rx) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let (tcp, _) = listener.accept().unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut socket = tokio_tungstenite::tungstenite::accept(tcp).unwrap();
        let _ = socket.read();
        ended_tx.send(()).unwrap();
    });
    let host = host(true);
    host.eval_source(&format!(
        r#"
        import {{ WebSocket as ShellWebSocket }} from 'websocket';
        ShellWebSocket.connect('ws://{address}/').then(socket => {{
            globalThis.liveShellSocket = socket;
            globalThis.pendingShellRead = socket.read();
            globalThis.wsDone = true;
        }});
    "#
    ))
    .unwrap();
    finish(&host);
    drop(host);
    ended_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    server.join().unwrap();
}
