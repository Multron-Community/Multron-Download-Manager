export type DownloadStatus = 'active' | 'paused' | 'completed' | 'queued';

export type CategoryFilter = 'all' | 'active' | 'completed' | 'paused';
export type FolderFilter = 'all' | 'downloads' | 'videos' | 'music';

export interface DownloadItemData {
  id: string;
  name: string;
  extension: string;
  totalSize: string;
  downloadedSize: string;
  status: DownloadStatus;
  progress: number; // 0 to 100
  speed?: string;
  remainingTime?: string;
  folder?: 'downloads' | 'videos' | 'music';
  url?: string;
}
