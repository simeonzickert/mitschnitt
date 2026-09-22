fn main() {
    let is_release = std::env::var("PROFILE").as_deref() == Ok("release");
    // CARGO_PRIMARY_PACKAGE ist nur fuer das direkt gebaute Crate gesetzt, nicht
    // fuer seine (dev-)dependencies. Ein nicht-primaeres Crate im Release-Modus
    // ist eine dev-dependency von `cargo test --release` und damit in Ordnung.
    let is_primary = std::env::var("CARGO_PRIMARY_PACKAGE").is_ok();

    if is_release && is_primary {
        panic!(
            "\n\nfixtures ist ein reines Test-Crate.\nNicht unter [dependencies] eintragen, sondern unter [dev-dependencies].\n"
        );
    }
}
