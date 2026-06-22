fn main() {
    // Embed the application icon. embed-resource compiles the .rc only for
    // Windows targets and is a no-op elsewhere, so this is safe on any host.
    embed_resource::compile("app.rc", embed_resource::NONE);
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=assets/brightvol.ico");
}
