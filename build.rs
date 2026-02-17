#[cfg(target_os = "windows")]
fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/favicon.ico");
    res.set("ProductName", "RustySound");
    res.set("FileDescription", "RustySound");
    res.set("InternalName", "RustySound");
    res.set("OriginalFilename", "rustysound.exe");
    res.set("CompanyName", "AD-Archer");
    res.set("LegalCopyright", "Copyright (c) 2026 AD-Archer");
    if let Err(err) = res.compile() {
        panic!("failed to compile Windows resources: {err}");
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {}
