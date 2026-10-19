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
    let a = access(serde_json::json!({"network":{"http":[{"host":"127.0.0.1"}]}}));
    assert!(!a.allows_socket("127.0.0.1", 1234));
    let mut a = access(serde_json::json!({"network":{"hosts":["127.0.0.1"]}}));
    assert!(a.allows_socket("127.0.0.1", 1234));
    assert!(!a.allows_socket("127.0.0.1", 0));
    a.granted = Some(Capabilities::default());
    assert!(!a.allows_socket("127.0.0.1", 1234));
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
    host.set_system_access(
        access(serde_json::json!({"network":{"hosts":["127.0.0.1"]}})),
        None,
    );
    host.eval_source(&format!(
        r#"import net from 'node:net';
        globalThis.done = false; globalThis.failure = '';
        (async () => {{ const socket = await net.connect('127.0.0.1', {port});
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
    host.set_system_access(
        access(serde_json::json!({"network":{"hosts":["127.0.0.1"]}})),
        None,
    );
    host.eval_source(&format!("import {{connect}} from 'net'; connect('127.0.0.1', {port}).then(s => {{ globalThis.socket = s; globalThis.connected = true; s.read().catch(() => {{}}); }});")).unwrap();
    wait(&host, "globalThis.connected === true");
    drop(host);
    assert_eq!(rx.recv_timeout(Duration::from_secs(3)).unwrap().unwrap(), 0);
    peer.join().unwrap();
}
#[test]
fn denied_socket_rejects_before_connecting() {
    let host = Host::new().unwrap();
    host.set_system_access(access(serde_json::json!({})), None);
    host.eval_source("import {connect} from 'net'; globalThis.denied = false; connect('127.0.0.1', 9).catch(e => globalThis.denied = String(e).includes('capabilities.network.hosts'));").unwrap();
    wait(&host, "globalThis.denied === true");
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
    host.set_system_access(
        access(serde_json::json!({"network":{"hosts":["127.0.0.1"]}})),
        None,
    );
    host.eval_source(&format!("import {{connect}} from 'net'; connect('127.0.0.1', {port}).then(s => {{ globalThis.socket = s; globalThis.connected = true; s.read().catch(e => globalThis.cancelled = true); }});")).unwrap();
    wait(&host, "globalThis.connected === true");
    host.eval_script("close", "globalThis.socket.close()")
        .unwrap();
    wait(&host, "globalThis.cancelled === true");
    peer.join().unwrap();
}
