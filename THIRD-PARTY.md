# Third-party software bundled with Solon

Solon itself is licensed under the Apache License 2.0 (`LICENSE`). The installer and the engine image
redistribute the following components, each under its own licence. Nothing below is modified unless stated.

## Shipped in the installer (`C:\Program Files\Solon`)

| Component | Version | Licence | Source |
|---|---|---|---|
| Docker CLI (`bin\docker-cli.exe`) | 29.7.2 | Apache-2.0 | <https://github.com/docker/cli> — downloaded unmodified by `installer/fetch-docker-cli.ps1` |
| Docker Compose plugin (`bin\cli-plugins\docker-compose.exe`) | 5.1.4 | Apache-2.0 | <https://github.com/docker/compose> |
| Urbanist font (`apps/desktop/src/assets/fonts`) | variable | SIL Open Font License 1.1 (`OFL.txt`) | <https://github.com/coreyhu/Urbanist> |
| WebView2 runtime | system | Microsoft licence | provided by Windows; not redistributed |

## The engine image (`image\vmlinuz`, `initrd.img`, `rootfs.vhd`)

The image is built by the reproducible pipeline in `image/` (see `image/build.sh`).

### Linux kernel — GPL-2.0 (with syscall exception)

- Source: Microsoft's WSL2 Linux kernel tree, tag `linux-msft-wsl-6.18.40.1`,
  <https://github.com/microsoft/WSL2-Linux-Kernel/releases/tag/linux-msft-wsl-6.18.40.1>
  (itself derived from <https://kernel.org>).
- Build: `image/kernel/build-kernel.sh` applies the configuration fragment `image/kernel/solon.config` on
  top of the upstream WSL configuration and compiles with the toolchain of Alpine 3.24. The exact resulting
  configuration is shipped next to every image as `kernel.config`.
- No source change is made to the kernel. The complete corresponding source is the upstream tag above; the
  configuration is in this repository. This satisfies the source-availability requirement of the GPL for the
  binary `vmlinuz` we distribute.

### Alpine Linux packages — various licences

The root filesystem is Alpine Linux 3.24 with the packages below, installed unmodified from the official
Alpine repositories. Each package keeps its upstream licence (mostly MIT, BSD, GPL-2.0, LGPL, Apache-2.0);
the licence of each package is recorded in Alpine's package index and in the `aports` tree
(<https://gitlab.alpinelinux.org/alpine/aports>). Notable components: Docker Engine, containerd, runc
(Apache-2.0), BusyBox (GPL-2.0), musl (MIT), e2fsprogs (GPL-2.0 / LGPL-2.0), iptables / nftables
(GPL-2.0), tini (MIT).

`alpine-baselayout-3.7.2-r1`, `alpine-baselayout-data-3.7.2-r1`, `alpine-keys-2.6-r0`, `busybox-1.37.0-r31`, `busybox-binsh-1.37.0-r31`, `ca-certificates-20260611-r0`, `ca-certificates-bundle-20260611-r0`, `containerd-2.3.5-r5`, `docker-cli-29.5.3-r1`, `docker-cli-compose-5.1.4-r1`, `docker-engine-29.5.3-r1`, `e2fsprogs-1.47.4-r0`, `e2fsprogs-extra-1.47.4-r0`, `e2fsprogs-libs-1.47.4-r0`, `gmp-6.3.0-r4`, `iptables-1.8.13-r0`, `jansson-2.15.0-r0`, `libblkid-2.42.3-r1`, `libcom_err-1.47.4-r0`, `libcrypto3-3.5.8-r0`, `libeconf-0.8.3-r0`, `libfdisk-2.42.3-r1`, `libmnl-1.0.5-r2`, `libmount-2.42.3-r1`, `libncursesw-6.6_p20260516-r0`, `libnftnl-1.3.1-r0`, `libseccomp-2.6.0-r2`, `libsmartcols-2.42.3-r1`, `libuuid-2.42.3-r1`, `libxtables-1.8.13-r0`, `musl-1.2.6-r2`, `ncurses-terminfo-base-6.6_p20260516-r0`, `nftables-1.1.6-r1`, `readline-8.3.3-r1`, `runc-1.4.3-r1`, `setarch-2.42.3-r1`, `skalibs-libs-2.15.0.0-r0`, `tini-static-0.19.0-r3`, `tzdata-2026c-r0`, `util-linux-misc-2.42.3-r1`, `utmps-libs-0.1.3.3-r0`, `zlib-1.3.2-r0`

The list for a given image is written by the build as `packages.txt` next to the manifest.

### Solon components inside the image

- `solon-agent` (PID 1 of the machine), `solon-debug`: Apache-2.0, this repository.
- `fuser` 0.15.1, vendored in `third_party/fuser` with one build-script change described in
  `third_party/fuser/SOLON-NOTES.md`: MIT (`third_party/fuser/LICENSE.md`), <https://github.com/cberner/fuser>.

## Rust dependencies (service, desktop back-end, agent)

485 crates, all under permissive licences (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode-3.0, MPL-2.0 for
a handful of unmodified crates). Generated from `cargo metadata`; regenerate with the command in
`CONTRIBUTING.md` when dependencies change.

| Crate | Licence |
|---|---|
| `adler2` | 0BSD OR MIT OR Apache-2.0 |
| `aho-corasick` | Unlicense OR MIT |
| `alloc-no-stdlib` | BSD-3-Clause |
| `alloc-stdlib` | BSD-3-Clause |
| `android_system_properties` | MIT OR Apache-2.0 |
| `anyhow` | MIT OR Apache-2.0 |
| `arbitrary` | MIT OR Apache-2.0 |
| `asn1-rs` | MIT OR Apache-2.0 |
| `asn1-rs-derive` | MIT OR Apache-2.0 |
| `asn1-rs-impl` | MIT/Apache-2.0 |
| `async-broadcast` | MIT OR Apache-2.0 |
| `async-channel` | Apache-2.0 OR MIT |
| `async-executor` | Apache-2.0 OR MIT |
| `async-io` | Apache-2.0 OR MIT |
| `async-lock` | Apache-2.0 OR MIT |
| `async-process` | Apache-2.0 OR MIT |
| `async-recursion` | MIT OR Apache-2.0 |
| `async-signal` | Apache-2.0 OR MIT |
| `async-task` | Apache-2.0 OR MIT |
| `async-trait` | MIT OR Apache-2.0 |
| `atk` | MIT |
| `atk-sys` | MIT |
| `atomic-waker` | Apache-2.0 OR MIT |
| `autocfg` | Apache-2.0 OR MIT |
| `base64` | MIT OR Apache-2.0 |
| `bit-set` | Apache-2.0 OR MIT |
| `bit-vec` | Apache-2.0 OR MIT |
| `bitflags` | MIT OR Apache-2.0 |
| `block-buffer` | MIT OR Apache-2.0 |
| `block2` | MIT |
| `blocking` | Apache-2.0 OR MIT |
| `bollard` | Apache-2.0 |
| `bollard-stubs` | Apache-2.0 |
| `brotli` | BSD-3-Clause AND MIT |
| `brotli-decompressor` | BSD-3-Clause/MIT |
| `bumpalo` | MIT OR Apache-2.0 |
| `bytemuck` | Zlib OR Apache-2.0 OR MIT |
| `byteorder` | Unlicense OR MIT |
| `byteorder-lite` | Unlicense OR MIT |
| `bytes` | MIT |
| `cairo-rs` | MIT |
| `cairo-sys-rs` | MIT |
| `camino` | MIT OR Apache-2.0 |
| `cargo-platform` | MIT OR Apache-2.0 |
| `cargo_metadata` | MIT |
| `cargo_toml` | Apache-2.0 OR MIT |
| `cc` | MIT OR Apache-2.0 |
| `cesu8` | Apache-2.0/MIT |
| `cfb` | MIT |
| `cfg-expr` | MIT OR Apache-2.0 |
| `cfg-if` | MIT OR Apache-2.0 |
| `cfg_aliases` | MIT |
| `chrono` | MIT OR Apache-2.0 |
| `combine` | MIT |
| `concurrent-queue` | Apache-2.0 OR MIT |
| `cookie` | MIT OR Apache-2.0 |
| `core-foundation` | MIT OR Apache-2.0 |
| `core-foundation-sys` | MIT OR Apache-2.0 |
| `core-graphics` | MIT OR Apache-2.0 |
| `core-graphics-types` | MIT OR Apache-2.0 |
| `cpufeatures` | MIT OR Apache-2.0 |
| `crc32fast` | MIT OR Apache-2.0 |
| `crossbeam-channel` | MIT OR Apache-2.0 |
| `crossbeam-utils` | MIT OR Apache-2.0 |
| `crypto-common` | MIT OR Apache-2.0 |
| `cssparser` | MPL-2.0 |
| `cssparser-macros` | MPL-2.0 |
| `ctor` | Apache-2.0 OR MIT |
| `ctor-proc-macro` | Apache-2.0 OR MIT |
| `darling` | MIT |
| `darling_core` | MIT |
| `darling_macro` | MIT |
| `data-encoding` | MIT |
| `dbus` | Apache-2.0/MIT |
| `der-parser` | MIT OR Apache-2.0 |
| `deranged` | MIT OR Apache-2.0 |
| `derive_arbitrary` | MIT OR Apache-2.0 |
| `derive_more` | MIT |
| `derive_more-impl` | MIT |
| `digest` | MIT OR Apache-2.0 |
| `dirs` | MIT OR Apache-2.0 |
| `dirs-sys` | MIT OR Apache-2.0 |
| `dispatch2` | Zlib OR Apache-2.0 OR MIT |
| `displaydoc` | MIT OR Apache-2.0 |
| `dlopen2` | MIT |
| `dlopen2_derive` | MIT |
| `dom_query` | MIT |
| `dpi` | Apache-2.0 AND MIT |
| `dtoa` | MIT OR Apache-2.0 |
| `dtoa-short` | MPL-2.0 |
| `dtor` | Apache-2.0 OR MIT |
| `dtor-proc-macro` | Apache-2.0 OR MIT |
| `dunce` | CC0-1.0 OR MIT-0 OR Apache-2.0 |
| `dyn-clone` | MIT OR Apache-2.0 |
| `embed-resource` | MIT |
| `embed_plist` | MIT OR Apache-2.0 |
| `endi` | MIT |
| `enumflags2` | MIT OR Apache-2.0 |
| `enumflags2_derive` | MIT OR Apache-2.0 |
| `equivalent` | Apache-2.0 OR MIT |
| `erased-serde` | MIT OR Apache-2.0 |
| `errno` | MIT OR Apache-2.0 |
| `event-listener` | Apache-2.0 OR MIT |
| `event-listener-strategy` | Apache-2.0 OR MIT |
| `fastrand` | Apache-2.0 OR MIT |
| `fdeflate` | MIT OR Apache-2.0 |
| `field-offset` | MIT OR Apache-2.0 |
| `find-msvc-tools` | MIT OR Apache-2.0 |
| `flate2` | MIT OR Apache-2.0 |
| `fnv` | Apache-2.0 / MIT |
| `foldhash` | Zlib |
| `foreign-types` | MIT/Apache-2.0 |
| `foreign-types-macros` | MIT/Apache-2.0 |
| `foreign-types-shared` | MIT/Apache-2.0 |
| `form_urlencoded` | MIT OR Apache-2.0 |
| `fuser` | MIT |
| `futures-channel` | MIT OR Apache-2.0 |
| `futures-core` | MIT OR Apache-2.0 |
| `futures-executor` | MIT OR Apache-2.0 |
| `futures-io` | MIT OR Apache-2.0 |
| `futures-lite` | Apache-2.0 OR MIT |
| `futures-macro` | MIT OR Apache-2.0 |
| `futures-sink` | MIT OR Apache-2.0 |
| `futures-task` | MIT OR Apache-2.0 |
| `futures-util` | MIT OR Apache-2.0 |
| `gdk` | MIT |
| `gdk-pixbuf` | MIT |
| `gdk-pixbuf-sys` | MIT |
| `gdk-sys` | MIT |
| `gdkwayland-sys` | MIT |
| `gdkx11` | MIT |
| `gdkx11-sys` | MIT |
| `generic-array` | MIT |
| `getrandom` | MIT OR Apache-2.0 |
| `gio` | MIT |
| `gio-sys` | MIT |
| `glib` | MIT |
| `glib-macros` | MIT |
| `glib-sys` | MIT |
| `glob` | MIT OR Apache-2.0 |
| `gobject-sys` | MIT |
| `gtk` | MIT |
| `gtk-sys` | MIT |
| `gtk3-macros` | MIT |
| `hashbrown` | MIT OR Apache-2.0 |
| `heck` | MIT OR Apache-2.0 |
| `hermit-abi` | MIT OR Apache-2.0 |
| `hex` | MIT OR Apache-2.0 |
| `html5ever` | MIT OR Apache-2.0 |
| `http` | MIT OR Apache-2.0 |
| `http-body` | MIT |
| `http-body-util` | MIT |
| `httparse` | MIT OR Apache-2.0 |
| `httpdate` | MIT OR Apache-2.0 |
| `hyper` | MIT |
| `hyper-named-pipe` | Apache-2.0 |
| `hyper-util` | MIT |
| `hyperlocal` | MIT |
| `iana-time-zone` | MIT OR Apache-2.0 |
| `iana-time-zone-haiku` | MIT OR Apache-2.0 |
| `ico` | MIT |
| `icu_collections` | Unicode-3.0 |
| `icu_locale_core` | Unicode-3.0 |
| `icu_normalizer` | Unicode-3.0 |
| `icu_normalizer_data` | Unicode-3.0 |
| `icu_properties` | Unicode-3.0 |
| `icu_properties_data` | Unicode-3.0 |
| `icu_provider` | Unicode-3.0 |
| `ident_case` | MIT/Apache-2.0 |
| `idna` | MIT OR Apache-2.0 |
| `idna_adapter` | Apache-2.0 OR MIT |
| `image` | MIT OR Apache-2.0 |
| `indexmap` | Apache-2.0 OR MIT |
| `infer` | MIT |
| `ipnet` | MIT OR Apache-2.0 |
| `is-docker` | MIT |
| `is-wsl` | MIT |
| `itoa` | MIT OR Apache-2.0 |
| `javascriptcore-rs` | MIT |
| `javascriptcore-rs-sys` | MIT |
| `jni` | MIT/Apache-2.0 |
| `jni-sys` | MIT OR Apache-2.0 |
| `jni-sys-macros` | MIT OR Apache-2.0 |
| `js-sys` | MIT OR Apache-2.0 |
| `json-patch` | MIT/Apache-2.0 |
| `jsonptr` | MIT OR Apache-2.0 |
| `keyboard-types` | MIT OR Apache-2.0 |
| `lazy_static` | MIT OR Apache-2.0 |
| `libappindicator` | Apache-2.0 OR MIT |
| `libappindicator-sys` | Apache-2.0 OR MIT |
| `libc` | MIT OR Apache-2.0 |
| `libdbus-sys` | Apache-2.0/MIT |
| `libloading` | ISC |
| `libredox` | MIT |
| `linux-raw-sys` | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| `litemap` | Unicode-3.0 |
| `lock_api` | MIT OR Apache-2.0 |
| `log` | MIT OR Apache-2.0 |
| `mac-notification-sys` | MIT/Apache-2.0 |
| `markup5ever` | MIT OR Apache-2.0 |
| `matchers` | MIT |
| `memchr` | Unlicense OR MIT |
| `memoffset` | MIT |
| `mime` | MIT OR Apache-2.0 |
| `minimal-lexical` | MIT/Apache-2.0 |
| `miniz_oxide` | MIT OR Zlib OR Apache-2.0 |
| `mio` | MIT |
| `moxcms` | BSD-3-Clause OR Apache-2.0 |
| `muda` | Apache-2.0 OR MIT |
| `ndk` | MIT OR Apache-2.0 |
| `ndk-sys` | MIT OR Apache-2.0 |
| `new_debug_unreachable` | MIT |
| `nix` | MIT |
| `nom` | MIT |
| `notify-rust` | MIT/Apache-2.0 |
| `nu-ansi-term` | MIT |
| `num-bigint` | MIT OR Apache-2.0 |
| `num-conv` | MIT OR Apache-2.0 |
| `num-integer` | MIT OR Apache-2.0 |
| `num-traits` | MIT OR Apache-2.0 |
| `num_enum` | BSD-3-Clause OR MIT OR Apache-2.0 |
| `num_enum_derive` | BSD-3-Clause OR MIT OR Apache-2.0 |
| `objc2` | MIT |
| `objc2-app-kit` | Zlib OR Apache-2.0 OR MIT |
| `objc2-cloud-kit` | Zlib OR Apache-2.0 OR MIT |
| `objc2-core-data` | Zlib OR Apache-2.0 OR MIT |
| `objc2-core-foundation` | Zlib OR Apache-2.0 OR MIT |
| `objc2-core-graphics` | Zlib OR Apache-2.0 OR MIT |
| `objc2-core-image` | Zlib OR Apache-2.0 OR MIT |
| `objc2-core-location` | Zlib OR Apache-2.0 OR MIT |
| `objc2-core-text` | Zlib OR Apache-2.0 OR MIT |
| `objc2-encode` | MIT |
| `objc2-exception-helper` | Zlib OR Apache-2.0 OR MIT |
| `objc2-foundation` | MIT |
| `objc2-io-surface` | Zlib OR Apache-2.0 OR MIT |
| `objc2-quartz-core` | Zlib OR Apache-2.0 OR MIT |
| `objc2-ui-kit` | Zlib OR Apache-2.0 OR MIT |
| `objc2-user-notifications` | Zlib OR Apache-2.0 OR MIT |
| `objc2-web-kit` | Zlib OR Apache-2.0 OR MIT |
| `oid-registry` | MIT OR Apache-2.0 |
| `once_cell` | MIT OR Apache-2.0 |
| `open` | MIT |
| `option-ext` | MPL-2.0 |
| `ordered-stream` | MIT OR Apache-2.0 |
| `page_size` | MIT/Apache-2.0 |
| `pango` | MIT |
| `pango-sys` | MIT |
| `parking` | Apache-2.0 OR MIT |
| `parking_lot` | MIT OR Apache-2.0 |
| `parking_lot_core` | MIT OR Apache-2.0 |
| `pem` | MIT |
| `percent-encoding` | MIT OR Apache-2.0 |
| `phf` | MIT |
| `phf_codegen` | MIT |
| `phf_generator` | MIT |
| `phf_macros` | MIT |
| `phf_shared` | MIT |
| `pin-project-lite` | Apache-2.0 OR MIT |
| `piper` | MIT OR Apache-2.0 |
| `pkg-config` | MIT OR Apache-2.0 |
| `plist` | MIT |
| `png` | MIT OR Apache-2.0 |
| `polling` | Apache-2.0 OR MIT |
| `potential_utf` | Unicode-3.0 |
| `powerfmt` | MIT OR Apache-2.0 |
| `ppv-lite86` | MIT OR Apache-2.0 |
| `precomputed-hash` | MIT |
| `proc-macro-crate` | MIT OR Apache-2.0 |
| `proc-macro-error` | MIT OR Apache-2.0 |
| `proc-macro-error-attr` | MIT OR Apache-2.0 |
| `proc-macro2` | MIT OR Apache-2.0 |
| `pxfm` | BSD-3-Clause OR Apache-2.0 |
| `quick-xml` | MIT |
| `quote` | MIT OR Apache-2.0 |
| `r-efi` | MIT OR Apache-2.0 OR LGPL-2.1-or-later |
| `rand` | MIT OR Apache-2.0 |
| `rand_chacha` | MIT OR Apache-2.0 |
| `rand_core` | MIT OR Apache-2.0 |
| `raw-window-handle` | MIT OR Apache-2.0 OR Zlib |
| `rcgen` | MIT OR Apache-2.0 |
| `redox_syscall` | MIT |
| `redox_users` | MIT |
| `ref-cast` | MIT OR Apache-2.0 |
| `ref-cast-impl` | MIT OR Apache-2.0 |
| `regex` | MIT OR Apache-2.0 |
| `regex-automata` | MIT OR Apache-2.0 |
| `regex-syntax` | MIT OR Apache-2.0 |
| `reqwest` | MIT OR Apache-2.0 |
| `rfd` | MIT |
| `ring` | Apache-2.0 AND ISC |
| `rustc-hash` | Apache-2.0 OR MIT |
| `rustc_version` | MIT OR Apache-2.0 |
| `rusticata-macros` | MIT/Apache-2.0 |
| `rustix` | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| `rustls` | Apache-2.0 OR ISC OR MIT |
| `rustls-pki-types` | MIT OR Apache-2.0 |
| `rustls-webpki` | ISC |
| `rustversion` | MIT OR Apache-2.0 |
| `ryu` | Apache-2.0 OR BSL-1.0 |
| `same-file` | Unlicense/MIT |
| `schemars` | MIT |
| `schemars_derive` | MIT |
| `scopeguard` | MIT OR Apache-2.0 |
| `selectors` | MPL-2.0 |
| `semver` | MIT OR Apache-2.0 |
| `serde` | MIT OR Apache-2.0 |
| `serde-untagged` | MIT OR Apache-2.0 |
| `serde_core` | MIT OR Apache-2.0 |
| `serde_derive` | MIT OR Apache-2.0 |
| `serde_derive_internals` | MIT OR Apache-2.0 |
| `serde_json` | MIT OR Apache-2.0 |
| `serde_repr` | MIT OR Apache-2.0 |
| `serde_spanned` | MIT OR Apache-2.0 |
| `serde_urlencoded` | MIT/Apache-2.0 |
| `serde_with` | MIT OR Apache-2.0 |
| `serde_with_macros` | MIT OR Apache-2.0 |
| `serialize-to-javascript` | MIT OR Apache-2.0 |
| `serialize-to-javascript-impl` | MIT OR Apache-2.0 |
| `servo_arc` | MIT OR Apache-2.0 |
| `sha2` | MIT OR Apache-2.0 |
| `sharded-slab` | MIT |
| `shlex` | MIT OR Apache-2.0 |
| `signal-hook-registry` | MIT OR Apache-2.0 |
| `simd-adler32` | MIT |
| `siphasher` | MIT/Apache-2.0 |
| `slab` | MIT |
| `smallvec` | MIT OR Apache-2.0 |
| `socket2` | MIT OR Apache-2.0 |
| `softbuffer` | MIT OR Apache-2.0 |
| `soup3` | MIT |
| `soup3-sys` | MIT |
| `stable_deref_trait` | MIT OR Apache-2.0 |
| `string_cache` | MIT OR Apache-2.0 |
| `string_cache_codegen` | MIT OR Apache-2.0 |
| `strsim` | MIT |
| `subtle` | BSD-3-Clause |
| `swift-rs` | MIT OR Apache-2.0 |
| `symlink` | MIT/Apache-2.0 |
| `syn` | MIT OR Apache-2.0 |
| `sync_wrapper` | Apache-2.0 |
| `synstructure` | MIT |
| `system-deps` | MIT OR Apache-2.0 |
| `tao` | Apache-2.0 |
| `tao-macros` | MIT OR Apache-2.0 |
| `target-lexicon` | Apache-2.0 WITH LLVM-exception |
| `tauri` | Apache-2.0 OR MIT |
| `tauri-build` | Apache-2.0 OR MIT |
| `tauri-codegen` | Apache-2.0 OR MIT |
| `tauri-macros` | Apache-2.0 OR MIT |
| `tauri-plugin` | Apache-2.0 OR MIT |
| `tauri-plugin-dialog` | Apache-2.0 OR MIT |
| `tauri-plugin-fs` | Apache-2.0 OR MIT |
| `tauri-plugin-notification` | Apache-2.0 OR MIT |
| `tauri-plugin-opener` | Apache-2.0 OR MIT |
| `tauri-plugin-window-state` | Apache-2.0 OR MIT |
| `tauri-runtime` | Apache-2.0 OR MIT |
| `tauri-runtime-wry` | Apache-2.0 OR MIT |
| `tauri-utils` | Apache-2.0 OR MIT |
| `tauri-winres` | MIT |
| `tauri-winrt-notification` | MIT OR Apache-2.0 |
| `tempfile` | MIT OR Apache-2.0 |
| `tendril` | MIT OR Apache-2.0 |
| `thiserror` | MIT OR Apache-2.0 |
| `thiserror-impl` | MIT OR Apache-2.0 |
| `thread_local` | MIT OR Apache-2.0 |
| `time` | MIT OR Apache-2.0 |
| `time-core` | MIT OR Apache-2.0 |
| `time-macros` | MIT OR Apache-2.0 |
| `tinystr` | Unicode-3.0 |
| `tokio` | MIT |
| `tokio-macros` | MIT |
| `tokio-rustls` | MIT OR Apache-2.0 |
| `tokio-util` | MIT |
| `toml` | MIT OR Apache-2.0 |
| `toml_datetime` | MIT OR Apache-2.0 |
| `toml_edit` | MIT OR Apache-2.0 |
| `toml_parser` | MIT OR Apache-2.0 |
| `toml_writer` | MIT OR Apache-2.0 |
| `tower` | MIT |
| `tower-http` | MIT |
| `tower-layer` | MIT |
| `tower-service` | MIT |
| `tracing` | MIT |
| `tracing-appender` | MIT |
| `tracing-attributes` | MIT |
| `tracing-core` | MIT |
| `tracing-log` | MIT |
| `tracing-subscriber` | MIT |
| `tray-icon` | MIT OR Apache-2.0 |
| `try-lock` | MIT |
| `typeid` | MIT OR Apache-2.0 |
| `typenum` | MIT OR Apache-2.0 |
| `uds_windows` | MIT |
| `unic-char-property` | MIT/Apache-2.0 |
| `unic-char-range` | MIT/Apache-2.0 |
| `unic-common` | MIT/Apache-2.0 |
| `unic-ucd-ident` | MIT/Apache-2.0 |
| `unic-ucd-version` | MIT/Apache-2.0 |
| `unicode-ident` | (MIT OR Apache-2.0) AND Unicode-3.0 |
| `unicode-segmentation` | MIT OR Apache-2.0 |
| `untrusted` | ISC |
| `url` | MIT OR Apache-2.0 |
| `urlpattern` | MIT |
| `utf8_iter` | Apache-2.0 OR MIT |
| `uuid` | Apache-2.0 OR MIT |
| `valuable` | MIT |
| `version-compare` | MIT |
| `version_check` | MIT/Apache-2.0 |
| `vswhom` | MIT |
| `vswhom-sys` | MIT |
| `walkdir` | Unlicense/MIT |
| `want` | MIT |
| `wasi` | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| `wasip2` | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| `wasm-bindgen` | MIT OR Apache-2.0 |
| `wasm-bindgen-futures` | MIT OR Apache-2.0 |
| `wasm-bindgen-macro` | MIT OR Apache-2.0 |
| `wasm-bindgen-macro-support` | MIT OR Apache-2.0 |
| `wasm-bindgen-shared` | MIT OR Apache-2.0 |
| `wasm-streams` | MIT OR Apache-2.0 |
| `web-sys` | MIT OR Apache-2.0 |
| `web_atoms` | MIT OR Apache-2.0 |
| `webkit2gtk` | MIT |
| `webkit2gtk-sys` | MIT |
| `webview2-com` | MIT |
| `webview2-com-macros` | MIT |
| `webview2-com-sys` | MIT |
| `widestring` | MIT OR Apache-2.0 |
| `winapi` | MIT/Apache-2.0 |
| `winapi-i686-pc-windows-gnu` | MIT/Apache-2.0 |
| `winapi-util` | Unlicense OR MIT |
| `winapi-x86_64-pc-windows-gnu` | MIT/Apache-2.0 |
| `window-vibrancy` | Apache-2.0 OR MIT |
| `windows` | MIT OR Apache-2.0 |
| `windows-collections` | MIT OR Apache-2.0 |
| `windows-core` | MIT OR Apache-2.0 |
| `windows-future` | MIT OR Apache-2.0 |
| `windows-implement` | MIT OR Apache-2.0 |
| `windows-interface` | MIT OR Apache-2.0 |
| `windows-link` | MIT OR Apache-2.0 |
| `windows-numerics` | MIT OR Apache-2.0 |
| `windows-result` | MIT OR Apache-2.0 |
| `windows-service` | MIT OR Apache-2.0 |
| `windows-strings` | MIT OR Apache-2.0 |
| `windows-sys` | MIT OR Apache-2.0 |
| `windows-targets` | MIT OR Apache-2.0 |
| `windows-threading` | MIT OR Apache-2.0 |
| `windows-version` | MIT OR Apache-2.0 |
| `windows_aarch64_gnullvm` | MIT OR Apache-2.0 |
| `windows_aarch64_msvc` | MIT OR Apache-2.0 |
| `windows_i686_gnu` | MIT OR Apache-2.0 |
| `windows_i686_gnullvm` | MIT OR Apache-2.0 |
| `windows_i686_msvc` | MIT OR Apache-2.0 |
| `windows_x86_64_gnu` | MIT OR Apache-2.0 |
| `windows_x86_64_gnullvm` | MIT OR Apache-2.0 |
| `windows_x86_64_msvc` | MIT OR Apache-2.0 |
| `winnow` | MIT |
| `winreg` | MIT |
| `wit-bindgen` | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| `writeable` | Unicode-3.0 |
| `wry` | Apache-2.0 OR MIT |
| `x11` | MIT |
| `x11-dl` | MIT |
| `x509-parser` | MIT OR Apache-2.0 |
| `yasna` | MIT OR Apache-2.0 |
| `yoke` | Unicode-3.0 |
| `yoke-derive` | Unicode-3.0 |
| `zbus` | MIT |
| `zbus_macros` | MIT |
| `zbus_names` | MIT |
| `zerocopy` | BSD-2-Clause OR Apache-2.0 OR MIT |
| `zerocopy-derive` | BSD-2-Clause OR Apache-2.0 OR MIT |
| `zerofrom` | Unicode-3.0 |
| `zerofrom-derive` | Unicode-3.0 |
| `zeroize` | Apache-2.0 OR MIT |
| `zerotrie` | Unicode-3.0 |
| `zerovec` | Unicode-3.0 |
| `zerovec-derive` | Unicode-3.0 |
| `zip` | MIT |
| `zlib-rs` | Zlib |
| `zmij` | MIT |
| `zopfli` | Apache-2.0 |
| `zvariant` | MIT |
| `zvariant_derive` | MIT |
| `zvariant_utils` | MIT |

## npm dependencies (desktop application)

Runtime dependencies bundled in the application (development tooling excluded):

| Package | Licence |
|---|---|
| `@tanstack/react-query` | MIT |
| `@tauri-apps/api` | Apache-2.0 OR MIT |
| `@tauri-apps/plugin-dialog` | MIT OR Apache-2.0 |
| `@tauri-apps/plugin-notification` | MIT OR Apache-2.0 |
| `@tauri-apps/plugin-opener` | MIT OR Apache-2.0 |
| `@xterm/addon-fit` | MIT |
| `@xterm/xterm` | MIT |
| `i18next` | MIT |
| `react` | MIT |
| `react-dom` | MIT |
| `react-i18next` | MIT |
