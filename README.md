<div align="center">
  <img src="./Multron_Dowland_Manager_logo.jpg" alt="Multron Download Manager Logo" width="160" height="160" style="border-radius: 50%;" />

  # Multron Download Manager

  **Ultra-fast, cross-platform download accelerator built for pure bandwidth saturation and zero-allocation efficiency.**

  <p align="center">
    <a href="https://golang.org/"><img src="https://img.shields.io/badge/Backend-Go%201.22+-00ADD8?style=for-the-badge&logo=go&logoColor=white" alt="Go Core" /></a>
    <a href="https://wails.io/"><img src="https://img.shields.io/badge/Framework-Wails%20v2-DF1A5E?style=for-the-badge&logo=wails&logoColor=white" alt="Wails" /></a>
    <a href="https://svelte.dev/"><img src="https://img.shields.io/badge/Frontend-Svelte%20%7C%20TypeScript-FF3E00?style=for-the-badge&logo=svelte&logoColor=white" alt="Svelte & TS" /></a>
    <a href="https://tailwindcss.com/"><img src="https://img.shields.io/badge/Style-Tailwind%20CSS-38B2AC?style=for-the-badge&logo=tailwind-css&logoColor=white" alt="Tailwind CSS" /></a>
  </p>

  <p align="center">
    <img src="https://img.shields.io/badge/Platform-Windows%20%7C%20macOS%20%7C%20Linux-blue?style=flat-square" alt="Platforms" />
    <img src="https://img.shields.io/badge/License-MIT-green.svg?style=flat-square" alt="License" />
    <img src="https://img.shields.io/badge/PRs-welcome-brightgreen.svg?style=flat-square" alt="PRs Welcome" />
    <img src="https://img.shields.io/badge/Status-Active%20Development-orange?style=flat-square" alt="Status" />
  </p>
</div>

---

## 🚀 Overview

**Multron Download Manager** is an open-source, resource-conscious, high-throughput download manager engineered from the ground up to overcome the limitations of legacy tools.

Unlike traditional managers that bottleneck disk performance by merging fragments after downloading, Multron utilizes **pre-allocated sparse files** and **concurrent dynamic chunking** to write network streams directly to disk offsets without post-process merging delays.

---

## ✨ Key Features

- **⚡ Raw Bandwidth Saturation:** Parallel multi-connection downloading via lightweight Go goroutines.
- **🔀 Dynamic Chunk Segmentation:** Actively detects lagging threads and dynamically subdivides pending byte ranges in real time.
- **💾 Zero-Merge Disk I/O:** Pre-allocates file storage upfront (`fallocate` on Linux, `SetFileInformationByHandle` on Windows) and writes incoming packets directly to target byte offsets.
- **🪶 Ultra-Low Resource Footprint:** Powered by native system WebViews (via Wails) instead of bloated Chromium engines (~35–50 MB idle memory).
- **🔁 Fault-Tolerant Resuming:** Seamlessly handles network drops and socket timeouts with cryptographic hash checks (`SHA-256` / `MD5`).
- **🌐 Native Messaging Ready:** Built-in protocol interface to capture downloads automatically from Chrome, Edge, and Firefox.
- **🎨 Glassmorphic Modern UI:** Dark-mode native interface with high-framerate chunk heatmaps, queue controls, and speed visualizers.

---

## 🏗️ Architecture

Multron enforces a strict boundary between low-level network I/O and user interface rendering:

```
┌─────────────────────────────────────────────────────────────┐
│                       Multron Core (Go)                     │
│  - Runtime Netpoller (epoll/kqueue/IOCP)                    │
│  - Dynamic Chunk Coordinator & Bandwidth Optimizer         │
│  - Sparse File Offset Direct Writer (Seek & Write)          │
└──────────────────────────────┬──────────────────────────────┘
                               │ Event Bus / Typed IPC
                               │ (Throttled @ 5Hz telemetry)
┌──────────────────────────────▼──────────────────────────────┐
│                    Multron Frontend (Svelte)                │
│  - Virtualized Task Tables (Smooth with 10,000+ items)      │
│  - Real-time Connection Speed & Progress Visualizers        │
│  - System Tray Integration & Native Notifications          │
└─────────────────────────────────────────────────────────────┘
```

---

## 🛠️ Tech Stack

| Domain | Technology | Purpose |
| :--- | :--- | :--- |
| **Engine (Core)** | **Go** | Concurrency, Goroutine pools, Direct File I/O, Socket control |
| **Desktop Bridge** | **Wails v2/v3** | Native WebView bridge, type-safe IPC, single-binary compilation |
| **UI Framework** | **TypeScript + Svelte** | Reactive state handling without Virtual DOM overhead |
| **Styling** | **Tailwind CSS** | Sleek, modern, and ultra-lightweight UI system |

---

## 📦 Getting Started

### Prerequisites

Ensure you have the following installed on your host machine:

- [Go (1.22+)](https://golang.org/dl/)
- [Node.js (LTS)](https://nodejs.org/) & [pnpm](https://pnpm.io/)
- [Wails CLI](https://wails.io/docs/gettingstarted/installation):
  ```bash
  go install github.com/wailsapp/wails/v2/cmd/wails@latest
  ```

### Installation & Development

1. **Clone the repository:**
   ```bash
   git clone https://github.com/your-username/multron-download-manager.git
   cd multron-download-manager
   ```

2. **Run in Live Development Mode:**
   ```bash
   wails dev
   ```
   *This starts the Go backend and Vite hot-reload dev server concurrently.*

3. **Compile Production Binary:**
   ```bash
   wails build -clean -s
   ```
   *The optimized standalone executable will be generated inside `build/bin/`.*

---

## 🗺️ Roadmap

- [x] Dynamic multi-threaded chunking engine
- [x] Pre-allocated zero-stitch direct disk writer
- [ ] Adaptive bandwidth throttling per task & global limiter
- [ ] Browser extensions integration (Chrome Web Store / Firefox Add-ons)
- [ ] HTTP/3 (QUIC) stream multiplexing
- [ ] BitTorrent / Magnet link protocol support

---

## 🤝 Contributing

Contributions make the open-source community an incredible place to learn, inspire, and create. Any contributions you make are **greatly appreciated**.

1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

---

## 📄 License

Distributed under the **MIT License**. See [`LICENSE`](./LICENSE) for more information.

<div align="center">
  <sub>Engineered with precision for maximum throughput. Built by the Multron Team.</sub>
</div>
