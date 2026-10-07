// The web UI is embedded into the binary. Make sure the folder exists so the
// backend also builds (with a placeholder page) before the UI was built.
fn main() {
    let dist = std::path::Path::new("web/dist");
    if !dist.join("index.html").exists() {
        std::fs::create_dir_all(dist).expect("create web/dist");
        std::fs::write(
            dist.join("index.html"),
            "<!doctype html><title>Monarch</title><p>The web UI has not been built. Run <code>npm run build</code> in <code>web/</code>.</p>",
        )
        .expect("write placeholder");
    }
    println!("cargo:rerun-if-changed=web/dist");
    println!("cargo:rerun-if-changed=migrations");
}
