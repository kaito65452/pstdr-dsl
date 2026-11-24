#!/bin/sh
set -eu

# ─── Configuration & Environment Overrides ────────────────────────────────────
PSTDR_YES="${PSTDR_YES:-0}"
PSTDR_LSP="${PSTDR_LSP:-1}"
PSTDR_STUDIO="${PSTDR_STUDIO:-0}"
PSTDR_INSTALL_DIR="${PSTDR_INSTALL_DIR:-$HOME/.local/bin}"
PSTDR_REPO_URL="${PSTDR_REPO_URL:-https://github.com/kaito65452/pstdr-dsl.git}"
PSTDR_BRANCH="${PSTDR_BRANCH:-main}"
PSTDR_STUDIO_REPO_URL="${PSTDR_STUDIO_REPO_URL:-https://github.com/kaito65452/packstudio.git}"

# ─── ANSI Colors (disabled automatically when stdout is not a TTY) ────────────
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    C_RESET="$(printf '\033[0m')"
    C_BOLD="$(printf '\033[1m')"
    C_CYAN="$(printf '\033[36m')"
    C_GREEN="$(printf '\033[32m')"
    C_YELLOW="$(printf '\033[33m')"
    C_RED="$(printf '\033[31m')"
    C_DIM="$(printf '\033[2m')"
    IS_TTY=1
else
    C_RESET=""
    C_BOLD=""
    C_CYAN=""
    C_GREEN=""
    C_YELLOW=""
    C_RED=""
    C_DIM=""
    IS_TTY=0
fi

TMP_DIR="$(mktemp -d 2>/dev/null || mktemp -d -t pstdr-install)"
STEP_PID=""

cleanup() {
    if [ -n "$STEP_PID" ]; then
        kill "$STEP_PID" 2>/dev/null || true
    fi
    if [ "$IS_TTY" -eq 1 ]; then
        printf '\033[?25h'
    fi
    if [ -n "$TMP_DIR" ] && [ -d "$TMP_DIR" ]; then
        rm -rf "$TMP_DIR"
    fi
}

trap cleanup EXIT INT TERM

print_banner() {
    printf '%s%s' "$C_CYAN" "$C_BOLD"
    printf '  ____  ____ _____ ____  ____  \n'
    printf ' |  _ \\/ ___|_   _|  _ \\|  _ \\ \n'
    printf ' | |_) \\___ \\ | | | | | | |_) |\n'
    printf ' |  __/ ___) || | | |_| |  _ < \n'
    printf ' |_|   |____/ |_| |____/|_| \\_\\\n'
    printf '%s' "$C_RESET"
    printf ' %sMinecraft Bedrock Add-On Language & Toolchain%s\n\n' "$C_DIM" "$C_RESET"
}

spinner_char() {
    case "$1" in
        0) printf '⠋' ;;
        1) printf '⠙' ;;
        2) printf '⠹' ;;
        3) printf '⠸' ;;
        4) printf '⠼' ;;
        5) printf '⠴' ;;
        6) printf '⠦' ;;
        7) printf '⠧' ;;
        8) printf '⠇' ;;
        *) printf '⠏' ;;
    esac
}

run_step() {
    step_label="$1"
    shift
    step_log="$TMP_DIR/step.log"
    : > "$step_log"

    "$@" > "$step_log" 2>&1 &
    STEP_PID=$!

    start_ts="$(date +%s)"
    idx=0

    if [ "$IS_TTY" -eq 1 ]; then
        printf '\033[?25l'
        while kill -0 "$STEP_PID" 2>/dev/null; do
            now_ts="$(date +%s)"
            elapsed=$((now_ts - start_ts))
            spin="$(spinner_char "$idx")"
            printf '\r  %s%s%s %s %s(%ds)%s   ' "$C_CYAN" "$spin" "$C_RESET" "$step_label" "$C_DIM" "$elapsed" "$C_RESET"
            idx=$(( (idx + 1) % 10 ))
            sleep 0.1 2>/dev/null || sleep 1
        done
    fi

    if wait "$STEP_PID"; then
        STEP_PID=""
        now_ts="$(date +%s)"
        elapsed=$((now_ts - start_ts))
        if [ "$IS_TTY" -eq 1 ]; then
            printf '\r\033[2K'
            printf '\033[?25h'
        fi
        printf '  %s✔%s %s %s(%ds)%s\n' "$C_GREEN" "$C_RESET" "$step_label" "$C_DIM" "$elapsed" "$C_RESET"
        return 0
    else
        exit_code=$?
        STEP_PID=""
        if [ "$IS_TTY" -eq 1 ]; then
            printf '\r\033[2K'
            printf '\033[?25h'
        fi
        printf '  %s✖%s %s %s(failed)%s\n\n' "$C_RED" "$C_RESET" "$step_label" "$C_RED" "$C_RESET"
        printf '%s--- Last 25 lines of log ---%s\n' "$C_YELLOW" "$C_RESET"
        tail -n 25 "$step_log" 2>/dev/null || true
        printf '%s----------------------------%s\n' "$C_YELLOW" "$C_RESET"
        return "$exit_code"
    fi
}

can_prompt() {
    [ "$PSTDR_YES" != "1" ] && [ -r /dev/tty ] && [ -t 1 ]
}

check_dependencies() {
    if [ -f "$HOME/.cargo/env" ]; then
        # shellcheck disable=SC1091
        . "$HOME/.cargo/env"
    fi

    if ! command -v git >/dev/null 2>&1; then
        printf '%sError:%s git is required but was not found in PATH.\n' "$C_RED" "$C_RESET"
        printf 'Please install git first and re-run this installer.\n'
        exit 1
    fi

    if ! command -v cargo >/dev/null 2>&1; then
        printf '%sNotice:%s Rust (cargo) was not found in PATH.\n' "$C_YELLOW" "$C_RESET"
        install_rust="n"
        if can_prompt; then
            printf 'Install Rust automatically via rustup? [Y/n]: '
            read -r ans < /dev/tty || ans="n"
            case "$ans" in
                ""|y|Y|yes|YES) install_rust="y" ;;
                *) install_rust="n" ;;
            esac
        elif [ "$PSTDR_YES" = "1" ]; then
            install_rust="y"
        fi

        if [ "$install_rust" = "y" ]; then
            if ! command -v curl >/dev/null 2>&1; then
                printf '%sError:%s curl is required to install rustup.\n' "$C_RED" "$C_RESET"
                exit 1
            fi
            run_step "Installing Rust toolchain (rustup)" sh -c "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y"
            if [ -f "$HOME/.cargo/env" ]; then
                # shellcheck disable=SC1091
                . "$HOME/.cargo/env"
            fi
        else
            printf 'Please install Rust from https://rustup.rs and re-run this installer.\n'
            exit 1
        fi
    fi
}

select_components() {
    if ! can_prompt; then
        return 0
    fi

    while true; do
        lsp_mark=" "
        [ "$PSTDR_LSP" = "1" ] && lsp_mark="x"
        studio_mark=" "
        [ "$PSTDR_STUDIO" = "1" ] && studio_mark="x"

        printf '%sSelect components to install:%s\n' "$C_BOLD" "$C_RESET"
        printf '  1) %s[x]%s pstdr-c      %s(Compiler & CLI — required)%s\n' "$C_GREEN" "$C_RESET" "$C_DIM" "$C_RESET"
        printf '  2) [%s] pstdr-lsp    %s(Language Server for editors)%s\n' "$lsp_mark" "$C_DIM" "$C_RESET"
        printf '  3) [%s] PackStudio   %s(Optional GUI/companion app)%s\n' "$studio_mark" "$C_DIM" "$C_RESET"
        printf '\nType 2 or 3 to toggle, Enter to continue, or q to cancel: '

        read -r choice < /dev/tty || choice=""
        case "$choice" in
            "")
                printf '\n'
                break
                ;;
            1)
                printf '%spstdr-c is required and cannot be unchecked.%s\n\n' "$C_YELLOW" "$C_RESET"
                ;;
            2)
                if [ "$PSTDR_LSP" = "1" ]; then
                    PSTDR_LSP="0"
                else
                    PSTDR_LSP="1"
                fi
                printf '\n'
                ;;
            3)
                if [ "$PSTDR_STUDIO" = "1" ]; then
                    PSTDR_STUDIO="0"
                else
                    PSTDR_STUDIO="1"
                fi
                printf '\n'
                ;;
            q|Q)
                printf 'Installation cancelled.\n'
                exit 0
                ;;
            *)
                printf '%sInvalid option:%s %s\n\n' "$C_YELLOW" "$C_RESET" "$choice"
                ;;
        esac
    done
}

find_binary() {
    bin_name="$1"
    repo_dir="$2"
    if [ -f "$repo_dir/target/release/$bin_name" ]; then
        printf '%s' "$repo_dir/target/release/$bin_name"
        return 0
    fi
    if [ -f "$repo_dir/crates/$bin_name/target/release/$bin_name" ]; then
        printf '%s' "$repo_dir/crates/$bin_name/target/release/$bin_name"
        return 0
    fi
    return 1
}

main() {
    print_banner
    check_dependencies
    select_components

    src_dir="$TMP_DIR/pstdr"
    mkdir -p "$PSTDR_INSTALL_DIR"

    run_step "Cloning pstdr ($PSTDR_BRANCH)" git clone --depth 1 --branch "$PSTDR_BRANCH" "$PSTDR_REPO_URL" "$src_dir"

    installed_list=""

    # 1. Build & install pstdr-c (required)
    run_step "Building pstdr-c (release)" cargo build --release --manifest-path "$src_dir/Cargo.toml" -p pstdr-cli
    if cli_bin="$(find_binary "pstdr-c" "$src_dir")"; then
        cp "$cli_bin" "$PSTDR_INSTALL_DIR/pstdr-c"
        chmod +x "$PSTDR_INSTALL_DIR/pstdr-c"
        installed_list="pstdr-c"
    else
        printf '%sError:%s Built binary pstdr-c was not found.\n' "$C_RED" "$C_RESET"
        exit 1
    fi

    # 2. Build & install pstdr-lsp (optional, default enabled)
    if [ "$PSTDR_LSP" = "1" ]; then
        if [ -f "$src_dir/crates/pstdr-lsp/Cargo.toml" ]; then
            run_step "Building pstdr-lsp (release)" cargo build --release --manifest-path "$src_dir/crates/pstdr-lsp/Cargo.toml"
            if lsp_bin="$(find_binary "pstdr-lsp" "$src_dir")"; then
                cp "$lsp_bin" "$PSTDR_INSTALL_DIR/pstdr-lsp"
                chmod +x "$PSTDR_INSTALL_DIR/pstdr-lsp"
                installed_list="$installed_list, pstdr-lsp"
            else
                printf '  %s!%s Built binary pstdr-lsp was not found; skipping.\n' "$C_YELLOW" "$C_RESET"
            fi
        else
            printf '  %s!%s crates/pstdr-lsp not found in repository; skipping pstdr-lsp.\n' "$C_YELLOW" "$C_RESET"
        fi
    fi

    # 3. Build & install PackStudio (optional, default disabled)
    if [ "$PSTDR_STUDIO" = "1" ]; then
        if [ -f "$src_dir/crates/packstudio/Cargo.toml" ]; then
            run_step "Building PackStudio (release)" cargo build --release --manifest-path "$src_dir/crates/packstudio/Cargo.toml"
            if studio_bin="$(find_binary "packstudio" "$src_dir")"; then
                cp "$studio_bin" "$PSTDR_INSTALL_DIR/packstudio"
                chmod +x "$PSTDR_INSTALL_DIR/packstudio"
                installed_list="$installed_list, packstudio"
            else
                printf '  %s!%s Built binary packstudio was not found; skipping.\n' "$C_YELLOW" "$C_RESET"
            fi
        else
            studio_dir="$TMP_DIR/packstudio"
            if run_step "Cloning PackStudio" git clone --depth 1 "$PSTDR_STUDIO_REPO_URL" "$studio_dir"; then
                if [ -f "$studio_dir/Cargo.toml" ]; then
                    run_step "Building PackStudio (release)" cargo build --release --manifest-path "$studio_dir/Cargo.toml"
                    if [ -f "$studio_dir/target/release/packstudio" ]; then
                        cp "$studio_dir/target/release/packstudio" "$PSTDR_INSTALL_DIR/packstudio"
                        chmod +x "$PSTDR_INSTALL_DIR/packstudio"
                        installed_list="$installed_list, packstudio"
                    else
                        printf '  %s!%s PackStudio binary not found after build; skipping.\n' "$C_YELLOW" "$C_RESET"
                    fi
                else
                    printf '  %s!%s PackStudio repository has no Cargo.toml; skipping.\n' "$C_YELLOW" "$C_RESET"
                fi
            else
                printf '  %s!%s PackStudio repository is not available yet (%s); skipping.\n' "$C_YELLOW" "$C_RESET" "$PSTDR_STUDIO_REPO_URL"
            fi
        fi
    fi

    printf '\n%s%sInstallation complete!%s\n' "$C_GREEN" "$C_BOLD" "$C_RESET"
    printf '  Installed : %s\n' "$installed_list"
    printf '  Location  : %s\n\n' "$PSTDR_INSTALL_DIR"

    case ":${PATH:-}:" in
        *":$PSTDR_INSTALL_DIR:"*)
            printf 'Run %spstdr-c --help%s or %spstdr-c doctor%s to get started.\n' "$C_CYAN" "$C_RESET" "$C_CYAN" "$C_RESET"
            ;;
        *)
            printf '%sWarning:%s %s is not in your PATH.\n' "$C_YELLOW" "$C_RESET" "$PSTDR_INSTALL_DIR"
            printf 'Add it to your shell configuration:\n\n'
            printf '  %sBash / Zsh:%s\n' "$C_BOLD" "$C_RESET"
            printf '    export PATH="%s:$PATH"\n\n' "$PSTDR_INSTALL_DIR"
            printf '  %sFish:%s\n' "$C_BOLD" "$C_RESET"
            printf '    fish_add_path %s\n\n' "$PSTDR_INSTALL_DIR"
            ;;
    esac
}

main "$@"
