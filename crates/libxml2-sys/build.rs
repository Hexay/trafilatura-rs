// Mirrors lxml 6.1.3's libxml2 build (buildlibxml.py: defaults + iconv + zlib, threads on); only switches that
// cannot affect in-memory HTML parsing, XPath or serialization are off. See PATCHES.md "Build configuration".
use std::env;
use std::path::PathBuf;

fn main() {
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let msvc = env::var("CARGO_CFG_TARGET_ENV").unwrap() == "msvc";
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
    cfg.define("LIBXML2_WITH_ICONV", "ON");
    if os == "windows" {
        let lib = build_win_iconv(msvc);
        // Not canonicalize(): its \\?\ prefix breaks MSVC include paths.
        let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        cfg.define("Iconv_INCLUDE_DIR", manifest.join("win-iconv"));
        cfg.define("Iconv_LIBRARY", lib);
    }
    // Release = -O3 -DNDEBUG (no fast-math, so no behaviour change).
    let dst = cfg.profile("Release").build();
    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-search=native={}/lib64", dst.display());
    // CMake names the static MSVC library libxml2s.lib.
    println!("cargo:rustc-link-lib=static={}", if msvc { "libxml2s" } else { "xml2" });
    match os.as_str() {
        "windows" => println!("cargo:rustc-link-lib=bcrypt"),
        "macos" | "ios" => println!("cargo:rustc-link-lib=iconv"),
        _ => println!("cargo:rustc-link-lib=m"),
    }
    println!("cargo:include={}/include/libxml2", dst.display());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=libxml2");
    println!("cargo:rerun-if-changed=win-iconv");
}

/// Static win-iconv (Windows has no iconv; lxml's Windows wheels link GNU libiconv). Returns the library path.
fn build_win_iconv(msvc: bool) -> PathBuf {
    cc::Build::new().file("win-iconv/win_iconv.c").include("win-iconv").warnings(false).compile("iconv");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    out.join(if msvc { "iconv.lib" } else { "libiconv.a" })
}
