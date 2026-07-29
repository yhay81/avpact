# Installing AVPact

AVPact publishes native archives for Linux x86_64, macOS Apple Silicon and
Intel, and Windows x86_64. The commands below download only the archive for the
current machine, verify its checksum and GitHub build provenance, and install
the binary under the current user account.

FFmpeg and FFprobe must also be available on `PATH`.

## macOS or Linux

The [GitHub CLI](https://cli.github.com/) is required. Run this from a clean
temporary directory:

```sh
version=v0.3.0
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) platform=macos-aarch64; checksum() { shasum -a 256 "$@"; } ;;
  Darwin-x86_64) platform=macos-x86_64; checksum() { shasum -a 256 "$@"; } ;;
  Linux-x86_64) platform=linux-x86_64; checksum() { sha256sum "$@"; } ;;
  *) printf 'Unsupported platform: %s-%s\n' "$(uname -s)" "$(uname -m)" >&2; exit 1 ;;
esac

archive="avpact-${version}-${platform}.tar.gz"
gh release download "$version" --repo yhay81/avpact \
  --pattern "$archive" --pattern SHA256SUMS
awk -v file="./$archive" \
  '$2 == file { print; found = 1 } END { if (!found) exit 1 }' \
  SHA256SUMS | checksum -c -
gh attestation verify "$archive" --repo yhay81/avpact
tar -xzf "$archive"
install -d "$HOME/.local/bin"
install -m 0755 "${archive%.tar.gz}/avpact" "$HOME/.local/bin/avpact"
"$HOME/.local/bin/avpact" --version
```

Add `$HOME/.local/bin` to `PATH` if it is not already present. Run
`avpact capabilities --format json` after installing FFmpeg to record the
available codecs and filters before using AVPact in automation.

## Windows

Run PowerShell in a clean temporary directory:

```powershell
$ErrorActionPreference = "Stop"
$version = "v0.3.0"
$archive = "avpact-$version-windows-x86_64.zip"
gh release download $version --repo yhay81/avpact `
  --pattern $archive --pattern "SHA256SUMS"
if ($LASTEXITCODE -ne 0) { throw "Release download failed" }
$checksumLine = Get-Content SHA256SUMS |
  Where-Object { ($_ -split '\s+')[1] -eq "./$archive" }
if (-not $checksumLine) { throw "Archive checksum not found" }
$expected = ($checksumLine -split '\s+')[0].ToLowerInvariant()
$actual = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw "Checksum mismatch" }
gh attestation verify $archive --repo yhay81/avpact
if ($LASTEXITCODE -ne 0) { throw "Attestation verification failed" }
Expand-Archive $archive -DestinationPath .
$bin = Join-Path $HOME ".local\bin"
New-Item -ItemType Directory -Force $bin | Out-Null
Copy-Item "avpact-$version-windows-x86_64\avpact.exe" `
  (Join-Path $bin "avpact.exe") -Force
& (Join-Path $bin "avpact.exe") --version
if ($LASTEXITCODE -ne 0) { throw "Installed binary failed" }
```

Add `$HOME\.local\bin` to the user `PATH` if necessary.

## Build from source

Rust 1.85 or newer is required:

```sh
git clone https://github.com/yhay81/avpact.git
cd avpact
cargo install --path . --locked
avpact --version
```

## Update or remove

To update, repeat the verified installation with the desired immutable release
version. To remove a native installation, delete
`$HOME/.local/bin/avpact` on macOS/Linux or
`$HOME\.local\bin\avpact.exe` on Windows. AVPact state and receipts are not
removed automatically.
