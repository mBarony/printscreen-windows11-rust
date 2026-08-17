fn main() {
    println!("cargo:rerun-if-changed=assets/rustshot.rc");
    println!("cargo:rerun-if-changed=assets/rustshot.exe.manifest");
    println!("cargo:rerun-if-changed=assets/icon.ico");

    check_rc_version();

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

/// O `VERSIONINFO` do `.rc` é uma segunda cópia da versão, e o workflow de
/// release rejeita um exe cujo VersionInfo não comece com a tag. Esquecer o
/// `.rc` num bump só aparecia lá, depois de a tag já estar publicada — aqui
/// aparece no primeiro `cargo build`.
fn check_rc_version() {
    let expected = std::env::var("CARGO_PKG_VERSION").expect("cargo define a versão");
    let rc = std::fs::read_to_string("assets/rustshot.rc").expect("assets/rustshot.rc legível");

    // FILEVERSION/PRODUCTVERSION usam vírgulas e um quarto componente (build).
    let comma = format!("{},0", expected.replace('.', ","));
    let quoted = format!("\"{expected}\"");
    let expectations = [
        (format!("FILEVERSION     {comma}"), "FILEVERSION"),
        (format!("PRODUCTVERSION  {comma}"), "PRODUCTVERSION"),
        (format!("VALUE \"FileVersion\",      {quoted}"), "VALUE FileVersion"),
        (format!("VALUE \"ProductVersion\",   {quoted}"), "VALUE ProductVersion"),
    ];
    for (needle, what) in expectations {
        assert!(
            rc.contains(&needle),
            "assets/rustshot.rc está em outra versão: {what} deveria conter `{needle}` \
             para casar com a {expected} do Cargo.toml"
        );
    }
}
