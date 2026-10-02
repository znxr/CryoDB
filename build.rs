fn main() {
    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("icons/icon.ico");
        resource.set("ProductName", "CryoDB");
        resource.set("FileDescription", "CryoDB");
        resource.set("CompanyName", "Mauricio Orquin [znxr]");
        resource.set(
            "LegalCopyright",
            "Copyright (c) 2026 Mauricio Orquin [znxr]",
        );
        resource.set("OriginalFilename", "cryodb.exe");
        resource.set("InternalName", "cryodb");
        resource
            .compile()
            .expect("embedding the Windows version resource");
    }
}
