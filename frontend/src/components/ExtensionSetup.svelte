<script lang="ts">
  import { app } from '../lib/store.svelte';
  import { api, errorText } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import type { Browser } from '../types';
  import Icon from './Icon.svelte';

  let browsers = $state<Browser[] | null>(null);
  let picked = $state<string | null>(null);
  let folder = $state<string | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let copied = $state(false);
  let baseline = $state(0);

  let browser = $derived(browsers?.find((b) => b.id === picked) ?? null);
  let firefox = $derived(browser?.engine === 'firefox');
  let mod = $derived(app.platform === 'macos' ? '⌘' : 'Ctrl');
  let connected = $derived((app.settings?.trustedExtensions.length ?? 0) > baseline);
  let sep = $derived(app.platform === 'windows' ? '\\' : '/');
  let pastePath = $derived(folder ? (firefox ? `${folder}${sep}manifest.json` : folder) : '');

  $effect(() => {
    if (!app.extensionSetupOpen) return;
    baseline = app.settings?.trustedExtensions.length ?? 0;
    folder = null;
    error = null;
    api
      .browserList()
      .then((list) => {
        browsers = list;
        const wanted = app.extensionSetupBrowser;
        app.extensionSetupBrowser = null;
        picked = (wanted && list.some((b) => b.id === wanted) ? wanted : null) ?? picked ?? list[0]?.id ?? null;
      })
      .catch(() => (browsers = []));
  });

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      copied = false;
    }
  }

  async function start() {
    if (!browser) return;
    busy = true;
    error = null;
    try {
      folder = await api.setupExtension(browser.id);
      await copy(pastePath);
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function showFolder() {
    try {
      folder ??= await api.extensionDir(firefox);
      await api.openPath(folder);
    } catch (e) {
      error = errorText(e);
    }
  }

  async function close() {
    app.extensionSetupOpen = false;
    if (app.settings && !app.settings.extensionSetupDone) {
      await app.saveSettings({ ...app.settings, extensionSetupDone: true });
    }
  }
</script>

{#if app.extensionSetupOpen}
  <div class="fixed inset-0 bg-black/60 backdrop-blur-sm z-[55] flex items-center justify-center p-4" role="presentation">
    <div class="w-full max-w-[560px] max-h-full overflow-y-auto bg-menu border border-line rounded-[14px] shadow-2xl p-5 space-y-4">
      <div class="flex items-start gap-3">
        <div class="w-10 h-10 rounded-full bg-accent-strong/25 text-accent flex items-center justify-center flex-shrink-0">
          <Icon name="puzzle" size={18} />
        </div>
        <div class="flex-1">
          <h2 class="text-[15px] font-semibold">{t('Set up the browser extension')}</h2>
          <p class="text-[12px] text-muted leading-relaxed">
            {t('Sends browser downloads, videos and magnet links to Multron. Multron Download Manager prepares everything; you only confirm two clicks in the browser.')}
          </p>
        </div>
        <button class="icon-btn" title={t('Close')} onclick={close}><Icon name="x" size={14} /></button>
      </div>

      {#if connected}
        <div class="rounded-[10px] border border-ok/40 bg-ok/10 p-4 flex items-center gap-3">
          <Icon name="check" size={18} class="text-ok" />
          <div class="text-[12.5px]">
            <div class="font-medium">{t('The extension is connected')}</div>
            <div class="text-muted">{t('Downloads from your browser now go to Multron Download Manager. Look for the Multron Download Manager button next to videos.')}</div>
          </div>
        </div>
        <div class="flex justify-end"><button class="btn btn-accent" onclick={close}>{t('Done')}</button></div>
      {:else}
        <section class="space-y-2">
          <h3 class="text-[11px] font-semibold text-dim uppercase tracking-wider">1 · {t('Choose your browser')}</h3>
          {#if browsers === null}
            <p class="text-[12px] text-muted">{t('Looking for browsers…')}</p>
          {:else if browsers.length === 0}
            <p class="text-[12px] text-muted">
              {t('No supported browser was found. Open the extension folder and load it from your browser’s extensions page.')}
            </p>
          {:else}
            <div class="grid grid-cols-2 gap-2">
              {#each browsers as b (b.id)}
                <button
                  type="button"
                  onclick={() => {
                    picked = b.id;
                    folder = null;
                  }}
                  class="min-h-[46px] px-3 py-1.5 rounded-[9px] border text-left text-[12.5px] flex items-center gap-2 cursor-pointer transition-colors
                    {picked === b.id ? 'border-accent/70 bg-accent-strong/15 text-white' : 'border-line text-white/90 hover:bg-hover hover:text-white'}"
                >
                  <Icon name="globe" size={15} class={picked === b.id ? 'text-accent' : 'text-muted'} />
                  <span class="flex flex-col min-w-0 flex-1 leading-tight">
                    <span class="font-medium truncate">{b.name}</span>
                    <span class="text-[10.5px] text-dim">{b.engine === 'firefox' ? 'Firefox' : 'Chromium'}</span>
                  </span>
                  <span class="flex items-center gap-1 text-[10px] font-semibold text-ok bg-ok/10 border border-ok/30 rounded-full px-1.5 py-[1px] flex-shrink-0"><Icon name="check" size={9} stroke={3} />{t('Installed')}</span>
                </button>
              {/each}
            </div>
          {/if}
        </section>

        {#if browser}
          <section class="space-y-2">
            <h3 class="text-[11px] font-semibold text-dim uppercase tracking-wider">2 · {t('Open the extensions page')}</h3>
            <button class="btn btn-accent w-full justify-center" disabled={busy} onclick={start}>
              <Icon name="bolt" size={13} />
              {busy ? t('Preparing…') : folder ? t('Open {name} again', { name: browser.name }) : t('Prepare and open {name}', { name: browser.name })}
            </button>
            <p class="text-[11px] text-dim">
              {t('Opens {page} and copies the extension folder’s path for you. The folder comes with Multron Download Manager and is updated with it.', { page: browser.extensionsPage })}
            </p>
          </section>

          {#if folder}
            <section class="space-y-2">
              <h3 class="text-[11px] font-semibold text-dim uppercase tracking-wider">3 · {t('In {name}', { name: browser.name })}</h3>
              <ol class="text-[12.5px] list-decimal pl-5 space-y-1.5 leading-relaxed">
                {#if firefox}
                  <li>{t('Click “Load Temporary Add-on…”.')}</li>
                  <li>{t('Press {mod}+V in the file name box and Enter. This picks manifest.json from the copied path.', { mod })}</li>
                {:else}
                  <li>{t('Turn on “Developer mode” (top right; in Edge it is in the left menu).')}</li>
                  <li>
                    {app.platform === 'macos'
                      ? t('Click “Load unpacked”, press ⌘+Shift+G, paste with ⌘+V and choose “Select”.')
                      : t('Click “Load unpacked”, press {mod}+V in the folder box and choose “Select Folder”.', { mod })}
                    <span class="text-muted">{t('Or drag the folder onto the extensions page.')}</span>
                  </li>
                {/if}
                <li>{t('Press “Allow” when Multron Download Manager asks. The extension connects by itself.')}</li>
              </ol>

              <div class="flex items-center gap-2">
                <input class="field mono flex-1 !text-[11px]" readonly value={pastePath} onfocus={(e) => e.currentTarget.select()} />
                <button class="btn" onclick={() => copy(pastePath)}>
                  <Icon name={copied ? 'check' : 'copy'} size={13} />{copied ? t('Copied') : t('Copy')}
                </button>
                <button class="btn" onclick={showFolder}><Icon name="folderOpen" size={13} />{t('Show folder')}</button>
              </div>

              {#if firefox}
                <p class="text-[11px] text-dim leading-relaxed">
                  {t('Firefox removes temporary add-ons when it closes. Run this assistant again after restarting Firefox.')}
                </p>
              {/if}

              {#if browser.flatpak && !browser.flatpakAutostart && browser.flatpakGrant}
                <div class="rounded-[9px] border border-warn/40 bg-warn/10 p-2.5 space-y-2 text-[11.5px] leading-relaxed">
                  <p>{t('{name} is a Flatpak app. Downloads work right away. To let the extension also start Multron Download Manager when it is closed, run this command once and restart the browser:', { name: browser.name })}</p>
                  <div class="flex items-center gap-2">
                    <input class="field mono flex-1 !text-[11px]" readonly value={browser.flatpakGrant} onfocus={(e) => e.currentTarget.select()} />
                    <button class="btn" onclick={() => copy(browser.flatpakGrant ?? '')}><Icon name="copy" size={13} />{t('Copy')}</button>
                  </div>
                </div>
              {/if}

              <div class="flex items-center gap-2 text-[12px] text-muted pt-1">
                <span class="w-[8px] h-[8px] rounded-full bg-warn animate-pulse"></span>
                {t('Waiting for the extension to connect…')}
              </div>
            </section>
          {/if}
        {/if}

        {#if error}
          <p class="text-[12px] text-bad break-all">{error}</p>
        {/if}

        <div class="flex justify-between gap-2 pt-1">
          <button class="btn" onclick={showFolder}><Icon name="folderOpen" size={13} />{t('Open extension folder')}</button>
          <button class="btn" onclick={close}>{t('Later')}</button>
        </div>
      {/if}
    </div>
  </div>
{/if}
