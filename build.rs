use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=assets/quire.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let icon = manifest_dir.join("assets").join("quire.ico");
    let quote = |value: &str| format!("\"{}\"", value.replace('\\', "\\\\"));
    let text = |value: &str| format!("\"{}\\0\"", value.replace('\\', "\\\\"));
    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let mut parts = version
        .split('.')
        .map(|part| part.parse::<u32>().unwrap_or(0));
    let (major, minor, patch) = (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    );
    let version_tuple = format!("{major}, {minor}, {patch}, 0");
    let resource = format!(
        "#include <windows.h>\n\
         #define IDI_MAIN 101\n\
         IDI_MAIN ICON {icon}\n\
         VS_VERSION_INFO VERSIONINFO\n\
         FILEVERSION {version_tuple}\n\
         PRODUCTVERSION {version_tuple}\n\
         FILEOS VOS_NT_WINDOWS32\n\
         FILETYPE VFT_APP\n\
         BEGIN\n\
           BLOCK \"StringFileInfo\"\n\
           BEGIN\n\
            BLOCK \"040904b0\"\n\
            BEGIN\n\
             VALUE \"CompanyName\", {company}\n\
             VALUE \"FileDescription\", {description}\n\
             VALUE \"FileVersion\", {file_version}\n\
             VALUE \"InternalName\", {internal}\n\
             VALUE \"OriginalFilename\", {original}\n\
             VALUE \"ProductName\", {product}\n\
             VALUE \"ProductVersion\", {product_version}\n\
            END\n\
           END\n\
           BLOCK \"VarFileInfo\"\n\
           BEGIN\n\
            VALUE \"Translation\", 0x0409, 0x04b0\n\
           END\n\
         END\n",
        icon = quote(&icon.display().to_string()),
        company = text("Quire"),
        description = text("Quire GPUI - local notes and organizer"),
        file_version = text(&format!("{version}.0")),
        internal = text("quire-gpui"),
        original = text("quire-gpui.exe"),
        product = text("Quire GPUI"),
        product_version = text(&format!("Quire GPUI {version}")),
    );

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let rc_path = out_dir.join("quire-gpui.rc");
    std::fs::write(&rc_path, resource).expect("write Windows resource script");
    embed_resource::compile(rc_path, embed_resource::NONE)
        .manifest_optional()
        .expect("Windows resource compiler rejected Quire GPUI resources");
}
