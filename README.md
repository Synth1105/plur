# plur-cli / plur GUI

Mod manager for browser customization that builds a unified `result.css` and `result.js`, then installs them into supported browsers.

**What’s new**
- JS mods supported via `plur.js`
- Auto-install pipeline for Firefox + Chromium-based browsers (Chrome, Chromium, Edge, Brave, Vivaldi)
- Legacy compatibility: best-effort import and output copy for CosmoCreeper/Sine

## CLI Usage

### Commands
```bash
# add a mod by GitHub user/repo
plur add synth1105 my-mod

# build (auto-installs after build)
plur build

# install only (no build)
plur install

# remove
plur remove synth1105 my-mod
```

### Mod structure
Each mod repo can include:
- `plur.css` (merged into `~/.plur/result.css`)
- `plur.js` (merged into `~/.plur/result.js`)
- `plur.chrome.css` (Firefox UI styles merged into `~/.plur/result.chrome.css`)
- `plur.theme.json` (Chromium theme colors merged into `~/.plur/theme.json`)

### Outputs
- `~/.plur/result.css`
- `~/.plur/result.js`
- `~/.plur/result.chrome.css`
- `~/.plur/theme.json`

## GUI Usage

- Add mods using the GUI form.
- Click “Sync & build” to clone, build, and install.
- Output paths are shown in the UI.

## Browser Install Behavior

### Firefox
- CSS is copied to each profile’s `chrome/plur.userContent.css` and imported from `userContent.css`.
- JS is copied to `chrome/plur.userContent.js`.
- UI CSS is copied to `chrome/plur.userChrome.css` and imported from `userChrome.css`.
- A Firefox extension package is generated at `~/.plur/extension-firefox/plur-firefox.xpi`.
- If Firefox policy directories are writable, a `policies.json` is written to force-install the extension.
- The extension is also copied into each profile’s `extensions/` folder as `plur@local.xpi` for auto-update on restart.
 - UI changes still require a browser restart to take full effect.

### Chromium-based (Chrome/Chromium/Edge/Brave/Vivaldi)
- An unpacked extension is generated at `~/.plur/extension-chromium`.
- `theme.json` is applied to the same extension to style browser UI where supported.
- The extension popup includes a small UI (status + reload tab button).
- One-time action: load the unpacked extension in your browser’s extensions page.
- After that, updates are automatic because the extension files are updated in place.

## Examples

### Example 1: Add + Build
```bash
plur add synth1105 sleek-tabs
plur build
```

### Example 2: Build a JS mod
Add a `plur.js` in your mod repo and run:
```bash
plur build
```

### Example 3: Firefox UI styling
Add a `plur.chrome.css` to your mod repo:
```css
/* example */
#nav-bar {
  background: #1f1f24 !important;
}
```

### Example 4: Chromium theme
Add a `plur.theme.json`:
```json
{
  "colors": {
    "frame": [20, 22, 26],
    "toolbar": [30, 32, 38],
    "tab_background_text": [240, 240, 245]
  }
}
```

### Example 3: Manual install only
```bash
plur install
```

## Legacy Compatibility (CosmoCreeper/Sine)

Best-effort compatibility is included:
- Legacy manager config files are scanned (JSON with repo URLs or `user/repo` entries).
- If detected, mods are imported into `~/.plur/config.toml`.
- Any legacy output paths found in configs are updated with `result.css` / `result.js`.

This is heuristic-based by design. If you want guaranteed import for a specific layout, provide the config path and format.
