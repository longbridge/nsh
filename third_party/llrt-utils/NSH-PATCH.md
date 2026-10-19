# LLRT utilities portability patch

Copied from awslabs/llrt revision `d9fd873e793c6551c5d92b2e31480f3279e3cd8d`,
`libs/llrt_utils` (Apache-2.0; upstream LICENSE and NOTICE are included).

The pinned upstream signal helpers define OS implementations only for Unix and
Windows while compiling callers on every target. This prevents every dependent
standard module from compiling for wasm32-unknown-unknown. The local patch adds
an Unsupported result on other targets, preserving native behavior. The two
upstream development/build path dependencies point to the same pinned git source.
Remove this patch when the upstream version handles non-native signal targets.
