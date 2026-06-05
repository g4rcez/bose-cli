fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .file("src/macos_rfcomm.m")
            .flag("-fobjc-arc")
            .compile("macos_rfcomm");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=IOBluetooth");
    }
}
