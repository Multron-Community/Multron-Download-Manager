<script lang="ts">
  import type { CategoryFilter, FolderFilter, DownloadItemData } from './types';
  import Sidebar from './components/Sidebar.svelte';
  import Header from './components/Header.svelte';
  import DownloadList from './components/DownloadList.svelte';
  import NewTaskModal from './components/NewTaskModal.svelte';

  let activeCategory = $state<CategoryFilter>('all');
  let activeFolder = $state<FolderFilter>('all');
  let searchQuery = $state<string>('');
  let isNewTaskOpen = $state<boolean>(false);

  let downloads = $state<DownloadItemData[]>([]);

  let counts = $derived({
    all: downloads.length,
    active: downloads.filter((d) => d.status === 'active').length,
    completed: downloads.filter((d) => d.status === 'completed').length,
    paused: downloads.filter((d) => d.status === 'paused').length
  });

  let totalSpeed = $derived.by(() => {
    const activeItems = downloads.filter((d) => d.status === 'active' && d.speed);
    if (activeItems.length === 0) return '0.0 MB/s';
    let totalMB = 0;
    for (const item of activeItems) {
      const match = item.speed?.match(/([\d.]+)/);
      if (match) totalMB += parseFloat(match[1]);
    }
    return `${totalMB.toFixed(1)} MB/s`;
  });

  let filteredDownloads = $derived.by(() => {
    return downloads.filter((item) => {
      if (activeCategory !== 'all' && item.status !== activeCategory) return false;
      if (activeFolder !== 'all' && item.folder !== activeFolder) return false;
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        return item.name.toLowerCase().includes(q) || item.extension.toLowerCase().includes(q);
      }
      return true;
    });
  });

  function handleToggleStatus(id: string) {
    downloads = downloads.map((item) => {
      if (item.id === id) {
        if (item.status === 'active') {
          return { ...item, status: 'paused', speed: undefined, remainingTime: undefined };
        } else if (item.status === 'paused' || item.status === 'queued') {
          return { ...item, status: 'active', speed: '12.5 MB/s', remainingTime: '3 dk kaldı' };
        }
      }
      return item;
    });
  }

  function handleDelete(id: string) {
    downloads = downloads.filter((item) => item.id !== id);
  }

  function handleOpenFolder(id: string) {
    const item = downloads.find((d) => d.id === id);
    alert(`"${item?.name}" dosyası klasörde gösteriliyor.`);
  }

  function handleAddNewTask(url: string, targetFolder: FolderFilter) {
    const filename = url.split('/').pop() || 'yeni_indirme.file';
    const parts = filename.split('.');
    const ext = parts.length > 1 ? parts.pop() || 'bin' : 'bin';

    const newItem: DownloadItemData = {
      id: Date.now().toString(),
      name: filename,
      extension: ext,
      totalSize: '250 MB',
      downloadedSize: '0 MB',
      status: 'active',
      progress: 0,
      speed: '18.5 MB/s',
      remainingTime: 'Hesaplanıyor...',
      folder: targetFolder === 'all' ? 'downloads' : targetFolder,
      url
    };

    downloads = [newItem, ...downloads];
  }
</script>

<div class="w-screen h-screen bg-[#161617] flex flex-col overflow-hidden">
  <div class="flex-1 flex overflow-hidden min-h-0">
      <Sidebar
        {activeCategory}
        {activeFolder}
        {counts}
        onSelectCategory={(cat) => { activeCategory = cat; activeFolder = 'all'; }}
        onSelectFolder={(folder) => { activeFolder = folder; activeCategory = 'all'; }}
      />
      <div class="flex-1 flex flex-col min-w-0 bg-[#161617]">
        <Header
          title={activeFolder !== 'all' ? `Klasör: ${activeFolder}` : 'İndirmeler'}
          {totalSpeed}
          {searchQuery}
          onSearchChange={(q) => (searchQuery = q)}
          onNewTask={() => (isNewTaskOpen = true)}
        />
        <DownloadList
          items={filteredDownloads}
          onToggleStatus={handleToggleStatus}
          onDelete={handleDelete}
          onOpenFolder={handleOpenFolder}
          onNewTask={() => (isNewTaskOpen = true)}
        />
      </div>
    </div>
  </div>

<NewTaskModal
  isOpen={isNewTaskOpen}
  onClose={() => (isNewTaskOpen = false)}
  onAdd={handleAddNewTask}
/>
