fn main() {
    println!("cargo:rerun-if-changed=native/drwav_bench.c");
    println!("cargo:rerun-if-changed=native/dr_wav.h");
    #[cfg(feature = "bench-c")]
    {
        cc::Build::new()
            .file("native/drwav_bench.c")
            .include("native")
            .flag_if_supported("-Wno-unused-function")
            .compile("ryf_drwav_bench");
    }
}
