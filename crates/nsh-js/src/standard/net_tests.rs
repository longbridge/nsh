use std::{
    io::{Read, Write},
    net::TcpListener,
    time::{Duration, Instant},
};

use nsh_core::access::{Access, Capabilities};

use crate::Host;
fn access(value: serde_json::Value) -> Access {
    Access {
        declared: Capabilities::from_json(&value).unwrap(),
        granted: None,
    }
}
fn wait(host: &Host, expression: &str) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !host.eval_script("socket-test", expression).unwrap() {
        assert!(
            Instant::now() < deadline,
            "socket expression did not become true: {expression}"
        );
        host.run_system();
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn tcp_permissions_never_inherit_http_grants() {
    let a = access(
        serde_json::json!({"network":{"http":[{"host":"127.0.0.1","methods":["GET"],"paths":["/"]}]}}),
    );
    assert!(a.allows_http("GET", "https://127.0.0.1/"));
    assert!(!a.allows_socket("127.0.0.1", 1234));
    let a = access(serde_json::json!({"network":{"hosts":["127.0.0.1"]}}));
    assert!(!a.allows_socket("127.0.0.1", 1234));
    let mut a = access(serde_json::json!({"network":{"sockets":["127.0.0.1:1234"]}}));
    assert!(a.allows_socket("127.0.0.1", 1234));
    assert!(!a.allows_socket("127.0.0.1", 1235));
    assert!(!a.allows_socket("127.0.0.1", 0));
    a.granted = Some(Capabilities::default());
    assert!(!a.allows_socket("127.0.0.1", 1234));
}
fn sockets(port: u16) -> Access {
    access(serde_json::json!({"network":{"sockets":[format!("127.0.0.1:{port}")]}}))
}
#[test]
fn socket_roundtrip_eof_and_limits() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let peer = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut bytes = [0; 4];
        stream.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"ping");
        stream.write_all(b"pong").unwrap();
    });
    let host = Host::new().unwrap();
    host.set_system_access(sockets(port), None);
    host.eval_source(&format!(
        r#"import net from 'net';
        globalThis.done = false; globalThis.failure = '';
        (async () => {{ const socket = await net.connect('127.0.0.1', {port});
            if (await socket.read(0) !== null) throw Error('zero read must be null');
            let range = false; try {{ socket.read(1048577); }} catch(e) {{ range = e instanceof RangeError && e.message === 'socket read exceeded the 1048576 byte limit'; }}
            if (!range) throw Error('read limit must throw synchronously');
            range = false; try {{ socket.write('x'.repeat(1048577)); }} catch(e) {{ range = e instanceof RangeError && e.message === 'socket write exceeded the 1048576 byte limit'; }}
            if (!range) throw Error('write limit must throw synchronously');
            await socket.write('ping'); const data = await socket.read();
            if (String.fromCharCode(...data) !== 'pong') throw Error('bad reply');
            if (await socket.read() !== null) throw Error('expected eof');
            let bounded = false; try {{ await socket.read(1048577); }} catch {{ bounded = true; }}
            if (!bounded) throw Error('missing bound'); socket.close(); globalThis.done = true;
        }})().catch(e => {{ globalThis.failure = String(e); globalThis.done = true; }});"#
    ))
    .unwrap();
    wait(&host, "globalThis.done === true");
    assert!(
        host.eval_script("result", "globalThis.failure === ''")
            .unwrap()
    );
    peer.join().unwrap();
}
#[test]
fn socket_vm_drop_releases_peer() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = std::sync::mpsc::channel();
    let peer = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut byte = [0];
        let result = stream.read(&mut byte);
        tx.send(result).unwrap();
    });
    let host = Host::new().unwrap();
    host.set_system_access(sockets(port), None);
    host.eval_source(&format!("import {{connect}} from 'net'; connect('127.0.0.1', {port}).then(s => {{ globalThis.socket = s; globalThis.connected = true; s.read().catch(() => {{}}); }});")).unwrap();
    wait(&host, "globalThis.connected === true");
    drop(host);
    assert_eq!(rx.recv_timeout(Duration::from_secs(3)).unwrap().unwrap(), 0);
    peer.join().unwrap();
}
#[test]
fn denied_socket_rejects_before_connecting() {
    for access in [
        access(serde_json::json!({})),
        access(serde_json::json!({"network":{"hosts":["*"]}})),
        Access::all(),
        sockets(10),
    ] {
        let host = Host::new().unwrap();
        host.set_system_access(access, None);
        host.eval_source("import {connect} from 'net'; globalThis.denied = false; try { connect('127.0.0.1', 9); } catch(e) { globalThis.denied = String(e).includes('add \"127.0.0.1:9\" to capabilities.network.sockets'); }").unwrap();
        wait(&host, "globalThis.denied === true");
    }
}

#[test]
fn closing_socket_cancels_pending_read() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let peer = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
    });
    let host = Host::new().unwrap();
    host.set_system_access(sockets(port), None);
    host.eval_source(&format!("import {{connect}} from 'net'; connect('127.0.0.1', {port}).then(s => {{ globalThis.socket = s; globalThis.connected = true; s.read().then(v => globalThis.cancelled = v === null); }});")).unwrap();
    wait(&host, "globalThis.connected === true");
    host.eval_script("close", "globalThis.socket.close()")
        .unwrap();
    wait(&host, "globalThis.cancelled === true");
    peer.join().unwrap();
}

#[test]
fn closing_socket_cancels_pending_write() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    // The peer never reads, so the kernel's buffers fill and writes wait.
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    let peer = std::thread::spawn(move || {
        let (_stream, _) = listener.accept().unwrap();
        let _ = done_rx.recv_timeout(Duration::from_secs(10));
    });
    let host = Host::new().unwrap();
    host.set_system_access(sockets(port), None);
    host.eval_source(&format!(
        r#"import {{connect}} from 'net';
        globalThis.failure = '';
        connect('127.0.0.1', {port}).then(s => {{
            globalThis.socket = s;
            const chunk = 'x'.repeat(1048576);
            let limited = false;
            const writes = [];
            for (let i = 0; i < 5; i++) {{
                try {{ writes.push(s.write(chunk).then(() => 'sent', e => String(e))); }}
                catch (e) {{ limited = e instanceof RangeError && String(e).includes('4194304 byte limit'); }}
            }}
            if (!limited) globalThis.failure = 'pending writes must be capped';
            globalThis.writes = Promise.all(writes);
            globalThis.connected = true;
        }});"#
    ))
    .unwrap();
    wait(&host, "globalThis.connected === true");
    let start = Instant::now();
    host.eval_script(
        "close",
        "globalThis.socket.close(); globalThis.writes.then(r => { globalThis.results = r; }); true",
    )
    .unwrap();
    wait(
        &host,
        "Array.isArray(globalThis.results) && globalThis.results.some(r => r.includes('socket is closed'))",
    );
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(
        host.eval_script("result", "globalThis.failure === ''")
            .unwrap()
    );
    done_tx.send(()).unwrap();
    peer.join().unwrap();
}

#[test]
fn sockets_are_capped_per_vm() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let host = Host::new().unwrap();
    host.set_system_access(sockets(port), None);
    host.eval_source(&format!(
        r#"import {{connect}} from 'net';
        const pending = [];
        for (let i = 0; i < 32; i++) pending.push(connect('127.0.0.1', {port}));
        globalThis.capped = false;
        try {{ connect('127.0.0.1', {port}); }} catch (e) {{ globalThis.capped = e instanceof RangeError; }}
        Promise.all(pending).then(all => {{
            all[0].close();
            connect('127.0.0.1', {port}).then(s => {{ s.close(); globalThis.reused = true; }});
        }});"#
    ))
    .unwrap();
    wait(&host, "globalThis.capped === true");
    wait(&host, "globalThis.reused === true");
    drop(listener);
}
