fn main() {
    println!("cargo:rerun-if-changed=assets/rustshot.rc");
    println!("cargo:rerun-if-changed=assets/rustshot.exe.manifest");
    println!("cargo:rerun-if-changed=assets/icon.ico");

    // Só há recursos Win32 para embutir quando o alvo é Windows. Em hosts
    // não-Windows sem rc.exe/windres (ex.: CI de lint em Linux) o
    // `manifest_optional` deixa o build seguir — o exe final para
    // distribuição deve ser produzido em toolchain Windows (§13 da spec).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/rustshot.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("falha ao embutir recursos Win32 (icone/manifesto)");
    }
}
