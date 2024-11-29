pub fn show() {
    const FOXIMG_VERSION: &str = env!("CARGO_PKG_VERSION");

    println!("foximg {FOXIMG_VERSION}: Simple & convenient image viewer\n");
    println!("Usage: foximg [OPTIONS] <path>");
    println!("Options:");
    println!("  -q, --quiet  Do not print log messages");
    println!("  -h, --help   Print help");
}
