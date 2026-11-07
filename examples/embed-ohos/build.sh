#!/usr/bin/env bash
# Assemble the example with a previously built native SDK and nsh CLI.
set -euo pipefail
APP="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$APP/../.." && pwd)"
PROFILE="${NSH_PROFILE:-debug}"
SDK="$ROOT/target/ohos/$PROFILE"
: "${OHOS_NDK_HOME:?Set OHOS_NDK_HOME to the SDK openharmony directory}"
: "${DEVECO_CONTENTS:?Set DEVECO_CONTENTS to DevEco-Studio.app/Contents}"
export DEVECO_SDK_HOME="$DEVECO_CONTENTS/sdk"
export PATH="$DEVECO_CONTENTS/tools/node/bin:$PATH"

mkdir -p "$APP/entry/libs/arm64-v8a"
cp "$SDK/arm64-v8a/libnsh_platform_ohos.so" "$APP/entry/libs/arm64-v8a/"
cp "$SDK/index.d.ts" "$APP/entry/src/main/cpp/types/libnsh_platform_ohos/Index.d.ts"
cp "$OHOS_NDK_HOME/native/llvm/lib/aarch64-linux-ohos/libc++_shared.so" "$APP/entry/libs/arm64-v8a/"

BUILD_FLAGS=()
if [[ "${NSH_SPLIT:-1}" == 1 ]]; then BUILD_FLAGS+=(--split); fi
"${NSH_CLI:-$ROOT/target/debug/nsh}" build "$ROOT/${NSH_EXAMPLE:-examples/gallery}" \
  -o "$APP/entry/src/main/resources/rawfile" ${BUILD_FLAGS[@]+"${BUILD_FLAGS[@]}"}

cd "$APP"
"$DEVECO_CONTENTS/tools/ohpm/bin/ohpm" install
"$DEVECO_CONTENTS/tools/hvigor/bin/hvigorw" assembleHap --no-daemon --mode module -p product=default -p buildMode=debug --no-hvigorw-daemon
