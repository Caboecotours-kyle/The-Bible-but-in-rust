fn main() {
    // Windows shows the icon stored inside the program file. The resource compiler is
    // only available when building on Windows, so a Windows build made elsewhere has no icon.
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=assets/icon/icon.ico");
        winresource::WindowsResource::new()
            .set_icon("assets/icon/icon.ico")
            .compile()
            .expect("the Windows icon embeds");
    }
}
