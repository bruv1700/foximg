# foximg

Simple & convenient image viewer built in Rust using [Raylib].

[Raylib]: (http://www.raylib.com/)

# Features

foximg prioritizes a wonderful UX, and fast decoding speeds thanks to [image-rs].

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

foximg has not been tested on MacOS yet.

I will release binaries for foximg soon :) For now, download the source and compile it yourself.

<!-- I'm linking to the arch wiki because specifications.freedesktop.org is 404 as of the time I'm 
writing this -->
[XDG Base Directory specification]: https://wiki.archlinux.org/title/XDG_Base_Directory
