# The HeroWorld website and launcher

How the website in `site/` is hosted, and how the launcher gets its files. Written for
whoever publishes the site (Arnd's publishing agent), who knows TypeScript apps but not
this repo.

## The website in one paragraph

`site/` is a finished static website: `index.html`, `style.css`, `app.js`, `fonts/`,
`media/`. There is **no build step**, no `package.json`, no framework and no server code.
Publishing means copying the folder's contents as they are. It goes to
**https://apps.futuremagic.de/HeroWorld/**. Every path in it is relative, so it works under
that sub-path (or any other) without changes.

## Publishing

1. Get the repo: `git clone https://github.com/ArndRosemeier/HeroWorld` (public), or pull.
2. Copy everything inside `site/` to the web root's `HeroWorld/` folder, keeping the
   folder structure:

   ```
   HeroWorld/index.html
   HeroWorld/style.css
   HeroWorld/app.js
   HeroWorld/fonts/...   (TTF, WOFF2 and their licence texts)
   HeroWorld/media/...   (WEBP screenshots, PNG icon, OGG music)
   ```

3. Make `/HeroWorld` (no slash) redirect to `/HeroWorld/`. Without the slash the
   relative paths resolve one level too high and the page loads without styles.
4. Put the host's legal links into the footer: `index.html` has the comment
   `<!-- LEGAL-LINKS ... -->` where the **Impressum** and **Datenschutz** links of
   futuremagic.de go (a German site needs both). Replace the comment with them, e.g.
   `<p><a href="/impressum">Impressum</a> · <a href="/datenschutz">Datenschutz</a></p>`.
5. Open the page and check: the logo letters swoop in, the "No. 0.xxx" box shows the
   current version (read live from GitHub), the noon/night slider drags, "What's new"
   lists the latest versions, and the Download button starts a download of
   `HeroWorldLauncher.exe`.

Nothing else is needed. To update later, copy `site/` again; `git log -- site/` shows
whether it changed since the last publish.

### Server settings

- MIME types (most servers know them; add them if not): `.webp` `image/webp`,
  `.ogg` `audio/ogg`, `.woff2` `font/woff2`, `.ttf` `font/ttf`.
- Caching: `index.html`, `app.js` and `style.css` short (or revalidate), `media/` and
  `fonts/` may be cached for a week. Files keep their names when they change, so do not
  cache them "immutable".
- Gzip or Brotli for `.html`, `.css`, `.js`, `.ttf` helps; the images and music are
  already compressed.
- If the site sends a Content-Security-Policy, the page needs:
  `default-src 'self'; connect-src 'self' https://api.github.com https://raw.githubusercontent.com; img-src 'self' data:; media-src 'self'; font-src 'self'; style-src 'self'; script-src 'self'`.
  It has no inline scripts or styles.

### What the page loads from elsewhere (for the privacy page)

- **No cookies, no analytics, no trackers, no Google Fonts.** All fonts are in `fonts/`.
- The visitor's browser asks GitHub for two things: the newest release
  (`api.github.com/repos/ArndRosemeier/HeroWorld/releases/tags/windows-latest`, for the
  version number and download size) and the change list
  (`raw.githubusercontent.com/ArndRosemeier/HeroWorld/main/WHATS_NEW.md`). GitHub sees
  the visitor's IP address for these. If either fails, the page keeps its built-in text.
- Downloads come from GitHub releases (`github.com/ArndRosemeier/HeroWorld/releases/...`).
- The music plays only when the visitor presses "Theme".

## What the page points at

| Button | URL |
| --- | --- |
| Download for Windows | `https://github.com/ArndRosemeier/HeroWorld/releases/download/windows-latest/HeroWorldLauncher.exe` |
| Plain zip | `https://github.com/ArndRosemeier/HeroWorld/releases/tag/windows-latest` |

If the release has no launcher yet (it is being rebuilt), `app.js` switches the button to
the release page.

## The launcher and the nightly release

The Windows build (`.github/workflows/windows-build.yml`, nightly, or by hand under
Actions) replaces the GitHub release `windows-latest` with:

- `HeroWorld-windows.zip`: the whole game, for people who skip the launcher;
- `HeroWorldLauncher.exe`: the launcher (crate `crates/hw-launcher`);
- `manifest.json` and the bundles `game.zip`, `packs.zip`, `robots.zip`, `music.zip`,
  written by `scripts/release-bundles.py`.

The launcher reads `manifest.json`, compares each bundle's content hash with what it
installed last (`<install folder>/.launcher/installed.json`) and downloads only the
bundles that changed, so most nightly updates are just `game.zip`. It checks every
download and every unpacked file against its SHA-256. It installs to
`%LOCALAPPDATA%\Programs\HeroWorld` by default and keeps its settings in
`%APPDATA%\HeroWorld\launcher.json`, beside the game's saves, which it never touches.

The launcher has its own version (`crates/hw-launcher/Cargo.toml`). Raise it when the
launcher changes: a launcher older than the one in `manifest.json` offers to update
itself. `HW_LAUNCHER_RELEASE=<url ending in />` points a launcher at another release
folder, for testing (`python -m http.server` in a folder made by `release-bundles.py`).

The launcher is not code-signed yet, so Windows SmartScreen warns on first start ("More
info", then "Run anyway"); the website's FAQ says so. Signing it (e.g. Azure Trusted
Signing) removes that.

## Screenshots

The site's pictures are in `site/media/` (WEBP, 1600 px wide). For clean shots without
the HUD, run the game with `HW_HUD=0`, e.g.
`HW_HUD=0 HW_VIEW=colossus ENGINE_SCREENSHOT=shot.png ENGINE_SCREENSHOT_FRAME=200 heroworld`.
