BIN = target/release/foximg
BIN_DIR = /usr/local/bin
SHARE_DIR = /usr/share

all: $(BIN)

Cargo.lock:
	@cargo fetch

$(BIN): Cargo.lock
	@cargo build --frozen --release

install: all
	mkdir -p $(BIN_DIR)
	mkdir -p $(SHARE_DIR)/applications
	mkdir -p $(SHARE_DIR)/pixmaps
	cp $(BIN) $(BIN_DIR)/foximg.tmp
	mv $(BIN_DIR)/foximg.tmp $(BIN_DIR)/foximg 	
	chmod 755 $(BIN_DIR)/foximg
	cp share/applications/foximg.desktop $(SHARE_DIR)/applications/foximg.desktop
	chmod 644 $(SHARE_DIR)/applications/foximg.desktop
	cp share/pixmaps/foximg.png $(SHARE_DIR)/pixmaps/foximg.png
	chmod 644 $(SHARE_DIR)/pixmaps/foximg.png
	mkdir -p /usr/lib/debug 
	objcopy --only-keep-debug $(BIN_DIR)/foximg /usr/lib/debug/foximg.debug 
	chmod 644 /usr/lib/debug/foximg.debug 
	strip $(BIN_DIR)/foximg 
	objcopy --add-gnu-debuglink=/usr/lib/debug/foximg.debug $(BIN_DIR)/foximg 

uninstall:
	rm -f $(BIN_DIR)/foximg
	rm -f $(SHARE_DIR)/applications/foximg.desktop
	rm -f $(SHARE_DIR)/pixmaps/foximg.png 
	rm -f /usr/lib/debug/foximg.debug

clean: Cargo.lock
	@cargo clean --frozen --release

.PHONY: all install clean
