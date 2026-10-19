use super::run;

#[test]
#[cfg(not(target_os = "windows"))]
fn url_module_keeps_native_url_brands_and_decodes_file_paths() {
    let out = run(r#"
        const constructors = [URL, URLSearchParams, Blob, File, Event, ReadableStream, AbortController];
        const url = await import('node:url');
        await import('node:zlib');
        const native = new URL('https://a%20b:p%40ss@xn--fiq228c.com:8443/你好?q=1#tail');
        const file = url.pathToFileURL('/你好/a b#%.txt');
        const options = url.urlToHttpOptions(native);
        const blob = await new Response('native').blob();
        const originalURL = URL;
        globalThis.URL = function () { throw new Error('replacement constructor must not be used'); };
        const branded = url.pathToFileURL('/a');
        globalThis.URL = originalURL;
        globalThis.out = [url.URL === URL, url.URLSearchParams === URLSearchParams,
            constructors.every((v,i) => v === [URL,URLSearchParams,Blob,File,Event,ReadableStream,AbortController][i]),
            file instanceof URL, file.href, url.fileURLToPath(file), blob instanceof Blob,
            url.format(native, {auth:false,fragment:false,search:false,unicode:true}),
            options.hostname, options.port, options.auth, options.hash, options.path,
            branded instanceof URL, url.format(new URL('https://[::1]/x'), {unicode:true})];
    "#);
    assert_eq!(
        out,
        r##"[true,true,true,true,"file:///%E4%BD%A0%E5%A5%BD/a%20b%23%25.txt","/你好/a b#%.txt",true,"https://中文.com:8443/%E4%BD%A0%E5%A5%BD","xn--fiq228c.com",8443,"a b:p@ss","#tail","/%E4%BD%A0%E5%A5%BD?q=1",true,"https://[::1]/x"]"##
    );
}

#[test]
fn compression_cap_applies_to_named_default_sync_and_callback_exports() {
    let out = run(r#"
        import zlib, { gzipSync, gunzipSync, gunzip } from 'zlib';
        import { Buffer } from 'buffer';
        const packed = gzipSync(Buffer.alloc(16 * 1024 * 1024 + 1));
        const errors = [];
        for (const inflate of [gunzipSync, zlib.gunzipSync]) {
            try { inflate(packed); errors.push(false); }
            catch(error) { errors.push(error instanceof RangeError); }
        }
        for (const inflate of [gunzip, zlib.gunzip]) {
            errors.push(await new Promise(resolve => inflate(packed, error => resolve(error instanceof RangeError))));
        }
        for (const maxOutputLength of [Infinity, NaN, -1, 0, 1.5, 16777217, null, '16777216']) {
            try { gunzipSync(packed, {maxOutputLength}); errors.push(false); }
            catch(error) { errors.push(error instanceof RangeError); }
        }
        let reads = 0;
        const inherited = Object.create({ get maxOutputLength() { reads++; return 4; } });
        try { gunzipSync(gzipSync('hello'), inherited); errors.push(false); }
        catch(error) { errors.push(error instanceof RangeError); }
        globalThis.out = [errors.every(Boolean), errors.length, reads,
            zlib.gunzipSync(zlib.gzipSync('你好')).toString()];
    "#);
    assert_eq!(out, r#"[true,13,1,"你好"]"#);
}
