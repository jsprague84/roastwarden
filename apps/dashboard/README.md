Roastwarden Dashboard
===================

A responsive React + Vite + TypeScript app for desktop and phone. Streams live telemetry via WebSocket, provides control endpoints, and supports auto‑tune.

Dev Quick Start
---------------
- Prereqs: Node 18+, npm or pnpm
- Start backend server on http://127.0.0.1:8080 (or adjust `vite.config.ts` proxy)
- Install + run:

```
cd roastwarden/apps/dashboard
npm install
npm run dev
```

This proxies `/api`, `/ws`, `/api-docs`, etc. to the backend.

Build
-----
```
npm run build
```

The static build is in `dist/`. You can serve it with any static server, or integrate into Axum by serving these files under `/app`.

Notes
-----
- Charts: ECharts via `echarts-for-react`
- State:
  - REST: TanStack Query
  - Live telemetry: WebSocket + Zustand
- PWA: basic manifest included (optional production polish later)

