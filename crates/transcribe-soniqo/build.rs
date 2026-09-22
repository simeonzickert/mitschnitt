#[cfg(target_os = "macos")]
use std::{
    collections::BTreeSet,
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

#[cfg(target_os = "macos")]
// 15.0, not 14.2: Qwen3ASR's CoreML decoder uses MLState / `prediction(from:using:)`,
// both macOS 15+, so a 14.2 target cannot compile the Swift bridge at all. The app
// already ships `minimumSystemVersion: "15.0"` (apps/desktop/src-tauri/tauri.conf.json)
// and the MLX metallib below was already built at 15.0, so this only removes a floor
// that the product no longer claimed.
const SONIQO_SWIFT_MACOS_DEPLOYMENT_TARGET: &str = "15.0";
#[cfg(target_os = "macos")]
const SONIQO_METALLIB_MACOS_DEPLOYMENT_TARGET: &str = "15.0";

#[cfg(target_os = "macos")]
fn swift_runtime_rpaths() -> Vec<String> {
    let mut paths = BTreeSet::from([PathBuf::from("/usr/lib/swift")]);

    if let Some(swift_bin) = swift_bin_path()
        && let Some(toolchain_root) = swift_bin
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
    {
        paths.insert(toolchain_root.join("lib/swift/macosx"));
    }

    paths
        .into_iter()
        .filter(|path| path.exists())
        .map(|path| path.display().to_string())
        .collect()
}

#[cfg(target_os = "macos")]
fn swift_bin_path() -> Option<PathBuf> {
    let output = xcrun(None).args(["--find", "swift"]).output().ok()?;

    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8(output.stdout).ok()?;
    let path = path.trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

#[cfg(target_os = "macos")]
fn xcrun(developer_dir: Option<&Path>) -> Command {
    let mut command = Command::new("xcrun");
    if let Some(developer_dir) = developer_dir {
        command.env("DEVELOPER_DIR", developer_dir);
    }
    command
}

#[cfg(target_os = "macos")]
fn has_metal_compiler(developer_dir: Option<&Path>) -> bool {
    xcrun(developer_dir)
        .args(["--find", "metal"])
        .output()
        .is_ok_and(|output| output.status.success())
}

/// The Command Line Tools ship no Metal compiler, so fall back to a full Xcode
/// install when it is present but not selected via `xcode-select`.
#[cfg(target_os = "macos")]
fn metal_developer_dir() -> Option<PathBuf> {
    if has_metal_compiler(None) {
        return None;
    }

    let developer_dir = ["/Applications/Xcode.app", "/Applications/Xcode-beta.app"]
        .into_iter()
        .map(|app| PathBuf::from(app).join("Contents/Developer"))
        .find(|candidate| has_metal_compiler(Some(candidate)));

    if developer_dir.is_none() {
        panic!(
            "no Metal compiler found; install Xcode, select it with \
             `sudo xcode-select -s /Applications/Xcode.app`, then run \
             `xcodebuild -downloadComponent MetalToolchain`"
        );
    }

    developer_dir
}

#[cfg(target_os = "macos")]
/// Das Urteil des Bauart-Waechters lebt in der Bibliothek, damit es Tests
/// dafuer gibt -- ein Waechter, der nur im Bauskript steht, laeuft in keinem.
#[cfg(target_os = "macos")]
#[path = "src/swift_build_check.rs"]
mod swift_build_check;

fn target_is_macos_apple_silicon() -> bool {
    env::var("CARGO_CFG_TARGET_OS").is_ok_and(|value| value == "macos")
        && env::var("CARGO_CFG_TARGET_ARCH").is_ok_and(|value| value == "aarch64")
}

/// In welcher Bauart die SWIFT-Seite uebersetzt wird -- unabhaengig davon, in
/// welcher Bauart Rust laeuft.
///
/// **Immer `release`, und das ist gemessen, nicht Geschmack (08.09.2026).**
/// swift-rs entscheidet die Swift-Bauart allein am Cargo-Schalter `DEBUG`
/// (Fork `git+yujonglee/swift-rs` rev `41a1605`, `src-rs/build.rs:277-278`
/// -> `swift build -c debug`). Der
/// Alltags-Bau der App ist `tauri build --debug` (siehe
/// `scripts/mitschnitt-deploy.sh`), also lief bis heute die GESAMTE
/// Swift-Seite mit `-Onone` -- einschliesslich der Sprechertrennung aus
/// `speech-swift`, die zum groessten Teil gewoehnlicher Swift-Code ist
/// (AHC-Verkettung, VBx-Schleife, k-Means) und nicht CoreML.
///
/// Gemessen an Kanal 0 der Sitzung vom 07.09.2026 (Vor-Ort-Termin,
/// 101 Minuten, vier Personen), gleicher Rechner, gleiche Eingabe:
///
/// | Kanallaenge | Swift `debug` | Swift `release` | Faktor |
/// |---|---|---|---|
/// | 1 min | 4,7 s | 2,3 s | 2,0 |
/// | 10 min | 81,4 s | 24,9 s | 3,3 |
/// | 30 min | 287,8 s | 77,3 s | 3,7 |
/// | 60 min | 1231,7 s | 167,6 s | 7,3 |
/// | 101 min | nach 55 min abgebrochen | 315,7 s | > 10 |
///
/// Der Abstand waechst mit der Laenge, weil der teuerste Teil (AHC ueber die
/// Einbettungen) quadratisch waechst: bei einer Minute kostet die falsche
/// Bauart das Doppelte, bei 101 Minuten mehr als das Zehnfache. In `release`
/// ist der Verlauf fast linear (26,3x bis 19,2x Echtzeit), in `debug` bricht er
/// ein (12,8x bis 2,9x bei 60 min).
///
/// Genau das hat den Prinzipal am Abend des 07.09. zweimal abbrechen lassen:
/// die App stand bei 5 %, und die Trennung haette knapp eine Stunde gebraucht.
///
/// Wer die Swift-Seite mit Symbolen debuggen will, setzt
/// `MITSCHNITT_SWIFT_PROFILE=debug` -- und weiss dann, was er sich einhandelt.
#[cfg(target_os = "macos")]
fn swift_configuration() -> &'static str {
    match env::var("MITSCHNITT_SWIFT_PROFILE").as_deref() {
        Ok("debug") => "debug",
        _ => "release",
    }
}

#[cfg(target_os = "macos")]
fn profile_target_dir(out_dir: &Path) -> Option<PathBuf> {
    out_dir.ancestors().nth(3).map(Path::to_path_buf)
}

#[cfg(target_os = "macos")]
fn find_mlx_metallib(root: &Path, profile: &str) -> Option<PathBuf> {
    [
        root.join(profile).join("mlx.metallib"),
        root.join("arm64-apple-macosx")
            .join(profile)
            .join("mlx.metallib"),
    ]
    .into_iter()
    .find(|path| path.exists())
}

#[cfg(target_os = "macos")]
fn swift_profile_dir(root: &Path, profile: &str) -> PathBuf {
    let direct = root.join(profile);
    if direct.exists() {
        return direct;
    }

    root.join("arm64-apple-macosx").join(profile)
}

#[cfg(target_os = "macos")]
fn collect_files(root: &Path, predicate: impl Fn(&Path) -> bool) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(path) = stack.pop() {
        for entry in fs::read_dir(path)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if predicate(&path) {
                files.push(path);
            }
        }
    }

    files.sort();
    Ok(files)
}

#[cfg(target_os = "macos")]
fn newest_modified_time(files: &[PathBuf]) -> io::Result<SystemTime> {
    files
        .iter()
        .map(|path| fs::metadata(path)?.modified())
        .try_fold(SystemTime::UNIX_EPOCH, |newest, modified| {
            modified.map(|modified| newest.max(modified))
        })
}

#[cfg(target_os = "macos")]
fn metallib_is_fresh(metallib: &Path, inputs: &[PathBuf]) -> bool {
    let Ok(output_modified) = fs::metadata(metallib).and_then(|metadata| metadata.modified())
    else {
        return false;
    };
    let Ok(input_modified) = newest_modified_time(inputs) else {
        return false;
    };

    output_modified >= input_modified
}

#[cfg(target_os = "macos")]
fn run_command(mut command: Command, context: &str) {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("failed to run {context}: {error}"));

    if !output.status.success() {
        panic!(
            "{context} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[cfg(target_os = "macos")]
fn prepare_swift_package(swift_build_dir: &Path) {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by Cargo"),
    );
    let swift_package_dir = manifest_dir.join("swift-lib");

    let mut command = Command::new("swift");
    command
        .arg("package")
        .arg("--package-path")
        .arg(&swift_package_dir)
        .arg("--scratch-path")
        .arg(swift_build_dir)
        .arg("resolve");
    run_command(command, "resolving Soniqo Swift dependencies");

    patch_speech_swift_manifest(swift_build_dir);
    patch_parakeet_streaming_offline_mode(swift_build_dir);
    patch_parakeet_word_frames(swift_build_dir);
}

#[cfg(target_os = "macos")]
fn patch_speech_swift_manifest(swift_build_dir: &Path) {
    let manifest = swift_build_dir
        .join("checkouts")
        .join("speech-swift")
        .join("Package.swift");
    let target = format!(".macOS(\"{SONIQO_SWIFT_MACOS_DEPLOYMENT_TARGET}\")");
    let contents = fs::read_to_string(&manifest).unwrap_or_else(|error| {
        panic!(
            "failed to read Soniqo speech-swift manifest {}: {error}",
            manifest.display()
        )
    });

    if contents.contains(&target) {
        return;
    }

    // Handles both a pristine checkout (15.0, upstream's own value) and one a previous
    // build already patched down to 14.2 — otherwise raising the target would panic on
    // an existing build tree.
    let patched = contents
        .replace(".macOS(\"15.0\")", &target)
        .replace(".macOS(\"14.2\")", &target);
    if patched == contents {
        panic!(
            "failed to patch Soniqo speech-swift manifest {}; expected macOS 15.0 platform declaration",
            manifest.display()
        );
    }

    write_patched_swift_source(&manifest, &patched);
}

#[cfg(target_os = "macos")]
include!("src/streaming_offline.rs");

#[cfg(target_os = "macos")]
include!("src/word_frames.rs");

#[cfg(target_os = "macos")]
fn patch_parakeet_streaming_offline_mode(swift_build_dir: &Path) {
    let path = swift_build_dir
        .join("checkouts")
        .join("speech-swift")
        .join("Sources")
        .join("ParakeetStreamingASR")
        .join("ParakeetStreamingASR.swift");
    let contents = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "failed to read Soniqo Parakeet streaming source {}: {error}",
            path.display()
        )
    });
    let patched = patch_parakeet_streaming_source(&contents).unwrap_or_else(|error| {
        panic!(
            "failed to patch Soniqo Parakeet streaming offline mode in {}: {error}",
            path.display()
        )
    });

    if patched == contents {
        return;
    }

    write_patched_swift_source(&path, &patched);
}

// Fork (02.09.2026): echte Wortzeiten. Drei Dateien, ein Weg -- Details und
// Begruendung in `src/word_frames.rs`. Bricht laut, wenn ein Anker nicht sitzt.
#[cfg(target_os = "macos")]
fn patch_parakeet_word_frames(swift_build_dir: &Path) {
    let sources = swift_build_dir
        .join("checkouts")
        .join("speech-swift")
        .join("Sources")
        .join("ParakeetASR");

    type SwiftSourcePatch = fn(&str) -> Result<String, &'static str>;
    let steps: [(&str, SwiftSourcePatch); 3] = [
        ("TDTGreedyDecoder.swift", patch_tdt_greedy_decoder_source),
        ("ParakeetASR.swift", patch_parakeet_asr_source),
        ("Vocabulary.swift", patch_parakeet_vocabulary_source),
    ];

    for (file_name, patch) in steps {
        let path = sources.join(file_name);
        let contents = fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "failed to read Soniqo Parakeet source {}: {error}",
                path.display()
            )
        });
        let patched = patch(&contents).unwrap_or_else(|error| {
            panic!(
                "failed to patch Soniqo Parakeet word frames in {}: {error}\n\
                 Wenn dieser Patch selbst geaendert wurde, traegt der Checkout \
                 noch die alte Fassung und passt auf keinen der beiden Anker. \
                 Dann den Ordner checkouts/speech-swift loeschen und neu bauen. \
                 Sonst ist speech-swift gesprungen und der Anker muss nachziehen.",
                path.display()
            )
        });
        if patched == contents {
            continue;
        }
        write_patched_swift_source(&path, &patched);
    }
}

/// Schreibt eine gepatchte Datei im Swift-Checkout zurueck. Der Checkout kommt
/// schreibgeschuetzt aus `swift package resolve` -- alle drei Patch-Stellen
/// brauchten denselben Ablauf, er steht jetzt einmal hier.
#[cfg(target_os = "macos")]
fn write_patched_swift_source(path: &Path, patched: &str) {
    let mut permissions = fs::metadata(path)
        .unwrap_or_else(|error| {
            panic!(
                "failed to read permissions for Soniqo Swift source {}: {error}",
                path.display()
            )
        })
        .permissions();
    if permissions.readonly() {
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions).unwrap_or_else(|error| {
            panic!(
                "failed to make Soniqo Swift source writable {}: {error}",
                path.display()
            )
        });
    }

    fs::write(path, patched).unwrap_or_else(|error| {
        panic!(
            "failed to patch Soniqo Swift source {}: {error}",
            path.display()
        )
    });
}

#[cfg(target_os = "macos")]
fn compile_mlx_metallib(swift_build_dir: &Path, profile: &str) -> PathBuf {
    let mlx_swift_dir = swift_build_dir.join("checkouts").join("mlx-swift");
    let kernels_dir = mlx_swift_dir
        .join("Source")
        .join("Cmlx")
        .join("mlx")
        .join("mlx")
        .join("backend")
        .join("metal")
        .join("kernels");
    let output_dir = swift_profile_dir(swift_build_dir, profile);
    let output_metallib = output_dir.join("mlx.metallib");

    let metal_sources = collect_files(&kernels_dir, |path| {
        path.extension().is_some_and(|ext| ext == "metal")
            && !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with("_nax.metal"))
    })
    .unwrap_or_else(|error| {
        panic!(
            "failed to collect Soniqo MLX Metal sources from {}: {error}",
            kernels_dir.display()
        )
    });

    if metal_sources.is_empty() {
        panic!(
            "no Soniqo MLX Metal sources found under {}",
            kernels_dir.display()
        );
    }

    let metal_headers = collect_files(&kernels_dir, |path| {
        path.extension().is_some_and(|ext| ext == "h")
            && !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with("_nax.h"))
    })
    .unwrap_or_else(|error| {
        panic!(
            "failed to collect Soniqo MLX Metal headers from {}: {error}",
            kernels_dir.display()
        )
    });
    let inputs = metal_sources
        .iter()
        .chain(metal_headers.iter())
        .cloned()
        .collect::<Vec<_>>();

    if metallib_is_fresh(&output_metallib, &inputs) {
        return output_metallib;
    }

    fs::create_dir_all(&output_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create Soniqo MLX metallib output directory {}: {error}",
            output_dir.display()
        )
    });

    let air_dir = output_dir.join("mlx-metallib-air");
    if air_dir.exists() {
        fs::remove_dir_all(&air_dir).unwrap_or_else(|error| {
            panic!(
                "failed to clean Soniqo MLX Metal temp directory {}: {error}",
                air_dir.display()
            )
        });
    }
    fs::create_dir_all(&air_dir).unwrap_or_else(|error| {
        panic!(
            "failed to create Soniqo MLX Metal temp directory {}: {error}",
            air_dir.display()
        )
    });

    let include_root = mlx_swift_dir.join("Source").join("Cmlx").join("mlx");
    let developer_dir = metal_developer_dir();
    let mut air_files = Vec::with_capacity(metal_sources.len());

    for (index, source) in metal_sources.iter().enumerate() {
        let air_file = air_dir.join(format!("{index}.air"));
        let mut command = xcrun(developer_dir.as_deref());
        command
            .args([
                "-sdk",
                "macosx",
                "metal",
                "-x",
                "metal",
                "-std=metal3.2",
                "-Wall",
                "-Wextra",
                "-fno-fast-math",
                "-Wno-c++17-extensions",
                "-Wno-c++20-extensions",
                "-c",
            ])
            .arg(format!(
                "-mmacosx-version-min={SONIQO_METALLIB_MACOS_DEPLOYMENT_TARGET}"
            ))
            .arg(source)
            .arg(format!("-I{}", kernels_dir.display()))
            .arg(format!("-I{}", include_root.display()))
            .arg("-o")
            .arg(&air_file);
        run_command(command, &format!("compiling {}", source.display()));
        air_files.push(air_file);
    }

    let mut command = xcrun(developer_dir.as_deref());
    command.args(["-sdk", "macosx", "metallib"]);
    command.args(&air_files);
    command.arg("-o").arg(&output_metallib);
    run_command(command, "linking Soniqo MLX metallib");

    output_metallib
}

#[cfg(target_os = "macos")]
fn copy_if_changed(source: &Path, destination: &Path) -> io::Result<()> {
    if destination.exists() && fs::read(source)? == fs::read(destination)? {
        return Ok(());
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination)?;

    Ok(())
}

#[cfg(target_os = "macos")]
fn desktop_resource_metallib_path() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR")?);
    Some(
        manifest_dir
            .parent()?
            .parent()?
            .join("apps/desktop/src-tauri/resources/mlx.metallib"),
    )
}

#[cfg(target_os = "macos")]
fn build_mlx_metallib() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let swift_build_dir = out_dir.join("swift-rs").join("soniqo-swift");
    let profile = swift_configuration();
    let built_metallib = compile_mlx_metallib(&swift_build_dir, profile);
    let metallib = find_mlx_metallib(&swift_build_dir, profile).unwrap_or_else(|| {
        panic!(
            "Soniqo MLX metallib build completed but mlx.metallib was not found under {}",
            swift_build_dir.display()
        )
    });
    assert_eq!(built_metallib, metallib);

    if let Some(target_dir) = profile_target_dir(&out_dir) {
        let destination = target_dir.join("mlx.metallib");
        copy_if_changed(&metallib, &destination).unwrap_or_else(|error| {
            panic!(
                "failed to copy Soniqo MLX metallib to {}: {error}",
                destination.display()
            )
        });
    }

    if let Some(destination) = desktop_resource_metallib_path() {
        if destination.parent().is_some_and(Path::exists) {
            copy_if_changed(&metallib, &destination).unwrap_or_else(|error| {
                panic!(
                    "failed to copy Soniqo MLX metallib to {}: {error}",
                    destination.display()
                )
            });
        }
    }
}

/// Prueft, dass `swift build` wirklich in der erwarteten Bauart gelaufen ist.
///
/// Der Beweis ist nicht mehr die blosse EXISTENZ der statischen Bibliothek im
/// Bauart-Ordner -- die liegt im inkrementellen Bauordner auch aus einem
/// frueheren Lauf noch da. Geprueft wird zusaetzlich, ob in DIESEM Lauf in die
/// falsche Bauart gebaut wurde. Die Regeln und ihre Grenzen stehen bei
/// [`swift_build_check::swift_configuration_verdict`], die Tests daneben.
#[cfg(target_os = "macos")]
fn assert_swift_configuration_used(
    swift_build_dir: &Path,
    configuration: &str,
    run_started: std::time::SystemTime,
) {
    use swift_build_check::SwiftConfigurationVerdict;

    let other_configuration = if configuration == "debug" {
        "release"
    } else {
        "debug"
    };
    let library = swift_library_path(swift_build_dir, configuration);
    let other_library = swift_library_path(swift_build_dir, other_configuration);

    match swift_build_check::swift_configuration_verdict(
        file_modified_at(&library),
        file_modified_at(&other_library),
        run_started,
    ) {
        SwiftConfigurationVerdict::Ok => {}
        SwiftConfigurationVerdict::OtherIsNewer => {
            println!(
                "cargo:warning=Soniqo-Swift: die Bauart '{other_configuration}' ({}) ist neuer \
                 als die gebaute Bauart '{configuration}' ({}). Erwartbar nach einem bewussten \
                 MITSCHNITT_SWIFT_PROFILE=debug-Lauf -- sonst nachsehen.",
                other_library.display(),
                library.display()
            );
        }
        SwiftConfigurationVerdict::Missing => panic!(
            "Soniqo-Swift wurde nicht in der Bauart '{configuration}' gebaut -- erwartet: {}. \
             Vermutlich liest swift-rs den Schalter 'DEBUG' nicht mehr (siehe \
             swift_configuration).",
            library.display()
        ),
        SwiftConfigurationVerdict::OtherFreshlyBuilt => panic!(
            "Soniqo-Swift wurde in DIESEM Lauf in der Bauart '{other_configuration}' gebaut \
             ({}), obwohl '{configuration}' verlangt war. Vermutlich liest swift-rs den \
             Schalter 'DEBUG' nicht mehr (siehe swift_configuration) -- die App waere am \
             101-Minuten-Kanal mehr als zehnmal langsamer.",
            other_library.display()
        ),
    }
}

#[cfg(target_os = "macos")]
fn swift_library_path(swift_build_dir: &Path, configuration: &str) -> PathBuf {
    swift_profile_dir(swift_build_dir, configuration).join("libsoniqo-swift.a")
}

/// Aenderungszeit einer Datei, `None` wenn sie fehlt oder nicht lesbar ist.
///
/// Nicht lesbar wird bewusst wie fehlend behandelt: der Waechter soll dann
/// laut werden, nicht stillschweigend durchwinken.
#[cfg(target_os = "macos")]
fn file_modified_at(path: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(path).and_then(|metadata| metadata.modified()).ok()
}

fn main() {
    // Ohne diese Zeile merkt Cargo eine Aenderung an dem Schalter NICHT und
    // laesst das Bauskript aus -- dann bleibt die zuletzt gebaute Swift-Bauart
    // stehen, und der Waechter weiter unten kann nichts melden, weil er gar
    // nicht laeuft. Genau das ist am 08.09.2026 einmal passiert.
    println!("cargo:rerun-if-env-changed=MITSCHNITT_SWIFT_PROFILE");

    // Der Anker fuer die Frische-Pruefung des Bauart-Waechters: alles, was
    // ab hier geschrieben wird, stammt aus DIESEM Lauf.
    #[cfg(target_os = "macos")]
    let run_started = std::time::SystemTime::now();

    #[cfg(target_os = "macos")]
    {
        if !target_is_macos_apple_silicon() {
            println!(
                "cargo:warning=Soniqo speech-swift linking is only available on macOS Apple Silicon"
            );
            return;
        }

        let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
        let swift_build_dir = out_dir.join("swift-rs").join("soniqo-swift");
        prepare_swift_package(&swift_build_dir);

        // swift-rs liest NUR `DEBUG`, um zwischen `swift build -c debug` und
        // `-c release` zu waehlen. Der Schalter wird hier bewusst umgestellt,
        // damit die Swift-Seite auch in einem Rust-Debug-Bau optimiert ist --
        // die Begruendung mit Messwerten steht bei `swift_configuration`.
        //
        // SAFETY: Ein Build-Skript laeuft einthreadig; hier ist noch kein
        // weiterer Thread gestartet, der die Umgebung lesen koennte.
        unsafe {
            env::set_var(
                "DEBUG",
                if swift_configuration() == "debug" {
                    "true"
                } else {
                    "false"
                },
            );
        }

        swift_rs::SwiftLinker::new(SONIQO_SWIFT_MACOS_DEPLOYMENT_TARGET)
            .with_package("soniqo-swift", "./swift-lib/")
            .link();

        // Waechter, kein Schmuck: die Umstellung oben haengt an einem
        // FREMDEN Implementierungsdetail (dem Namen `DEBUG` in swift-rs).
        // Aendert swift-rs das, faellt der Bau still auf `-Onone` zurueck und
        // die App ist wieder zehnmal langsamer, ohne dass irgendwo etwas
        // rot wird. Also wird nachgesehen, ob die Bibliothek wirklich aus dem
        // erwarteten Ordner kommt.
        assert_swift_configuration_used(&swift_build_dir, swift_configuration(), run_started);

        // Damit die Bauart zur Laufzeit ABLESBAR ist und nicht geraten werden
        // muss: der Waechter oben schuetzt nur, wenn das Bauskript laeuft --
        // wird es ausgelassen, sagt diese Zeichenkette trotzdem, aus welchem
        // Lauf die gelinkte Bibliothek stammt.
        println!(
            "cargo:rustc-env=SONIQO_SWIFT_CONFIGURATION={}",
            swift_configuration()
        );

        build_mlx_metallib();

        for path in swift_runtime_rpaths() {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{path}");
        }

        println!("cargo:rustc-link-lib=c++");
        println!("cargo:rerun-if-changed=swift-lib/src");
        println!("cargo:rerun-if-changed=swift-lib/Package.swift");
        println!("cargo:rerun-if-changed=swift-lib/Package.resolved");
    }

    #[cfg(not(target_os = "macos"))]
    {
        println!(
            "cargo:warning=Soniqo speech-swift linking is only available on macOS Apple Silicon"
        );
    }
}
