# Contributing to Lipwise

Thanks for helping out! Bug reports, model results on your hardware, and pull requests are all welcome. For anything bigger than a small fix, please open an issue first so we can agree on the approach.

## Build from source

Requirements: Rust (stable), Node 20+, **CMake ≥ 3.21** (builds transcribe.cpp), and a C++ toolchain (Visual Studio 2022 Build Tools on Windows, Xcode Command Line Tools on macOS, `build-essential` on Linux).

Linux also needs: `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libasound2-dev libxdo-dev`.

```sh
npm install
npm run tauri dev      # run with hot reload
npm run tauri build    # installers in src-tauri/target/release/bundle
```

`tauri build` also signs the update bundles, so it needs the updater key in the environment: `TAURI_SIGNING_PRIVATE_KEY` (the key file's contents) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

GPU acceleration for transcription: `npm run tauri build -- --features vulkan` (needs the Vulkan SDK) or `--features cuda`.

Dev builds load the UI from the Vite dev server, so they never register themselves as a login item and never check for updates.

## Tests

```sh
cd src-tauri
cargo test --lib
# Optional end-to-end checks against real models and providers:
LIPWISE_TEST_MODEL=path/to/model.gguf LIPWISE_TEST_WAV=speech.wav cargo test --lib transcribes_real_audio -- --ignored --nocapture
LIPWISE_TEST_PROVIDER=anthropic ANTHROPIC_API_KEY=... cargo test --lib live_dictation_examples -- --ignored --nocapture
```

CI (`.github/workflows/build.yml`) runs the frontend build and `cargo test` on Windows, macOS and Linux for every push and pull request.

## Project layout

```
src-tauri/src/
  pipeline.rs    shortcut → record → transcribe → AI → paste state machine
  llm.rs         prompts + Claude / OpenAI-compatible clients
  local_ai.rs    hardware detection, llama.cpp + model install, managed llama-server
  download.rs    resumable, SHA-256-verified downloads
  transcribe.rs  transcribe.cpp wrapper
  models.rs      model catalog, resumable verified downloads
  audio.rs       mic capture and resampling
  input.rs       paste and read selection via clipboard + synthetic keys
  overlay.rs     floating status pill
  permissions.rs microphone and typing (Accessibility) access per platform
  updater.rs     background update checks against GitHub Releases
  settings.rs, history.rs, lib.rs (commands, tray, startup)
src/             React UI (pages/, overlay.tsx)
docs/assets/     README screenshots and demo
```

## How the local AI setup works

1. **Detects the hardware**: RAM, CPU threads, and each GPU's dedicated memory (the Windows display-adapter registry, `nvidia-smi` and amdgpu sysfs on Linux, unified memory on Apple Silicon), plus free disk space.
2. **Picks a backend and model.** Metal on Apple Silicon, Vulkan on any GPU with 2 GB+ of its own memory (NVIDIA, AMD, Intel), otherwise CPU. It recommends the largest model that fits comfortably in GPU or unified memory, and the smaller one on CPU-only machines, stepping down if the disk is short on space.
3. **Gets llama.cpp**, reusing a recent `llama-server` already on your PATH (or Homebrew's). Otherwise it downloads the newest official build for your OS and backend from GitHub and checks the release's SHA-256 digest.
4. **Downloads the model** from Hugging Face at a pinned revision and verifies its SHA-256. Downloads resume if interrupted.
5. **Starts `llama-server`** on a random localhost port, with no console window, thinking turned off (`--reasoning off`), and GPU layers fitted automatically. It waits for `/health` and then switches Lipwise to it.

The server starts with Lipwise (or on first use) and stops when you quit or switch providers. A server left behind by a crash is cleaned up at the next start. Everything lives in the app's data folder, and **Remove Local AI** deletes it all. Server output goes to `local-ai.log` in that folder.

## Releasing

1. Bump `version` in `src-tauri/tauri.conf.json` (and `Cargo.toml` / `package.json` to match).
2. Tag and push: `git tag v0.2.0 && git push origin v0.2.0`.
3. The `release` workflow builds Windows (NSIS), macOS (Apple Silicon, signed and notarized) and Linux (AppImage, deb, rpm) and attaches them to a **draft** release.
4. Publish the draft. Installed copies with "Update Automatically" on download it within a few hours and install it on their next restart.

Repository secrets the workflow needs:

| Secret | What it is |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | Contents of the updater private key (`npx tauri signer generate`). Its public half is `plugins.updater.pubkey` in `tauri.conf.json`. Losing it means installed copies can't update. |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | That key's password |
| `APPLE_CERTIFICATE` | "Developer ID Application" certificate exported as `.p12`, base64-encoded |
| `APPLE_CERTIFICATE_PASSWORD` | The `.p12` export password |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | Apple ID email used for notarization |
| `APPLE_PASSWORD` | An app-specific password for that Apple ID (appleid.apple.com) |
| `APPLE_TEAM_ID` | Your 10-character Team ID |
