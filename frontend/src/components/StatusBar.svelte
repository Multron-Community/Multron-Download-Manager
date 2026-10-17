<script lang="ts">
  import { t } from '../lib/i18n.svelte';
  import { onMount } from 'svelte';
  import { app } from '../lib/store.svelte';
  import { api } from '../lib/api';
  import { speed } from '../lib/format';
  import Icon from './Icon.svelte';
  import type { BridgeStatus } from '../types';

  let bridge = $state<BridgeStatus | null>(null);

  onMount(() => {
    const load = async () => {
      try {
        bridge = await api.bridgeStatus();
      } catch {
        bridge = null;
      }
    };
    load();
    const id = setInterval(load, 5000);
    return () => clearInterval(id);
  });

  let torrent = $derived(app.stats.torrent);
</script>

<footer class="h-[26px] px-3 border-t border-line bg-panel flex items-center justify-between text-[11px] text-muted flex-shrink-0 gap-4">
  <div class="flex items-center gap-4 min-w-0">
    <span class="flex items-center gap-1" title={t('Browser extension bridge')}>
      <span class="w-[7px] h-[7px] rounded-full {bridge?.running ? 'bg-ok' : 'bg-bad'}"></span>
      {bridge?.running ? t('Browser bridge · {address}', { address: `127.0.0.1:${bridge.port}` }) : bridge?.error ?? t('Browser bridge off')}
    </span>
    {#if app.toolProgress}
      <span class="text-accent truncate">
        {app.toolProgress.tool.startsWith('Multron Download Manager') ? t('Downloading') : t('Installing')} {app.toolProgress.tool}… {app.toolProgress.total ? Math.round((app.toolProgress.downloaded / app.toolProgress.total) * 100) + '%' : ''}
      </span>
    {/if}
    {#if app.updateReady}
      <button class="text-ok hover:underline cursor-pointer" onclick={() => app.installUpdate()}>
        {t('Restart to update to {version}', { version: app.updateReady.version })}
      </button>
    {:else if app.appUpdate?.hasUpdate && !app.toolProgress}
      <button class="text-warn hover:underline cursor-pointer" onclick={() => app.downloadUpdate()}>
        {t('Update {version} available', { version: app.appUpdate.latest ?? '' })}
      </button>
    {/if}
  </div>
  <div class="flex items-center gap-4 flex-shrink-0">
    {#if torrent}
      <span class="flex items-center gap-1" title={t('Mainline DHT')}>
        <Icon name="globe" size={12} />
        {torrent.dhtEnabled ? t('DHT {count} nodes', { count: torrent.dhtNodes ?? 0 }) : t('DHT off')}
      </span>
      <span class="flex items-center gap-1" title={t('Connected peers')}><Icon name="users" size={12} />{torrent.peers}</span>
      {#if torrent.listenPort}
        <span title={t('Incoming TCP/UDP port')}>Port {torrent.listenPort}</span>
      {/if}
    {/if}
    <label class="flex items-center gap-1 cursor-pointer {app.settings?.whenDone && app.settings.whenDone !== 'nothing' ? 'text-warn' : ''}" title={t('When all downloads have finished')}>
      <Icon name="power" size={11} />
      <select class="bg-transparent outline-none cursor-pointer text-[11px]" value={app.settings?.whenDone ?? 'nothing'} onchange={(e) => app.setWhenDone(e.currentTarget.value as 'nothing' | 'sleep' | 'shutdown' | 'quit')}>
        <option value="nothing">{t('Do nothing')}</option>
        <option value="sleep">{t('Sleep')}</option>
        <option value="shutdown">{t('Shut down')}</option>
        <option value="quit">{t('Quit Multron Download Manager')}</option>
      </select>
    </label>
    <button
      class="flex items-center gap-1 cursor-pointer hover:text-white {app.stats.altSpeed ? 'text-warn' : ''}"
      title={app.stats.altSpeed ? t('Alternative speed limits are on (click to turn off)') : t('Turn on alternative speed limits')}
      onclick={() => app.toggleAltSpeed()}
    >
      <Icon name="turtle" size={13} />
    </button>
    <span class="flex items-center gap-1 text-accent"><Icon name="down" size={11} />{speed(app.stats.downloadSpeed)}</span>
    <span class="flex items-center gap-1 text-seed"><Icon name="up" size={11} />{speed(app.stats.uploadSpeed)}</span>
    <span class="text-dim">v{app.version}</span>
  </div>
</footer>
