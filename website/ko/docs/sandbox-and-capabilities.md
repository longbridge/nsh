---
title: 샌드박스와 권한
description: "NSH 앱의 메모리·시간·노드 제한, 릴리스 빌드의 보안 강화, nsh.json의 권한 선언과 호스트 앱이 부여하는 권한을 설명합니다."
order: 21
group: delivery
---

# 샌드박스와 권한

Rust 앱은 배포하는 앱에 네이티브 코드로 컴파일되어 호스트 앱과 같은 신뢰 수준에서 실행됩니다. Rust에서 NSH 서비스(`fetch`, 이미지, 웹 뷰, 상태 표시줄)를 호출할 때도 TSX와 동일하게 호스트가 부여한 권한에 따라 네이티브 런타임의 검사를 거칩니다. 앱에 포함한 TSX와 JavaScript, 또는 내려받은 [서명된 패키지](../../docs/signed-packages.md)의 코드는 뷰마다 별도의 QuickJS 가상 머신(VM)에서 실행됩니다. VM 외부에 접근하려면 언제나 NSH 네이티브 런타임을 거칩니다. 호스트 앱은 다음 규칙을 통해 서드파티 미니 앱이나 내려받은 업데이트처럼 다른 사람이 작성한 코드의 실행 범위를 제어합니다.

- **리소스 제한**은 앱 하나가 사용할 수 있는 메모리, 시간, 네이티브 리소스의 양을 제한합니다.
- **보안 강화**는 릴리스 빌드에서 문자열을 실행 가능한 코드로 바꾸는 경로를 차단합니다.
- **권한(capabilities)**은 앱이 데이터를 요청할 수 있는 호스트, 웹 뷰에 표시할 페이지, 상태 표시줄 변경 가능 여부 등 외부에 접근할 범위를 정합니다. 앱이 선언한 권한을 호스트가 더 좁게 부여할 수 있습니다.

번들에는 앱 코드만 들어갑니다. 반응형 그래프, 렌더러, 앱이 가져오는 모든 모듈은 네이티브 코드입니다. 런타임은 JS 코드를 신뢰하지 않고 모든 제한과 검사를 Rust에서 적용하므로, 앱이 JS 코드를 바꿔 검사를 우회할 수 없습니다.

## 리소스 제한

리소스 제한은 VM에서 실행되는 TSX와 JavaScript에 적용됩니다. 다른 앱에 내장한 앱에는 호스트 앱이 정한 제한이 적용됩니다. `nsh package`로 패키징하거나(웹 포함) `nsh run`으로 실행한 독립 실행형 앱은 일반 네이티브 앱과 마찬가지로 개발자 자신의 코드를 실행하므로 기기가 제공하는 메모리와 시간을 사용할 수 있습니다. 인터프리터 스택은 4096 KB로, 재귀가 너무 깊으면 앱이 처리할 수 있는 `RangeError`가 발생합니다.

호스트에 내장한 앱의 기본 제한은 다음과 같습니다.

| 제한 항목 | 기본값 | 초과 시 동작 |
|---|---|---|
| JS 힙 | 256 MB | VM이 중지됩니다. |
| 인터프리터 스택 | 4096 KB(최댓값) | 깊은 재귀 호출에서 앱이 처리할 수 있는 `RangeError`가 발생합니다. |
| 번들 평가 시간 | 10초 | 평가를 중단합니다. |
| 그 밖의 JS 호출 시간 | 2초 | 해당 호출을 중단하고 앱은 계속 실행합니다. |
| 유지하는 노드 수 | 200,000개 | VM이 중지됩니다. |
| 한 번에 처리할 변경 사항(대기 중인 연산) | 값 8,000,000개 | VM이 중지됩니다. |
| 캔버스 그리기 한 번의 연산 수 | 1,000,000개 | VM이 중지됩니다. |

**시간은 호출 단위로 계산합니다.** 호출 하나는 런타임이 JS에 요청하는 작업 하나입니다. 예를 들어 이벤트 하나와 그에 따른 렌더링을 처리하거나, 목록의 한 행을 바인딩하거나, 타이머 하나를 실행하거나, 한 프레임의 `requestAnimationFrame` 콜백들을 실행하거나, 마이크로태스크 큐를 비우는 작업입니다. 앱이 오래 실행되어도 호출 시간이 누적되지는 않습니다. 허용 시간 안에 반환하지 않는 개별 호출만 중단됩니다. 중첩된 호출은 바깥쪽 호출의 시간 한도를 공유합니다.

중단된 호출은 `catch`로도 막을 수 없는 예외로 끝나며 초과한 시간 한도가 오류 메시지에 표시됩니다.

```text
the script was interrupted: a call into JS exceeded its budget of 2000 ms
```

VM은 계속 실행되므로 다음 이벤트, 타이머, 프레임은 정상적으로 처리됩니다.

**메모리 부족이나 할당량 초과가 발생하면 VM이 중지됩니다.** 오류는 호스트 앱의 오류 핸들러에 한 번 보고되며 개발 빌드에서는 앱 화면 위에도 표시됩니다. 이후 JS 코드는 실행되지 않고 화면에는 마지막 뷰가 남습니다. 네이티브 뷰와 대기 중인 변경 사항은 JS 힙 제한이 적용되지 않는 네이티브 메모리에 저장됩니다. 노드 수와 연산 수 제한은 앱이 UI를 통해 기기의 메모리를 소진하는 일을 막습니다.

> [!TIP]
> 일반적인 앱에서는 기본 제한으로 충분합니다. 호출 시간이 2초에 이르면 그동안 화면 갱신도 멈춥니다. `setTimeout`으로 작업을 나누거나 이벤트 핸들러 밖으로 옮기세요.

호스트 앱은 뷰마다 제한을 설정합니다. [iOS](../../docs/ios.md#limits)는 `NshAppView.limits`, [Android](../../docs/android.md#limits)는 `NshView.setLimits`, [GPUI 앱](./gpui.md#gpui-앱에-임베딩)은 `AppOptions::limits` 또는 `AppView::set_limits`를 사용합니다. 설정은 다음에 로드할 때부터 적용됩니다.

개발 중 TSX 앱이 호스트의 제한 안에서 어떻게 동작하는지 확인하려면 [`--limits`](./cli.md#nsh-run)를 사용하세요. `nsh run App.tsx --limits`는 위 기본값을 적용합니다. `nsh run --limits memory=128MB,call=500ms`는 호스트 앱에서처럼 메모리와 호출 시간만 지정하고 나머지는 기본값을 사용합니다.

## 보안 강화

개발 서버에서 불러오는 경우를 제외한 모든 릴리스 빌드는 로드 시 다음 보호 조치를 적용합니다.

- `eval`을 제거하고 `Function`, `AsyncFunction`, `GeneratorFunction`, `AsyncGeneratorFunction` 생성자를 호출하면 예외를 발생시킵니다. NSH는 문자열로 만든 코드를 실행하지 않습니다.

  ```text
  TypeError: the Function constructor is disabled: nsh runs no code from strings (development builds allow it)
  ```

- 모든 모듈이 공유하는 프로토타입(`Object`, `Array`, `Function`, `String`, `Number`, `Promise`, `Map`, 타입 지정 배열 등)을 동결합니다. 따라서 모듈이나 라이브러리 하나가 `Array.prototype`을 바꿔 다른 모듈에 영향을 줄 수 없습니다. 오류 서브클래스가 `this.name`을 지정할 수 있도록 `Error.prototype`은 쓰기 가능한 상태로 둡니다.

일반적으로 프로토타입을 동결하면 상속된 속성을 재정의하는 관용 코드가 깨질 수 있습니다(이른바 “override mistake”). 예를 들어 상속받은 속성이 동결되어 있으면 `Sub.prototype.constructor = Sub`나 `obj.toString = …`에서 예외가 발생합니다. NSH는 자주 재정의하는 멤버(`constructor`, `toString`, `valueOf`, `toJSON` 등)를 접근자로 바꿉니다. 세터가 값을 대입한 객체 자체에 속성을 정의하므로 이 방식을 쓰는 코드나 axios 같은 라이브러리도 동작합니다.

`nsh dev`와 `nsh run`이 제공하는 번들에는 `--release` 사용 여부와 관계없이 이 보안 강화를 적용하지 않으므로 `eval`로 디버깅할 수 있습니다. 동적 코드에 의존한다면 배포 전에 보안 강화를 적용하는 `nsh test`나 패키징한 빌드로 동작을 확인하세요.

## 권한 선언

앱은 VM 외부에서 접근할 수 있는 기능을 `nsh.json`에 선언합니다.

```json
{
  "name": "Quotes",
  "id": "com.example.quotes",
  "version": "1.0.0",
  "build": 3,
  "capabilities": {
    "network": {
      "hosts": ["api.example.com", "*.cdn.example.com"],
      "http": [{ "host": "quotes.example.com", "methods": ["GET"], "pathPrefix": "/v1/" }],
      "sockets": ["feed.example.com:7000"]
    },
    "webview": { "hosts": ["*.example.com"], "input": true },
    "statusBar": true,
    "clipboard": true,
    "picker": true,
    "speech": true,
    "notifications": true,
    "host": ["getUserInfo", "bridge.*"],
    "openUrl": { "schemes": ["https", "mailto"] }
  }
}
```

**`"capabilities"`를 생략한 독립 실행형 앱은 기본적으로 제한 없이 동작합니다.** 다만 원시 TCP 소켓은 모든 앱에서 `network.sockets`에 대상을 명시해야 하며 호스트 함수도 `host`를 통해 권한을 부여해야 합니다. **`"capabilities"`를 지정하면 명시한 항목만 허용됩니다.** 알 수 없는 키는 빌드 오류이므로 오타로 누락된 권한을 실행 전에 발견할 수 있습니다. 더 최신 버전의 NSH로 만든 [서명된 패키지](../../docs/signed-packages.md#packages-from-newer-versions-of-nsh)가 이전 런타임이 모르는 권한을 선언하면 해당 런타임은 그 권한을 부여하지 않습니다.

### `network`

`fetch`(모든 리디렉션 포함), WebSocket, 원격 이미지(`<Image>`, `placeholder`, 캔버스 이미지, `Image.getSize`, `Image.prefetch`), 동영상 소스, 원시 TCP 소켓(`net.connect`)에 적용됩니다.

| 키 | 타입 | 설명 |
|---|---|---|
| `hosts` | `string[]` | 모든 메서드와 스킴(`http`, `https`, `ws`, `wss`)으로 접근을 허용할 호스트입니다. |
| `http` | `object[]` | 호스트와 아래 조건을 조합해 접근 범위를 좁히는 규칙입니다. |
| `http[].host` | `string` | 호스트 또는 호스트 패턴입니다. 필수입니다. |
| `http[].methods` | `string[]` | `["GET", "HEAD"]`처럼 허용할 메서드를 나열합니다. 생략하면 모두 허용합니다. |
| `http[].schemes` | `string[]` | 허용할 스킴입니다. 하나만 지정할 때는 문자열 `scheme`도 사용할 수 있습니다. 생략하면 `http`, `https`, `ws`, `wss`를 모두 허용합니다. |
| `http[].port` | `number` | 허용할 포트입니다. 포트가 없는 URL에는 스킴의 기본값(`https`는 443)을 사용합니다. 규칙에 포트를 지정하지 않으면 모든 포트를 허용합니다. |
| `http[].paths` | `string[]` | `["/v1/quote"]`처럼 정확히 일치해야 하는 경로입니다. 쿼리 문자열은 포함하지 않습니다. |
| `http[].pathPrefixes` | `string[]` | 경로 구간 단위로 검사할 접두사입니다. `/v1`은 `/v1`과 `/v1/quote` 모두, `/v1/`은 그 뒤에 이어지는 경로를 허용합니다. 단일 문자열 `pathPrefix` 및 `path_prefixes` 표기도 지원합니다. |
| `sockets` | `string[]` | [`net.connect`](../../modules/standard-modules.md)가 연결할 정확한 원시 TCP 대상입니다. 각각 `"host:port"`로 지정하며 IPv6는 `"[::1]:9000"`처럼 표기합니다. |

`http` 규칙에 `paths`와 `pathPrefixes`가 모두 없으면 모든 경로가 허용됩니다. 둘 다 지정하면 어느 한쪽과 일치하는 경로가 허용됩니다.

호스트 패턴은 `*`(모든 호스트), `*.example.com`(`example.com` 자체를 제외한 모든 하위 도메인) 또는 정확한 호스트 이름입니다. 검사할 때 대소문자와 포트는 무시합니다. 마지막 `@` 뒤에서 호스트를 읽으므로 `https://api.example.com@evil.io/`의 검사 대상은 `evil.io`입니다.

WebSocket 핸드셰이크는 HTTP GET 요청입니다. `ws://`는 `http://`, `wss://`는 `https://`로 검사하므로 해당 URL의 GET을 허용하는 `hosts` 또는 `http` 권한은 WebSocket도 허용합니다. 원시 소켓에는 HTTP 규칙을 적용할 수 없으므로 `sockets`에서만 정확한 호스트와 포트를 각각 지정해 허용합니다.

`asset:` URI(앱 자체 파일)는 항상 허용됩니다. 이미지와 동영상에는 `data:` URI나 상대 경로도 사용할 수 있습니다.

### `webview`

| 키 | 타입 | 설명 |
|---|---|---|
| `hosts` | `string[]` | [웹 뷰](../../docs/web-view-and-video.md)가 표시하거나 이동할 호스트입니다(`http`, `https`). 권한 목록에 `webview`를 포함하지 않으면 앱은 웹 페이지를 표시할 수 없습니다. |
| `input` | `boolean` | 웹 뷰의 ref를 통해 페이지에 신뢰할 수 있는 입력을 전달합니다([`sendInput`](../../docs/web-view-and-video.md#input-as-the-users-own)). `"capabilities"`가 없는 앱에서도 명시적으로 선언해야 부여됩니다. |

웹 뷰 내 최상위 페이지 이동도 로드 전에 플랫폼의 탐색 델리게이트가 검사합니다. 페이지 자체의 하위 리소스(스크립트, 이미지, XHR)는 웹 콘텐츠에서 관리하며 이 검사에 포함되지 않습니다. `about:blank`와 인라인 HTML(`html`)은 웹 뷰 사용 권한만 있으면 됩니다. `"webview": { "hosts": [] }`를 지정하면 원격 페이지를 허용하지 않고도 두 기능을 사용할 수 있습니다.

신뢰할 수 있는 입력은 `injectJavaScript`와 달리 사용자 활성화 상태를 전달하므로 `input`을 `hosts`와 분리합니다. 페이지는 이 권한으로 새 창을 열거나 클립보드에 쓰고, 시스템 파일 선택기, WebAuthn 또는 권한 요청을 시작하거나 `isTrusted`를 검사하는 버튼을 누를 수 있습니다. 권한이 없으면 코어가 `sendInput`을 무시하고 추가할 키를 로그에 기록합니다.

### `statusBar`

`"statusBar": true`는 상태 표시줄 변경을 허용합니다. 여기에는 [`nsh/base`](../../modules/nsh-base.md#status-bar)의 페이지와 대화 상자에서 쓰는 `statusBar`, `nsh.json`의 기본 `"statusBar"` 설정이 포함됩니다.

### `clipboard`

`"clipboard": true`는 시스템 클립보드 읽기·쓰기를 허용합니다([`Clipboard`](../../modules/nsh.md#clipboard)). 사용자가 선택 가능한 텍스트의 편집 메뉴에서 직접 복사하는 동작에는 이 권한이 필요하지 않습니다.

### `picker`

`"picker": true`는 시스템 사진 선택기를 열 수 있게 합니다([`Picker`](../../modules/nsh.md#picker)). 사용자가 선택한 항목만 앱에 전달됩니다.

### `speech`

`"speech": true`는 앱이 마이크와 시스템 음성 인식기를 사용해 사용자의 말을 텍스트로 받아오는 것을 허용합니다([`Speech`](../../modules/nsh.md#speech)). 시스템의 사용자 권한 요청은 별도로 한 번 표시됩니다.

### `notifications`

`"notifications": true`는 시스템 알림 표시와 앱 아이콘 배지 설정을 허용합니다([`Notifications`](../../modules/nsh.md#notifications)). 사용자 동의가 필요한 시스템에서는 별도의 권한 요청이 한 번 표시됩니다.

### `openUrl`

앱 외부에서 URL을 여는 권한입니다([`openUrl`](../../modules/nsh.md#openurl), `links="open"`을 지정한 `TextView` 링크). `"openUrl": true`는 모든 스킴을, `"openUrl": { "schemes": ["https", "mailto"] }`는 목록에 있는 스킴만 허용합니다. 허용되지 않은 스킴의 URL을 열면 `ERR_ACCESS_DENIED`로 거부되며, `permission`은 `"openUrl"`, `resource`는 해당 URL입니다. 코드를 실행하거나 앱 자체 데이터에 접근할 수 있는 `javascript:`, `data:`, `file:` 등의 URL은 이 권한과 관계없이 항상 차단됩니다.

### 저장소

모든 앱은 별도 권한 없이 [`Storage`](../../modules/nsh.md#storage)로 기기에 데이터를 저장할 수 있습니다. 저장소에는 해당 앱의 데이터만 들어가며 저장 위치는 호스트 앱이 결정합니다.

### `host`

`host`는 `nsh`의 `host.call(name, args)`로 호출할 수 있는 호스트 앱의 함수를 나열합니다. 정확한 이름(`"getUserInfo"`)이나 `*`로 끝나는 접두사를 지정합니다. 예를 들어 `"bridge.*"`는 `bridge.call`과 `bridge.share`에 일치합니다. `"host": true`는 모든 함수를 허용합니다. 호스트 함수는 기본으로 허용되지 않으므로 목록에 없는 함수 호출은 `"capabilities"`가 없는 앱에서도 거부합니다. `host` 권한 없이 `nsh`에서 `host`를 가져오는 앱을 빌드하면 모든 호출이 거부된다는 경고가 표시됩니다. 함수 이름은 1~128자로 `A`–`Z`, `a`–`z`, `0`–`9`, `_`, `.`, `:`, `-`만 사용할 수 있고 `..`는 포함할 수 없습니다. 유효하지 않은 이름은 권한을 확인하기 전에 거부합니다. 접두사는 `"bridge.*"`처럼 구분자로 끝내세요. `"bridge*"`는 `bridgeAdmin`까지 허용합니다. 호스트 이벤트(`host.on(event, listener)`)는 호스트가 보낸 대로 앱의 리스너에 전달됩니다.

```ts
import { host } from "nsh";

if (host.available()) {
  const user = await host.call("getUserInfo");
  const stop = host.on("loginChanged", (payload) => console.log(payload));
  stop();
}
```

`host.available()`은 호스트를 내장한 플랫폼에서 참입니다. 그 밖의 환경에서 `host.call`은 `code`가 `"ERR_FEATURE_UNAVAILABLE_ON_PLATFORM"`인 오류로 거부됩니다.

### 권한이 거부되면

| 요청 | 결과 |
|---|---|
| `fetch` | 다른 요청 실패와 마찬가지로 Promise가 `TypeError`로 거부됩니다. `code`는 `ERR_ACCESS_DENIED`, `permission`은 `"network"`, `resource`는 요청 또는 리디렉션 URL입니다. 네트워크로 요청을 보내지 않습니다. |
| `Image.getSize`, `Image.prefetch` | Promise가 `ERR_ACCESS_DENIED`로 거부되며 `permission`은 `"network"`, `resource`는 URL입니다. 이미지를 요청하지 않습니다. |
| 원격 이미지, 동영상 소스, 웹 뷰 소스 또는 `html` | 아무것도 표시하지 않고 요소의 `onError`에 `code`가 `"ERR_ACCESS_DENIED"`인 오류를 전달합니다. 웹 뷰에서 허용되지 않은 호스트로 이동하는 경우도 같은 방식으로 취소하고 보고합니다. |
| 상태 표시줄 변경, 캔버스 이미지 | 상태를 변경하거나 이미지를 그리지 않고 거부 사실을 로그에 기록합니다. |
| 클립보드 | `Clipboard`의 모든 함수가 `ERR_ACCESS_DENIED`로 거부됩니다. `permission`은 `"clipboard"`, `resource`는 `""`이며 시스템 클립보드에 접근하지 않습니다. |
| 사진 선택기 | `pickImages`는 선택기를 표시하지 않고 `ERR_ACCESS_DENIED`로 거부됩니다. `permission`은 `"picker"`, `resource`는 `""`입니다. |
| 음성 인식 | `Speech.start`는 `ERR_ACCESS_DENIED` 예외를 발생시킵니다. `permission`은 `"speech"`, `resource`는 `""`이며 마이크는 켜지지 않습니다. |
| 알림 | `Notifications`의 각 함수가 `ERR_ACCESS_DENIED`로 거부되거나 예외를 발생시킵니다. `permission`은 `"notifications"`, `resource`는 `""`이며 아무것도 표시하지 않습니다. |
| `openUrl` | Promise가 `ERR_ACCESS_DENIED`로 거부됩니다. `permission`은 `"openUrl"`, `resource`는 URL이며 아무것도 열리지 않습니다. |
| 호스트 함수 호출 | `host.call`은 `ERR_ACCESS_DENIED`로 거부되고 `host.post`는 예외를 발생시킵니다. `permission`은 `"host"`, `resource`는 함수 이름이며 호스트에는 호출이 전달되지 않습니다. |
| 파일(`fs/promises`) | Promise가 `ERR_ACCESS_DENIED`로 거부됩니다. `permission`은 `"fs"`, `resource`는 경로이며 디스크에 접근하지 않습니다. |
| 프로세스(`process`) | `run`은 거부되고 `spawn`과 `exit`는 `ERR_ACCESS_DENIED` 예외를 발생시킵니다. `permission`은 `"process"`, `resource`는 명령(`exit`는 `"exit"`)이며 자식 프로세스는 시작되지 않습니다. |
| TCP 소켓(`net.connect`) | DNS 조회 전에 Promise가 `ERR_ACCESS_DENIED`로 거부됩니다. `permission`은 `"network"`, `resource`는 `"host:port"`입니다. |
| WebSocket | `websocket` 모듈의 `connect`는 `ERR_ACCESS_DENIED`로 거부됩니다. `permission`은 `"network"`, `resource`는 URL입니다. 전역 `WebSocket`은 연결에 실패했을 때와 같이 `error` 이벤트에 이어 `close` 이벤트(코드 1006)를 발생시킵니다. |

[표준 모듈](../../modules/standard-modules.md#failures)의 오류에는 `code`로 `"ERR_ACCESS_DENIED"`가 전달되므로 앱에서 권한 거부와 작업 실패를 구분할 수 있습니다. `permission`은 필요한 권한, `resource`는 요청 대상을 나타냅니다. [`nsh`](../../modules/nsh.md) 모듈도 같은 규칙을 따릅니다. Promise가 `ERR_ACCESS_DENIED`, `permission`, `resource`와 함께 거부되며 `fetch`는 다른 요청 실패와 마찬가지로 `TypeError`를 사용합니다.

```ts
import { Clipboard } from "nsh";

try {
  await Clipboard.writeText(link);
} catch (error) {
  if (error.code !== "ERR_ACCESS_DENIED") throw error;
  console.warn(`${error.permission} is not granted`); // "clipboard"
}
```

Rust 컴포넌트에서는 같은 권한 거부를 Rust API의 반환값으로 받습니다. 클립보드 읽기는 비어 있는 클립보드의 값, 파일 선택기는 빈 선택 결과, `open_url`은 `false`, `fetch`는 오류 메시지가 담긴 `Err`를 반환합니다.

각 거부 사유는 한 번씩 기록되며 추가해야 할 키도 함께 표시됩니다.

```text
fetch GET https://evil.example.org/x is not granted: add `evil.example.org` to capabilities.network in nsh.json
```

리디렉션은 NSH가 직접 따라가며 이동할 때마다 권한을 확인합니다. 스트리밍 요청 본문은 두 번 전송하지 않으므로 스트리밍 업로드 중 `307` 또는 `308` 리디렉션을 받으면 실패합니다.

### 권한 선언의 배포

권한 선언은 앱과 함께 전달됩니다.

- `nsh build`와 `nsh package`는 번들 옆에 `capabilities.json`을 작성합니다.
- `nsh publish`는 선언을 [서명된 패키지](../../docs/signed-packages.md) 헤더에 넣어 서명 대상에 포함합니다.
- `nsh dev`는 빌드할 때마다 선언을 전송하므로 개발 환경에서도 같은 규칙이 적용됩니다. `capabilities`를 변경하면 핫 패치 대신 앱을 다시 로드합니다.

## 호스트 앱이 부여하는 권한

NSH를 내장한 호스트 앱은 같은 JSON 형식으로 허용할 권한의 **상한**을 정할 수 있습니다.

```rust
// iOS, in Rust (nsh_platform_ios::AppView)
let grant = nsh_engine::Capabilities::from_json(&serde_json::json!({
    "network": { "hosts": ["api.example.com"] }
}))?;
app.set_granted_capabilities(Some(grant));
```

```swift
// iOS
try app.grantCapabilities(#"{ "network": { "hosts": ["api.example.com"] } }"#)
```

```java
// Android
nsh.setGrantedCapabilities("{ \"network\": { \"hosts\": [\"api.example.com\"] } }");
```

요청은 **앱의 선언과 호스트의 허용 범위를 모두** 충족해야 합니다. 위 예시에서는 앱이 `"hosts": ["*"]`를 선언해도 `api.example.com`에만 접근할 수 있으며 웹 페이지 표시나 상태 표시줄 변경은 허용되지 않습니다. 호스트가 별도로 범위를 지정하지 않으면(기본값 `nil` 또는 `null`) 각 앱의 선언이 적용됩니다.

호스트의 권한 설정은 다음에 로드할 때부터 적용됩니다. 서드파티 미니 앱이라면 각 앱에 자체 API 호스트만 허용하고, 선언을 검토한 [서명된 패키지](../../docs/signed-packages.md)에서 실행하며, 리소스 제한은 기본값을 유지하거나 더 낮출 수 있습니다.

> [!WARNING]
> 호스트가 등록하는 [네이티브 컴포넌트](../../docs/native-components.md)는 호스트 자체 코드이므로 이 권한 검사의 적용을 받지 않습니다. 해당 뷰에서 실행할 모든 앱에 안전한 컴포넌트만 등록하세요.

## 요약

| 계층 | 적용 주체 | 설정 주체 |
|---|---|---|
| 리소스 제한(힙, 스택, 시간, 노드) | VM별 런타임 | 내장 앱에서는 호스트가 뷰별 설정 |
| 보안 강화 | 로드 시 런타임 | 개발 환경 외에는 항상 적용 |
| 권한 | 런타임(네트워크 요청, 이미지, 동영상, 웹 뷰, 상태 표시줄, 파일, 프로세스, 소켓) | 앱의 선언(`nsh.json`)과 호스트 앱의 허용 범위 |
| 코드 무결성 | 서명 검증 | `nsh publish` 및 호스트 앱이 신뢰하는 루트 키 |

## 참고 문서

- [서명된 패키지](../../docs/signed-packages.md)
- [네트워킹](../../docs/networking.md)
- [iOS 앱에 삽입하기](../../docs/ios.md), [Android 앱에 삽입하기](../../docs/android.md)
