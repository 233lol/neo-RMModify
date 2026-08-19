// 生成 assets/app.ico（多尺寸程序图标），源码变更时自动同步；
// Windows 下用 embed-resource 把图标编译进 exe（内部自动选择 windres / rc）
include!("src/icon.rs");

use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let assets = manifest.join("assets");
    let _ = std::fs::create_dir_all(&assets);
    let ico = assets.join("app.ico");
    let bytes = ico_bytes();
    if std::fs::read(&ico).ok().as_deref() != Some(bytes.as_slice()) {
        std::fs::write(&ico, &bytes).expect("写入 assets/app.ico 失败");
    }
    println!("cargo:rerun-if-changed=src/icon.rs");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_into_exe(&assets, &ico);
    }
}

/// 生成 app.rc（引用 app.ico）并交给 embed-resource 编译链接
fn embed_into_exe(assets: &Path, ico: &Path) {
    if !ico.exists() {
        eprintln!("警告: 缺少 {}，跳过 exe 图标嵌入", ico.display());
        return;
    }
    let rc = assets.join("app.rc");
    std::fs::write(&rc, "IDI_APP_ICON ICON \"app.ico\"\n").expect("写入 app.rc 失败");
    println!("cargo:rerun-if-changed=assets/app.rc");
    let assets_str = assets.to_string_lossy().replace('\\', "/");
    let result = embed_resource::compile(&rc, embed_resource::ParamsIncludeDirs(&[assets_str.as_str()]));
    if let Err(err) = result.manifest_optional() {
        eprintln!("警告: embed-resource 嵌入图标失败（{err}），窗口图标不受影响");
    }
}
