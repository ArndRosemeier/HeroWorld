//! Puts the icon into the Windows exe, so Explorer and the taskbar show it.
fn main() {
    println!("cargo:rerun-if-changed=icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("icon.ico");
        res.set("FileDescription", "HeroWorld Launcher");
        res.set("ProductName", "HeroWorld");
        if let Err(e) = res.compile() {
            panic!("could not add the launcher icon: {e}");
        }
    }
}
