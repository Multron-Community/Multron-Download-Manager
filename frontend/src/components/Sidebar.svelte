<script lang="ts">
  import type { CategoryFilter, FolderFilter } from '../types';

  interface Props {
    activeCategory: CategoryFilter;
    activeFolder: FolderFilter;
    counts: { all: number; active: number; completed: number; paused: number };
    onSelectCategory: (cat: CategoryFilter) => void;
    onSelectFolder: (folder: FolderFilter) => void;
  }

  let { activeCategory, activeFolder, counts, onSelectCategory, onSelectFolder }: Props = $props();

  const isCatActive = (cat: CategoryFilter) => activeCategory === cat && activeFolder === 'all';
</script>

<aside class="w-[220px] bg-[#1e1e1f] border-r border-[#2c2c2e] flex flex-col justify-between p-[20px_16px] select-none flex-shrink-0 h-full">
  <div class="space-y-6">
    <nav class="space-y-[4px]">
      {#each [
        { id: 'all', label: 'Tüm İndirmeler', count: counts.all, icon: 'M4 6h16M4 12h16M4 18h16', isSvgFill: false },
        { id: 'active', label: 'Devam Edenler', count: counts.active, icon: 'M19 14l-7 7m0 0l-7-7m7 7V3', isSvgFill: false },
        { id: 'completed', label: 'Tamamlananlar', count: counts.completed, icon: 'M5 13l4 4L19 7', isSvgFill: false },
        { id: 'paused', label: 'Duraklatılanlar', count: counts.paused, icon: 'pause', isSvgFill: true }
      ] as item}
        <button
          type="button"
          onclick={() => { onSelectCategory(item.id as CategoryFilter); onSelectFolder('all'); }}
          class="w-full h-[37px] px-[12px] py-[10px] rounded-[10px] flex items-center justify-between text-[13px] font-medium transition-colors cursor-pointer {isCatActive(item.id as CategoryFilter) ? 'bg-[#2c2c2e] text-white' : 'text-[#8e8e93] hover:bg-[#252528] hover:text-white'}"
        >
          <div class="flex items-center gap-[10px]">
            {#if item.icon === 'pause'}
              <svg class="w-4 h-4 fill-current stroke-none" viewBox="0 0 24 24">
                <rect x="6" y="4" width="4" height="16" rx="1" />
                <rect x="14" y="4" width="4" height="16" rx="1" />
              </svg>
            {:else}
              <svg class="w-4 h-4 stroke-current" fill="none" viewBox="0 0 24 24" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d={item.icon} />
              </svg>
            {/if}
            <span>{item.label}</span>
          </div>
          <span class="text-[11px] font-medium {isCatActive(item.id as CategoryFilter) ? 'text-white' : 'text-[#555558]'}">{item.count}</span>
        </button>
      {/each}
    </nav>

    <div class="space-y-[6px]">
      <h3 class="text-[11px] font-semibold text-[#555558] px-1 uppercase tracking-wider">Klasörler</h3>
      <nav class="space-y-[4px]">
        {#each [
          { id: 'downloads', name: 'İndirilenler', icon: 'M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z' },
          { id: 'videos', name: 'Videolar', icon: 'M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z' },
          { id: 'music', name: 'Müzikler', icon: 'M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12 0c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3' }
        ] as folder}
          <button
            type="button"
            onclick={() => onSelectFolder(folder.id as FolderFilter)}
            class="w-full h-[37px] px-[12px] py-[10px] rounded-[10px] flex items-center gap-[10px] text-[13px] font-normal transition-colors cursor-pointer {activeFolder === folder.id ? 'bg-[#2c2c2e] text-white' : 'text-[#8e8e93] hover:bg-[#252528] hover:text-white'}"
          >
            <svg class="w-4 h-4 stroke-current" fill="none" viewBox="0 0 24 24" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d={folder.icon} />
            </svg>
            <span>{folder.name}</span>
          </button>
        {/each}
      </nav>
    </div>
  </div>

  <div class="space-y-[8px] pt-4 border-t border-[#2c2c2e]/60">
    <div class="flex items-center justify-between text-[11px]">
      <span class="text-[#8e8e93]">Macintosh HD</span>
      <span class="text-[#555558]">124 GB boş</span>
    </div>
    <div class="w-full h-[4px] bg-[#2c2c2e] rounded-[2px] overflow-hidden">
      <div class="h-full bg-[#8e8e93] rounded-[2px] transition-all duration-300 w-[74%]"></div>
    </div>
  </div>
</aside>
