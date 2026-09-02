# native/

Vendored **dr_wav** for Criterion only. Not linked into the `ryf` library.

| File | What |
|---|---|
| `dr_wav.h` | [mackron/dr_libs](https://github.com/mackron/dr_libs) `@dfe8377631000664666519fdb83da193fd8037f4` (v0.14.6). Public domain or MIT-0. |
| `drwav_bench.c` | Thin memory decode (`f32`) / encode (s16, f32) FFI. |

Enable with `--features bench-c` (`cc` compiles this). Product path stays
pure Rust, zero C.
