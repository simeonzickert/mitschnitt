#![allow(clippy::single_match)]

// ABWEICHUNG VON crates.io (Mitschnitt, 22.09.2026) -- Grund: LGPL-3 Paragraf 4.
//
// mp3lame-sys 0.1.11 baut LAME statisch und linkt es fest ins Programm
// (`--disable-shared`, `cargo:rustc-link-lib=static=mp3lame`). Das gemessene
// Ergebnis: 67 `_lame_`-Symbole im ausgelieferten Binary. Damit ist Mitschnitt
// ein "Combined Work" im Sinne der LGPL, das der Empfaenger nicht mehr
// auseinandernehmen kann -- er kann die Bibliothek weder austauschen noch durch
// eine eigene Fassung ersetzen. Genau das verlangt Paragraf 4 aber.
//
// Deshalb baut dieser Zweig LAME auf macOS als DYNAMISCHE Bibliothek, setzt
// ihren Ladenamen auf `@rpath/libmp3lame.0.dylib` und legt eine Kopie unter
// `vendor/lame-dylib/` ab, von wo der Bundle-Schritt sie nach
// `Mitschnitt.app/Contents/Frameworks/` traegt. Der Empfaenger kann die Datei
// dort ersetzen.
//
// Linux und Windows bleiben unveraendert statisch -- dort wird heute nichts
// weitergegeben, und ein halber Umbau waere schlechter als keiner.
//
// Der Quelltext der Bibliothek liegt vollstaendig daneben in `lame-3.100/`.

const LAME_DIR: &str = "lame-3.100";

/// Der Dateiname, unter dem libtool die Bibliothek auf macOS ablegt.
#[cfg(unix)]
const MACOS_DYLIB: &str = "libmp3lame.0.dylib";

#[cfg(unix)]
fn build() {
    let mut config = autotools::Config::new(LAME_DIR);

    let host = std::env::var("HOST").expect("To have env:HOST");
    let target = std::env::var("TARGET").expect("To have env:TARGET");
    // Siehe Kopf der Datei: auf macOS dynamisch, sonst wie im Original statisch.
    let dynamisch = target.contains("apple-darwin");

    // ABWEICHUNG: der mpg123-Dekoder MUSS in der dynamischen Fassung mit hinein.
    // LAMEs eigenes `libmp3lame/Makefile.am` uebergibt beim Bau der Bibliothek
    // fest `-export-symbols include/libmp3lame.sym`, und diese Liste nennt auch
    // die Dekoder-Funktionen (`hip_*`, `lame_decode*`). Beim statischen Bau ist
    // das folgenlos, weil die Liste dort nicht ausgewertet wird. Beim
    // dynamischen Bau ist es ein Abbruch:
    //   "_lame_decode", referenced from: <initial-undefines>
    //   ld: symbol(s) not found for architecture arm64
    // (gemessen 22.09.2026 mit --disable-decoder). Der Dekoder-Code liegt danach
    // ungenutzt in der Bibliothek; das kostet ein paar Kilobyte und ist der
    // Preis dafuer, dass die Bibliothek eine vollstaendige, austauschbare LAME
    // ist -- was Paragraf 4 ohnehin verlangt.
    if dynamisch || cfg!(feature = "decoder") {
        config.enable("decoder", None);
    } else {
        config.disable("decoder", None);
    }

    if let Ok(override_host) = std::env::var("MP3LAME_SYS_OVERRIDE_HOST") {
        config.config_option("host", Some(override_host.as_str()));
    } else if host != target {
        #[cfg(not(feature = "target_host"))]
        {
            if target.contains("android") {
                //Assume cross-compilation for android target
                config.config_option("host", Some(target.as_str()));
            } else if target.contains("apple") {
                if target.starts_with("aarch64") {
                    config.config_option("host", Some("arm-apple-darwin"));
                } else if target.starts_with("x86_64") {
                    config.config_option("host", Some("x86_64-apple-darwin"));
                } else {
                    println!("cargo:warning=Unsupported Apple target");
                }
            } else {
                println!("cargo:warning=Cross-compilation may not be supported");
            }
        }
        #[cfg(feature = "target_host")]
        {
            config.config_option("host", Some(target.as_str()));
        }
    }

    //Android cross compilation may require this flag
    if target.contains("android") || target.contains("ios") {
        config.cflag("-DSTDC_HEADERS");
    }

    if dynamisch {
        config.enable_shared().disable_static();
    } else {
        config.disable_shared().enable_static();
    }

    let res = config.disable("rpath", None)
                    .disable("frontend", None)
                    .disable("gtktest", None)
                    .with("pic", None)
                    .fast_build(true)
                    .build();

    //libraries are installed in <out>/lib
    println!("cargo:rustc-link-search=native={}/lib", res.display());

    if dynamisch {
        macos_dylib_vorbereiten(&res.join("lib"));
        println!("cargo:rustc-link-lib=dylib=mp3lame");
    } else {
        println!("cargo:rustc-link-lib=static=mp3lame");
    }
}

/// Setzt den Ladenamen der frisch gebauten Bibliothek auf `@rpath/...` und legt
/// eine Kopie an einen festen Ort, den der Bundle-Schritt kennt.
///
/// Der Ladename MUSS vor dem Linken stehen: der Linker schreibt genau den Namen
/// ins Programm, den die Bibliothek in diesem Moment traegt. Waere er noch der
/// absolute Pfad im Bauordner, liefe die App nur auf diesem Rechner.
#[cfg(unix)]
fn macos_dylib_vorbereiten(libdir: &std::path::Path) {
    let dylib = libdir.join(MACOS_DYLIB);
    assert!(
        dylib.is_file(),
        "LAME wurde nicht als dynamische Bibliothek gebaut: {} fehlt",
        dylib.display()
    );

    let status = std::process::Command::new("install_name_tool")
        .arg("-id")
        .arg(format!("@rpath/{MACOS_DYLIB}"))
        .arg(&dylib)
        .status()
        .expect("install_name_tool laesst sich nicht starten");
    assert!(status.success(), "install_name_tool -id ist gescheitert");

    // Fester Ablageort fuer den Bundle-Schritt. Der Pfad haengt an diesem
    // Paket (`vendor/mp3lame-sys`), nicht am Bauordner -- der liegt bei uns per
    // Symlink auf einer externen Platte und traegt einen Hash im Namen.
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("To have env:CARGO_MANIFEST_DIR");
    let ziel_dir = std::path::Path::new(&manifest)
        .parent()
        .expect("vendor/ liegt ueber diesem Paket")
        .join("lame-dylib");
    std::fs::create_dir_all(&ziel_dir).expect("Zielordner fuer die Bibliothek anlegen");
    let ziel = ziel_dir.join(MACOS_DYLIB);
    // Erst loeschen: ein Ueberschreiben einer bereits geladenen Datei kann
    // fehlschlagen, ein Ersetzen nicht.
    let _ = std::fs::remove_file(&ziel);
    std::fs::copy(&dylib, &ziel).expect("Bibliothek an den festen Ort kopieren");
    println!("cargo:warning=libmp3lame dynamisch gebaut, Kopie: {}", ziel.display());
}

//On windows we cannot just use `nmake` as VS solution there doesn't have x64 files
//so instead just directly compile it.
//On unix targets we just rely on autotools to figure shit out
#[cfg(windows)]
fn build() {
    const INCLUDE_MSVC: &str = ".include_msvc";

    //fucking cargo and its annoying treatment of file modifications of crate
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is not set by retarded cargo");
    let lame_dir = std::path::Path::new(LAME_DIR);
    let include_msvc = std::path::Path::new(&out_dir).join(INCLUDE_MSVC);
    //copy config.h
    let _ = std::fs::create_dir(&include_msvc);
    std::fs::copy(lame_dir.join("configMS.h"), include_msvc.join("config.h")).expect("Copy config.h");

    let mut cc = cc::Build::new();
    cc.warnings(false)
      .extra_warnings(false)
      .file(lame_dir.join("libmp3lame/bitstream.c"))
      .file(lame_dir.join("libmp3lame/encoder.c"))
      .file(lame_dir.join("libmp3lame/fft.c"))
      .file(lame_dir.join("libmp3lame/gain_analysis.c"))
      .file(lame_dir.join("libmp3lame/id3tag.c"))
      .file(lame_dir.join("libmp3lame/lame.c"))
      .file(lame_dir.join("libmp3lame/newmdct.c"))
      .file(lame_dir.join("libmp3lame/presets.c"))
      .file(lame_dir.join("libmp3lame/psymodel.c"))
      .file(lame_dir.join("libmp3lame/quantize_pvt.c"))
      .file(lame_dir.join("libmp3lame/vector/xmm_quantize_sub.c"))
      .file(lame_dir.join("libmp3lame/quantize.c"))
      .file(lame_dir.join("libmp3lame/reservoir.c"))
      .file(lame_dir.join("libmp3lame/set_get.c"))
      .file(lame_dir.join("libmp3lame/tables.c"))
      .file(lame_dir.join("libmp3lame/takehiro.c"))
      .file(lame_dir.join("libmp3lame/util.c"))
      .file(lame_dir.join("libmp3lame/vbrquantize.c"))
      .file(lame_dir.join("libmp3lame/VbrTag.c"))
      .file(lame_dir.join("libmp3lame/version.c"))
      .include(lame_dir.join("include"))
      .include(&include_msvc)
      .include(lame_dir.join("libmp3lame"))
      .define("TAKEHIRO_IEEE754_HACK", None)
      .define("FLOAT8", Some("float"))
      .define("REAL_IS_FLOAT", Some("1"))
      .define("BS_FORMAT", Some("BINARY"))
      .define("HAVE_CONFIG_H", None)
      .shared_flag(false)
      .pic(false)
      .warnings(false);

    #[cfg(feature = "decoder")]
    {
        cc.define("HAVE_MPGLIB", None)
          .include(lame_dir.join("mpglib"))
          .file(lame_dir.join("mpglib/common.c"))
          .file(lame_dir.join("mpglib/dct64_i386.c"))
          .file(lame_dir.join("mpglib/decode_i386.c"))
          .file(lame_dir.join("mpglib/interface.c"))
          .file(lame_dir.join("mpglib/layer1.c"))
          .file(lame_dir.join("mpglib/layer2.c"))
          .file(lame_dir.join("mpglib/layer3.c"))
          .file(lame_dir.join("mpglib/tabinit.c"))
          .file(lame_dir.join("libmp3lame/mpglib_interface.c"));
    }

    if let Ok(compiler) = std::env::var("CC") {
        let compiler = std::path::Path::new(&compiler);
        let compiler = compiler.file_stem().expect("To have file name in CC").to_str().unwrap();
        match compiler {
            //because `cc` crate is retarded and cannot handle clang-cl correctly
            "clang-cl" => {
                cc.flag("/W0");
            },
            _ => (),
        }
    }

    cc.compile("mp3lame")
}

fn main() {
    if std::env::var("DOCS_RS").map(|docs| docs == "1").unwrap_or(false) {
        //skip docs.rs build
        return;
    }

    build();
}
