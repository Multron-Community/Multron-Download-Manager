<script lang="ts">
  import type { DownloadItemData } from '../types';

  interface Props {
    item: DownloadItemData;
    onToggleStatus: (id: string) => void;
    onDelete: (id: string) => void;
    onOpenFolder?: (id: string) => void;
  }

  let { item, onToggleStatus, onDelete, onOpenFolder }: Props = $props();

  const getStatusLabel = (status: string) => {
    switch (status) {
      case 'active': return 'İndiriliyor';
      case 'paused': return 'Duraklatıldı';
      case 'completed': return 'Tamamlandı';
      case 'queued': return 'Bekliyor';
      default: return status;
    }
  };

  const getBadgeBg = (ext: string) => {
    switch (ext.toLowerCase()) {
      case 'dmg': case 'iso': return 'bg-[#2c2c2e] text-white';
      case 'zip': case 'xip': case 'rar': case '7z': return 'bg-[#2c2c2e] text-white';
      case 'mp4': case 'mov': case 'mkv': return 'bg-[#2c2c2e] text-white';
      case 'mp3': case 'wav': case 'flac': return 'bg-[#2c2c2e] text-white';
      default: return 'bg-[#2c2c2e] text-white';
    }
  };
</script>

<div
  class="w-full p-4 border border-[#222223] rounded-[12px] transition-all group flex flex-col gap-3
    {item.status === 'active' ? 'bg-[#1c1c1e] shadow-sm' : 'bg-[#161617] hover:bg-[#1a1a1c]'}"
>
  <!-- Upper Info Section -->
  <div class="flex items-center justify-between h-[36px]">
    <!-- File Details (Icon + Name & Metadata) -->
    <div class="flex items-center gap-[12px] min-w-0 flex-1 pr-4">
      <!-- File Extension Badge -->
      <div class="w-[36px] h-[36px] rounded-[8px] flex items-center justify-center flex-shrink-0 font-bold text-[10px] uppercase select-none tracking-wider {getBadgeBg(item.extension)}">
        {item.extension.slice(0, 3)}
      </div>

      <!-- Name & Meta -->
      <div class="flex flex-col justify-center gap-[4px] min-w-0 flex-1">
        <h3 class="text-[14px] font-medium text-white truncate leading-tight group-hover:text-gray-100">
          {item.name}
        </h3>
        
        <!-- Metadata Line -->
        <div class="flex items-center gap-[8px] text-[11px] leading-tight">
          <span class="text-[#8e8e93] font-normal">{item.downloadedSize} / {item.totalSize}</span>
          <span class="w-[3px] h-[3px] rounded-full bg-[#555558] flex-shrink-0"></span>
          
          <span class={item.status === 'active' || item.status === 'queued' ? 'text-white font-normal' : 'text-[#8e8e93] font-normal'}>
            {getStatusLabel(item.status)}
          </span>

          {#if item.status === 'active' || item.status === 'paused'}
            <span class="w-[3px] h-[3px] rounded-full bg-[#555558] flex-shrink-0"></span>
            <span class="text-white font-normal">%{item.progress}</span>
          {/if}
        </div>
      </div>
    </div>

    <!-- Metrics & Actions -->
    <div class="flex items-center gap-[16px] flex-shrink-0">
      <!-- Metrics (Active status speed & remaining time) -->
      {#if item.status === 'active' && item.speed}
        <div class="flex flex-col items-end gap-[2px] leading-tight text-right">
          <span class="text-[13px] font-medium text-white">{item.speed}</span>
          {#if item.remainingTime}
            <span class="text-[11px] text-[#8e8e93]">{item.remainingTime}</span>
          {/if}
        </div>
      {/if}

      <!-- Delete Action Button (Visible on hover) -->
      <button
        type="button"
        onclick={() => onDelete(item.id)}
        class="w-7 h-7 rounded-full bg-[#2c2c2e]/60 hover:bg-red-500/20 hover:text-red-400 text-[#8e8e93] opacity-0 group-hover:opacity-100 transition-all flex items-center justify-center cursor-pointer"
        title="Sil"
      >
        <svg class="w-3.5 h-3.5 stroke-current" fill="none" viewBox="0 0 24 24" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
        </svg>
      </button>

      <!-- Main Action Circle Button (Pause/Play/Open Folder) -->
      <button
        type="button"
        onclick={() => {
          if (item.status === 'completed') {
            onOpenFolder?.(item.id);
          } else {
            onToggleStatus(item.id);
          }
        }}
        class="w-[28px] h-[28px] rounded-full bg-[#2c2c2e] hover:bg-[#3a3a3d] transition-colors flex items-center justify-center text-white cursor-pointer active:scale-95 shadow-sm"
        title={item.status === 'completed' ? 'Klasörde Göster' : item.status === 'active' ? 'Duraklat' : 'Başlat'}
      >
        {#if item.status === 'active'}
          <!-- Pause Icon -->
          <svg class="w-3 h-3 fill-current stroke-none" viewBox="0 0 24 24">
            <rect x="6" y="5" width="4" height="14" rx="1" />
            <rect x="14" y="5" width="4" height="14" rx="1" />
          </svg>
        {:else if item.status === 'completed'}
          <!-- Folder Open Icon -->
          <svg class="w-3.5 h-3.5 stroke-current" fill="none" viewBox="0 0 24 24" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M5 19a2 2 0 01-2-2V7a2 2 0 012-2h4l2 2h4a2 2 0 012 2v1M5 19h14a2 2 0 002-2v-5a2 2 0 00-2-2H9a2 2 0 00-2 2v5a2 2 0 01-2 2z" />
          </svg>
        {:else}
          <!-- Play / Resume Icon -->
          <svg class="w-3 h-3 fill-current ml-0.5" viewBox="0 0 24 24">
            <path d="M8 5v14l11-7z" />
          </svg>
        {/if}
      </button>
    </div>
  </div>

  <!-- Progress Bar (Active or Paused status) -->
  {#if item.status === 'active' || item.status === 'paused'}
    <div class="w-full h-[4px] bg-[#2c2c2e] rounded-[2px] overflow-hidden">
      <div
        class="h-full rounded-[2px] transition-all duration-300 {item.status === 'active' ? 'bg-white' : 'bg-[#48484a]'}"
        style="width: {item.progress}%"
      ></div>
    </div>
  {/if}
</div>
