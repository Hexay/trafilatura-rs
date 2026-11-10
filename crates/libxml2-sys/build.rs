// Mirrors lxml's wheel build of libxml2 (buildlibxml.py: defaults + iconv, no zlib/lzma/python); only the
// switches that cannot affect parsing or serialization are turned off.
fn main() {
    let off = [
        "LIBXML2_WITH_PYTHON",
        "LIBXML2_WITH_PROGRAMS",
        "LIBXML2_WITH_TESTS",
        "LIBXML2_WITH_ZLIB",
        "LIBXML2_WITH_LZMA",
        "LIBXML2_WITH_ICU",
        "LIBXML2_WITH_HTTP",
        "LIBXML2_WITH_MODULES",
        "LIBXML2_WITH_CATALOG",
        "LIBXML2_WITH_DEBUG",
        "BUILD_SHARED_LIBS",
    ];
    let mut cfg = cmake::Config::new("libxml2");
    for opt in off {
        cfg.define(opt, "OFF");
    }
    // Release = -O3 -DNDEBUG (no fast-math, so no behaviour change).
    let dst = cfg.define("LIBXML2_WITH_ICONV", "ON").profile("Release").build();
    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-search=native={}/lib64", dst.display());
    println!("cargo:rustc-link-lib=static=xml2");
    // TODO: macOS/Windows need libiconv linked explicitly; glibc provides it.
    println!("cargo:rustc-link-lib=m");
    println!("cargo:include={}/include/libxml2", dst.display());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=libxml2");
}
