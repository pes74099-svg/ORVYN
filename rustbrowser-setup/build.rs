fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/orvyn.ico");
        res.set("ProductName", "ORVYN");
        res.set("FileDescription", "ORVYN Setup");
        res.set("LegalCopyright", "© 2026 ORVYN");
        res.compile().expect("failed to compile Windows icon resource");
        // На MSVC winres сам подключает resource.lib, а .o нужен только GNU-линкеру.
        if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
            let out = std::env::var("OUT_DIR").expect("OUT_DIR");
            let obj = std::path::Path::new(&out).join("resource.o");
            println!("cargo:rustc-link-arg={}", obj.display());
        }
    }
}
