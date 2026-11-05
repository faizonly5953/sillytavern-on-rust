# SillyTavern RS

A clean rebuild of [SillyTavern](https://github.com/SillyTavern/SillyTavern) as a native
desktop and Android app, using Tauri 2 + Rust with a React UI.

There is no background server. Everything the old Express server did (character cards,
chats, prompt building, calling LLM APIs) runs in-process in Rust and stops when the
app closes. The UI talks to Rust through Tauri commands; replies stream over a Tauri channel.

## Features (current)

- Character cards: import PNG (V2 `chara` / V3 `ccv3`) and JSON (V1, Pygmalion, V2, V3),
  create, edit, delete, favorites, avatar upload. Unknown card fields are preserved.
- Chats in SillyTavern's JSONL format: multiple chats per character, rename, delete,
  greetings with alternate greetings as swipes.
- Generation: send, stop, regenerate (new swipe), swipe left/right, continue, impersonate,
  edit/delete/copy messages, reasoning display.
- Providers: any OpenAI-compatible API (OpenAI, OpenRouter, DeepSeek, Groq, Ollama,
  LM Studio, KoboldCpp, llama.cpp), Anthropic Claude, Google Gemini. Streaming via SSE.
- Prompt builder in SillyTavern's default order (main prompt, persona, description,
  personality, scenario, examples, history, post-history instructions) with a context
  budget, macros (`{{char}}`, `{{user}}`, `{{random}}`, `{{roll}}`, dates, ...) and a
  prompt inspector.
- Personas with avatars, dark/light theme, text size, mobile layout.

Not yet ported: World Info / lorebooks, group chats, tokenizers (token counts are
estimates), Text Completion backends, extensions, STscript.

## Data

Same layout as SillyTavern's `data/default-user/`, so existing data can be copied in:

```
characters/*.png        chats/<character>/*.jsonl
User Avatars/*.png      worlds/
settings.json           secrets.json
```

Location: the OS app-data folder (`%APPDATA%\com.sillytavern.rs` on Windows, app-private
storage on Android). On desktop, set `SILLYTAVERN_RS_DATA` to use another folder.
API keys are stored in `secrets.json` in plain text, as SillyTavern does.

## Layout

```
src-tauri/src/
  lib.rs              app setup, command registration
  commands.rs         every UI -> Rust command
  state.rs            in-process state (settings, caches, running generations)
  paths.rs            data folders, safe file names, atomic writes
  png_meta.rs         PNG tEXt chunk read/write
  character.rs        card parsing, import, create/update/delete
  chat.rs             JSONL chats
  settings.rs         settings, personas, secrets
  macros.rs           {{macro}} substitution
  prompt.rs           prompt builder + context budget
  providers/          openai.rs, claude.rs, gemini.rs, sse.rs
  avatar_protocol.rs  avatar:// images + thumbnails
src/
  lib/                types, api (invoke wrappers), store (zustand), utils
  components/         Sidebar, ChatView, MessageItem, Composer, SettingsSheet,
                      CharacterEditor, ChatsPanel, PromptInspector, ui
```

## Develop

Requirements: Rust (stable), Node 20+, pnpm. For Android: Android SDK + NDK and JDK 17/21.

```bash
pnpm install
pnpm tauri dev                 # desktop
cd src-tauri && cargo test     # Rust unit tests
```

### Android

```bash
pnpm tauri android dev                                   # run on a device/emulator
pnpm tauri android build --apk --target aarch64 --debug  # installable debug APK
```

Environment used on the original machine (everything off the system drive):

```
ANDROID_HOME=D:\Android\Sdk
NDK_HOME=D:\Android\Sdk\ndk\25.2.9519653
JAVA_HOME=D:\Software\AndroidStudio\AS\jbr
GRADLE_USER_HOME=D:\GradleCache
RUSTUP_HOME=D:\Software\Rust\rustup
CARGO_HOME=D:\Software\Rust\cargo
```

## Releases and auto-update

No server is needed: GitHub Actions builds everything and GitHub Releases hosts it.

1. Bump the version and tag it (the tag must match `package.json`):
   ```bash
   npm version patch        # or minor / major; creates commit + tag vX.Y.Z
   git push --follow-tags
   ```
2. `.github/workflows/release.yml` creates a draft release, builds the Windows
   installer (signed for the updater, plus `latest.json`) and the signed Android APK,
   then publishes the release.
3. Apps check on start (and from Settings → Appearance → Check for updates):
   - **Windows**: `tauri-plugin-updater` downloads, verifies the signature, installs, restarts.
   - **Android**: the app finds the newer GitHub release and opens its APK; the user taps
     Install. Android only accepts it when it is signed with the same key as the installed app.

Only published releases count. Tags with a suffix (`v1.2.0-beta.1`) become prereleases
and never show the popup.

Required repository secrets (Settings → Secrets and variables → Actions):

| Secret | Value |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | content of the updater private key |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | its password |
| `ANDROID_KEYSTORE_BASE64` | the release keystore, base64-encoded |
| `ANDROID_KEYSTORE_PASSWORD` | keystore/key password |
| `ANDROID_KEY_ALIAS` | key alias |

Keep the private key and keystore backed up outside the repository. Losing the updater key
breaks desktop auto-update; losing the keystore means Android users must uninstall and
reinstall to get new versions.

## License

AGPL-3.0, like SillyTavern.
