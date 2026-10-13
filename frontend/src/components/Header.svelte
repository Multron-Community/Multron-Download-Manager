<script lang="ts">
  interface Props {
    title?: string;
    totalSpeed?: string;
    searchQuery: string;
    onSearchChange: (query: string) => void;
    onNewTask: () => void;
  }

  let {
    title = 'İndirmeler',
    totalSpeed = '18.6 MB/s',
    searchQuery,
    onSearchChange,
    onNewTask
  }: Props = $props();
</script>

<header class="h-[60px] px-6 border-b border-[#2c2c2e] flex items-center justify-between flex-shrink-0 bg-[#161617] select-none" style="--wails-draggable:drag">
  <!-- Title & Total Speed Badge -->
  <div class="flex items-center gap-[12px]">
    <h1 class="text-[16px] font-semibold text-white tracking-tight">{title}</h1>
    {#if totalSpeed}
      <div class="h-[22px] px-[8px] py-[4px] bg-[#2c2c2e] rounded-[6px] flex items-center justify-center">
        <span class="text-[11px] font-medium text-[#8e8e93]">{totalSpeed}</span>
      </div>
    {/if}
  </div>

  <!-- Actions (Search Box & New Task Button) -->
  <div class="flex items-center gap-[12px]" style="--wails-draggable:no-drag">
    <!-- Search Input -->
    <div class="w-[160px] h-[28px] px-[10px] bg-[#1e1e1f] border border-[#2c2c2e] rounded-[8px] flex items-center gap-[8px] transition-all focus-within:border-[#8e8e93]/50 focus-within:ring-1 focus-within:ring-[#8e8e93]/20">
      <svg class="w-3 h-3 text-[#8e8e93] flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
        <path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
      </svg>
      <input
        type="text"
        placeholder="Ara..."
        value={searchQuery}
        oninput={(e) => onSearchChange(e.currentTarget.value)}
        class="w-full bg-transparent text-[12px] text-white placeholder-[#8e8e93] outline-none border-none p-0 font-normal"
      />
      {#if searchQuery}
        <button
          type="button"
          onclick={() => onSearchChange('')}
          aria-label="Temizle"
          class="text-[#8e8e93] hover:text-white transition-colors"
        >
          <svg class="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      {/if}
    </div>

    <!-- New Task Button -->
    <button
      type="button"
      onclick={onNewTask}
      class="h-[28px] px-[12px] py-[6px] bg-white text-[#161617] rounded-[8px] flex items-center gap-[6px] text-[12px] font-semibold hover:bg-gray-200 transition-colors cursor-pointer shadow-sm active:scale-[0.98]"
    >
      <svg class="w-3 h-3 stroke-current" fill="none" viewBox="0 0 24 24" stroke-width="3">
        <path stroke-linecap="round" stroke-linejoin="round" d="M12 4v16m8-8H4" />
      </svg>
      <span>Yeni Görev</span>
    </button>
  </div>
</header>
