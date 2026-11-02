// Recompila cuando cambian los archivos de traducción (`fl!` los lee en compilación).
fn main() {
    println!("cargo:rerun-if-changed=i18n");
    println!("cargo:rerun-if-changed=i18n.toml");
}
