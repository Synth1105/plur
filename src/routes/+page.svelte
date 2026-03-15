<script>
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { openPath } from "@tauri-apps/plugin-opener";

  let user = $state("");
  let repo = $state("");
  let mods = $state([]);
  let logs = $state([]);
  let resultPath = $state("");
  let resultJsPath = $state("");
  let resultChromePath = $state("");
  let themePath = $state("");
  let buildCount = $state(0);
  let jsCount = $state(0);
  let chromeCount = $state(0);
  let busy = $state(false);
  let addBusy = $state(false);
  let removeBusy = $state(false);
  let error = $state("");

  const normalizeError = (err) =>
    typeof err === "string" ? err : err?.message ?? "Unknown error";

  const refreshMods = async () => {
    try {
      mods = await invoke("list_mods");
    } catch (err) {
      error = normalizeError(err);
    }
  };

  const handleAdd = async (event) => {
    event.preventDefault();
    error = "";
    addBusy = true;
    try {
      mods = await invoke("add_mod", { user, repo });
      user = "";
      repo = "";
    } catch (err) {
      error = normalizeError(err);
    } finally {
      addBusy = false;
    }
  };

  const handleBuild = async () => {
    error = "";
    busy = true;
    logs = [];
    resultPath = "";
    resultJsPath = "";
    resultChromePath = "";
    themePath = "";
    buildCount = 0;
    jsCount = 0;
    chromeCount = 0;
    try {
      const report = await invoke("sync_and_build");
      logs = report.logs ?? [];
      resultPath = report.result_path ?? "";
      resultJsPath = report.result_js_path ?? "";
      resultChromePath = report.result_chrome_path ?? "";
      themePath = report.theme_path ?? "";
      buildCount = report.count ?? 0;
      jsCount = report.js_count ?? 0;
      chromeCount = report.chrome_count ?? 0;
      await refreshMods();
    } catch (err) {
      error = normalizeError(err);
    } finally {
      busy = false;
    }
  };

  const handleRemove = async (mod) => {
    error = "";
    removeBusy = true;
    try {
      mods = await invoke("remove_mod", { name: mod.name, repo: mod.repo });
    } catch (err) {
      error = normalizeError(err);
    } finally {
      removeBusy = false;
    }
  };

  const handleOpen = async () => {
    if (!resultPath) return;
    try {
      await openPath(resultPath);
    } catch (err) {
      error = normalizeError(err);
    }
  };

  onMount(() => {
    refreshMods();
  });
</script>

<svelte:head>
  <link rel="preconnect" href="https://fonts.googleapis.com" />
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin />
  <link
    href="https://fonts.googleapis.com/css2?family=DM+Sans:wght@400;500;700&family=Space+Grotesk:wght@500;700&display=swap"
    rel="stylesheet"
  />
</svelte:head>

<main class="page">
  <div class="page__bg"></div>
  <section class="hero">
    <div class="hero__content reveal" style="--delay: 80ms">
      <span class="hero__eyebrow">Plur Mod Manager</span>
      <h1>Make every browser feel personal.</h1>
      <p>
        Add mods from GitHub and build a single `result.css` without leaving the
        desktop app.
      </p>
    </div>
    <div class="hero__stats reveal" style="--delay: 160ms">
      <div>
        <strong>{mods.length}</strong>
        <span>Mods tracked</span>
      </div>
      <div>
        <strong>{buildCount}</strong>
        <span>Styles merged</span>
      </div>
      <div>
        <strong>{jsCount}</strong>
        <span>Scripts merged</span>
      </div>
      <div>
        <strong>{chromeCount}</strong>
        <span>UI styles merged</span>
      </div>
    </div>
  </section>

  <section class="grid">
    <div class="card reveal" style="--delay: 220ms">
      <div class="card__header">
        <h2>Add a mod</h2>
        <p>Connect a GitHub repo and keep it ready to build.</p>
      </div>
      <form class="form" onsubmit={handleAdd}>
        <label>
          GitHub user
          <input
            placeholder="synth1105"
            bind:value={user}
            required
            autocomplete="off"
          />
        </label>
        <label>
          Repository
          <input
            placeholder="my-mod"
            bind:value={repo}
            required
            autocomplete="off"
          />
        </label>
        <button class="primary" type="submit" disabled={addBusy}>
          {addBusy ? "Adding..." : "Add mod"}
        </button>
      </form>
    </div>

    <div class="card reveal" style="--delay: 300ms">
      <div class="card__header">
        <h2>Sync and build</h2>
        <p>Clone new mods, pull dependencies, and emit `result.css`.</p>
      </div>
      <div class="actions">
        <button class="primary" type="button" onclick={handleBuild} disabled={busy}>
          {busy ? "Building..." : "Sync & build"}
        </button>
        <button class="ghost" type="button" onclick={refreshMods} disabled={busy}>
          Refresh list
        </button>
      </div>
      <div class="result">
        <div>
          <span>Output CSS</span>
          <strong>{resultPath || "~/.plur/result.css"}</strong>
        </div>
        <div>
          <span>Output JS</span>
          <strong>{resultJsPath || "~/.plur/result.js"}</strong>
        </div>
        <div>
          <span>Output UI CSS</span>
          <strong>{resultChromePath || "~/.plur/result.chrome.css"}</strong>
        </div>
        <div>
          <span>Theme</span>
          <strong>{themePath || "~/.plur/theme.json"}</strong>
        </div>
        <button class="ghost" type="button" onclick={handleOpen} disabled={!resultPath}>
          Open result
        </button>
      </div>
      {#if error}
        <div class="error">{error}</div>
      {/if}
    </div>

    <div class="card list reveal" style="--delay: 380ms">
      <div class="card__header">
        <h2>Tracked mods</h2>
        <p>Stored in `~/.plur/mods`.</p>
      </div>
      <div class="list__body">
        {#if mods.length === 0}
          <p class="empty">No mods yet. Add one to get started.</p>
        {:else}
          {#each mods as mod}
            <div class="list__item">
              <div>
                <strong>{mod.name}</strong>
                <span>{mod.repo}</span>
              </div>
              <div class="list__actions">
                <span class="badge" class:badge--ready={mod.installed}>
                  {mod.installed ? "Ready" : "Not synced"}
                </span>
                <button
                  class="ghost danger"
                  type="button"
                  onclick={() => handleRemove(mod)}
                  disabled={removeBusy}
                >
                  {removeBusy ? "Removing..." : "Remove"}
                </button>
              </div>
            </div>
          {/each}
        {/if}
      </div>
    </div>

    <div class="card log reveal" style="--delay: 460ms">
      <div class="card__header">
        <h2>Build log</h2>
        <p>Latest sync and build output.</p>
      </div>
      <div class="log__body">
        {#if logs.length === 0}
          <p class="empty">Run a build to see logs.</p>
        {:else}
          <pre>{logs.join("\n")}</pre>
        {/if}
      </div>
    </div>
  </section>
</main>

<style>
  :global(body) {
    margin: 0;
    font-family: "DM Sans", "SF Pro Text", sans-serif;
    color: #101417;
    background: #f4f2ec;
  }

  :global(*) {
    box-sizing: border-box;
  }

  .page {
    position: relative;
    min-height: 100vh;
    padding: 64px clamp(20px, 4vw, 64px) 80px;
    overflow: hidden;
  }

  .page__bg {
    position: absolute;
    inset: 0;
    background:
      radial-gradient(circle at 20% 10%, rgba(255, 214, 165, 0.6), transparent 55%),
      radial-gradient(circle at 80% 20%, rgba(187, 217, 255, 0.8), transparent 50%),
      linear-gradient(120deg, #f4f2ec 0%, #f9f7f2 40%, #f0f6ff 100%);
    z-index: -2;
  }

  .page__bg::after {
    content: "";
    position: absolute;
    inset: 0;
    background-image: linear-gradient(rgba(16, 20, 23, 0.05) 1px, transparent 1px),
      linear-gradient(90deg, rgba(16, 20, 23, 0.05) 1px, transparent 1px);
    background-size: 36px 36px;
    opacity: 0.35;
  }

  .hero {
    display: grid;
    grid-template-columns: minmax(0, 1.1fr) minmax(0, 0.7fr);
    gap: 32px;
    margin-bottom: 48px;
    align-items: end;
  }

  .hero__content h1 {
    font-family: "Space Grotesk", "SF Pro Display", sans-serif;
    font-size: clamp(2.4rem, 3.5vw, 3.8rem);
    margin: 12px 0;
    letter-spacing: -0.02em;
  }

  .hero__content p {
    font-size: 1.05rem;
    max-width: 36ch;
    color: #3d464f;
  }

  .hero__eyebrow {
    text-transform: uppercase;
    letter-spacing: 0.24em;
    font-size: 0.72rem;
    font-weight: 700;
    color: #6a4a3a;
  }

  .hero__stats {
    display: grid;
    gap: 20px;
    background: rgba(255, 255, 255, 0.7);
    border: 1px solid rgba(16, 20, 23, 0.08);
    border-radius: 18px;
    padding: 20px 24px;
    box-shadow: 0 18px 40px rgba(24, 33, 43, 0.08);
  }

  .hero__stats strong {
    font-size: 1.6rem;
    font-family: "Space Grotesk", "SF Pro Display", sans-serif;
  }

  .hero__stats span {
    display: block;
    color: #4b5560;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 24px;
  }

  .card {
    background: rgba(255, 255, 255, 0.78);
    border-radius: 20px;
    border: 1px solid rgba(16, 20, 23, 0.08);
    padding: 24px;
    box-shadow: 0 16px 40px rgba(24, 33, 43, 0.08);
    backdrop-filter: blur(12px);
  }

  .card__header h2 {
    font-family: "Space Grotesk", "SF Pro Display", sans-serif;
    margin: 0 0 6px;
  }

  .card__header p {
    margin: 0 0 20px;
    color: #4b5560;
  }

  .form {
    display: grid;
    gap: 16px;
  }

  label {
    display: grid;
    gap: 8px;
    font-weight: 600;
    font-size: 0.9rem;
    color: #2a3036;
  }

  input {
    padding: 12px 14px;
    border-radius: 14px;
    border: 1px solid rgba(16, 20, 23, 0.15);
    font-size: 0.95rem;
    font-family: inherit;
    background: #fffaf0;
  }

  input:focus {
    outline: 2px solid rgba(231, 151, 80, 0.6);
    border-color: transparent;
  }

  button {
    border: none;
    border-radius: 999px;
    font-size: 0.95rem;
    font-weight: 600;
    padding: 12px 20px;
    cursor: pointer;
  }

  button:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }

  .primary {
    background: #151b21;
    color: #f8f5ef;
    box-shadow: 0 14px 24px rgba(21, 27, 33, 0.25);
  }

  .ghost {
    background: transparent;
    border: 1px solid rgba(16, 20, 23, 0.2);
    color: #1f2933;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }

  .result {
    margin-top: 18px;
    padding: 14px 16px;
    border-radius: 16px;
    background: rgba(250, 248, 242, 0.9);
    border: 1px solid rgba(16, 20, 23, 0.08);
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }

  .result span {
    display: block;
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.18em;
    color: #6b7280;
  }

  .result strong {
    display: block;
    font-size: 0.9rem;
    color: #1f2933;
  }

  .error {
    margin-top: 16px;
    padding: 12px 14px;
    border-radius: 12px;
    background: #ffe7d6;
    color: #9c3b00;
    font-weight: 600;
  }

  .list__body {
    display: grid;
    gap: 12px;
  }

  .list__item {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 14px;
    border-radius: 14px;
    background: rgba(255, 252, 246, 0.9);
    border: 1px solid rgba(16, 20, 23, 0.08);
  }

  .list__actions {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .list__item strong {
    display: block;
    font-size: 0.95rem;
  }

  .list__item span {
    font-size: 0.82rem;
    color: #6b7280;
  }

  .badge {
    align-self: center;
    padding: 6px 12px;
    border-radius: 999px;
    background: #f4d9c6;
    color: #7a3b07;
    font-weight: 700;
    font-size: 0.75rem;
  }

  .badge--ready {
    background: #d6f2e3;
    color: #1d6a45;
  }

  .danger {
    background: rgba(255, 235, 235, 0.7);
    border: 1px solid rgba(185, 68, 68, 0.35);
    color: #a01b1b;
  }

  .log__body {
    min-height: 180px;
    max-height: 280px;
    overflow: auto;
    background: #0f1419;
    color: #e6edf5;
    border-radius: 16px;
    padding: 16px;
    font-family: "SF Mono", "JetBrains Mono", monospace;
    font-size: 0.85rem;
  }

  pre {
    margin: 0;
    white-space: pre-wrap;
  }

  .empty {
    color: #6b7280;
    margin: 0;
  }

  .reveal {
    opacity: 0;
    transform: translateY(12px);
    animation: rise 700ms ease forwards;
    animation-delay: var(--delay);
  }

  @keyframes rise {
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  @media (max-width: 900px) {
    .hero {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 700px) {
    .page {
      padding: 48px 20px 64px;
    }

    .result {
      flex-direction: column;
      align-items: flex-start;
    }
  }
</style>
