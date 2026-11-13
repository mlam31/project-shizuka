use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=src/ui/main.slint");
    slint_build::compile("src/ui/main.slint").unwrap();

    embed_windows_icon();
}

/// Embeds `assets/icon/Shizuka.ico` into Windows executables so Explorer and
/// the taskbar show the app's kanji icon instead of the default placeholder.
///
/// No-op when building for any other target (macOS, iOS, ...).
fn embed_windows_icon() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let icon = manifest.join("assets").join("icon").join("Shizuka.ico");
    let rc = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("shizuka-icon.rc");

    // An absolute path keeps llvm-rc/rc.exe from guessing where the .ico lives.
    std::fs::write(&rc, format!("1 ICON \"{}\"\n", icon.display()))
        .expect("failed to write shizuka-icon.rc");
    println!("cargo:rerun-if-changed={}", icon.display());

    // Cross-compiling from macOS: embed-resource falls back to llvm-rc, which
    // Homebrew does not put on PATH. Point it at the cellar copy if needed.
    let has_rc = std::env::var_os("RC").is_some()
        || std::env::var_os("RC_x86_64_pc_windows_msvc").is_some();
    let has_llvm_rc = std::env::var_os("PATH").map_or(false, |paths| {
        std::env::split_paths(&paths).any(|dir| dir.join("llvm-rc").exists())
    });
    if !has_rc && !has_llvm_rc {
        let candidate = ["/opt/homebrew/opt/llvm/bin/llvm-rc", "/usr/local/opt/llvm/bin/llvm-rc"]
            .iter()
            .find(|p| std::path::Path::new(p).exists());
        match candidate {
            Some(p) => std::env::set_var("RC", p),
            None => panic!(
                "No Windows resource compiler found: install LLVM \
                 (brew install llvm) so the app icon can be embedded"
            ),
        }
    }

    // The icon is cosmetic, but a silently skipped compiler would ship an
    // un-iconed exe, so only `NotWindows`/`Ok` are acceptable.
    match embed_resource::compile(&rc, embed_resource::NONE) {
        embed_resource::CompilationResult::Ok
        | embed_resource::CompilationResult::NotWindows => {}
        other => panic!("failed to embed the app icon into the exe: {other:?}"),
    }
}
