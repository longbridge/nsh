---
title: 하위 호환성
description: Native Shell이 릴리스 간에 기존 앱의 동작을 보장하는 범위와 이를 검증하는 nsh-compat 기록, 릴리스 절차, 호환성을 유지하며 API를 추가하거나 변경하는 방법을 설명합니다.
order: 22
group: delivery
---

# 하위 호환성

호스트 앱은 NSH 앱을 [서명 패키지](../../docs/signed-packages.md)로 내려받아 플러그인으로 실행합니다. 배포된 앱을 다시 빌드할 기회가 없을 수도 있습니다. 개발팀이 다른 업무를 맡거나, 소스가 업무 절차의 일부로 남거나, 호스트 앱이 자체 일정에 맞춰 NSH를 업데이트하기 때문입니다. 그래서 Native Shell은 **호환성이 보장되는 릴리스를 대상으로 빌드한 앱은 이후 모든 릴리스에서도 수정 없이 실행된다**는 원칙을 지킵니다. NSH 업데이트 때문에 기존 앱이 동작하지 않으면 NSH의 버그입니다.

이 문서에서는 호환성을 보장하는 범위와 검증 방법, 이 원칙을 지키면서 NSH를 변경하는 방법을 설명합니다.

## 앱이 의존할 수 있는 항목

호환성 보장은 빌드한 앱이 사용하는 다음 항목을 모두 포함합니다.

| 항목 | 보장하는 이유 |
| --- | --- |
| 모든 모듈(`nsh`, `nsh/*`, `nsh/base`, 표준 모듈)이 내보내는 항목과 각 멤버 | 번들이 이름으로 가져오므로 이름이 하나라도 사라지면 앱을 시작할 수 없습니다. |
| 컴파일된 JSX가 호출하는 `nsh:rt` 함수 | 번들에는 빌드에 사용한 컴파일러의 출력이 그대로 들어 있습니다. |
| 타이머, `fetch`, `console` 등의 전역 API와 그 멤버 | 앱이 이름으로 호출합니다. |
| 요소의 속성, 이벤트와 페이로드, 스타일 키와 값 | 앱이 이름으로 전달합니다. 속성 이름이 바뀌면 기존 속성이 무시되어 앱이 잘못 동작합니다. |
| Tailwind 유틸리티와 변형 | 런타임이 번들에 들어 있는 클래스 문자열을 해석합니다. |
| 기본값, 단위, 페이로드, 콜백 순서 등의 동작 | 앱 코드는 작성 당시 관찰한 동작에 의존합니다. |
| TypeScript 선언(`.nsh/types`) | 개발팀이 버그를 수정할 때 기존 앱 소스가 계속 타입 검사를 통과해야 합니다. |
| 패키지 형식과 서명 | 이전 릴리스에서 서명한 패키지도 계속 검증에 통과해야 합니다. |

이를 위해 다음 규칙을 적용합니다.

1. **배포한 API를 유지합니다.** 모듈, 내보낸 항목, 멤버, 속성, 이벤트, 페이로드 키, 스타일 키와 값, Tailwind 유틸리티, 전역 API, `nsh:rt` 함수를 제거하지 않습니다.
2. **이름을 바꿔도 이전 이름을 유지합니다.** 새 이름과 같은 구현을 가리키는 별칭으로 계속 제공하고, 새 이름과 함께 `@deprecated`를 표시합니다. 다른 모듈로 항목을 옮길 때도 기존 모듈에서 계속 내보냅니다.
3. **타입은 허용 범위를 넓히는 방향으로만 변경합니다.** 유니온 타입에 선택지를 추가하고, 함수에 오버로드나 끝부분의 선택적 매개변수를 추가하거나, 필수 매개변수를 선택적으로 바꿀 수 있습니다. 앱이 전달하는 속성, 스타일, 옵션에는 선택적 멤버만 추가합니다. 앱이 읽는 값도 같은 방식으로 확장됩니다. 예를 들어 `platform.os`에 플랫폼 하나가 추가되거나, 상태나 사유를 나타내는 타입에 새 경우가 생길 수 있습니다. 기존 경우의 값은 유지됩니다. 앱은 다음처럼 `switch`의 `default`에서 모르는 값을 처리합니다.

   ```tsx
   switch (platform.os) {
     case "ios":
     case "android":
       return <MobileLayout />;
     default:
       // The desktops, the web, and platforms NSH adds later.
       return <WideLayout />;
   }
   ```

4. **기존 동작을 유지합니다.** 새로운 동작은 앱이 새 속성, 옵션 또는 값을 지정할 때 적용합니다.
5. **`nsh:rt`에는 함수를 추가할 수 있습니다.** 기존 함수의 시그니처와 의미는 유지합니다.
6. **이전 형식을 계속 읽을 수 있습니다.** 이후 릴리스에서도 이전 릴리스의 패키지, 인증서, 폐기 목록, `app.json`을 읽습니다. 패키지는 필요한 최소 런타임 버전을 명시합니다(`nsh publish --min-runtime`).
7. **보안은 유일한 예외입니다.** 안전하게 유지할 수 없는 권한은 차단하고, 그 항목과 사유를 기록한 뒤 릴리스 노트에 알립니다.

이 보장은 호환 릴리스로 처음 기록된 버전부터 시작하며 이후 버전 번호와 관계없이 계속 적용됩니다. 릴리스 사이에 `main`에만 있는 API는 변경될 수 있습니다.

## 호환성 검증

릴리스에서 확정한 내용은 NSH 코드와 별도인 [longbridge/nsh-compat](https://github.com/longbridge/nsh-compat) 저장소에 보관합니다. NSH의 변경 사항을 이 기록과 대조하지만, NSH 쪽 PR에서 기록 자체를 바꿀 수는 없습니다. 따라서 API 이름을 바꾸는 PR에서 이전 이름의 기록까지 덮어쓸 수 없습니다.

```
nsh                                       nsh-compat
├─ crates/          the code              ├─ releases/v<version>/   one per compatible release, never changed
└─ tests/compat/    apps that test the    │  ├─ runtime.txt         every export, global and nsh:rt function
   ├─ cases/        API being written     │  ├─ types/              the declarations apps build against
   └─ examples/                           │  ├─ tailwind.txt        every Tailwind class and what it sets
                                          │  └─ apps/               the test apps as that release built them
                                          ├─ closed.txt             security closures, each with its reason
                                          └─ src/                   the checks
```

NSH에 제출된 모든 PR은 해당 체크아웃에 대해 `nsh-compat`의 검사를 실행합니다. CI의 **Compat** 워크플로나 로컬의 `cargo xtask compat`로 실행할 수 있습니다.

| 검사 | 통과 조건 |
| --- | --- |
| `checkout_apps_run` | 현재 체크아웃으로 빌드한 `tests/compat/`의 앱이 스크립트가 지정한 대로 동작하고, 검사한다고 명시한 API의 각 부분을 스크립트가 실제로 검증합니다. |
| `nothing_released_is_removed` | 기록된 각 릴리스의 내보낸 항목, 멤버와 멤버 종류, 타입 선언, Tailwind 클래스와 그 클래스가 지정하는 값이 현재도 동일하거나 확장된 형태로 제공됩니다. |
| `released_apps_run` | 기록된 각 릴리스의 컴파일러로 빌드한 앱이 그 빌드 결과 그대로 현재 런타임에서 시작되고, 스크립트가 지정한 대로 동작합니다. |

첫 번째 검사는 개발 중인 API를 검증하고, 나머지 두 검사는 이전에 기록한 모든 릴리스의 호환성을 검증합니다. 릴리스된 API의 이름을 바꾸고 이전 이름을 지우면 다음과 같이 실패합니다.

```
v0.2.0: runtime [nsh] Platform: removed
Keep it: a rename keeps the old name as a @deprecated alias of the new one, a type only widens.
```

NSH 유지보수자가 `nsh-compat` 변경 사항을 검토하며, 해당 저장소의 CI도 NSH의 `main`을 대상으로 검사를 실행합니다.

## 릴리스 절차

1. **개발.** 새 API를 검사하는 스크립트와 테스트 사례를 `tests/compat/`에 넣어 API와 함께 `main`에 반영합니다. 릴리스에 기록되기 전에는 이름이나 형태를 바꿀 수 있으며, 이때 테스트 사례도 함께 수정합니다.
2. **기록.** 릴리스할 때 `cargo xtask compat record <version>`을 실행하면 `nsh-compat` 체크아웃의 `releases/v<version>/`에 릴리스의 공개 API, 해당 컴파일러로 빌드한 `tests/compat/`의 모든 앱과 검증 스크립트를 저장합니다. 스크립트가 모든 부분을 검증한 API만 기록합니다.
3. **확정.** `nsh-compat`에 PR을 올려 기록을 병합합니다. 이후 NSH의 모든 변경 사항은 이 기록 및 앞선 모든 릴리스의 기록을 기준으로 검증합니다.

## 호환성을 지키며 개발하기

### API 추가

내보낸 항목, 전역 API, 속성, 이벤트, 스타일 키를 추가할 때마다 테스트 사례를 작성합니다.

1. `tests/compat/cases/<domain>/<case>.tsx`에 새 API를 사용하고 결과를 표시하는 컴포넌트를 작성해 기본값으로 내보냅니다. 도메인은 `base`, `core`, `events`, `layout`, `paint`, `props`, `modules`, `web`입니다. 특정 플랫폼 또는 `nsh.json` 설정 하나에 따라 결정되는 동작에는 `android`, `macos`, `browser`도 사용합니다.
2. 해당 도메인의 `App.tsx`에 사례를 추가합니다. 이 앱은 사례를 한 번에 하나씩 표시합니다.
3. 도메인의 `script.txt`에 `tap id:case-<case>`, API의 동작을 확인하는 단계, 새로 추가한 각 항목에 대한 `covers <item>` 단계를 차례로 덧붙입니다.
4. `cargo xtask compat`를 실행합니다. 한 도메인만 검사하려면 `NSH_COMPAT_APP=<domain>`을 설정합니다.

```txt
# props-media-video
tap id:case-props-media-video
prop id:clip src https://example.com/clip.mp4
prop id:clip paused true
tap id:play
prop id:clip paused false
covers [props] src
covers [props] paused
```

`covers` 단계 자체도 검증 대상입니다. 앱이 해당 항목을 실제로 사용해야 합니다. 속성, 이벤트, 스타일 키의 경우 빌드된 앱에서 해당 항목을 제거하면 스크립트의 검사 하나 이상이 실패해야 합니다. 항목은 `[<module>] <export>`, `[globals] <name>`, `[props] <name>`, `[events] <name>`, `[styles] <name>`으로 표기합니다. 컴포넌트 고유 속성은 `[props] <Component>.<name>`으로, 플랫폼별 속성은 `ios.<name>`, `android.<name>`, `desktop.<name>`, `web.<name>`으로 표기합니다.

### API 이름이나 위치 변경

새 이름을 추가하고, 기존 이름은 같은 구현의 별칭으로 유지하면서 새 이름을 안내하는 `@deprecated` 선언을 붙입니다. 테스트 사례와 스크립트는 새 이름으로 수정합니다. 어떤 릴리스에도 아직 기록되지 않은 API는 바로 이름을 바꿀 수 있습니다.

### 타입 변경

유니온 타입의 선택지, 오버로드, 끝부분의 선택적 매개변수, 앱이 전달하는 객체의 선택적 멤버를 추가해 타입을 확장합니다. 앱이 읽는 유니온 타입도 새로운 경우마다 멤버 하나를 추가하는 방식으로 확장합니다. 매개변수의 이름은 바꿀 수 있으며 기존 개수, 선택 여부, 타입에 대한 호출 규약은 유지합니다.

### 동작 변경

기존 동작을 유지하고 새 속성, 옵션, 값으로 새 동작을 선택할 수 있게 합니다. 버그 수정이라도 정상적으로 실행되던 앱이 관찰하는 결과가 달라지면 동작 변경입니다. 앱이 새 동작을 선택하도록 만들거나, 어떤 앱도 이전 결과에 의존할 수 없음을 확인하세요. 테스트 사례에 해당 동작을 검증하는 단계를 추가해 이후에도 이를 보장합니다.

### 컴파일러 변경

추가 절차는 없습니다. 각 릴리스의 테스트 앱은 그 버전의 컴파일러가 만든 결과 그대로 보관되며, 현재 테스트 사례는 현재 컴파일러로 새로 빌드합니다. 컴파일러가 번들에서 호출하도록 생성하는 `nsh:rt` 함수는 기존 항목을 유지한 채 확장됩니다.

### 보안을 위한 API 차단

먼저 `nsh-compat`에 PR을 올려 `closed.txt`에 항목과 사유를 추가하고 병합합니다. 형식은 `<entry> -- <reason>`이고, 항목 이름은 검사 출력에 표시된 대로 씁니다. 그 뒤 NSH에서 API를 제거하면 호환성 검사를 통과합니다. 차단한 이유는 릴리스 노트에 명시합니다.

## 스크립트

`script.txt`는 한 줄에 한 단계를 작성합니다. 조작에 사용할 수 있는 단계는 `tap <selector>`, `type <selector> <text>`, `key <selector> <key>`, `scroll <selector> <x> <y>`, `touch start|move|end|cancel`, `event <selector> <onEvent> [<key>=<value>...]`, `wait <ms>`, `keyboard <height> <duration>`, `pixelratio <ratio>`입니다. 검증에는 `expect <text>`, `absent <text>`, `frame <selector> <x> <y> <width> <height>`, `origin`, `prop` / `noprop`, `style` / `nostyle`, `listens`, `state`, `statusbar`, `log <text>`를 사용합니다. 스크립트 첫 부분에서 `viewport <w> <h>`, `platform <os> [desktop]`, `fontscale <scale>`로 화면 환경을 설정할 수 있습니다. 선택자는 `id:<testID>`, `label:<label>`, `text:<text>`, `span:<text>` 중 하나입니다. 속성과 스타일은 앱에서와 같은 형식으로 씁니다(`style id:card backgroundColor #3366ffff`). 스크립트를 작성할 때 `NSH_COMPAT_DUMP=1`을 설정하면 각 단계에서 화면에 추가되거나 사라지는 항목을 확인할 수 있습니다.

## 명령어

```sh
cargo xtask compat                      # the checks against this checkout
NSH_COMPAT_APP=props cargo xtask compat -v
cargo xtask compat record 0.2.0         # at a release: record it in nsh-compat, then open a pull request there
```

`cargo xtask compat`는 `target/nsh-compat/`에 `nsh-compat` 체크아웃을 유지하고 실행할 때마다 최신 상태로 갱신합니다. 로컬 변경 사항이 있는 체크아웃은 그대로 사용하므로 검사 변경을 PR로 제출하기 전에 NSH를 대상으로 시험할 수 있습니다.
