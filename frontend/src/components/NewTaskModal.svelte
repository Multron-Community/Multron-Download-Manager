<script lang="ts">
  import type { FolderFilter } from '../types';

  interface Props {
    isOpen: boolean;
    onClose: () => void;
    onAdd: (url: string, folder: FolderFilter) => void;
  }

  let { isOpen, onClose, onAdd }: Props = $props();

  let url = $state('');
  let selectedFolder = $state<FolderFilter>('downloads');

  function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    if (!url.trim()) return;
    onAdd(url.trim(), selectedFolder);
    url = '';
    onClose();
  }
</script>

{#if isOpen}
  <!-- Backdrop Overlay -->
  <div class="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
    <!-- Modal Card -->
    <div
      class="w-full max-w-md bg-[#1e1e1f] border border-[#2c2c2e] rounded-[12px] shadow-2xl p-6 flex flex-col gap-5 animate-in fade-in zoom-in-95 duration-150"
    >
      <!-- Modal Header -->
      <div class="flex items-center justify-between">
        <h2 class="text-[16px] font-semibold text-white">Yeni İndirme Görevi</h2>
        <button
          type="button"
          onclick={onClose}
          aria-label="Kapat"
          class="w-6 h-6 rounded-full bg-[#2c2c2e] text-[#8e8e93] hover:text-white flex items-center justify-center transition-colors cursor-pointer"
        >
          <svg class="w-3.5 h-3.5 stroke-current" fill="none" viewBox="0 0 24 24" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      <!-- Form -->
      <form onsubmit={handleSubmit} class="flex flex-col gap-4">
        <!-- URL Input -->
        <div class="space-y-1.5">
          <label for="url-input" class="text-[12px] font-medium text-[#8e8e93]">
            İndirme Bağlantısı (URL)
          </label>
          <input
            id="url-input"
            type="url"
            required
            placeholder="https://example.com/file.zip"
            bind:value={url}
            class="w-full h-9 px-3 bg-[#161617] border border-[#2c2c2e] rounded-[8px] text-[13px] text-white placeholder-[#555558] outline-none focus:border-[#8e8e93] transition-colors"
          />
        </div>

        <!-- Folder Target Selector -->
        <div class="space-y-1.5">
          <label for="folder-select" class="text-[12px] font-medium text-[#8e8e93]">
            Hedef Klasör
          </label>
          <select
            id="folder-select"
            bind:value={selectedFolder}
            class="w-full h-9 px-3 bg-[#161617] border border-[#2c2c2e] rounded-[8px] text-[13px] text-white outline-none focus:border-[#8e8e93] transition-colors cursor-pointer"
          >
            <option value="downloads">İndirilenler</option>
            <option value="videos">Videolar</option>
            <option value="music">Müzikler</option>
          </select>
        </div>

        <!-- Buttons -->
        <div class="flex items-center justify-end gap-3 pt-2">
          <button
            type="button"
            onclick={onClose}
            class="h-8 px-4 bg-[#2c2c2e] text-[#8e8e93] hover:text-white rounded-[8px] text-[12px] font-medium transition-colors cursor-pointer"
          >
            İptal
          </button>
          <button
            type="submit"
            class="h-8 px-4 bg-white text-[#161617] hover:bg-gray-200 rounded-[8px] text-[12px] font-semibold transition-colors cursor-pointer shadow-sm"
          >
            İndirmeyi Başlat
          </button>
        </div>
      </form>
    </div>
  </div>
{/if}
