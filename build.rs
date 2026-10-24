fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icons/xvw.ico");
        if let Err(e) = res.compile() {
            eprintln!("Failed to compile Windows resources: {e}");
        }
    }

    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg=/stack:{}", 8 * 1024 * 1024);
    }
}
