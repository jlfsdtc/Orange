fn main() {
    // Compile streamvbyte from vendored source
    let svb_dir = "vendor/streamvbyte";

    let mut build = cc::Build::new();
    build
        .include(format!("{}/include", svb_dir))
        .file(format!("{}/src/streamvbyte_encode.c", svb_dir))
        .file(format!("{}/src/streamvbyte_decode.c", svb_dir))
        .file(format!("{}/src/streamvbytedelta_encode.c", svb_dir))
        .file(format!("{}/src/streamvbytedelta_decode.c", svb_dir))
        .file(format!("{}/src/streamvbyte_zigzag.c", svb_dir));

    // Platform-specific SIMD sources
    if cfg!(target_arch = "x86_64") {
        build
            .file(format!("{}/src/streamvbyte_x64_encode.c", svb_dir))
            .file(format!("{}/src/streamvbyte_x64_decode.c", svb_dir))
            .file(format!("{}/src/streamvbytedelta_x64_encode.c", svb_dir))
            .file(format!("{}/src/streamvbytedelta_x64_decode.c", svb_dir))
            .file(format!("{}/src/streamvbyte_0124_encode.c", svb_dir))
            .file(format!("{}/src/streamvbyte_0124_decode.c", svb_dir));
    } else if cfg!(target_arch = "aarch64") {
        build
            .file(format!("{}/src/streamvbyte_arm_encode.c", svb_dir))
            .file(format!("{}/src/streamvbyte_arm_decode.c", svb_dir));
    }

    build.flag_if_supported("-march=native");
    build.flag_if_supported("-O3");
    // Silence `-Wunused-function` from upstream streamvbyte sources — those
    // helpers are kept by upstream for ABI reasons and we don't patch vendor.
    build.flag_if_supported("-Wno-unused-function");
    build.compile("streamvbyte");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=vendor/streamvbyte");
}
