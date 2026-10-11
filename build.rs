fn main() {
    // Windows shows the icon stored inside the program file.
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/icon/icon.ico");
        winresource::WindowsResource::new()
            .set_icon("assets/icon/icon.ico")
            .compile()
            .expect("the Windows icon embeds");
    }
}
