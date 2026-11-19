// Windows: give Blip.exe its icon and version info.
fn main() {
    println!("cargo:rerun-if-changed=blip.rc");
    println!("cargo:rerun-if-changed=../../../assets/blip.ico");
    println!("cargo:rerun-if-env-changed=BLIP_BC_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=BLIP_BC_CLIENT_SECRET");
    #[cfg(windows)]
    {
        let v = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
        let parts: Vec<&str> = v.split('.').collect();
        let num = |i: usize| parts.get(i).and_then(|p| p.parse::<u32>().ok()).unwrap_or(0);
        let defs = [format!("VER_MAJOR={}", num(0)), format!("VER_MINOR={}", num(1)), format!("VER_PATCH={}", num(2)), format!("VER_STR=\"{v}\"")];
        let _ = embed_resource::compile("blip.rc", defs.iter()).manifest_optional();
    }
}
