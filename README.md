# foximg

Simple & convenient image viewer built in Rust using [Raylib].

[Raylib]: http://www.raylib.com/

# Features

foximg prioritizes a wonderful UX, and fast decoding speeds thanks to [image-rs]. foximg also allows
for reading image metadata in the command-line as either TOML or JSON.

- Drag and drop an image to load it and its folder, or right-click and press `Open...`
- Click the buttons on each side (Or press A or D) to go through the photo library.
- Support for:
    - PNG (Static and Animated)
    - Bitmaps
    - JPEG
    - DDS
    - HDR
    - ICO
    - QOI
    - TIFF
    - Netpbm
    - OpenEXR
    - WebP (Static and Animated)
    - GIF
- Basic photo manipulation:
    - Rotating,
    - Mirroring,
    - Zooming in and out:
        - With the scroll wheel, or pressing W or S.
        - Slowly zoom in and out by pressing Ctrl+W or Ctrl+S.
    - Dragging across a zoomed in image.
- Customizable Theme.
- Quality of Life features:
    - Keeps state since last exit.
    - Keeps track of foximg windows and only updates the state of the first one opened.
    - Pretty logging.

[image-rs]: https://www.image-rs.org/

# Installation

On Windows, this will create config files and other miscelanious files on the executable directory. 
I strongly reccommend to install it on its own seperate folder.

On Linux, foximg complies with the [XDG Base Directory specification].

<!--foximg has not been tested on MacOS yet.
    ^^^
    I really disliked finding out how expensive developing for MacOS is. for the forseeable I won't
    be caring about MacOS :)
-->

Binaries for Windows and Debian (hopefully Ubuntu/Mint etc..) are in the Releases section for the
latest release. Otherwise, build the application from source. 

Installing from source requires **GNU Make**, **CMake** and **Cargo**. On Linux, you'll additionally 
require developement headers for X11, OpenGL and ALSA. These, and CMake, are prerequisites for Raylib. 
Check the library's [readme].

```
git clone https://github.com/bruv1700/foximg.git
cd foximg
make
sudo make install
```

The Makefle is provided to make installing from source much more convenient. There's absolutely no
reason to use it other than that. For maintaining/debugging/etc.. just use Cargo directly.

[XDG Base Directory specification]: https://specifications.freedesktop.org/basedir-spec/latest/
[readme]: https://github.com/raysan5/raylib?tab=readme-ov-file#build-and-installation
