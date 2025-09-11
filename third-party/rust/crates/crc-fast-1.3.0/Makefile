PROJECT_NAME := crc_fast

# Detect operating system
UNAME_S := $(shell uname -s)

# Determine OS-specific variables
ifeq ($(UNAME_S),Linux)
	DESTDIR ?= /usr/local
    LIB_EXTENSION := so
    INSTALL_LIB_DIR := /lib
    INSTALL_INCLUDE_DIR := /include
    POST_INSTALL := ldconfig
else ifeq ($(UNAME_S),Darwin)
    DESTDIR ?=
    # on macOS, there's not really a default location, so require DESTDIR
    ifeq ($(DESTDIR),)
        $(error On macOS, DESTDIR must be set for installation. Common locations include /usr/local or /opt/homebrew)
    endif
    LIB_EXTENSION := dylib
    INSTALL_LIB_DIR := /lib
    INSTALL_INCLUDE_DIR := /include
    POST_INSTALL := true
else
    # Windows
    DESTDIR ?=
    ifeq ($(DESTDIR),)
        $(error On Windows, DESTDIR must be set for installation. Common locations include C:\)
    endif
    LIB_EXTENSION := dll
    # Use relative paths when DESTDIR is set to avoid path joining issues
    PREFIX ?= Program Files\\$(PROJECT_NAME)
    INSTALL_LIB_DIR := $(PREFIX)\\bin
    INSTALL_INCLUDE_DIR := $(PREFIX)\\include
    POST_INSTALL := true
endif

# Library name with extension
LIB_NAME := lib$(PROJECT_NAME).$(LIB_EXTENSION)

# Default target
.PHONY: all
all: build

# Build the library using Cargo
.PHONY: build
build: test
	cargo build --release

# Test the library using Cargo
.PHONY: test
test:
	cargo test

# Install the library and headers
.PHONY: install
install: print-paths build
	@install -d $(DESTDIR)$(INSTALL_LIB_DIR)
	@install -d $(DESTDIR)$(INSTALL_INCLUDE_DIR)

	install -m 644 target/release/$(LIB_NAME) $(DESTDIR)$(INSTALL_LIB_DIR)/

	install -m 644 lib$(PROJECT_NAME).h $(DESTDIR)$(INSTALL_INCLUDE_DIR)/

	@if [ -z "$(DESTDIR)" ] && [ "$(POST_INSTALL)" != "true" ]; then \
		$(POST_INSTALL); \
	fi

# Uninstall the library and headers
.PHONY: uninstall
uninstall: print-paths
	rm -f $(DESTDIR)$(INSTALL_LIB_DIR)/$(LIB_NAME)
	rm -f $(DESTDIR)$(INSTALL_INCLUDE_DIR)/lib$(PROJECT_NAME).h

	@if [ -z "$(DESTDIR)" ] && [ "$(UNAME_S)" = "Linux" ]; then \
		ldconfig; \
	fi

# Clean build artifacts
.PHONY: clean
clean:
	cargo clean

# Print installation paths (useful for debugging)
.PHONY: print-paths
print-paths:
	@echo "Installation paths:"
	@echo "Library dir: $(DESTDIR)$(INSTALL_LIB_DIR)"
	@echo "Include dir: $(DESTDIR)$(INSTALL_INCLUDE_DIR)"