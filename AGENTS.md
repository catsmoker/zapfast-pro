# ZapFast agent guide

ZapFast is a small native WhatsApp client: Rust, egui, and the
[whatsapp-rust](https://github.com/oxidezap/whatsapp-rust) library for the protocol.

## Scope

- Keep it a small native client. No browser engine, no telemetry, no hosted backend, no second account system.
- Protocol behavior comes from whatsapp-rust. Do not reimplement it here, and do not treat a protobuf field as a supported feature.
- Keep changes narrow. Preserve existing behavior unless the task changes it.

## Privacy

- Do not read chat rows, message bodies, contacts, or other user content from `archive.db` or exported logs, not even read-only. Schema, column existence, and row counts are fine.
- When user data is needed for a bug, give the user the query or command and let them report back.
- Never log message contents, phone numbers, keys, or QR payloads at a level that ships. Treat existing captures the same way.

## Setup

- `rust-toolchain.toml` pins Rust 1.98.0. CI uses stable plus the same components; fix new fmt or lint failures on toolchain bumps.
- Linux builds need: `libxkbcommon-dev libwayland-dev libgl1-mesa-dev libasound2-dev cmake perl` (Debian) or `libxkbcommon wayland mesa alsa-lib cmake perl` (Arch). Voice needs cmake for bundled libopus and ALSA headers; video fallback needs no extra setup for H.264 (`openh264` builds its C++ from source, `nasm` only adds SIMD paths).
- `whatsapp-rust` is pinned to a git rev in `Cargo.toml` because crates.io 0.7.0 needs nightly Rust. Do not unpin or upgrade without checking stable builds and snapshot recovery behavior.
- GIF search key: Settings overrides the build-time `ZAPFAST_GIPHY_KEY` (`option_env!`, `FASTSAPP_GIPHY_KEY` still works as fallback). The repo carries no key.
- `demo` runs use a temp dir, never touch the linked account, archive, or tray, and can run beside the live app.

## Architecture

- `src/ui/` draws and pushes `model::Action`s; `src/app.rs` applies them after the frame. Never mutate app state from inside a view except the view's own fields (composer text, search text, flags).
- `src/backend.rs` is the UI handle to a tokio runtime on its own thread; `src/backend/worker.rs` owns the `Bot`, archive, downloads, and avatars. The sides talk only via `Command` (UI to runtime) and `Event` (runtime to UI); every event that affects the UI must wake the window through `Waker`.
- `src/model.rs` holds app types. Views never touch protobufs; the worker translates in `classify()` and `parse_conversation()`.
- Canonicalize every arriving `Jid` through `Worker::canonical`. A chat behind a privacy id (`@lid`) is filed under its phone number once the mapping is known.
- `src/archive.rs` is the only copy of history (WhatsApp replays it once, at link time) and keeps each message's raw protobuf for attachment keys. It opens with SQLCipher and a random key in the OS keyring. A locked or missing key stops linking; never fall back to a disposable archive. Tests use fixtures and mock credentials only. Plaintext migration checkpoints the WAL and verifies an encrypted staging file before atomic replacement.
- Polls use `Client::polls()`. Keep the creator identity and key in the archive (`backend/worker/polls.rs`, `archive/polls.rs`); the UI only gets counts and its own selection. Keep each voter's latest timestamp and message id, including encrypted updates whose parent has not arrived; replay must not undo a newer vote or withdrawal. Decryption runs in batches of eight with retry after reconnect. Visible polls auto-request phone history anchored after the creation message; `poll_history.rs` serializes requests and retries 30s to 15m with no UI timer. History timestamps are Unix seconds despite `Ms` in the library and wire names; do not multiply by 1,000.
- Group delivery uses `archive::receipts`: save recipients at send time, record each receipt, show the least advanced recipient. Never promote from one reader, apply a receipt to earlier messages, or infer audience from current membership. History trusts the phone's aggregate status, not a partial `user_receipt` list.
- Private read state uses the `regular_low` collection only. `backend::read_sync` allows one write at a time and backs off the whole queue on failure; pending positions stay in the archive until acknowledged. Snapshot recovery belongs to whatsapp-rust.
- Group metadata comes from `groups().get_metadata`, pumped two per 5s tick (`pump_group_info`) to avoid rate limits. Back off 30s doubling, seven tries; item-not-found, forbidden, and not-authorized are final.
- Older history is archive first, then phone on demand (`Command::FetchOlder`, `sync_type == ON_DEMAND`). Chats with a name and no messages anchor at the present with an empty message id and ask the phone on load or open.
- A 403/404/410 download goes through `client.media_reupload().request(..)` once more before reporting "No longer on WhatsApp's servers". Download failures live in the bubble as "..., click to retry", never as toasts.
- Attachments larger than 64 MiB are refused from metadata (`model::ATTACHMENT_DOWNLOAD_LIMIT`); auto-download checks use `attachment_too_large()`.
- The effective attachment folder is `AppDirs::media_dir()` (Settings override, else cache). The worker validates picks (outside the cache tree), repoints archive rows by file name, and reports `Event::MediaDirChanged`; logout clears only `media_cache_dir()`, preserving custom folders.
- Text with emoji goes through `widgets::line` / `widgets::rich_text` or `markup::layout`, never a bare `Label`. Bodies paint via `markup::paint_selectable` and single lines via `widgets::selectable_rich_text`; keep `selectable_labels` false elsewhere. Cross-message copy is rebuilt by `src/transcript.rs` in `[time, date] Name:` form; `refine` maps emoji placeholders back through each row's `placements`.
- `src/voice.rs` is OGG/Opus codec plus 64-bar waveform and mono/48kHz resampler; `src/audio.rs` is playback/recording via rodio. `Command::SendVoice` normalizes (gain capped) and sends push-to-talk with waveform and open quote; `Command::MarkPlayed` sends the played receipt once per incoming voice message. Own right-aligned bubbles need explicit `Layout::left_to_right` for rows like the voice player.
- `src/updates/` downloads verified GitHub releases and installs via a helper after explicit restart. Keep asset checksums, package-manager detection, startup acknowledgement, and rollback intact. Portable builds carry `packaging/zapfast-portable.txt`.
- `src/theme/custom.rs` scans local JSON palettes off the UI thread and caches the last usable choice. New icons go in `assets/icons/` as 24px Lucide-style SVGs plus the `icons!` table entry.
- `src/paths.rs` migrates `fastsapp` then `fastwhatsapp` dirs once, after the single-instance guard and outside demos. Keep the guard's `fastsapp:` wire identity for old copies.
- The app outlives the window: `main` loops `eframe::run_native`, close with "keep running" sets `hide_intent` and `App::background_frame` keeps link, archive, and tray alive until tray, notification, or relaunch sets `wants_show`. `src/tray.rs` is Linux (ksni), `src/tray_native.rs` is Windows/macOS. `src/notify.rs` notifies for live `Event::Incoming` from others when away from that chat. macOS has no title bar; keep `macos.rs` menu and traffic-light handling across window recreation.
- Platform code goes behind `cfg` or target modules; every change must keep Linux, macOS, and Windows compiling.
- Experimental tools live in `activity.rs` (presence-only tracker, session-only) and `call_diagnostics.rs` (local-only snapshot, no network I/O, no new deps); both render on `Page::Advanced` (`ui/advanced.rs`). Never invent call, location, or presence data: use the `Unknown`/`Unavailable`/`Relayed` states.
- `eframe` runs with vsync off in `glow_options`: a hidden Wayland window stops frame callbacks and would block ping replies, so repaints stay event-driven.

## egui pitfalls already hit here

- `consume_key(Modifiers::NONE, key)` also matches with Shift held; the composer inspects events itself to separate Enter from Shift+Enter.
- `with_layout(..., Align::Center)` inside a vertical container claims full height; wrap it in `ui.horizontal`.
- `ui.horizontal` inside a right-aligned bubble lays out right to left; see `mirrored_row`. Register the bubble click target from last frame's rect before contents (links and quotes win), and the empty strip beside it earlier still. Double-click on either replies; the body keeps it for word selection.
- `Popup::context_menu` binds to the inner widget's right-click, so the bubble reads right-click over its own rect and opens `Popup::menu` itself.

## Verify

Full checks (same as CI, run before finishing):

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets
cargo test --locked --all-targets --all-features
RUSTDOCFLAGS='-D warnings' cargo doc --locked --all-features --no-deps
```

Focused runs:

```sh
cargo test --locked --all-targets -- <filter>
cargo test --locked --all-targets --all-features -- <filter>
cargo run --features demo -- --demo
cargo run --features demo -- --demo-page login
cargo run --features demo -- --demo-shot shot.png --demo-page chat,light
cargo test --locked --all-targets --all-features  # includes headless layout of every screen in demo
```

Linux CI also needs the GUI deps from Setup. CI additionally compile-checks Windows arm64 (`cargo check --locked --all-targets --all-features --target aarch64-pc-windows-msvc`) and smoke-tests a macOS demo window.

## Releasing

Never use em dashes in user-facing writing. Use commas, colons, parentheses, or full stops.

- Do not release every fix. Accumulate on `main` until a feature or a batch of fixes is worth announcing. Exception: a regression in something just released goes out immediately.
- Match the style of the previous two stable releases in this repo: short summary, `New` and `Fixed` sections with bold user results, `Thanks`, full-changelog link. Credit implementers and reporters with issue or PR numbers. Use only synthetic offline demo content for media, verify every link, describe limits honestly.
- Steps in order: bump `version` in `Cargo.toml` and update `Cargo.lock` with a build, run full checks, commit and push; tag `vX.Y.Z` and push the tag; wait for all platform builds, artifacts, and `checksums.txt`; replace generated notes with written notes; after assets exist, update `zapfast_version` in `docs/_config.yml` and the version menu in `docs/_data/versions.yml` (current version points to `/download/` plus Changelog link, never point at missing files); update AUR from `packaging/arch/` templates and validate with `makepkg -f` (a recipe-only `zapfast-git` change needs no app release). See `PACKAGING.md` for native-package builds.

## Definition of done

- Add focused tests for changed behavior. Extend `demo` sample data and headless layout coverage when adding content or states; use `--demo-shot` to inspect.
- Update README when behavior, settings, files, or network access changes.
- Run the full checks above. Do not weaken a lint, delete a test, or add `allow` without explaining why the rule does not apply.
- Report platform coverage honestly (ran vs only compiled).
