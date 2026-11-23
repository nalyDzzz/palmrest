//! Puts the icon and the program's name into the exe, where Explorer and the task bar look for them.

fn main() {
    println!("cargo:rerun-if-changed=assets/palmrest.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("assets/palmrest.ico");
    resource.set("ProductName", "Palmrest");
    resource.set("FileDescription", "Palmrest");
    // Without the Windows SDK's resource compiler the program still builds, only without its icon.
    if let Err(error) = resource.compile() {
        println!("cargo:warning=the exe gets no icon: {error}");
    }
}
