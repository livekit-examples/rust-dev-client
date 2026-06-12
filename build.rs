fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("macos") | Ok("ios") => println!("cargo:rustc-link-arg=-ObjC"),
        _ => {}
    }
}
