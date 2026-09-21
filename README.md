<p align="center">
  <img src="Multron_Dowland_Manager_logo.png" alt="Multron Logo" width="140" />
</p>

<h1 align="center">Multron Download Manager</h1>

<p align="center">
  Ağ bant genişliğini maksimuma çıkaran, doğrudan disk offset yazımı yapan, yüksek performanslı ve modern masaüstü indirme yöneticisi.
</p>

<p align="center">
  <img src="[https://img.shields.io/badge/Go-1.21+-00ADD8?style=flat&logo=go&logoColor=white](https://img.shields.io/badge/Go-1.21+-00ADD8?style=flat&logo=go&logoColor=white)" alt="Go" />
  <img src="[https://img.shields.io/badge/Svelte-5-FF3E00?style=flat&logo=svelte&logoColor=white](https://img.shields.io/badge/Svelte-5-FF3E00?style=flat&logo=svelte&logoColor=white)" alt="Svelte 5" />
  <img src="[https://img.shields.io/badge/Wails-v2-DF0000?style=flat&logo=wails&logoColor=white](https://img.shields.io/badge/Wails-v2-DF0000?style=flat&logo=wails&logoColor=white)" alt="Wails" />
  <img src="[https://img.shields.io/badge/TailwindCSS-v4-06B6D4?style=flat&logo=tailwindcss&logoColor=white](https://img.shields.io/badge/TailwindCSS-v4-06B6D4?style=flat&logo=tailwindcss&logoColor=white)" alt="Tailwind CSS" />
  <img src="[https://img.shields.io/badge/License-MIT-blue.svg](https://img.shields.io/badge/License-MIT-blue.svg)" alt="License" />
</p>

---

## ⚡ Temel Mimari ve Özellikler

* **Doğrudan Disk Offset Yazımı (Direct Disk Offset I/O):** İndirilen parçalar geçici dosyalara yazılıp işlem sonunda birleştirilmez (post-download merge yok). Disk üzerinde önceden ayrılan bloklara doğrudan ilgili bayt aralığı yazılır; indirme bittiği an dosya hazırdır.
* **Dinamik Parçalı İndirme:** Ağ durumuna ve sunucu yanıtlarına göre eşzamanlı HTTP bağlantılarını dinamik olarak bölerek bant genişliğini doyurur.
* **Düşük Bellek Ayak İzi (Zero-Allocation Hedefi):** Go motoru tarafında minimum bellek kopyalaması ve optimize edilmiş `io.Writer` akışları.
* **Hafif ve Hızlı Arayüz:** Electron yerine işletim sisteminin yerel WebKit motorunu kullanan Wails, Svelte 5 reaktivitesi ve Tailwind CSS v4 mimarisi.
* **Sözleşme Tabanlı IPC:** Go backend ile Svelte frontend arasında tip güvenli ve olay güdümlü (event-driven) veri senkronizasyonu.

---

## 🛠️ Teknoloji Yığını

| Katman | Teknoloji | Açıklama |
| :--- | :--- | :--- |
| **Çekirdek Motor** | **Go (Golang)** | Ağ soketleri, segment yönetimi ve dosya I/O |
| **Masaüstü Köprüsü** | **Wails** | İki yönlü IPC ve yerel sistem pencere kontrolü |
| **Kullanıcı Arayüzü**| **Svelte 5 + TypeScript** | Yüksek performanslı ve hafif reaktif arayüz |
| **Stil / Tasarım** | **Tailwind CSS v4** | Modern, karanlık tema odaklı UI bileşenleri |

---

## 📁 Proje Yapısı

```text
Multron-Download-Manager/
├── app.go              # Wails köprü mantığı ve olay bağlayıcıları
├── main.go             # Masaüstü pencere yapılandırması ve giriş noktası
├── pkg/
│   └── engine/         # Bağımsız Go indirme motoru ve disk yazıcısı
├── frontend/
│   ├── src/
│   │   ├── App.svelte  # Ana kontrol paneli
│   │   ├── main.ts     # Frontend başlangıç noktası
│   │   └── style.css   # Tailwind v4 tema direktifleri
│   ├── package.json
│   └── vite.config.ts
├── wails.json          # Wails derleme ve paketleme ayarları
└── README.md
```

---

## 🚀 Geliştirme Ortamı Kurulumu

### Önkoşullar
* **Go** (1.21 veya üzeri)
* **Node.js** (v18 veya üzeri) ve **npm**
* **Wails CLI:**
  ```bash
  go install github.com/wailsapp/wails/v2/cmd/wails@latest
  ```

### Depoyu Klonlama ve Çalıştırma

```bash
# 1. Depoyu klonlayın
git clone https://github.com/Multron-Community/Multron-Download-Manager.git
cd Multron-Download-Manager

# 2. Geliştirici modunda başlatın (Hot-Reload devrede)
wails dev
```

---

## 🌿 Git ve Geliştirme İş Akışı

Doğrudan `main` dalına commit atılmaz. Tüm özellikler ilgili çalışma dallarından Pull Request açılarak ana dala birleştirilir:

* `feat/engine-core` : Go indirme motoru, chunk bölücü ve I/O işlemleri.
* `feat/ui-dashboard` : Svelte 5 paneli, hız grafikleri ve kullanıcı etkileşimleri.

---

## 📄 Lisans

Bu proje **MIT Lisansı** altında lisanslanmıştır. Detaylar için [LICENSE](LICENSE) dosyasına göz atabilirsiniz.