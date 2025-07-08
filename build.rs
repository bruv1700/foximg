fn winresource() {
    const LANGID_EN_US: u16 = 0x0409;

    let mut res = winresource::WindowsResource::new();
    let fileflags = if cfg!(debug_assertions) {
        0x1 // VS_FF_DEBUG
    } else {
        0x0
    };

    res.set_version_info(winresource::VersionInfo::FILEFLAGS, fileflags)
        .set_icon("res/foximg.ico")
        .set_language(LANGID_EN_US)
        .compile()
        .unwrap();
}

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        winresource();
    }
}
