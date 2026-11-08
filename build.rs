// On Windows, put the Senthym icon and name inside the .exe. Nothing happens on other systems.
fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Senthym");
        res.set("FileDescription", "Senthym");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the icon: {e}");
        }
    }
}
