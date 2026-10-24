# Boxxy Terminal — COSMIC Desktop Panel Applet Companion
# Targets: build, install, uninstall, enable, disable, test, clean, debug, help

APP_ID       := dev.boxxy.CosmicBoxxyCompanion
BIN_NAME     := cosmic-boxxy-companion
PREFIX       ?= $(HOME)/.local
BIN_DIR      := $(PREFIX)/bin
APPS_DIR     := $(PREFIX)/share/applications
ICONS_APPS   := $(PREFIX)/share/icons/hicolor/scalable/apps
ICONS_STATUS := $(PREFIX)/share/icons/hicolor/scalable/status
PANEL_WINGS  := $(HOME)/.config/cosmic/com.system76.CosmicPanel.Panel/v1/plugins_wings

.PHONY: build install uninstall enable disable test clean debug help

## build     — Compile the release binary via Cargo
build:
	@cargo build --release
	@echo "[OK] Built target/release/$(BIN_NAME)"

## install   — Build and install binary, desktop entry, and icons to ~/.local
install: build
	@mkdir -p "$(BIN_DIR)" "$(APPS_DIR)" "$(ICONS_APPS)" "$(ICONS_STATUS)"
	@install -Dm755 "target/release/$(BIN_NAME)" "$(BIN_DIR)/$(BIN_NAME)"
	@sed "s|^Exec=.*|Exec=$(BIN_DIR)/$(BIN_NAME)|" \
		"resources/$(APP_ID).desktop" > "$(APPS_DIR)/$(APP_ID).desktop"
	@install -Dm644 "resources/icons/boxxyclaw-symbolic.svg" \
		"$(ICONS_APPS)/$(APP_ID)-symbolic.svg"
	@install -Dm644 "resources/icons/boxxyclaw.svg" \
		"$(ICONS_APPS)/$(APP_ID).svg"
	@install -Dm644 "resources/icons/boxxyclaw-symbolic.svg" \
		"$(ICONS_STATUS)/boxxyclaw-symbolic.svg"
	@install -Dm644 "resources/icons/boxxyclaw-off-symbolic.svg" \
		"$(ICONS_STATUS)/boxxyclaw-off-symbolic.svg"
	@echo "[OK] Installed $(BIN_NAME) to $(BIN_DIR)/$(BIN_NAME)"
	@echo "[OK] Installed desktop entry to $(APPS_DIR)/$(APP_ID).desktop"
	@echo "     Run 'make enable' or add 'Boxxy Terminal Companion' in COSMIC Settings → Panel → Applets."

## uninstall — Remove the binary, desktop entry, and icons from ~/.local
uninstall: disable
	@rm -f "$(BIN_DIR)/$(BIN_NAME)"
	@rm -f "$(APPS_DIR)/$(APP_ID).desktop"
	@rm -f "$(ICONS_APPS)/$(APP_ID)-symbolic.svg"
	@rm -f "$(ICONS_APPS)/$(APP_ID).svg"
	@rm -f "$(ICONS_STATUS)/boxxyclaw-symbolic.svg"
	@rm -f "$(ICONS_STATUS)/boxxyclaw-off-symbolic.svg"
	@echo "[OK] Uninstalled $(APP_ID)"

## enable    — Add the applet to COSMIC Panel's right wing (plugins_wings)
enable:
	@if [ -f "$(PANEL_WINGS)" ]; then \
		if grep -q '"$(APP_ID)"' "$(PANEL_WINGS)"; then \
			echo "[OK] $(APP_ID) is already enabled in COSMIC Panel."; \
		else \
			sed -i '0,/\], \[/s/\], \[/],\n[\n    "$(APP_ID)",/' "$(PANEL_WINGS)"; \
			echo "[OK] Added $(APP_ID) to $(PANEL_WINGS)."; \
		fi; \
	else \
		echo "COSMIC Panel config not found at $(PANEL_WINGS). Enable via COSMIC Settings → Panel → Applets."; \
	fi

## disable   — Remove the applet from COSMIC Panel's plugins_wings
disable:
	@if [ -f "$(PANEL_WINGS)" ] && grep -q '"$(APP_ID)"' "$(PANEL_WINGS)"; then \
		sed -i '/"$(APP_ID)"/d' "$(PANEL_WINGS)"; \
		echo "[OK] Removed $(APP_ID) from $(PANEL_WINGS)."; \
	fi

## test      — Run unit tests
test:
	@cargo test

## clean     — Remove Cargo build artifacts
clean:
	@cargo clean
	@echo "[OK] Build artifacts cleaned."

## debug     — Tail COSMIC panel logs
debug:
	@journalctl --user -f | grep -iE "boxxy|cosmic-panel"

## help      — Show this help
help:
	@grep -E '^##' Makefile | sed 's/^##[ ]*//'
