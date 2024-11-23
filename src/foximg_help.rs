pub fn show() {
    const FOXIMG_VERSION: &str = env!("CARGO_PKG_VERSION");

    println!("foximg {FOXIMG_VERSION}: simple & convenient image viewer");
    println!("foximg --help: Show help");
    println!("foximg <path>: Open foximg with an image from a path");
}
