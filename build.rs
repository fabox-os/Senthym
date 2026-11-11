// On Windows, put the Senthym icon and name inside the .exe. Nothing happens on other systems.
fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Senthym");
        res.set("FileDescription", "Senthym");
        res.set("FileVersion", "1.1.3");
        res.set("ProductVersion", "1.1.3");
        res.set_version_info(winresource::VersionInfo::FILEVERSION, 0x0001_0001_0003_0000);
        res.set_version_info(winresource::VersionInfo::PRODUCTVERSION, 0x0001_0001_0003_0000);
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the icon: {e}");
        }
    }
}
