fn main() {
    println!("cargo:rerun-if-changed=native/drwav_bench.c");
    println!("cargo:rerun-if-changed=native/dr_wav.h");
    #[cfg(feature = "bench-c")]
    {
        let c = std::path::Path::new("native/drwav_bench.c");
        if !c.exists() {
            println!("cargo:warning=bench-c skipped: native/ is not in the crates.io package");
            return;
        }
        cc::Build::new()
            .file("native/drwav_bench.c")
            .include("native")
            .flag_if_supported("-Wno-unused-function")
            .compile("ryf_drwav_bench");
    }
}
