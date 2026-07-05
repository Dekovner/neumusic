fn main() {
    embed_resource::compile("assets/app.rc", embed_resource::NONE);

    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("windows") {
        println!("cargo:rustc-link-arg-bin=neumusic=-Wl,--subsystem,windows");
    }
}
