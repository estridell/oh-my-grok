#!/usr/bin/env bash
#
# oh-my-grok installer
#
# Usage:
#   curl -fsSL https://github.com/estridell/oh-my-grok/releases/latest/download/install.sh | bash
#   curl -fsSL https://github.com/estridell/oh-my-grok/releases/latest/download/install.sh | bash -s 0.1.0
#
# Environment:
#   OH_MY_GROK_HOME         State directory (default: ~/.oh-my-grok)
#   OH_MY_GROK_BIN_DIR      Managed executable directory (default: $OH_MY_GROK_HOME/bin)
#   OH_MY_GROK_RELEASES_URL Release base override, primarily for testing

set -euo pipefail

TARGET="${1:-}"
RELEASES_URL="${OH_MY_GROK_RELEASES_URL:-https://github.com/estridell/oh-my-grok/releases}"
OMG_HOME="${OH_MY_GROK_HOME:-$HOME/.oh-my-grok}"
DOWNLOAD_DIR="$OMG_HOME/downloads"
BIN_DIR="${OH_MY_GROK_BIN_DIR:-$OMG_HOME/bin}"

if [[ -n "$TARGET" ]] && [[ ! "$TARGET" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Invalid version format: $TARGET (expected X.Y.Z)" >&2
    exit 1
fi

DOWNLOADER=""
if command -v curl >/dev/null 2>&1; then
    DOWNLOADER="curl"
elif command -v wget >/dev/null 2>&1; then
    DOWNLOADER="wget"
else
    echo "Either curl or wget is required but neither is installed." >&2
    exit 1
fi

download_file() {
    local url="$1" output="${2:-}"
    if [[ "$DOWNLOADER" == "curl" ]]; then
        if [[ -n "$output" ]]; then
            curl -fsSL --retry 3 --retry-delay 1 -o "$output" "$url"
        else
            curl -fsSL --retry 3 --retry-delay 1 "$url"
        fi
    elif [[ -n "$output" ]]; then
        wget -q -O "$output" "$url"
    else
        wget -q -O - "$url"
    fi
}

download_file_parallel() {
    local url="$1" output="$2"
    if [[ "$DOWNLOADER" != "curl" ]]; then
        download_file "$url" "$output"
        return
    fi

    local size
    size=$(curl -fsSL --head "$url" 2>/dev/null | awk -F'[: \r\n]+' 'tolower($1)=="content-length"{print $2; exit}') || true
    if [[ -z "$size" ]] || ! [[ "$size" -ge 16777216 ]] 2>/dev/null; then
        download_file "$url" "$output"
        return
    fi

    local chunks=8
    local chunk_size=$(( (size + chunks - 1) / chunks ))
    local tmpdir
    tmpdir=$(mktemp -d 2>/dev/null) || {
        download_file "$url" "$output"
        return
    }
    local pids=() i start end
    for i in $(seq 0 $((chunks - 1))); do
        start=$((i * chunk_size))
        end=$((start + chunk_size - 1))
        [[ $end -ge $size ]] && end=$((size - 1))
        curl -fsSL --retry 3 -r "${start}-${end}" -o "${tmpdir}/$(printf 'chunk.%03d' "$i")" "$url" &
        pids+=($!)
    done
    local all_ok=true pid
    for pid in "${pids[@]}"; do
        wait "$pid" || all_ok=false
    done
    if [[ "$all_ok" == true ]] && cat "${tmpdir}"/chunk.* > "$output" 2>/dev/null; then
        rm -rf "$tmpdir"
        return
    fi
    rm -rf "$tmpdir"
    download_file "$url" "$output"
}

case "$(uname -s)" in
    Darwin) os="macos" ;;
    Linux) os="linux" ;;
    *) echo "Unsupported OS: $(uname -s). oh-my-grok v1 supports Linux and macOS." >&2; exit 1 ;;
esac

case "$(uname -m)" in
    x86_64|amd64|AMD64) arch="x86_64" ;;
    arm64|aarch64|ARM64) arch="aarch64" ;;
    *) echo "Unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

if [[ -z "$TARGET" ]]; then
    echo "Fetching latest oh-my-grok version..." >&2
    version=$(download_file "${RELEASES_URL}/latest/download/version" | tr -d '\r' | head -n1 | tr -d '[:space:]')
else
    version="$TARGET"
fi

if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Invalid release version: ${version:-<empty>}" >&2
    exit 1
fi

platform="${os}-${arch}"
artifact="omg-${version}-${platform}"
release_url="${RELEASES_URL}/download/v${version}"
binary_path="$DOWNLOAD_DIR/$artifact"
binary_tmp="${binary_path}.tmp.$$"
sums_tmp="$DOWNLOAD_DIR/SHA256SUMS.tmp.$$"

mkdir -p "$DOWNLOAD_DIR" "$BIN_DIR"
trap 'rm -f "$binary_tmp" "$sums_tmp" "${BIN_DIR}/.omg-link.$$" "${BIN_DIR}/.oh-my-grok-link.$$"' EXIT

echo "Installing oh-my-grok $version ($platform)..." >&2
echo "  Downloading $artifact..." >&2
download_file_parallel "${release_url}/${artifact}" "$binary_tmp"
download_file "${release_url}/SHA256SUMS" "$sums_tmp"

expected=$(awk -v name="$artifact" '$2 == name || $2 == "*" name { print $1; exit }' "$sums_tmp")
if [[ -z "$expected" ]]; then
    echo "Error: SHA256SUMS does not contain $artifact." >&2
    exit 1
fi

if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$binary_tmp" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "$binary_tmp" | awk '{print $1}')
else
    echo "Error: sha256sum or shasum is required to verify the download." >&2
    exit 1
fi

if [[ "$actual" != "$expected" ]]; then
    echo "Error: checksum mismatch for $artifact." >&2
    exit 1
fi

chmod +x "$binary_tmp"
if ! "$binary_tmp" --version </dev/null >/dev/null 2>&1; then
    echo "Error: downloaded oh-my-grok failed to run; the existing install is unchanged." >&2
    exit 1
fi
mv -f "$binary_tmp" "$binary_path"

if [[ "$(dirname "$BIN_DIR")" == "$(dirname "$DOWNLOAD_DIR")" ]]; then
    link_target="../$(basename "$DOWNLOAD_DIR")/$artifact"
else
    link_target="$binary_path"
fi

ln -s "$link_target" "${BIN_DIR}/.omg-link.$$"
ln -s "$link_target" "${BIN_DIR}/.oh-my-grok-link.$$"
mv -f "${BIN_DIR}/.omg-link.$$" "$BIN_DIR/omg"
mv -f "${BIN_DIR}/.oh-my-grok-link.$$" "$BIN_DIR/oh-my-grok"

mkdir -p "$OMG_HOME/completions/bash" "$OMG_HOME/completions/zsh"
"$BIN_DIR/omg" completions bash > "$OMG_HOME/completions/bash/omg.bash" 2>/dev/null || true
"$BIN_DIR/omg" completions zsh > "$OMG_HOME/completions/zsh/_omg" 2>/dev/null || true
if mkdir -p "$HOME/.config/fish/completions" 2>/dev/null; then
    "$BIN_DIR/omg" completions fish > "$HOME/.config/fish/completions/omg.fish" 2>/dev/null || true
fi

CONFIG_FILE="$OMG_HOME/config.toml"
if [[ ! -f "$CONFIG_FILE" ]]; then
    printf '[cli]\ninstaller = "gh-release"\nchannel = "stable"\n' > "$CONFIG_FILE"
elif grep -q '^\[cli\]' "$CONFIG_FILE"; then
    config_tmp="${CONFIG_FILE}.tmp.$$"
    awk '
        /^\[cli\][[:space:]]*(#.*)?$/ { print; print "installer = \"gh-release\""; print "channel = \"stable\""; in_cli=1; next }
        /^\[.*\][[:space:]]*(#.*)?$/ { in_cli=0 }
        in_cli && /^[[:space:]]*(installer|channel)[[:space:]]*=/ { next }
        { print }
    ' "$CONFIG_FILE" > "$config_tmp" && mv "$config_tmp" "$CONFIG_FILE"
else
    printf '\n[cli]\ninstaller = "gh-release"\nchannel = "stable"\n' >> "$CONFIG_FILE"
fi

path_has_dir() {
    case ":$PATH:" in *":$1:"*) return 0 ;; *) return 1 ;; esac
}

SYMLINK_CREATED=""
if ! path_has_dir "$BIN_DIR"; then
    for candidate in "$HOME/.local/bin" "/usr/local/bin"; do
        if path_has_dir "$candidate" && [[ -d "$candidate" && -w "$candidate" ]]; then
            ln -sf "$BIN_DIR/omg" "$candidate/omg"
            ln -sf "$BIN_DIR/oh-my-grok" "$candidate/oh-my-grok"
            SYMLINK_CREATED="$candidate"
            break
        fi
    done
fi

user_shell="$(basename "${SHELL:-}")"
config_file=""
case "$user_shell" in
    bash) config_file="$HOME/.bashrc" ;;
    zsh) config_file="$HOME/.zshrc" ;;
    fish) config_file="$HOME/.config/fish/config.fish" ;;
esac

if [[ -n "$config_file" && -z "$SYMLINK_CREATED" ]] && ! path_has_dir "$BIN_DIR"; then
    mkdir -p "$(dirname "$config_file")"
    # Rewrite the physical dotfile target instead of replacing a stow-style
    # symlink with a regular file.
    if [[ -e "$config_file" || -L "$config_file" ]]; then
        resolved_config="$config_file"
        depth=0
        while [[ -L "$resolved_config" && $depth -lt 40 ]]; do
            link_value=$(readlink "$resolved_config") || break
            if [[ "$link_value" == /* ]]; then
                resolved_config="$link_value"
            else
                resolved_config="$(cd "$(dirname "$resolved_config")" && pwd -P)/$link_value"
            fi
            depth=$((depth + 1))
        done
        if [[ ! -L "$resolved_config" ]]; then
            config_file="$(cd "$(dirname "$resolved_config")" && pwd -P)/$(basename "$resolved_config")"
        fi
    fi
    if [[ "$user_shell" == "fish" ]]; then
        new_block="# >>> oh-my-grok installer >>>
fish_add_path $BIN_DIR
# <<< oh-my-grok installer <<<"
    elif [[ "$user_shell" == "zsh" ]]; then
        new_block="# >>> oh-my-grok installer >>>
export PATH=\"$BIN_DIR:\$PATH\"
fpath=($OMG_HOME/completions/zsh \$fpath)
autoload -Uz compinit && compinit -C
# <<< oh-my-grok installer <<<"
    else
        new_block="# >>> oh-my-grok installer >>>
export PATH=\"$BIN_DIR:\$PATH\"
[[ -r \"$OMG_HOME/completions/bash/omg.bash\" ]] && source \"$OMG_HOME/completions/bash/omg.bash\"
# <<< oh-my-grok installer <<<"
    fi
    rc_tmp="${config_file}.tmp.$$"
    if grep -qs '# >>> oh-my-grok installer >>>' "$config_file" 2>/dev/null; then
        awk '
            /# >>> oh-my-grok installer >>>/ { skip=1; next }
            /# <<< oh-my-grok installer <<</ { skip=0; next }
            !skip { print }
        ' "$config_file" > "$rc_tmp"
    else
        [[ -f "$config_file" ]] && cp "$config_file" "$rc_tmp" || : > "$rc_tmp"
    fi
    printf '\n%s\n' "$new_block" >> "$rc_tmp"
    mv "$rc_tmp" "$config_file"
fi

rm -f "$sums_tmp"
trap - EXIT

echo "oh-my-grok $version installed." >&2
if path_has_dir "$BIN_DIR" || [[ -n "$SYMLINK_CREATED" ]]; then
    echo "Run 'omg' or 'oh-my-grok' to get started." >&2
elif [[ -n "$config_file" ]]; then
    echo "Restart your terminal, then run 'omg' or 'oh-my-grok'." >&2
else
    echo "Add $BIN_DIR to PATH, then run 'omg' or 'oh-my-grok'." >&2
fi
