# 交付模型

shun 把交付拆成三個正交的部分：

## Payload

應用程式目錄打包成 zstd 壓縮的 tar，附帶 SHA-256 清單（`shun-manifest.json`）。
封存可以嵌入安裝器二進位（`include_bytes!`，單檔安裝器模式）或作為
sidecar 攜帶。解壓時逐條目校驗清單並串流產出進度事件。

## Flow

一次交付執行是一串 `FlowEvent`——`started`、`progress { phase, step, percent }`、
`completed`、`failed`——由殼 UI 直接渲染。進度是**多層**的：線上安裝器在
下載（download）與解壓（extract）兩個階段同時推進。安裝流解壓 payload、
落盤清單（供解除安裝消費），隨後要麼註冊（本機模式），要麼寫入可攜標記
（可攜模式）。

## Targets

- **install** —— NSIS 式註冊（使用者級 ARP 條目、自複製解除安裝器、開始
  功能表捷徑、深層連結），外加零登錄檔的可攜模式。解除安裝按清單移除全部痕跡。
- **flash** —— 區塊裝置寫入與寫後校驗（映像燒錄）。後端隨 evernight
  燒錄器落地；trait 介面與裝置列舉今日已就緒。

## WebView2（Windows）

殼自身就是 Tauri 應用，因此 WebView2 執行時是它自己 UI 的硬前提。交付
清單選擇策略：要求系統執行時、內嵌 Evergreen 離線安裝器，或私有攜帶
固定版執行時——一份副本由殼與已裝應用共享，橫跨安裝與可攜模式。
