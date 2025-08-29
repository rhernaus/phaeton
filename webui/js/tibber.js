// Tibber prices chart and plan overlay
(function initTibberChart() {
  const canvas = document.getElementById('tibber_chart');
  const section = document.getElementById('tibber_section');
  const tooltip = document.getElementById('tibber_tooltip');
  const overlayToggle = document.getElementById('tibber_overlay_toggle');
  if (!section) return;

  let points = [];
  let showOverlay = overlayToggle ? !!overlayToggle.checked : true;

  function shouldShowSection() {
    const cfg = window.currentConfig || {};
    const scheduleMode = (cfg.schedule && cfg.schedule.mode) || 'time';
    return scheduleMode === 'tibber';
  }

  function resizeCanvas() {
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    const cssWidth = Math.floor(rect.width || 800);
    const cssHeight = Math.floor(rect.height || 160);
    canvas.width = Math.max(320, cssWidth) * dpr;
    canvas.height = Math.max(120, cssHeight) * dpr;
    const ctx = canvas.getContext('2d');
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }

  function draw() {
    if (!canvas || !points || points.length === 0) return;
    const ctx = canvas.getContext('2d');
    const W = canvas.width / (window.devicePixelRatio || 1);
    const H = canvas.height / (window.devicePixelRatio || 1);
    ctx.clearRect(0, 0, W, H);
    ctx.fillStyle = '#1a2332';
    ctx.fillRect(0, 0, W, H);

    // Compute ranges
    const tVals = points.map(p => new Date(p.starts_at).getTime() / 1000);
    const tMin = Math.min.apply(null, tVals);
    const tMax = Math.max.apply(null, tVals.map((t, i) => {
      const end = points[i].ends_at ? new Date(points[i].ends_at).getTime() / 1000 : (t + 3600);
      return end;
    }));
    const tSpan = Math.max(1, tMax - tMin);
    const vMax = Math.max(0.05, Math.ceil(points.reduce((m, p) => Math.max(m, Number(p.total) || 0), 0) * 100) / 100);

    function mapX(tSec) { return 40 + ((tSec - tMin) / tSpan) * (W - 60); }
    function mapY(v) { return H - 20 - (v / vMax) * (H - 40); }

    // Grid
    ctx.strokeStyle = 'rgba(255,255,255,0.1)';
    ctx.lineWidth = 1;
    for (let i = 0; i <= 4; i++) {
      const y = mapY((vMax / 4) * i);
      ctx.beginPath(); ctx.moveTo(40, y); ctx.lineTo(W - 20, y); ctx.stroke();
    }

    // Bars for price per hour
    const barPad = 2;
    points.forEach((p, idx) => {
      const t0 = new Date(p.starts_at).getTime() / 1000;
      const t1 = p.ends_at ? new Date(p.ends_at).getTime() / 1000 : (t0 + 3600);
      const x0 = mapX(t0), x1 = mapX(t1);
      const y0 = mapY(0), y1 = mapY(Number(p.total) || 0);
      const xL = Math.min(x0, x1) + barPad;
      const xR = Math.max(x0, x1) - barPad;
      const w = Math.max(1, xR - xL);
      ctx.fillStyle = '#3b82f6';
      ctx.globalAlpha = 0.7;
      ctx.fillRect(xL, Math.min(y0, y1), w, Math.abs(y1 - y0));
      ctx.globalAlpha = 1.0;

      if (showOverlay && p.will_charge) {
        ctx.fillStyle = 'rgba(16,185,129,0.25)';
        ctx.fillRect(xL, 10, w, H - 30);
      }
    });

    // Axes
    ctx.strokeStyle = 'rgba(255,255,255,0.2)';
    ctx.lineWidth = 1; ctx.beginPath(); ctx.moveTo(40, 10); ctx.lineTo(40, H - 20); ctx.lineTo(W - 20, H - 20); ctx.stroke();
    ctx.fillStyle = '#94a3b8'; ctx.font = '12px -apple-system, sans-serif';
    ctx.fillText(`${vMax.toFixed(2)} €/kWh`, 4, mapY(vMax) + 4);
    ctx.fillText('0', 24, H - 22);

    const numTicks = 6; const step = tSpan / numTicks;
    for (let i = 0; i <= numTicks; i++) {
      const tTick = tMin + step * i; const x = mapX(tTick);
      ctx.strokeStyle = 'rgba(255,255,255,0.15)'; ctx.beginPath(); ctx.moveTo(x, H - 20); ctx.lineTo(x, H - 16); ctx.stroke();
      const d = new Date(tTick * 1000); const hh = String(d.getHours()).padStart(2, '0'); const dd = String(d.getDate()).padStart(2, '0'); const label = `${dd} ${hh}:00`;
      const textW = ctx.measureText(label).width;
      ctx.fillText(label, Math.min(Math.max(40, x - textW / 2), W - 20 - textW), H - 4);
    }
  }

  function attachHover() {
    if (!canvas || !tooltip) return;
    canvas.addEventListener('mousemove', e => {
      if (!points || points.length === 0) return;
      const rect = canvas.getBoundingClientRect();
      const cssX = e.clientX - rect.left;
      // Hit test by finding nearest bar center
      const dpr = window.devicePixelRatio || 1;
      const W = canvas.width / dpr;
      const H = canvas.height / dpr;
      const tVals = points.map(p => new Date(p.starts_at).getTime() / 1000);
      const tMin = Math.min.apply(null, tVals);
      const tMax = Math.max.apply(null, tVals.map((t, i) => {
        const end = points[i].ends_at ? new Date(points[i].ends_at).getTime() / 1000 : (t + 3600);
        return end;
      }));
      const tSpan = Math.max(1, tMax - tMin);
      function mapX(tSec) { return 40 + ((tSec - tMin) / tSpan) * (W - 60); }
      let nearestIdx = 0; let bestDx = 1e9; let nearestX = 0;
      points.forEach((p, idx) => {
        const t0 = new Date(p.starts_at).getTime() / 1000;
        const t1 = p.ends_at ? new Date(p.ends_at).getTime() / 1000 : (t0 + 3600);
        const xc = (mapX(t0) + mapX(t1)) / 2;
        const dx = Math.abs((cssX / rect.width) * W - xc);
        if (dx < bestDx) { bestDx = dx; nearestIdx = idx; nearestX = xc; }
      });
      const p = points[nearestIdx];
      const price = Number(p.total) || 0;
      const dt = new Date(p.starts_at);
      const label = `${dt.toLocaleString()} — ${price.toFixed(4)} €/kWh${p.will_charge ? ' · planned' : ''}`;
      tooltip.textContent = label;
      const parent = canvas.parentElement; const parentRect = parent ? parent.getBoundingClientRect() : rect;
      const Wcss = rect.width; const scale = Wcss / W; const cssX2 = nearestX * scale + (rect.left - parentRect.left); const top = rect.top - parentRect.top + 12;
      tooltip.style.left = `${cssX2}px`; tooltip.style.top = `${top}px`; tooltip.style.display = '';
    });
    canvas.addEventListener('mouseleave', () => { tooltip.style.display = 'none'; });
  }

  async function fetchPlan() {
    try {
      const res = await fetch('/api/tibber/plan');
      const body = await res.json();
      if (Array.isArray(body.points)) {
        points = body.points;
        draw();
      }
    } catch (e) {
      // eslint-disable-next-line no-console
      console.error('tibber plan error', e);
    }
  }

  function refreshVisibility() {
    if (!section) return;
    const visible = shouldShowSection();
    section.style.display = visible ? '' : 'none';
    if (visible) { resizeCanvas(); fetchPlan(); }
  }

  window.addEventListener('resize', () => { if (shouldShowSection()) { resizeCanvas(); draw(); } });
  if (overlayToggle) {
    overlayToggle.addEventListener('change', () => { showOverlay = !!overlayToggle.checked; draw(); });
  }

  // Refresh visibility periodically to react to config changes
  setInterval(refreshVisibility, 2000);
  // Kick off once
  setTimeout(refreshVisibility, 0);
  // Refresh plan periodically (prices rarely change)
  setInterval(() => { if (shouldShowSection()) fetchPlan(); }, 5 * 60 * 1000);
  attachHover();
})();


