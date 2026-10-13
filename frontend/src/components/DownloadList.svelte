<script lang="ts">
  import type { DownloadItemData } from '../types';
  import DownloadItem from './DownloadItem.svelte';

  interface Props {
    items: DownloadItemData[];
    onToggleStatus: (id: string) => void;
    onDelete: (id: string) => void;
    onOpenFolder?: (id: string) => void;
    onNewTask?: () => void;
  }

  let { items, onToggleStatus, onDelete, onOpenFolder, onNewTask }: Props = $props();
</script>

<main class="flex-1 p-6 overflow-y-auto custom-scrollbar flex flex-col gap-[8px] bg-[#161617]">
  {#if items.length > 0}
    {#each items as item (item.id)}
      <DownloadItem
        {item}
        {onToggleStatus}
        {onDelete}
        {onOpenFolder}
      />
    {/each}
  {:else}
    <!-- Empty State -->
    <div class="flex-1 flex flex-col items-center justify-center min-h-[360px] text-center p-8 border border-dashed border-[#2c2c2e]/70 rounded-[12px] bg-[#1a1a1c]/20 my-auto">
      <div class="w-14 h-14 rounded-full bg-[#2c2c2e]/60 flex items-center justify-center text-[#8e8e93] mb-4 shadow-sm">
        <svg class="w-7 h-7 stroke-current" fill="none" viewBox="0 0 24 24" stroke-width="1.5">
          <path stroke-linecap="round" stroke-linejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
        </svg>
      </div>
      <h3 class="text-[15px] font-semibold text-white mb-1.5">Henüz indirme görevi eklenmedi</h3>
      <p class="text-[13px] text-[#8e8e93] max-w-sm mb-5 leading-relaxed">
        Yeni bir dosya indirmek için aşağıdaki veya sağ üstteki buton ile URL ekleyebilirsiniz.
      </p>
      {#if onNewTask}
        <button
          type="button"
          onclick={onNewTask}
          class="h-[32px] px-[14px] bg-white text-[#161617] rounded-[8px] text-[12px] font-semibold hover:bg-gray-200 transition-colors cursor-pointer"
        >
          Yeni Görev Ekle
        </button>
      {/if}
    </div>
  {/if}
</main>
